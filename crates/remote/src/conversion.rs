//! Protocol conversions are quote-only graph links, not AMM liquidity pools.
use alloy_primitives::{Address, U256};
use async_trait::async_trait;

use crate::error::QuoteError;
use crate::model::{ConversionKind, Pool, PoolKind};
use crate::rpc::RpcClient;
use crate::source::QuoteSource;
use crate::util::{selector, u256_to_f64, u256_word};

pub struct ProtocolConversions {
    rpc: RpcClient,
}

impl ProtocolConversions {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }

    /// Check the deployment's underlying token before admitting configured edges.
    pub async fn validate_pool(&self, pool: &Pool) -> Result<(), QuoteError> {
        if pool.coins.len() != 2 || pool.coins[1] != pool.address || pool.coins[0] == pool.coins[1]
        {
            return Err(QuoteError::UnknownPool);
        }
        match pool.kind {
            PoolKind::Conversion {
                kind: ConversionKind::WstEth,
                ..
            } => {
                let data = self
                    .rpc
                    .eth_call(pool.address, &selector("stETH()"))
                    .await?;
                if data.len() != 32 || Address::from_slice(&data[12..32]) != pool.coins[0] {
                    return Err(QuoteError::RpcError(
                        "wrapper underlying does not match configuration".into(),
                    ));
                }
                let unit = U256::from(10u64).pow(U256::from(18));
                for (i, j) in [(0, 1), (1, 0)] {
                    if pool.kind.allows(i, j) {
                        self.amount_out(pool, i, j, unit).await?;
                    }
                }
                Ok(())
            }
            _ => Err(QuoteError::UnknownPool),
        }
    }
}

#[async_trait]
impl QuoteSource for ProtocolConversions {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError> {
        if amount_in.is_zero() {
            return Err(QuoteError::ZeroAmount);
        }
        if pool.coins.len() != 2 || !pool.kind.allows(i, j) {
            return Err(QuoteError::UnknownPool);
        }
        let signature = match (pool.kind, i, j) {
            (
                PoolKind::Conversion {
                    kind: ConversionKind::WstEth,
                    ..
                },
                0,
                1,
            ) => "getWstETHByStETH(uint256)",
            (
                PoolKind::Conversion {
                    kind: ConversionKind::WstEth,
                    ..
                },
                1,
                0,
            ) => "getStETHByWstETH(uint256)",
            _ => return Err(QuoteError::UnknownPool),
        };
        // Use the protocol's exact integer conversion for the actual amount,
        // not a floating-point multiplication by a cached exchange rate.
        let data = self
            .rpc
            .eth_call(
                pool.address,
                &format!("{}{}", selector(signature), u256_word(amount_in)),
            )
            .await?;
        if data.len() != 32 {
            return Err(QuoteError::RpcError(
                "short protocol conversion result".into(),
            ));
        }
        let out = U256::from_be_slice(&data);
        if out.is_zero() {
            return Err(QuoteError::InsufficientLiquidity);
        }
        Ok(out)
    }

    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        let unit = U256::from(10u64).pow(U256::from(18));
        Ok(u256_to_f64(self.amount_out(pool, i, j, unit).await?) / u256_to_f64(unit))
    }

    async fn reserve_in(&self, _pool: &Pool, _i: usize) -> Result<U256, QuoteError> {
        // No AMM reserve or reserve-ratio penalty. This is not a claim about
        // a user's balances/allowances or future protocols' deposit caps.
        Ok(U256::MAX)
    }
}
