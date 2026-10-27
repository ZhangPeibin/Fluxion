use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alloy_primitives::{Address, U256};
use async_trait::async_trait;

use crate::error::QuoteError;
use crate::model::{Pool, PoolKind};
use crate::rpc::RpcClient;
use crate::source::{QuoteSource, constant_product_out, ratio};
use crate::util::{pad_address, u32_word, u256_to_f64, u256_word};

const GET_RESERVES: &str = "0902f1ac";
const SLOT0: &str = "3850c7bd";
const QUOTE_EXACT_INPUT_SINGLE: &str = "c6a5026a";
const GET_DY_INT128: &str = "5e0d443f";
const GET_DY_U256: &str = "556d6e9f";

const QUOTER_V2: &str = "0x61fFE014bA17989E743c5F6cB21bF9697530B21e";

/// Decode the raw ABI return of `getReserves()` (uint112 reserve0, uint112
/// reserve1, uint32 blockTimestampLast) into the two reserves. The return is
/// three 32-byte words; the first two hold the reserves right-aligned.
pub fn decode_reserves(data: &[u8]) -> Result<[U256; 2], QuoteError> {
    if data.len() < 64 {
        return Err(QuoteError::RpcError("short getReserves result".to_string()));
    }
    Ok([
        U256::from_be_slice(&data[0..32]),
        U256::from_be_slice(&data[32..64]),
    ])
}

#[async_trait]
pub trait V2Reserves: Send + Sync {
    async fn get_reserves(&self, pool: Address) -> Result<[U256; 2], QuoteError>;
}

/// Reads V2 reserves over raw JSON-RPC (`eth_call getReserves`).
pub struct RpcV2Reserves {
    rpc: RpcClient,
}

impl RpcV2Reserves {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }
}

#[async_trait]
impl V2Reserves for RpcV2Reserves {
    async fn get_reserves(&self, pool: Address) -> Result<[U256; 2], QuoteError> {
        let data = self.rpc.eth_call(pool, GET_RESERVES).await?;
        decode_reserves(&data)
    }
}

pub struct UniswapV2 {
    reserves: Arc<dyn V2Reserves>,
}

impl UniswapV2 {
    pub fn new(reserves: Arc<dyn V2Reserves>) -> Self {
        Self { reserves }
    }
}

#[async_trait]
impl QuoteSource for UniswapV2 {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError> {
        let r = self.reserves.get_reserves(pool.address).await?;
        let (ri, rj) = match (i, j) {
            (0, 1) => (r[0], r[1]),
            (1, 0) => (r[1], r[0]),
            _ => return Err(QuoteError::UnknownPool),
        };
        constant_product_out(ri, rj, amount_in)
    }

    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        let r = self.reserves.get_reserves(pool.address).await?;
        let (ri, rj) = match (i, j) {
            (0, 1) => (r[0], r[1]),
            (1, 0) => (r[1], r[0]),
            _ => return Err(QuoteError::UnknownPool),
        };
        Ok(ratio(rj, ri))
    }

    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        let r = self.reserves.get_reserves(pool.address).await?;
        match i {
            0 => Ok(r[0]),
            1 => Ok(r[1]),
            _ => Err(QuoteError::UnknownPool),
        }
    }
}

/// Uniswap V3 quotes via `QuoterV2.quoteExactInputSingle`.
pub struct UniswapV3 {
    rpc: RpcClient,
}

impl UniswapV3 {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }

    fn fee(pool: &Pool) -> Result<u32, QuoteError> {
        match pool.kind {
            PoolKind::UniswapV3 { fee } => Ok(fee),
            _ => Err(QuoteError::UnknownPool),
        }
    }

    async fn quote_exact_input_single(
        &self,
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        fee: u32,
    ) -> Result<U256, QuoteError> {
        let data = format!(
            "{QUOTE_EXACT_INPUT_SINGLE}{}{}{}{}{}",
            pad_address(token_in),
            pad_address(token_out),
            u256_word(amount_in),
            u32_word(fee),
            "0".repeat(64), // sqrtPriceLimitX96 = 0 -> contract uses MIN/MAX
        );
        let out = self.rpc.eth_call(quoter_v2(), &data).await?;
        if out.len() < 32 {
            return Err(QuoteError::RpcError("short quote result".to_string()));
        }
        Ok(U256::from_be_slice(&out[0..32]))
    }
}

