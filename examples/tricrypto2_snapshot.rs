//! Capture deployed Tricrypto2 view inputs and independent raw get_dy vectors.
use alloy_primitives::{Address, U256, keccak256};
use fluxion::{rpc::RpcClient, state::multicall};
use serde_json::json;
use std::collections::BTreeSet;

fn data(name: &str, args: &[U256]) -> String {
    let hash = keccak256(name.as_bytes());
    let mut result = hash[..4]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    for a in args {
        result += &format!("{a:064x}");
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
    let pool: Address = "0xd51a44d3fae010294c616388b506acda1bfaae46".parse()?;
    let views: Address = "0x40745803c2faa8e8402e2ae935933d07ca8f355c".parse()?;
    let math: Address = "0x8f68f4810cce3194b6cb6f3d50fa58c2c9bdd1d5".parse()?;
    let coins = [
        "0xdac17f958d2ee523a2206206994597c13d831ec7",
        "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
    ]
    .map(|s| s.parse::<Address>().unwrap());
    let mut calls = Vec::new();
    for name in ["coins(uint256)", "balances(uint256)"] {
        for i in 0..3 {
            calls.push((pool, data(name, &[U256::from(i)])));
        }
    }
    for name in ["A()", "gamma()", "D()"] {
        calls.push((pool, data(name, &[])));
    }
    for i in 0..2 {
        calls.push((pool, data("price_scale(uint256)", &[U256::from(i)])));
    }
    for name in ["mid_fee()", "out_fee()", "fee_gamma()"] {
        calls.push((pool, data(name, &[])));
    }
    let words = multicall(&rpc, &calls)
        .await?
        .into_iter()
        .map(|(ok, bytes)| {
            if !ok || bytes.len() != 32 {
                Err("invalid state response")
            } else {
                Ok(U256::from_be_slice(&bytes))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (coin, actual) in coins.iter().zip(&words[..3]) {
        if U256::from_be_slice(coin.as_slice()) != *actual {
            return Err("unexpected coin order".into());
        }
    }
    let small = [
        U256::from(1_000_000_000u64),
        U256::from(1_670_000),
        U256::from(333_333_333_333_333_333u128),
    ];
    let units = [
        U256::from(1_000_000),
        U256::from(100_000_000),
        U256::from(10u128.pow(18)),
    ];
    let mut requests = Vec::new();
    let mut calls = Vec::new();
    for i in 0..3 {
        let inputs = BTreeSet::from([
            U256::from(1),
            U256::from(1_000_000),
            units[i],
            units[i] + U256::from(1),
            small[i],
            small[i] + U256::from(1),
            small[i] * U256::from(10),
            small[i] * U256::from(100),
            small[i] * U256::from(1_000_000),
        ]);
        for j in 0..3 {
            if i != j {
                for amount in &inputs {
                    requests.push((i, j, *amount));
                    calls.push((
                        pool,
                        data(
                            "get_dy(uint256,uint256,uint256)",
                            &[U256::from(i), U256::from(j), *amount],
                        ),
                    ));
                }
            }
        }
    }
    let mut quotes = Vec::new();
    for ((i, j, amount), (ok, bytes)) in requests.into_iter().zip(multicall(&rpc, &calls).await?) {
        if ok && bytes.len() != 32 {
            return Err("invalid get_dy response".into());
        }
        quotes.push(json!({"i":i,"j":j,"amount_in":amount.to_string(),"success":ok,"amount_out":ok.then(||U256::from_be_slice(&bytes).to_string())}));
    }
    let fingerprints = json!({"pool":rpc.code_hash(pool).await?,"views":rpc.code_hash(views).await?,"math":rpc.code_hash(math).await?});
    if rpc.block_hash(header.number).await? != header.hash.to_string() {
        return Err("block hash changed".into());
    }
    let report = json!({"schema_version":1,"profile":"tricrypto2","block":header,"hash_unchanged":true,"pool":pool,"coins":coins,"views":views,"math":math,"runtime_code_hashes":fingerprints,
        "balances":words[3..6].iter().map(ToString::to_string).collect::<Vec<_>>(),"parameters":{"amplification":words[6].to_string(),"gamma":words[7].to_string(),"stored_d":words[8].to_string(),"price_scale":[words[9].to_string(),words[10].to_string()],"mid_fee":words[11].to_string(),"out_fee":words[12].to_string(),"fee_gamma":words[13].to_string()},"quotes":quotes});
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = output {
        std::fs::write(path, &text)?;
    }
    println!("{text}");
    Ok(())
}
