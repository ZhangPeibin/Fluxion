//! Read-only external allocation diagnostics. Supported paths are re-quoted at
//! one block; unknown venues and shared pools are explicitly unsupported, never
//! turned into a partial "total". Diagnostic tokens do not enter the live graph.
use crate::{
    config::Config,
    discovery::token_allowed,
    model::{Pool, PoolKind},
    rpc::RpcClient,
    source::QuoteSource,
};
use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalQuote {
    pub amount_out: String,
    pub block_number: Option<u64>,
    pub latency_ms: u128,
    pub best_route: Value,
    pub gas_cost_usd: Option<String>,
    #[serde(skip)]
    normalized_for_diagnostic: bool,
}

pub async fn paraswap(
    client: &reqwest::Client,
    input: Address,
    output: Address,
    amount: U256,
    decimals: [u8; 2],
) -> Result<ExternalQuote, String> {
    let started = Instant::now();
    let response = client
        .get("https://api.paraswap.io/prices")
        .query(&[
            ("srcToken", format!("{input:#x}")),
            ("destToken", format!("{output:#x}")),
            ("amount", amount.to_string()),
            ("srcDecimals", decimals[0].to_string()),
            ("destDecimals", decimals[1].to_string()),
            ("side", "SELL".into()),
            ("network", "1".into()),
        ])
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?
        .error_for_status()
        .map_err(|e| e.without_url().to_string())?;
    let value: Value = response
        .json()
        .await
        .map_err(|e| e.without_url().to_string())?;
    let route = &value["priceRoute"];
    let parse_address = |key: &str| {
        route[key]
            .as_str()
            .ok_or_else(|| format!("missing {key}"))?
            .parse::<Address>()
            .map_err(|e| e.to_string())
    };
    if parse_address("srcToken")? != input
        || parse_address("destToken")? != output
        || U256::from_str_radix(route["srcAmount"].as_str().ok_or("missing srcAmount")?, 10)
            .map_err(|e| e.to_string())?
            != amount
    {
        return Err("aggregator quote does not match requested tokens/input".into());
    }
    let out = route["destAmount"].as_str().ok_or("missing destAmount")?;
    U256::from_str_radix(out, 10).map_err(|e| e.to_string())?;
    if !route["bestRoute"].is_array() {
        return Err("missing bestRoute".into());
    }
    Ok(ExternalQuote {
        amount_out: out.into(),
        block_number: route["blockNumber"].as_u64(),
        latency_ms: started.elapsed().as_millis(),
        best_route: route["bestRoute"].clone(),
        gas_cost_usd: route["gasCostUSD"].as_str().map(str::to_string),
        normalized_for_diagnostic: false,
    })
}

#[derive(Debug, Serialize)]
pub struct Replay {
    pub status: String,
    pub amount_out: Option<String>,
    pub error: Option<String>,
    pub excluded_tokens: Vec<Address>,
    pub branches: Vec<Value>,
    pub weights_renormalized: bool,
}

#[derive(Default)]
pub struct ReferenceMetadata {
    pools: HashMap<Address, Pool>,
}

