//! Local Fluxion client: CLI, MCP bridge, remote client and token storage.

pub mod cli;
pub mod client;
pub mod mcp;
pub mod routing;
pub mod token;

pub use fluxion_protocol::{capability, swap};
