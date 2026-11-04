//! Minimal MCP stdio server exposing the `swap_quote` tool.
//!
//! This is a local provider: it forwards to the warm local runtime over its
//! internal HTTP endpoint, so repeated tool calls do not pay discovery or
//! snapshot warmup.

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::capability::CapabilityError;
use crate::client::{LocalClient, request_id};
use crate::swap::quote::{QuoteErrorResponse, QuoteRequest};
use crate::token;

pub async fn run_stdio(client: LocalClient) -> i32 {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some(response) = handle_line(line, &client).await {
                    let mut text = match serde_json::to_string(&response) {
                        Ok(text) => text,
                        Err(error) => {
                            eprintln!("mcp response encode error: {error}");
                            return 1;
                        }
                    };
                    text.push('\n');
                    if out.write_all(text.as_bytes()).await.is_err() || out.flush().await.is_err() {
                        return 1;
                    }
                }
            }
            Ok(None) => return 0,
            Err(error) => {
                eprintln!("mcp stdin error: {error}");
                return 1;
            }
        }
    }
}

async fn handle_line(line: &str, client: &LocalClient) -> Option<Value> {
    let value: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": {"code": -32700, "message": format!("parse error: {error}")}
            }));
        }
    };
    handle_request(value, client).await
}

async fn handle_request(value: Value, client: &LocalClient) -> Option<Value> {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if method.starts_with("notifications/") {
        return None;
    }
    let id = value.get("id").cloned()?;
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    let result = match method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {
                "name": "fluxion",
                "version": env!("CARGO_PKG_VERSION"),
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list()),
        "tools/call" => Ok(call_tool(params, client).await),
        other => Err((-32601, format!("method not found: {other}"))),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message)) => {
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
        }
    })
}

fn tools_list() -> Value {
    json!({
        "tools": [{
            "name": "swap_quote",
            "description": "Get an exact-in swap quote from the warm local Fluxion runtime. \
                            Token inputs may be built-in symbols (LDO, ETH, USDC) or 0x addresses. \
                            Amounts are human units such as 100 or 0.5.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "token_in": {"type": "string", "description": "Input token symbol or address"},
                    "token_out": {"type": "string", "description": "Output token symbol or address"},
                    "amount": {"type": "string", "description": "Human amount, e.g. 100 or 0.5"},
                    "decimals_in": {"type": "integer", "minimum": 0, "maximum": 38},
                    "decimals_out": {"type": "integer", "minimum": 0, "maximum": 38},
                    "split": {"type": "boolean", "default": true},
                    "max_hops": {"type": "integer", "minimum": 1},
                    "split_max_branches": {"type": "integer", "minimum": 2, "maximum": 8}
                },
                "required": ["token_in", "token_out", "amount"]
            }
        }]
    })
}

async fn call_tool(params: Value, client: &LocalClient) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if name != "swap_quote" {
        return tool_error(CapabilityError::invalid_argument(format!(
            "unknown tool: {name}"
        )));
    }
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    match quote_tool(&arguments, client).await {
        Ok(response) => tool_result(serde_json::to_value(response).unwrap_or(Value::Null)),
        Err(error) => tool_error(error),
    }
}

async fn quote_tool(arguments: &Value, client: &LocalClient) -> Result<Value, CapabilityError> {
    let token_in = string_arg(arguments, "token_in")?;
    let token_out = string_arg(arguments, "token_out")?;
    let amount = string_arg(arguments, "amount")?;
    let token_in = token::resolve(&token_in, optional_u8(arguments, "decimals_in")?)?;
    let token_out = token::resolve(&token_out, optional_u8(arguments, "decimals_out")?)?;
    let amount_in = token::parse_amount(&amount, token_in.decimals)?;
    let request = QuoteRequest {
        token_in: token_in.address,
        token_out: token_out.address,
        amount_in,
        max_hops: optional_usize(arguments, "max_hops")?,
        amount_out_min: None,
        split: arguments
            .get("split")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        split_max_branches: optional_usize(arguments, "split_max_branches")?,
    };
    let response = client
        .swap_quote(&request, token_in, token_out, "ethereum", "local-mcp")
        .await?;
    Ok(serde_json::to_value(response).unwrap_or(Value::Null))
}

fn string_arg(arguments: &Value, name: &str) -> Result<String, CapabilityError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| CapabilityError::invalid_argument(format!("missing argument: {name}")))
}

fn optional_u8(arguments: &Value, name: &str) -> Result<Option<u8>, CapabilityError> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| CapabilityError::invalid_argument(format!("invalid {name}"))),
    }
}

fn optional_usize(arguments: &Value, name: &str) -> Result<Option<usize>, CapabilityError> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| CapabilityError::invalid_argument(format!("invalid {name}"))),
    }
}

fn tool_result(value: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
        }],
        "isError": false,
    })
}

fn tool_error(error: CapabilityError) -> Value {
    let value =
        serde_json::to_value(QuoteErrorResponse::new(request_id(), error)).unwrap_or(Value::Null);
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
        }],
        "isError": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityErrorCode;

    #[test]
    fn exposes_swap_quote_tool() {
        let tools = tools_list();
        assert_eq!(tools["tools"][0]["name"], "swap_quote");
        assert_eq!(tools["tools"][0]["inputSchema"]["required"][0], "token_in");
    }

    #[test]
    fn rejects_unknown_tool() {
        let result = futures::executor::block_on(call_tool(
            json!({"name": "nope", "arguments": {}}),
            &LocalClient::new("http://127.0.0.1:1"),
        ));
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn error_codes_remain_structured() {
        let value = tool_error(CapabilityError::new(
            CapabilityErrorCode::NoPath,
            "no path",
            false,
        ));
        assert_eq!(value["isError"], true);
        assert!(
            value["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("NO_PATH")
        );
    }
}
