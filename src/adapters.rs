use std::sync::Arc;

use alloy_primitives::{Address, U256};
use async_trait::async_trait;
use serde_json::json;

use crate::error::QuoteError;
use crate::model::Pool;
use crate::source::{QuoteSource, constant_product_out, ratio};
use crate::util::hex_decode;

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

/// Reads reserves over raw JSON-RPC (`eth_call getReserves`).
pub struct RpcV2Reserves {
    client: reqwest::Client,
    rpc_url: String,
}

impl RpcV2Reserves {
    pub fn new(rpc_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            rpc_url,
        }
    }
}

#[async_trait]
impl V2Reserves for RpcV2Reserves {
    async fn get_reserves(&self, pool: Address) -> Result<[U256; 2], QuoteError> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{ "to": format!("{pool:#x}"), "data": "0x0902f1ac" }, "latest"],
            "id": 1
        });
        let resp: serde_json::Value = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| QuoteError::RpcError(e.to_string()))?
            .json()
            .await
            .map_err(|e| QuoteError::RpcError(e.to_string()))?;
        if let Some(err) = resp.get("error") {
            return Err(QuoteError::RpcError(err.to_string()));
        }
        let result = resp
            .get("result")
            .and_then(|r| r.as_str())
            .ok_or_else(|| QuoteError::RpcError("missing result".to_string()))?;
        let data = hex_decode(result).map_err(QuoteError::RpcError)?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
