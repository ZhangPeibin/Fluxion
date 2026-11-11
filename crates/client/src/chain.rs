//! Supported local EVM chains.

use crate::capability::CapabilityError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chain {
    Ethereum,
    Bsc,
}

pub const DEFAULT_CHAIN: Chain = Chain::Bsc;

impl Chain {
    pub fn parse(value: &str) -> Result<Self, CapabilityError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "bsc" | "bnb" | "binance" | "56" => Ok(Self::Bsc),
            "ethereum" | "eth" | "mainnet" | "1" => Ok(Self::Ethereum),
            other => Err(CapabilityError::invalid_argument(format!(
                "unsupported chain `{other}`; expected bsc or ethereum"
            ))),
        }
    }

    pub fn from_id(chain_id: u64) -> Result<Self, CapabilityError> {
        match chain_id {
            1 => Ok(Self::Ethereum),
            56 => Ok(Self::Bsc),
            other => Err(CapabilityError::invalid_argument(format!(
                "unsupported chain id {other}; expected 1 or 56"
            ))),
        }
    }

    pub const fn chain_id(self) -> u64 {
        match self {
            Self::Ethereum => 1,
            Self::Bsc => 56,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ethereum => "ethereum",
            Self::Bsc => "bsc",
        }
    }

    pub const fn native_symbol(self) -> &'static str {
        match self {
            Self::Ethereum => "ETH",
            Self::Bsc => "BNB",
        }
    }

    pub const fn alchemy_network(self) -> &'static str {
        match self {
            Self::Ethereum => "eth-mainnet",
            Self::Bsc => "bnb-mainnet",
        }
    }
}
