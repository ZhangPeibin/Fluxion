//! Remote yield intelligence: curated universe, provider normalization,
//! scoring and a warm read-only snapshot.

pub mod history;
pub mod provider;
pub mod score;
pub mod store;

use thiserror::Error;

pub use eollo_protocol::yield_capability::{Registry, RegistryEntry};
pub use score::discover;
pub use store::{YieldOptions, YieldStore};

#[derive(Debug, Error)]
pub enum YieldError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("yield data unavailable: {0}")]
    Unavailable(String),
    #[error("yield data is stale")]
    Stale,
    #[error("{0}")]
    Internal(String),
}
