use std::collections::HashMap;

use alloy_primitives::{Address, U256};
use async_trait::async_trait;

use crate::error::QuoteError;
use crate::model::Pool;
use crate::util::u256_to_f64;

#[async_trait]
pub trait QuoteSource {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError>;

    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError>;

    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError>;
}

pub(crate) fn constant_product_out(
    reserve_in: U256,
    reserve_out: U256,
    amount_in: U256,
) -> Result<U256, QuoteError> {
    if amount_in.is_zero() {
        return Err(QuoteError::ZeroAmount);
    }
    let fee_num = U256::from(997u64);
    let fee_den = U256::from(1000u64);
    let amount_with_fee = amount_in.checked_mul(fee_num).ok_or(QuoteError::Overflow)?;
    let num = amount_with_fee
        .checked_mul(reserve_out)
        .ok_or(QuoteError::Overflow)?;
    let den = reserve_in
        .checked_mul(fee_den)
        .and_then(|v| v.checked_add(amount_with_fee))
        .ok_or(QuoteError::Overflow)?;
    if den.is_zero() {
        return Err(QuoteError::InsufficientLiquidity);
    }
    Ok(num / den)
}

pub(crate) fn ratio(out: U256, input: U256) -> f64 {
    if input.is_zero() {
        return f64::INFINITY;
    }
    u256_to_f64(out) / u256_to_f64(input)
}

pub struct MockV2 {
    reserves: HashMap<Address, [U256; 2]>,
}

impl MockV2 {
    pub fn new(reserves: HashMap<Address, [U256; 2]>) -> Self {
        Self { reserves }
    }

    fn pair(&self, pool: &Pool) -> Result<[U256; 2], QuoteError> {
        self.reserves
            .get(&pool.address)
            .copied()
            .ok_or(QuoteError::UnknownPool)
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
        let r = self.pair(pool)?;
        let (ri, rj) = match (i, j) {
            (0, 1) => (r[0], r[1]),
            (1, 0) => (r[1], r[0]),
            _ => return Err(QuoteError::UnknownPool),
        };
        constant_product_out(ri, rj, amount_in)
    }

    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        let r = self.pair(pool)?;
        let (ri, rj) = match (i, j) {
            (0, 1) => (r[0], r[1]),
            (1, 0) => (r[1], r[0]),
            _ => return Err(QuoteError::UnknownPool),
        };
        Ok(ratio(rj, ri))
    }

    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        let r = self.pair(pool)?;
        match i {
            0 => Ok(r[0]),
            1 => Ok(r[1]),
            _ => Err(QuoteError::UnknownPool),
        }
    }
}

pub struct MockPool {
    reserves: HashMap<Address, Vec<U256>>,
}

impl MockPool {
    pub fn new(reserves: HashMap<Address, Vec<U256>>) -> Self {
        Self { reserves }
    }

    fn coins(&self, pool: &Pool) -> Result<&[U256], QuoteError> {
        self.reserves
            .get(&pool.address)
            .map(|v| v.as_slice())
            .ok_or(QuoteError::UnknownPool)
    }
}

#[async_trait]
impl QuoteSource for MockPool {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError> {
        let r = self.coins(pool)?;
        let (Some(&ri), Some(&rj)) = (r.get(i), r.get(j)) else {
            return Err(QuoteError::UnknownPool);
        };
        if i == j {
            return Err(QuoteError::UnknownPool);
        }
        constant_product_out(ri, rj, amount_in)
    }

    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        let r = self.coins(pool)?;
        let (Some(&ri), Some(&rj)) = (r.get(i), r.get(j)) else {
            return Err(QuoteError::UnknownPool);
        };
        Ok(ratio(rj, ri))
    }

    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        let r = self.coins(pool)?;
        r.get(i).copied().ok_or(QuoteError::UnknownPool)
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

    async fn mid_price(&self, _pool: &Pool, _i: usize, _j: usize) -> Result<f64, QuoteError> {
        Ok(u256_to_f64(self.rate))
    }

    async fn reserve_in(&self, _pool: &Pool, _i: usize) -> Result<U256, QuoteError> {
        Ok(U256::MAX)
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
    async fn v2_mid_price_is_reserve_ratio() {
        let p = pool(11);
        let mut reserves = HashMap::new();
        reserves.insert(p.address, [U256::from(1000u64), U256::from(3000u64)]);
        let src = MockV2::new(reserves);
        let price = src.mid_price(&p, 0, 1).await.unwrap();
        assert!((price - 3.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn fixed_rate_scales_input() {
        let src = FixedRate::new(U256::from(3u64));
        let out = src
            .amount_out(&pool(12), 0, 1, U256::from(5u64))
            .await
            .unwrap();
        assert_eq!(out, U256::from(15u64));
    }

    #[tokio::test]
    async fn zero_amount_is_an_error() {
        let src = FixedRate::new(U256::from(3u64));
        assert_eq!(
            src.amount_out(&pool(13), 0, 1, U256::ZERO).await,
            Err(QuoteError::ZeroAmount)
        );
    }

    #[tokio::test]
    async fn mock_pool_quotes_between_any_pair_of_coins() {
        let p = Pool {
            kind: crate::model::PoolKind::Curve,
            address: Address::with_last_byte(20),
            coins: vec![
                Address::with_last_byte(1),
                Address::with_last_byte(2),
                Address::with_last_byte(3),
            ],
        };
        let mut reserves = HashMap::new();
        reserves.insert(
            p.address,
            vec![
                U256::from(1000u64),
                U256::from(1000u64),
                U256::from(1000u64),
            ],
        );
        let src = MockPool::new(reserves);
        // coin 2 -> coin 0 must be quotable
        let out = src.amount_out(&p, 2, 0, U256::from(100u64)).await.unwrap();
        assert!(out > U256::ZERO);
    }
}
