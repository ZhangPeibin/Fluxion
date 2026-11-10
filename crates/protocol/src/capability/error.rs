//! Stable error surface for capability calls. Adapters and HTTP/CLI clients
//! should branch on `code`, not on human-readable messages.

use serde::Serialize;

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
    UserRejected,
    SimulationFailed,
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
            Self::UserRejected => "USER_REJECTED",
            Self::SimulationFailed => "SIMULATION_FAILED",
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
}