#[async_trait]
impl QuoteSource for UniswapV3 {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError> {
        let fee = Self::fee(pool)?;
        let (token_in, token_out) = match (i, j) {
            (0, 1) => (pool.coins[0], pool.coins[1]),
            (1, 0) => (pool.coins[1], pool.coins[0]),
            _ => return Err(QuoteError::UnknownPool),
        };
        self.quote_exact_input_single(token_in, token_out, amount_in, fee)
            .await
    }

    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        let out = self.rpc.eth_call(pool.address, SLOT0).await?;
        if out.len() < 32 {
            return Err(QuoteError::RpcError("short slot0 result".to_string()));
        }
        let sqrt = u256_to_f64(U256::from_be_slice(&out[0..32]));
        let q = 79228162514264337593543950336.0_f64; // 2^96
        let price = (sqrt / q) * (sqrt / q); // token1 per token0
        Ok(match (i, j) {
            (0, 1) => price,
            (1, 0) => 1.0 / price,
            _ => return Err(QuoteError::UnknownPool),
        })
    }

    async fn reserve_in(&self, _pool: &Pool, _i: usize) -> Result<U256, QuoteError> {
        Ok(U256::MAX)
    }
}

/// Curve classic stableswap quotes via `get_dy(int128,int128,uint256)`.
pub struct Curve {
    rpc: RpcClient,
}

impl Curve {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }

    async fn get_dy(&self, pool: &Pool, i: usize, j: usize, dx: U256) -> Result<U256, QuoteError> {
        // Classic stableswap uses int128 indices; crypto pools use uint256.
        let mut last = QuoteError::RpcError("get_dy failed".to_string());
        for selector in [GET_DY_INT128, GET_DY_U256] {
            let data = format!(
                "{selector}{}{}{}",
                u256_word(U256::from(i)),
                u256_word(U256::from(j)),
                u256_word(dx)
            );
            match self.rpc.eth_call(pool.address, &data).await {
                Ok(out) if out.len() >= 32 => return Ok(U256::from_be_slice(&out[0..32])),
                Ok(_) => last = QuoteError::RpcError("short get_dy result".to_string()),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}

#[async_trait]
impl QuoteSource for Curve {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError> {
        if i == j || i >= pool.coins.len() || j >= pool.coins.len() {
            return Err(QuoteError::UnknownPool);
        }
        self.get_dy(pool, i, j, amount_in).await
    }

    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        // Approximate spot price with a tiny amount.
        let dx = U256::from(1_000_000u64);
        let dy = self.get_dy(pool, i, j, dx).await?;
        Ok(u256_to_f64(dy) / u256_to_f64(dx))
    }

    async fn reserve_in(&self, _pool: &Pool, _i: usize) -> Result<U256, QuoteError> {
        Ok(U256::MAX)
    }
}

/// Dispatches quoting to the right adapter based on the pool kind.
pub struct Dispatcher {
    v2: Box<dyn QuoteSource + Send + Sync>,
    v3: Option<Box<dyn QuoteSource + Send + Sync>>,
    curve: Option<Box<dyn QuoteSource + Send + Sync>>,
}

impl Dispatcher {
    pub fn new(
        v2: Box<dyn QuoteSource + Send + Sync>,
        v3: Option<Box<dyn QuoteSource + Send + Sync>>,
        curve: Option<Box<dyn QuoteSource + Send + Sync>>,
    ) -> Self {
        Self { v2, v3, curve }
    }

    fn source(&self, kind: PoolKind) -> Result<&(dyn QuoteSource + Send + Sync), QuoteError> {
        match kind {
            PoolKind::UniswapV2 => Ok(self.v2.as_ref()),
            PoolKind::UniswapV3 { .. } => self.v3.as_deref().ok_or(QuoteError::UnknownPool),
            PoolKind::Curve => self.curve.as_deref().ok_or(QuoteError::UnknownPool),
        }
    }
}

#[async_trait]
impl QuoteSource for Dispatcher {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError> {
        self.source(pool.kind)?
            .amount_out(pool, i, j, amount_in)
            .await
    }

    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        self.source(pool.kind)?.mid_price(pool, i, j).await
    }

    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        self.source(pool.kind)?.reserve_in(pool, i).await
    }
}

fn quoter_v2() -> Address {
    QUOTER_V2.parse().unwrap()
}

/// Caches coarse-stage mid-prices (per pool direction) with a TTL so the
/// router's path search does not hit RPC for every edge on every request.
type PriceCache = Mutex<HashMap<(Address, usize, usize), (f64, Instant)>>;
type ReserveCache = Mutex<HashMap<(Address, usize), (U256, Instant)>>;

