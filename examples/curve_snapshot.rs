//! Capture deployed classic 3pool state/get_dy vectors for offline replay.
use alloy_primitives::{Address, U256, keccak256};
use fluxion::{
    curve_local::{COINS, PROFILE, THREE_POOL},
    rpc::RpcClient,
    state::multicall,
};
use serde_json::json;
use std::collections::BTreeSet;

fn data(signature: &str, args: &[U256]) -> String {
    let digest = keccak256(signature.as_bytes());
    let mut result = digest[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    for argument in args {
        result += &format!("{argument:064x}");
    }
    result
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut block = None;
    let mut output = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--block" => block = Some(args.next().ok_or("missing block")?.parse::<u64>()?),
            "--out" => output = Some(args.next().ok_or("missing output")?),
            _ => return Err("use --block N --out PATH.json".into()),
        }
    }
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let header = rpc.block_header(block).await?;
    let rpc = rpc.at_block_hash(header.hash);
    let pool: Address = THREE_POOL.parse()?;
    let mut calls = Vec::new();
    for signature in ["coins(uint256)", "balances(uint256)"] {
        for i in 0..3 {
            calls.push((pool, data(signature, &[U256::from(i)])));
        }
    }
    calls.push((pool, data("A()", &[])));
    calls.push((pool, data("fee()", &[])));
    let words = multicall(&rpc, &calls)
        .await?
        .into_iter()
        .map(|(ok, bytes)| {
            if !ok || bytes.len() != 32 {
                Err("bad state result")
            } else {
                Ok(U256::from_be_slice(&bytes))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let coins = COINS.map(|address| address.parse::<Address>().unwrap());
    for i in 0..3 {
        if words[i] != U256::from_be_slice(coins[i].as_slice()) {
            return Err("unexpected coins".into());
        }
    }
    let mut requests = Vec::new();
    let mut calls = Vec::new();
    for i in 0..3 {
        let unit = if i == 0 {
            U256::from(10u128.pow(18))
        } else {
            U256::from(1_000_000)
        };
        let inputs = BTreeSet::from([
            U256::from(1_000_000),
            unit,
            unit + U256::from(1),
            unit * U256::from(1000),
            unit * U256::from(10000),
            unit * U256::from(100000),
        ]);
        for j in 0..3 {
            if i == j {
                continue;
            }
            for amount in &inputs {
                requests.push((i, j, *amount));
                calls.push((
                    pool,
                    data(
                        "get_dy(int128,int128,uint256)",
                        &[U256::from(i), U256::from(j), *amount],
                    ),
                ));
            }
        }
    }
    let mut quotes = Vec::new();
    for ((i, j, amount), (ok, bytes)) in requests.into_iter().zip(multicall(&rpc, &calls).await?) {
        if !ok || bytes.len() != 32 {
            return Err("bad get_dy result".into());
        }
        quotes.push(json!({"i":i,"j":j,"amount_in":amount.to_string(),"amount_out":U256::from_be_slice(&bytes).to_string()}));
    }
    if rpc.block_hash(header.number).await? != header.hash.to_string() {
        return Err("block hash changed".into());
    }
    let report = json!({"schema_version":1,"profile":PROFILE,"block":header,"hash_unchanged":true,"pool":pool,"coins":coins,
        "balances":words[3..6].iter().map(ToString::to_string).collect::<Vec<_>>(),"amplification":words[6].to_string(),"fee":words[7].to_string(),"quotes":quotes});
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = output {
        std::fs::write(path, &text)?;
    }
    println!("{text}");
    Ok(())
}
