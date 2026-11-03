//! Capture verified Curve deployment state/get_dy vectors, including reverts.
use alloy_primitives::{U256, keccak256};
use fluxion::{
    curve_local::CurveProfile, curve_plain::PlainProfile, rpc::RpcClient, state::multicall,
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
    let mut profile = CurveProfile::Classic3;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--block" => block = Some(args.next().ok_or("missing block")?.parse::<u64>()?),
            "--out" => output = Some(args.next().ok_or("missing output")?),
            "--pool" => {
                profile = match args.next().as_deref() {
                    Some("classic-3pool") => CurveProfile::Classic3,
                    Some("2crv") => CurveProfile::Plain(PlainProfile::TwoCrv),
                    Some("frax-usdc") => CurveProfile::Plain(PlainProfile::FraxUsdc),
                    _ => return Err("pool must be classic-3pool, 2crv or frax-usdc".into()),
                }
            }
            _ => return Err("use --pool NAME --block N --out PATH.json".into()),
        }
    }
    let rpc = RpcClient::new("https://eth.drpc.org".into());
    let header = rpc.block_header(block).await?;
    let rpc = rpc.at_block_hash(header.hash);
    let deployment = profile.pool();
    let pool = deployment.address;
    let coins = deployment.coins;
    let count = coins.len();
    let mut calls = Vec::new();
    for signature in ["coins(uint256)", "balances(uint256)"] {
        for i in 0..count {
            calls.push((pool, data(signature, &[U256::from(i)])));
        }
    }
    calls.push((pool, data(profile.a_getter(), &[])));
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
    for (actual, coin) in words.iter().take(count).zip(&coins) {
        if *actual != U256::from_be_slice(coin.as_slice()) {
            return Err("unexpected coins".into());
        }
    }
    let mut requests = Vec::new();
    let mut calls = Vec::new();
    for (i, unit) in profile.units().into_iter().enumerate() {
        let mut inputs = BTreeSet::from([
            U256::from(1_000_000),
            unit,
            unit + U256::from(1),
            unit * U256::from(1000),
            unit * U256::from(10000),
            unit * U256::from(100000),
        ]);
        if matches!(profile, CurveProfile::Plain(_)) {
            inputs.insert(U256::from(1));
            inputs.insert(U256::from(2));
        }
        for j in 0..count {
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
        if ok && bytes.len() != 32 {
            return Err("bad get_dy result".into());
        }
        quotes.push(
            json!({"i":i,"j":j,"amount_in":amount.to_string(),"success":ok,
            "amount_out":ok.then(||U256::from_be_slice(&bytes).to_string())}),
        );
    }
    if rpc.block_hash(header.number).await? != header.hash.to_string() {
        return Err("block hash changed".into());
    }
    let report = json!({"schema_version":2,"profile":profile.name(),"a_precision":profile.a_precision(),"balance_source":profile.balance_source(),"block":header,"hash_unchanged":true,"pool":pool,"coins":coins,
        "balances":words[count..count*2].iter().map(ToString::to_string).collect::<Vec<_>>(),"amplification":words[count*2].to_string(),"fee":words[count*2+1].to_string(),"quotes":quotes});
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = output {
        std::fs::write(path, &text)?;
    }
    println!("{text}");
    Ok(())
}
