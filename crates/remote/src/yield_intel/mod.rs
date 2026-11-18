//! Remote yield intelligence: curated universe, provider normalization,
//! scoring and a warm read-only snapshot.

pub mod provider;
pub mod registry;
pub mod score;
pub mod store;

use thiserror::Error;

pub use registry::{Registry, RegistryEntry};
pub use score::discover;
pub use store::{YieldOptions, YieldStore};

#[derive(Debug, Error)]
pub enum YieldError {
    #[error("{0}")]
    Invalid(String),
    #[error("yield data unavailable: {0}")]
    Unavailable(String),
    #[error("yield data is stale")]
    Stale,
    #[error("{0}")]
    Internal(String),
}
