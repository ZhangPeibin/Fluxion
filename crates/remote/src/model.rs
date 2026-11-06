use std::collections::HashMap;

use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};

/// Supported protocol interfaces, independent of any configured deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum ConversionKind {
    #[serde(rename = "wsteth")]
    WstEth,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Token {
    pub address: Address,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, Default)]
pub enum V3Venue {
    #[default]
    #[serde(rename = "uniswap-v3")]
    Uniswap,
    #[serde(rename = "sushi-v3")]
    SushiSwap,
    #[serde(rename = "pancake-v3")]
    PancakeSwap,
}

impl V3Venue {
    pub fn name(self) -> &'static str {
        match self {
            Self::Uniswap => "uniswap-v3",
            Self::SushiSwap => "sushi-v3",
            Self::PancakeSwap => "pancake-v3",
        }
    }

    pub fn pool_kind(self, fee: u32) -> PoolKind {
        match self {
            Self::Uniswap => PoolKind::UniswapV3 { fee },
            Self::SushiSwap => PoolKind::SushiSwapV3 { fee },
            Self::PancakeSwap => PoolKind::PancakeSwapV3 { fee },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PoolKind {
    UniswapV2,
    ShibaSwapV2,
    UniswapV3 {
        fee: u32,
    },
    SushiSwapV3 {
        fee: u32,
    },
    PancakeSwapV3 {
        fee: u32,
    },
    Curve,
    Conversion {
        kind: ConversionKind,
        wrap: bool,
        unwrap: bool,
    },
}

impl PoolKind {
    pub fn name(&self) -> &'static str {
        match self {
            PoolKind::UniswapV2 => "uniswap-v2",
            PoolKind::ShibaSwapV2 => "shibaswap-v2",
            PoolKind::UniswapV3 { .. } => "uniswap-v3",
            PoolKind::SushiSwapV3 { .. } => "sushi-v3",
            PoolKind::PancakeSwapV3 { .. } => "pancake-v3",
            PoolKind::Curve => "curve",
            PoolKind::Conversion {
                kind: ConversionKind::WstEth,
                ..
            } => "wsteth",
        }
    }

    pub fn is_v2(self) -> bool {
        matches!(self, Self::UniswapV2 | Self::ShibaSwapV2)
    }

    pub fn is_conversion(self) -> bool {
        matches!(self, Self::Conversion { .. })
    }

    pub fn v3_venue(self) -> Option<V3Venue> {
        match self {
            Self::UniswapV3 { .. } => Some(V3Venue::Uniswap),
            Self::SushiSwapV3 { .. } => Some(V3Venue::SushiSwap),
            Self::PancakeSwapV3 { .. } => Some(V3Venue::PancakeSwap),
            _ => None,
        }
    }

    pub fn v3_fee(self) -> Option<u32> {
        match self {
            Self::UniswapV3 { fee } | Self::SushiSwapV3 { fee } | Self::PancakeSwapV3 { fee } => {
                Some(fee)
            }
            _ => None,
        }
    }

    pub fn is_v3(self) -> bool {
        self.v3_venue().is_some()
    }

    pub fn allows(self, i: usize, j: usize) -> bool {
        match self {
            Self::Conversion { wrap, unwrap, .. } => {
                matches!((i, j), (0, 1) if wrap) || matches!((i, j), (1, 0) if unwrap)
            }
            _ => i != j,
        }
    }

    pub fn operation(self, i: usize) -> &'static str {
        if self.is_conversion() {
            if i == 0 { "wrap" } else { "unwrap" }
        } else {
            "swap"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pool {
    pub kind: PoolKind,
    pub address: Address,
    pub coins: Vec<Address>,
}

pub const UNISWAP_V3_FACTORY: &str = "0x1f98431c8ad98523631ae4a59f267346ea31f984";
pub const UNISWAP_V3_QUOTER: &str = "0x61fFE014bA17989E743c5F6cB21bF9697530B21e";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V3Deployment {
    pub venue: V3Venue,
    pub factory: Address,
    pub quoter: Address,
}

#[derive(Debug, Clone, Default)]
pub struct V3Deployments {
    by_venue: HashMap<V3Venue, V3Deployment>,
}

impl V3Deployments {
    pub fn new(deployments: impl IntoIterator<Item = V3Deployment>) -> Self {
        Self {
            by_venue: deployments
                .into_iter()
                .map(|deployment| (deployment.venue, deployment))
                .collect(),
        }
    }

    pub fn uniswap_default() -> Self {
        Self::new([V3Deployment {
            venue: V3Venue::Uniswap,
            factory: UNISWAP_V3_FACTORY.parse().unwrap(),
            quoter: UNISWAP_V3_QUOTER.parse().unwrap(),
        }])
    }

    pub fn get(&self, venue: V3Venue) -> Option<&V3Deployment> {
        self.by_venue.get(&venue)
    }

    pub fn iter(&self) -> impl Iterator<Item = &V3Deployment> {
        self.by_venue.values()
    }

    pub fn is_empty(&self) -> bool {
        self.by_venue.is_empty()
    }

    pub fn factory(&self, venue: V3Venue) -> Option<Address> {
        self.get(venue).map(|deployment| deployment.factory)
    }

    pub fn quoter(&self, venue: V3Venue) -> Option<Address> {
        self.get(venue).map(|deployment| deployment.quoter)
    }
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
