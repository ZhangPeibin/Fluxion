//! `swap` capability family. The current implementation exposes `swap.quote`;
//! `swap.build` and execution belong to later phases.

pub mod quote;

pub use quote::{
    AllocationResponse, HopResponse, QuoteContext, QuoteErrorResponse, QuoteRequest, QuoteResponse,
};

pub const CAPABILITY: &str = "swap.quote";
