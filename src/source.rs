use std::collections::HashMap;

use alloy_primitives::{Address, U256};
use async_trait::async_trait;

use crate::error::QuoteError;
use crate::model::Pool;

#[async_trait]
pub trait QuoteSource {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError>;
}

pub struct MockV2 {
    reserves: HashMap<Address, [U256; 2]>,
}

impl MockV2 {
    pub fn new(reserves: HashMap<Address, [U256; 2]>) -> Self {
        Self { reserves }
    }
}

#[async_trait]
impl QuoteSource for MockV2 {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError> {
        let r = self
            .reserves
            .get(&pool.address)
            .ok_or(QuoteError::UnknownPool)?;
        let (reserve_in, reserve_out) = match (i, j) {
            (0, 1) => (r[0], r[1]),
            (1, 0) => (r[1], r[0]),
            _ => return Err(QuoteError::UnknownPool),
        };
        if amount_in.is_zero() {
            return Err(QuoteError::ZeroAmount);
        }
        let fee_num = U256::from(997u64);
        let fee_den = U256::from(1000u64);
        let amount_with_fee = amount_in.checked_mul(fee_num).ok_or(QuoteError::Overflow)?;
        let num = amount_with_fee
            .checked_mul(reserve_out)
            .ok_or(QuoteError::Overflow)?;
        let reserve_scaled = reserve_in
            .checked_mul(fee_den)
            .ok_or(QuoteError::Overflow)?;
        let den = reserve_scaled
            .checked_add(amount_with_fee)
            .ok_or(QuoteError::Overflow)?;
        if den.is_zero() {
            return Err(QuoteError::InsufficientLiquidity);
        }
        Ok(num / den)
    }
}

pub struct FixedRate {
    rate: U256,
}

impl FixedRate {
    pub fn new(rate: U256) -> Self {
        Self { rate }
    }
}

#[async_trait]
impl QuoteSource for FixedRate {
    async fn amount_out(
        &self,
        _pool: &Pool,
        _i: usize,
        _j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError> {
        if amount_in.is_zero() {
            return Err(QuoteError::ZeroAmount);
        }
        amount_in.checked_mul(self.rate).ok_or(QuoteError::Overflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(addr: u8) -> Pool {
        Pool {
            kind: crate::model::PoolKind::UniswapV2,
            address: Address::with_last_byte(addr),
            coins: vec![Address::with_last_byte(1), Address::with_last_byte(2)],
        }
    }

    #[tokio::test]
    async fn v2_constant_product_matches_formula() {
        let p = pool(10);
        let mut reserves = HashMap::new();
        reserves.insert(p.address, [U256::from(1000u64), U256::from(3_000_000u64)]);
        let src = MockV2::new(reserves);
        let out = src.amount_out(&p, 0, 1, U256::from(1u64)).await.unwrap();
        let expected = U256::from(997u64) * U256::from(3_000_000u64)
            / (U256::from(1_000_000u64) + U256::from(997u64));
        assert_eq!(out, expected);
    }

    #[tokio::test]
    async fn fixed_rate_scales_input() {
        let src = FixedRate::new(U256::from(3u64));
        let out = src
            .amount_out(&pool(11), 0, 1, U256::from(5u64))
            .await
            .unwrap();
        assert_eq!(out, U256::from(15u64));
    }

    #[tokio::test]
    async fn zero_amount_is_an_error() {
        let src = FixedRate::new(U256::from(3u64));
        assert_eq!(
            src.amount_out(&pool(12), 0, 1, U256::ZERO).await,
            Err(QuoteError::ZeroAmount)
        );
    }
}