pub struct CachingSource {
    inner: Box<dyn QuoteSource + Send + Sync>,
    ttl: Duration,
    prices: PriceCache,
    reserves: ReserveCache,
}

impl CachingSource {
    pub fn new(inner: Box<dyn QuoteSource + Send + Sync>, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            prices: Mutex::new(HashMap::new()),
            reserves: Mutex::new(HashMap::new()),
        }
    }

    /// Pre-populates the mid-price cache for a pool direction.
    pub async fn warm(&self, pool: &Pool, i: usize, j: usize) {
        let _ = self.mid_price(pool, i, j).await;
    }

    /// Number of mid-prices currently cached (for diagnostics).
    pub fn cached_count(&self) -> usize {
        self.prices.lock().unwrap().len()
    }
}

#[async_trait]
impl QuoteSource for CachingSource {
    async fn amount_out(
        &self,
        pool: &Pool,
        i: usize,
        j: usize,
        amount_in: U256,
    ) -> Result<U256, QuoteError> {
        self.inner.amount_out(pool, i, j, amount_in).await
    }

    async fn mid_price(&self, pool: &Pool, i: usize, j: usize) -> Result<f64, QuoteError> {
        let key = (pool.address, i, j);
        {
            let prices = self.prices.lock().unwrap();
            if let Some((price, at)) = prices.get(&key)
                && at.elapsed() < self.ttl
            {
                return Ok(*price);
            }
        }
        let price = self.inner.mid_price(pool, i, j).await?;
        self.prices
            .lock()
            .unwrap()
            .insert(key, (price, Instant::now()));
        Ok(price)
    }

    async fn reserve_in(&self, pool: &Pool, i: usize) -> Result<U256, QuoteError> {
        let key = (pool.address, i);
        {
            let reserves = self.reserves.lock().unwrap();
            if let Some((value, at)) = reserves.get(&key)
                && at.elapsed() < self.ttl
            {
                return Ok(*value);
            }
        }
        let value = self.inner.reserve_in(pool, i).await?;
        self.reserves
            .lock()
            .unwrap()
            .insert(key, (value, Instant::now()));
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::hex_decode;

    fn fixture_pool() -> Pool {
        Pool {
            kind: crate::model::PoolKind::UniswapV2,
            address: Address::with_last_byte(1),
            coins: vec![Address::with_last_byte(2), Address::with_last_byte(3)],
        }
    }

    struct FixtureReserves {
        reserves: [U256; 2],
    }

    #[async_trait]
    impl V2Reserves for FixtureReserves {
        async fn get_reserves(&self, _pool: Address) -> Result<[U256; 2], QuoteError> {
            Ok(self.reserves)
        }
    }

    // Captured from the WETH/USDC Uniswap V2 pair (0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc)
    // at a fixed block; raw `getReserves()` return.
    const RAW_GET_RESERVES: &str = "0x000000000000000000000000000000000000000000000000000008d3d2e93f5b\
        0000000000000000000000000000000000000000000000e48f03d863b0c3ee36\
        000000000000000000000000000000000000000000000000000000006ae07af3";

    #[test]
    fn decodes_raw_get_reserves() {
        let bytes = hex_decode(RAW_GET_RESERVES).unwrap();
        let r = decode_reserves(&bytes).unwrap();
        let expected0 = U256::from_str_radix("08d3d2e93f5b", 16).unwrap();
        let expected1 = U256::from_str_radix("e48f03d863b0c3ee36", 16).unwrap();
        assert_eq!(r, [expected0, expected1]);
    }

    #[tokio::test]
    async fn adapter_quotes_from_pinned_reserves_without_network() {
        let r0 = U256::from_str_radix("08d3d2e93f5b", 16).unwrap();
        let r1 = U256::from_str_radix("e48f03d863b0c3ee36", 16).unwrap();
        let adapter = UniswapV2::new(Arc::new(FixtureReserves { reserves: [r0, r1] }));

        let amount_in = U256::from(1_000_000_000_000_000_000u128); // 1 WETH (18 decimals)
        let out = adapter
            .amount_out(&fixture_pool(), 1, 0, amount_in)
            .await
            .unwrap();
        let expected = constant_product_out(r1, r0, amount_in).unwrap();
        assert_eq!(out, expected);
        assert!(out > U256::ZERO);
    }
}
