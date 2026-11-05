//! Discover ShibaSwap's V2 pools through its factory, bounded to allowlisted
//! token pairs. No dependency on a possibly unavailable ShibaSwap subgraph.
use std::collections::HashMap;

use alloy_primitives::{Address, U256};

use super::{DiscoveryError, allowlist};
use crate::adapters::decode_reserves;
use crate::config::Config;
use crate::model::{Pool, PoolKind};
use crate::rpc::RpcClient;
use crate::state::{GET_RESERVES, multicall};
use crate::util::{pad_address, u256_to_f64};

const GET_PAIR: &str = "e6a43905";
const TOKEN0: &str = "0dfe1681";
const TOKEN1: &str = "d21220a7";
const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const USDC_WETH: &str = "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc";

fn address(data: &[u8]) -> Option<Address> {
    (data.len() == 32 && data[..12].iter().all(|b| *b == 0))
        .then(|| Address::from_slice(&data[12..]))
}

fn rpc_error(error: impl std::fmt::Display) -> DiscoveryError {
    DiscoveryError::Request(error.to_string())
}

fn allowlisted_pairs(config: &Config) -> Result<Vec<[Address; 2]>, DiscoveryError> {
    let mut tokens = allowlist(config)
        .iter()
        .map(|token| {
            token
                .parse::<Address>()
                .map_err(|_| DiscoveryError::Address(token.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    tokens.sort();
    tokens.dedup();
    let mut pairs = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        for other in &tokens[i + 1..] {
            pairs.push([*token, *other]);
        }
    }
    Ok(pairs)
}

/// Reserve-normalized USD estimates, anchored in major stables and the
/// canonical Uniswap V2 USDC/WETH pair. Non-anchor prices propagate through
/// discovered V2 reserve ratios. These are discovery filters, not quote oracles.
fn liquidity_values(pools: &[(Pool, [U256; 2])], weth_usd_raw: Option<f64>) -> Vec<Option<f64>> {
    let mut prices: HashMap<Address, f64> = HashMap::from([
        (
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                .parse()
                .unwrap(),
            1e-6,
        ),
        (
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
                .parse()
                .unwrap(),
            1e-6,
        ),
        (
            "0x6b175474e89094c44da98b954eedeac495271d0f"
                .parse()
                .unwrap(),
            1e-18,
        ),
    ]);
    if let Some(price) = weth_usd_raw {
        prices.insert(WETH.parse().unwrap(), price);
    }
    for _ in 0..pools.len() {
        let mut changed = false;
        for (pool, reserves) in pools {
            for (i, j) in [(0, 1), (1, 0)] {
                if !prices.contains_key(&pool.coins[j])
                    && let Some(price) = prices.get(&pool.coins[i]).copied()
                {
                    let value = price * u256_to_f64(reserves[i]) / u256_to_f64(reserves[j]);
                    if value.is_finite() && value > 0.0 {
                        prices.insert(pool.coins[j], value);
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    pools
        .iter()
        .map(|(pool, reserves)| {
            Some(
                u256_to_f64(reserves[0]) * prices.get(&pool.coins[0])?
                    + u256_to_f64(reserves[1]) * prices.get(&pool.coins[1])?,
            )
        })
        .collect()
}

fn select_pools(
    pools: Vec<Pool>,
    values: Vec<Option<f64>>,
    min_liquidity: u128,
    max_pools: usize,
) -> Vec<Pool> {
    let mut ranked = pools
        .into_iter()
        .zip(values)
        .filter(|(_, usd)| {
            min_liquidity == 0
                || usd.is_some_and(|value| value.is_finite() && value >= min_liquidity as f64)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|(a, av), (b, bv)| {
        bv.unwrap_or(0.0)
            .total_cmp(&av.unwrap_or(0.0))
            .then_with(|| a.address.cmp(&b.address))
    });
    ranked
        .into_iter()
        .take(max_pools)
        .map(|(pool, _)| pool)
        .collect()
}

pub async fn discover_shibaswap_v2(
    config: &Config,
    rpc: &RpcClient,
) -> Result<Vec<Pool>, DiscoveryError> {
    discover_factory_v2(
        config,
        rpc,
        config.shibaswap_v2_factory,
        PoolKind::ShibaSwapV2,
        "ShibaSwap",
    )
    .await
}

/// Discover a V2-compatible deployment through its `getPair` factory. This is
/// the config-only path for same-interface forks.
pub async fn discover_factory_v2(
    config: &Config,
    rpc: &RpcClient,
    factory: Address,
    kind: PoolKind,
    label: &str,
) -> Result<Vec<Pool>, DiscoveryError> {
    if config.max_pools == 0 {
        return Ok(Vec::new());
    }
    // Volume and age are not available from getPair/getReserves. Do not silently
    // promise filters this discovery source cannot evaluate.
    if config.min_volume_24h > 0 || config.min_pool_age_days > 0 {
        return Err(DiscoveryError::Request(format!(
            "{label} factory discovery cannot evaluate volume/age filters; disable this platform or use zero thresholds"
        )));
    }
    let pairs = allowlisted_pairs(config)?;
    let mut discovered = Vec::new();
    for chunk in pairs.chunks(200) {
        let calls = chunk
            .iter()
            .map(|coins| {
                (
                    factory,
                    format!(
                        "{GET_PAIR}{}{}",
                        pad_address(coins[0]),
                        pad_address(coins[1])
                    ),
                )
            })
            .collect::<Vec<_>>();
        for (coins, (ok, data)) in chunk
            .iter()
            .zip(multicall(rpc, &calls).await.map_err(rpc_error)?)
        {
            if ok && let Some(pool) = address(&data).filter(|a| *a != Address::ZERO) {
                discovered.push(Pool {
                    kind,
                    address: pool,
                    coins: coins.to_vec(),
                });
            }
        }
    }
    discovered.sort_by_key(|pool| pool.address);
    discovered.dedup_by_key(|pool| pool.address);
    let mut usable = Vec::new();
    for chunk in discovered.chunks(100) {
        let calls = chunk
            .iter()
            .flat_map(|pool| {
                [GET_RESERVES, TOKEN0, TOKEN1].map(|selector| (pool.address, selector.to_string()))
            })
            .collect::<Vec<_>>();
        let results = multicall(rpc, &calls).await.map_err(rpc_error)?;
        for (pool, result) in chunk.iter().zip(results.as_chunks::<3>().0) {
            if result.iter().any(|(ok, _)| !ok) {
                continue;
            }
            let Ok(reserves) = decode_reserves(&result[0].1) else {
                continue;
            };
            if reserves.iter().any(U256::is_zero)
                || address(&result[1].1) != Some(pool.coins[0])
                || address(&result[2].1) != Some(pool.coins[1])
            {
                continue;
            }
            usable.push((pool.clone(), reserves));
        }
    }
    let weth_price = if config.min_liquidity > 0 {
        let data = rpc
            .eth_call(USDC_WETH.parse().unwrap(), GET_RESERVES)
            .await
            .map_err(rpc_error)?;
        let reserves = decode_reserves(&data).map_err(rpc_error)?;
        let price = u256_to_f64(reserves[0]) / 1e6 / u256_to_f64(reserves[1]);
        if !price.is_finite() || price <= 0.0 {
            return Err(rpc_error("invalid WETH liquidity anchor"));
        }
        Some(price)
    } else {
        None
    };
    let values = liquidity_values(&usable, weth_price);
    Ok(select_pools(
        usable.into_iter().map(|(pool, _)| pool).collect(),
        values,
        config.min_liquidity,
        config.max_pools,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_queries_only_both_allowlisted_token_pairs() {
        let config = Config {
            token_allowlist: vec![
                format!("{:#x}", Address::with_last_byte(2)),
                format!("{:#x}", Address::with_last_byte(1)),
                format!("{:#x}", Address::with_last_byte(1)),
            ],
            ..Default::default()
        };
        assert_eq!(
            allowlisted_pairs(&config).unwrap(),
            vec![[Address::with_last_byte(1), Address::with_last_byte(2)]]
        );
    }

    #[test]
    fn reserve_liquidity_estimate_handles_raw_token_units() {
        let weth: Address = WETH.parse().unwrap();
        let shib = Address::with_last_byte(5);
        let pools = vec![(
            Pool {
                kind: PoolKind::ShibaSwapV2,
                address: Address::with_last_byte(10),
                coins: vec![weth, shib],
            },
            [
                U256::from(10u128.pow(18) * 100),
                U256::from(10u128.pow(18) * 1_000_000),
            ],
        )];
        assert!(
            (liquidity_values(&pools, Some(2000.0 / 1e18))[0].unwrap() - 400_000.0).abs() < 1e-6
        );
        assert_eq!(liquidity_values(&pools, None), vec![None]);
    }

    #[test]
    fn pool_limit_preserves_deepest_pool_and_liquidity_floor() {
        let make = |id| Pool {
            kind: PoolKind::ShibaSwapV2,
            address: Address::with_last_byte(id),
            coins: vec![],
        };
        let result = select_pools(
            vec![make(1), make(2), make(3)],
            vec![Some(1000.0), Some(100_000.0), None],
            50_000,
            1,
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].address, Address::with_last_byte(2));
    }
}
