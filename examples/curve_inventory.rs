//! Read-only Curve deployment inventory. No Graph credentials or server needed.
use alloy_primitives::{U256, keccak256};
use fluxion::{config::Config, discovery::discover_curve, rpc::RpcClient};
use serde_json::{Map, json};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut block = None;
    let mut output = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--block" => block = Some(args.next().ok_or("missing block")?.parse::<u64>()?),
            "--out" => output = Some(args.next().ok_or("missing output")?),
            _ => {
                return Err(
                    format!("unknown argument {flag}; use --block N --out PATH.json").into(),
                );
            }
        }
    }
    let config = Config::default();
    let rpc = RpcClient::new(config.rpc_url.clone());
    let header = rpc.block_header(block).await?;
    let rpc = rpc.at_block_hash(header.hash);
    let mut pools = discover_curve(&config, &rpc).await?;
    pools.sort_by_key(|p| p.address);
    let mut rows = Vec::new();
    for pool in pools {
        let mut values = Map::new();
        for (signature, index) in [
            ("A()", None),
            ("A_precise()", None),
            ("fee()", None),
            ("gamma()", None),
            ("price_scale(uint256)", Some(0)),
            ("mid_fee()", None),
            ("out_fee()", None),
            ("offpeg_fee_multiplier()", None),
        ] {
            let hash = keccak256(signature.as_bytes());
            let mut data = hash[..4]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            if let Some(index) = index {
                data += &format!("{index:064x}");
            }
            let value = rpc
                .eth_call(pool.address, &data)
                .await
                .ok()
                .filter(|bytes| bytes.len() == 32)
                .map(|bytes| U256::from_be_slice(&bytes).to_string());
            values.insert(signature.to_string(), json!(value));
        }
        rows.push(json!({"address":pool.address,"coins":pool.coins,"runtime_code_hash":rpc.code_hash(pool.address).await?,"interface_values":values}));
    }
    let unchanged = rpc.block_hash(header.number).await? == header.hash.to_string();
    let report = json!({"block":header,"hash_unchanged":unchanged,"registry_ids":config.curve_registry_ids,
        "token_filter":"default all-token allowlist; unchanged discovery rules","rpc_attempts":rpc.request_count(),"pools":rows});
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = output {
        std::fs::write(path, &text)?;
    }
    println!("{text}");
    if !unchanged {
        return Err("canonical block changed during inventory".into());
    }
    Ok(())
}