impl ReferenceMetadata {
    /// Drop non-allowlisted exchanges and renormalize the remaining reference
    /// weights. This is a diagnostic plan, not the aggregator's reported quote.
    #[expect(
        clippy::too_many_arguments,
        reason = "requested identity and pinned pricing context match full replay"
    )]
    pub async fn replay_allowed(
        &mut self,
        quote: &ExternalQuote,
        input: Address,
        output: Address,
        amount: U256,
        config: &Config,
        rpc: &RpcClient,
        source: &(dyn QuoteSource + Send + Sync),
    ) -> Replay {
        let filtered = allowlisted(quote, config);
        match filtered {
            Ok(filtered) => {
                self.replay(&filtered, input, output, amount, config, rpc, source)
                    .await
            }
            Err(error) => Replay {
                status: "unsupported_or_failed".into(),
                amount_out: None,
                error: Some(error),
                excluded_tokens: vec![],
                branches: vec![],
                weights_renormalized: true,
            },
        }
    }
    async fn pool(
        &mut self,
        address: Address,
        venue: &str,
        rpc: &RpcClient,
    ) -> Result<Pool, String> {
        let kind = match venue.to_ascii_lowercase().as_str() {
            "uniswapv2" => PoolKind::UniswapV2,
            "shibaswap" => PoolKind::ShibaSwapV2,
            "uniswapv3" => PoolKind::UniswapV3 { fee: 0 },
            _ => return Err(format!("unsupported venue: {venue}")),
        };
        if let Some(pool) = self.pools.get(&address) {
            if std::mem::discriminant(&pool.kind) != std::mem::discriminant(&kind) {
                return Err("venue/pool identity mismatch".into());
            }
            return Ok(pool.clone());
        }
        async fn word(
            rpc: &RpcClient,
            address: Address,
            selector: &str,
        ) -> Result<Vec<u8>, String> {
            let value = rpc
                .eth_call(address, selector)
                .await
                .map_err(|e| e.to_string())?;
            if value.len() != 32 {
                return Err("invalid pool metadata word".into());
            }
            Ok(value)
        }
        let factory = Address::from_slice(&word(rpc, address, "c45a0155").await?[12..]);
        let expected: Address = match kind {
            PoolKind::UniswapV2 => "0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f",
            PoolKind::ShibaSwapV2 => "0x115934131916c8b277dd010ee02de363c09d037c",
            _ => "0x1f98431c8ad98523631ae4a59f267346ea31f984",
        }
        .parse()
        .unwrap();
        if factory != expected {
            return Err(format!("unsupported factory {factory} for {venue}"));
        }
        let a = Address::from_slice(&word(rpc, address, "0dfe1681").await?[12..]);
        let b = Address::from_slice(&word(rpc, address, "d21220a7").await?[12..]);
        let kind = if matches!(kind, PoolKind::UniswapV3 { .. }) {
            let fee = U256::from_be_slice(&word(rpc, address, "ddca3f43").await?)
                .to_string()
                .parse::<u32>()
                .map_err(|e| e.to_string())?;
            PoolKind::UniswapV3 { fee }
        } else {
            kind
        };
        let pool = Pool {
            kind,
            address,
            coins: vec![a, b],
        };
        self.pools.insert(address, pool.clone());
        Ok(pool)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "replay explicitly carries requested quote identity and pinned pricing context"
    )]
    pub async fn replay(
        &mut self,
        quote: &ExternalQuote,
        input: Address,
        output: Address,
        amount: U256,
        config: &Config,
        rpc: &RpcClient,
        source: &(dyn QuoteSource + Send + Sync),
    ) -> Replay {
        let mut excluded = BTreeSet::new();
        let mut branches = Vec::new();
        let mut used = HashSet::new();
        let result = self
            .replay_inner(
                quote,
                input,
                output,
                amount,
                config,
                rpc,
                source,
                &mut excluded,
                &mut branches,
                &mut used,
            )
            .await;
        match result {
            Ok(out) => Replay {
                status: "replayed".into(),
                amount_out: Some(out.to_string()),
                error: None,
                excluded_tokens: excluded.into_iter().collect(),
                branches,
                weights_renormalized: quote.normalized_for_diagnostic,
            },
            Err(error) => Replay {
                status: "unsupported_or_failed".into(),
                amount_out: None,
                error: Some(error),
                excluded_tokens: excluded.into_iter().collect(),
                branches,
                weights_renormalized: quote.normalized_for_diagnostic,
            },
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "bounded diagnostic context is kept explicit"
    )]
    async fn replay_inner(
        &mut self,
        quote: &ExternalQuote,
        input: Address,
        output: Address,
        amount: U256,
        config: &Config,
        rpc: &RpcClient,
        source: &(dyn QuoteSource + Send + Sync),
        excluded: &mut BTreeSet<Address>,
        branches: &mut Vec<Value>,
        used: &mut HashSet<Address>,
    ) -> Result<U256, String> {
        let routes = quote.best_route.as_array().ok_or("invalid bestRoute")?;
        if routes.is_empty() || routes.len() > 32 {
            return Err("empty or excessive external route count".into());
        }
        let amounts = routes
            .iter()
            .map(|route| {
                let swap = route["swaps"]
                    .as_array()
                    .and_then(|swaps| swaps.first())
                    .ok_or("missing external swaps")?;
                exchange_weights(swap)?
                    .into_iter()
                    .try_fold(U256::ZERO, |sum, (_, weight)| {
                        sum.checked_add(weight)
                            .ok_or_else(|| "reference weight overflow".to_string())
                    })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let input_sum = amounts
            .iter()
            .try_fold(U256::ZERO, |sum, input| sum.checked_add(*input))
            .ok_or("reference input overflow")?;
        if !quote.normalized_for_diagnostic && input_sum != amount {
            return Err("reported reference input weights do not conserve requested input".into());
        }
        let shares = distribute(amount, &amounts)?;
        let mut total = U256::ZERO;
        for (route, incoming) in routes.iter().zip(shares) {
            if incoming.is_zero() {
                continue;
            }
            let swaps = route["swaps"].as_array().ok_or("missing swaps")?;
            if swaps.is_empty() || swaps.len() > 8 {
                return Err("unsupported swap depth".into());
            }
            let mut current = incoming;
            let mut token = input;
            let mut trace = Vec::new();
            for swap in swaps {
                let from = address(&swap["srcToken"])?;
                let to = address(&swap["destToken"])?;
                if from != token {
                    return Err("external route token continuity mismatch".into());
                }
                let weights = exchange_weights(swap)?;
                let portions = distribute(
                    current,
                    &weights
                        .iter()
                        .map(|(_, amount)| *amount)
                        .collect::<Vec<_>>(),
                )?;
                let mut out = U256::ZERO;
                for ((exchange, _), portion) in weights.into_iter().zip(portions) {
                    if portion.is_zero() {
                        continue;
                    }
                    let venue = exchange["exchange"].as_str().ok_or("missing venue")?;
                    let mut value = portion;
                    if venue.eq_ignore_ascii_case("wsteth") {
                        let conversion = config
                            .protocol_conversions
                            .iter()
                            .find(|c| {
                                (c.underlying == from && c.wrapped == to)
                                    || (c.wrapped == from && c.underlying == to)
                            })
                            .ok_or("unconfigured conversion in reference")?;
                        let pool = conversion.pool();
                        let i = usize::from(from == conversion.wrapped);
                        value = source
                            .amount_out(&pool, i, 1 - i, value)
                            .await
                            .map_err(|e| e.to_string())?;
                        trace.push(json!({"venue":venue,"pool":pool.address,"amount_in":portion.to_string(),"amount_out":value.to_string()}));
                    } else {
                        let addresses = exchange["poolAddresses"]
                            .as_array()
                            .ok_or("missing reference pool addresses")?;
                        if addresses.is_empty() || addresses.len() > 8 {
                            return Err("invalid reference pool depth".into());
                        }
                        let path = exchange_path(exchange, from, to, addresses.len())?;
                        for token in &path {
                            if !token_allowed(config, *token) {
                                excluded.insert(*token);
                            }
                        }
                        for (idx, pool_address) in addresses.iter().enumerate() {
                            let address = address(pool_address)?;
                            if !used.insert(address) {
                                return Err("shared_pool_requires_stateful_simulation".into());
                            }
                            let pool = self.pool(address, venue, rpc).await?;
                            let i = pool
                                .coins
                                .iter()
                                .position(|token| *token == path[idx])
                                .ok_or("reference pool input mismatch")?;
                            let j = 1 - i;
                            if pool.coins[j] != path[idx + 1] {
                                return Err("reference pool output mismatch".into());
                            }
                            let before = value;
                            value = source
                                .amount_out(&pool, i, j, value)
                                .await
                                .map_err(|e| e.to_string())?;
                            trace.push(json!({"venue":venue,"pool":address,"token_in":path[idx],"token_out":path[idx+1],"amount_in":before.to_string(),"amount_out":value.to_string()}));
                        }
                    }
                    out = out.checked_add(value).ok_or("reference output overflow")?;
                }
                current = out;
                token = to;
            }
            if token != output {
                return Err("reference final token mismatch".into());
            }
            total = total
                .checked_add(current)
                .ok_or("reference total overflow")?;
            branches.push(json!({"amount_in":incoming.to_string(),"amount_out":current.to_string(),"path":trace}));
        }
        Ok(total)
    }
}

fn allowlisted(quote: &ExternalQuote, config: &Config) -> Result<ExternalQuote, String> {
    let mut routes = Vec::new();
    for route in quote.best_route.as_array().ok_or("invalid bestRoute")? {
        let mut route = route.clone();
        let mut viable = true;
        for swap in route["swaps"].as_array_mut().ok_or("missing swaps")? {
            let from = address(&swap["srcToken"])?;
            let to = address(&swap["destToken"])?;
            let mut keep = Vec::new();
            for exchange in swap["swapExchanges"]
                .as_array()
                .ok_or("missing exchanges")?
            {
                let path = if exchange["exchange"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case("wsteth"))
                {
                    vec![from, to]
                } else {
                    exchange_path(
                        exchange,
                        from,
                        to,
                        exchange["poolAddresses"]
                            .as_array()
                            .ok_or("missing pool addresses")?
                            .len(),
                    )?
                };
                if path.iter().all(|token| token_allowed(config, *token)) {
                    keep.push(exchange.clone());
                }
            }
            if keep.is_empty() {
                viable = false;
                break;
            }
            swap["swapExchanges"] = json!(keep);
        }
        if viable {
            routes.push(route);
        }
    }
    if routes.is_empty() {
        return Err("no fully allowlisted reference routes".into());
    }
    let mut filtered = quote.clone();
    filtered.best_route = json!(routes);
    filtered.normalized_for_diagnostic = true;
    Ok(filtered)
}

fn address(value: &Value) -> Result<Address, String> {
    value
        .as_str()
        .ok_or("missing address")?
        .parse::<Address>()
        .map_err(|e| e.to_string())
}

fn exchange_weights(swap: &Value) -> Result<Vec<(&Value, U256)>, String> {
    let exchanges = swap["swapExchanges"]
        .as_array()
        .ok_or("missing swapExchanges")?;
    if exchanges.is_empty() || exchanges.len() > 32 {
        return Err("empty or excessive exchange count".into());
    }
    exchanges
        .iter()
        .map(|exchange| {
            Ok((
                exchange,
                U256::from_str_radix(
                    exchange["srcAmount"]
                        .as_str()
                        .ok_or("missing branch srcAmount")?,
                    10,
                )
                .map_err(|e| e.to_string())?,
            ))
        })
        .collect()
}

fn distribute(amount: U256, weights: &[U256]) -> Result<Vec<U256>, String> {
    let total = weights
        .iter()
        .try_fold(U256::ZERO, |sum, n| sum.checked_add(*n))
        .ok_or("reference weight overflow")?;
    if total.is_zero() {
        return Err("zero reference input".into());
    }
    let mut remaining = amount;
    let last = weights
        .iter()
        .rposition(|weight| !weight.is_zero())
        .ok_or("zero reference input")?;
    weights
        .iter()
        .enumerate()
        .map(|(idx, weight)| {
            let share = if weight.is_zero() {
                U256::ZERO
            } else if idx == last {
                remaining
            } else {
                let q = (amount / total)
                    .checked_mul(*weight)
                    .ok_or("reference allocation overflow")?;
                q.checked_add(
                    (amount % total)
                        .checked_mul(*weight)
                        .ok_or("reference allocation overflow")?
                        / total,
                )
                .ok_or("reference allocation overflow")?
            };
            remaining = remaining
                .checked_sub(share)
                .ok_or("reference allocation overdraw")?;
            Ok(share)
        })
        .collect()
}

fn exchange_path(
    exchange: &Value,
    from: Address,
    to: Address,
    pools: usize,
) -> Result<Vec<Address>, String> {
    let data = &exchange["data"]["path"];
    if let Some(path) = data.as_array() {
        if path.iter().all(Value::is_string) {
            let result = path.iter().map(address).collect::<Result<Vec<_>, _>>()?;
            if result.len() == pools + 1
                && result.first() == Some(&from)
                && result.last() == Some(&to)
            {
                return Ok(result);
            }
        } else if path.len() == pools {
            let mut result = vec![from];
            for hop in path {
                if address(&hop["tokenIn"])? != *result.last().unwrap() {
                    return Err("reference internal path discontinuity".into());
                }
                result.push(address(&hop["tokenOut"])?);
            }
            if result.last() == Some(&to) {
                return Ok(result);
            }
        }
    }
    if pools == 1 {
        Ok(vec![from, to])
    } else {
        Err("unsupported reference path encoding".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_weighting_conserves_integer_input_and_rejects_overflow() {
        assert_eq!(
            distribute(U256::from(101), &[U256::from(1), U256::from(1)]).unwrap(),
            vec![U256::from(50), U256::from(51)]
        );
        assert!(distribute(U256::from(1), &[U256::MAX, U256::from(1)]).is_err());
        assert_eq!(
            distribute(U256::from(101), &[U256::from(1), U256::from(1), U256::ZERO]).unwrap(),
            vec![U256::from(50), U256::from(51), U256::ZERO]
        );
    }

    #[tokio::test]
    async fn reference_replay_rejects_shared_pool_totals_and_exposes_coverage() {
        use crate::source::FixedRate;
        let a = Address::with_last_byte(1);
        let b = Address::with_last_byte(2);
        let excluded = Address::with_last_byte(3);
        let p = Address::with_last_byte(10);
        let p2 = Address::with_last_byte(11);
        let p3 = Address::with_last_byte(12);
        let mut metadata = ReferenceMetadata {
            pools: HashMap::from([
                (
                    p,
                    Pool {
                        kind: PoolKind::UniswapV2,
                        address: p,
                        coins: vec![a, b],
                    },
                ),
                (
                    p2,
                    Pool {
                        kind: PoolKind::UniswapV2,
                        address: p2,
                        coins: vec![a, excluded],
                    },
                ),
                (
                    p3,
                    Pool {
                        kind: PoolKind::UniswapV2,
                        address: p3,
                        coins: vec![excluded, b],
                    },
                ),
            ]),
        };
        let config = Config {
            token_allowlist: vec![a.to_string(), b.to_string()],
            ..Default::default()
        };
        let rpc = RpcClient::new("http://127.0.0.1:1".into());
        let source = FixedRate::new(U256::from(2));
        let exchange = json!({"exchange":"uniswapv2","srcAmount":"50","poolAddresses":[p],"data":{"path":[a,b]}});
        let mut quote = ExternalQuote {
            amount_out: "202".into(),
            block_number: Some(1),
            latency_ms: 0,
            gas_cost_usd: None,
            normalized_for_diagnostic: false,
            best_route: json!([{"swaps":[{"srcToken":a,"destToken":b,"swapExchanges":[exchange.clone(),exchange]}]}]),
        };
        quote.best_route[0]["swaps"][0]["swapExchanges"][1]["srcAmount"] = json!("51");
        let shared = metadata
            .replay(&quote, a, b, U256::from(101), &config, &rpc, &source)
            .await;
        assert!(shared.amount_out.is_none());
        assert!(shared.error.unwrap().contains("shared_pool"));
        quote.best_route[0]["swaps"][0]["swapExchanges"][1] = json!({"exchange":"uniswapv2","srcAmount":"51","poolAddresses":[p2,p3],"data":{"path":[a,excluded,b]}});
        let full = metadata
            .replay(&quote, a, b, U256::from(101), &config, &rpc, &source)
            .await;
        assert_eq!(full.amount_out.as_deref(), Some("304"));
        assert_eq!(full.excluded_tokens, vec![excluded]);
        let allowed = metadata
            .replay_allowed(&quote, a, b, U256::from(101), &config, &rpc, &source)
            .await;
        assert_eq!(allowed.amount_out.as_deref(), Some("202"));
        assert!(allowed.weights_renormalized);
        assert!(!full.weights_renormalized);
        quote.best_route[0]["swaps"][0]["swapExchanges"][1]["srcAmount"] = json!("50");
        let malformed = metadata
            .replay(&quote, a, b, U256::from(101), &config, &rpc, &source)
            .await;
        assert!(malformed.amount_out.is_none());
        assert!(malformed.error.unwrap().contains("input weights"));
        quote.best_route[0]["swaps"][0]["swapExchanges"][1]["srcAmount"] = json!("51");
        quote.best_route[0]["swaps"][0]["swapExchanges"][0]["exchange"] = json!("unknown_venue");
        assert!(
            metadata
                .replay(&quote, a, b, U256::from(101), &config, &rpc, &source)
                .await
                .amount_out
                .is_none()
        );
    }
}
