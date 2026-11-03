//! Lido v4's view arithmetic, using internal ETH/internal shares, not the
//! rounded public total-supply ratio. This immutable model has no RPC handles.
use crate::{error::QuoteError, source::ratio};
use alloy_primitives::U256;

pub const SUPPORTED_VERSION: u64 = 4;

pub struct WstEthState {
    internal_ether: U256,
    internal_shares: U256,
}

impl WstEthState {
    pub fn new(
        version: u64,
        total_pooled_ether: U256,
        total_shares: U256,
        external_ether: U256,
        external_shares: U256,
    ) -> Result<Self, QuoteError> {
        if version != SUPPORTED_VERSION || total_shares.bit_len() > 128 {
            return Err(QuoteError::IncompleteState);
        }
        let internal_ether = total_pooled_ether
            .checked_sub(external_ether)
            .ok_or(QuoteError::IncompleteState)?;
        let internal_shares = total_shares
            .checked_sub(external_shares)
            .ok_or(QuoteError::IncompleteState)?;
        if internal_ether.is_zero() || internal_shares.is_zero() || internal_ether.bit_len() > 128 {
            return Err(QuoteError::IncompleteState);
        }
        Ok(Self {
            internal_ether,
            internal_shares,
        })
    }

    pub fn internal_ether(&self) -> U256 {
        self.internal_ether
    }
    pub fn internal_shares(&self) -> U256 {
        self.internal_shares
    }

    pub fn amount_out(&self, amount: U256, wrap: bool) -> Result<U256, QuoteError> {
        if amount.is_zero() {
            return Err(QuoteError::ZeroAmount);
        }
        // Lido v4 requires strictly less than uint128.max, not <= max. The
        // multiplication then fits uint256; do not relax the protocol input cap.
        if amount >= U256::from(u128::MAX) {
            return Err(QuoteError::Overflow);
        }
        let (numerator, denominator) = if wrap {
            (self.internal_shares, self.internal_ether)
        } else {
            (self.internal_ether, self.internal_shares)
        };
        Ok(amount.checked_mul(numerator).ok_or(QuoteError::Overflow)? / denominator)
    }

    pub fn mid_price(&self, wrap: bool) -> f64 {
        if wrap {
            ratio(self.internal_shares, self.internal_ether)
        } else {
            ratio(self.internal_ether, self.internal_shares)
        }
    }
}
