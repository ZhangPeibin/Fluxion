//! Stable error surface for capability calls. Adapters and HTTP/CLI clients
//! should branch on `code`, not on human-readable messages.

use serde::Serialize;

use crate::{error::QuoteError, local_state::StateError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapabilityErrorCode {
    InvalidArgument,
    NoPath,
    InsufficientLiquidity,
    AmountOutBelowMin,
    RpcError,
    StaleState,
    StateIncomplete,
    Unsupported,
    Timeout,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CapabilityError {
    pub code: CapabilityErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl CapabilityErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidArgument => "INVALID_ARGUMENT",
            Self::NoPath => "NO_PATH",
            Self::InsufficientLiquidity => "INSUFFICIENT_LIQUIDITY",
            Self::AmountOutBelowMin => "AMOUNT_OUT_BELOW_MIN",
            Self::RpcError => "RPC_ERROR",
            Self::StaleState => "STALE_STATE",
            Self::StateIncomplete => "STATE_INCOMPLETE",
            Self::Unsupported => "UNSUPPORTED",
            Self::Timeout => "TIMEOUT",
            Self::Internal => "INTERNAL",
        }
    }
}

impl CapabilityError {
    pub fn code_string(&self) -> &'static str {
        self.code.as_str()
    }

    pub fn new(code: CapabilityErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(CapabilityErrorCode::InvalidArgument, message, false)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(CapabilityErrorCode::Internal, message, false)
    }

    pub fn from_quote(error: &QuoteError) -> Self {
        let (code, retryable) = match error {
            QuoteError::ZeroAmount => (CapabilityErrorCode::InvalidArgument, false),
            QuoteError::AmountOutBelowMin => (CapabilityErrorCode::AmountOutBelowMin, false),
            QuoteError::NoPath => (CapabilityErrorCode::NoPath, false),
            QuoteError::InsufficientLiquidity => {
                (CapabilityErrorCode::InsufficientLiquidity, false)
            }
            QuoteError::IncompleteState => (CapabilityErrorCode::StateIncomplete, true),
            QuoteError::UnknownPool | QuoteError::Overflow => {
                (CapabilityErrorCode::Internal, false)
            }
            QuoteError::RpcError(_) => (CapabilityErrorCode::RpcError, true),
        };
        Self::new(code, error.to_string(), retryable)
    }

    pub fn from_state(error: StateError) -> Self {
        let (code, retryable) = match error {
            StateError::NotReady | StateError::Stale | StateError::Lagging | StateError::Reorg => {
                (CapabilityErrorCode::StaleState, true)
            }
            StateError::Incomplete => (CapabilityErrorCode::StateIncomplete, true),
            StateError::Unsupported => (CapabilityErrorCode::Unsupported, false),
            StateError::SyncFailed => (CapabilityErrorCode::RpcError, true),
        };
        Self::new(code, error.to_string(), retryable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_errors_map_to_stable_codes() {
        assert_eq!(
            CapabilityError::from_quote(&QuoteError::ZeroAmount).code,
            CapabilityErrorCode::InvalidArgument
        );
        assert_eq!(
            CapabilityError::from_quote(&QuoteError::NoPath).code,
            CapabilityErrorCode::NoPath
        );
        assert!(CapabilityError::from_quote(&QuoteError::RpcError("down".into())).retryable);
    }

    #[test]
    fn state_errors_map_to_stable_codes() {
        let stale = CapabilityError::from_state(StateError::Lagging);
        assert_eq!(stale.code, CapabilityErrorCode::StaleState);
        assert!(stale.retryable);
        assert_eq!(
            CapabilityError::from_state(StateError::Unsupported).code,
            CapabilityErrorCode::Unsupported
        );
    }
}
