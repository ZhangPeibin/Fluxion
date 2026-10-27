use alloy_primitives::{Address, U256};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Token {
    pub address: Address,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolKind {
    UniswapV2,
    UniswapV3 { fee: u32 },
    Curve,
}

impl PoolKind {
    pub fn name(&self) -> &'static str {
        match self {
            PoolKind::UniswapV2 => "uniswap-v2",
            PoolKind::UniswapV3 { .. } => "uniswap-v3",
            PoolKind::Curve => "curve",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pool {
    pub kind: PoolKind,
    pub address: Address,
    pub coins: Vec<Address>,
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub pool_idx: usize,
    pub i: usize,
    pub j: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hop {
    pub pool: Pool,
    pub i: usize,
    pub j: usize,
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub amount_out: U256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub hops: Vec<Hop>,
    pub amount_out: U256,
    pub price_impact_bps: i64,
}

#[derive(Debug, Clone)]
pub struct QuoteRequest {
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub max_hops: Option<usize>,
    pub amount_out_min: Option<U256>,
}
