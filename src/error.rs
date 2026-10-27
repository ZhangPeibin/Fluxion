use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QuoteError {
    #[error("no path found")]
    NoPath,
    #[error("insufficient liquidity")]
    InsufficientLiquidity,
    #[error("zero amount in")]
    ZeroAmount,
    #[error("unknown pool")]
    UnknownPool,
    #[error("arithmetic overflow")]
    Overflow,
}
