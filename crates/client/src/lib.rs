//! Local Fluxion client: CLI, MCP bridge, remote client and token storage.

pub mod agent_id;
pub mod auth;
pub mod cli;
pub mod client;
pub mod mcp;
pub mod routing;
pub mod token;
pub mod transaction;
pub mod wallet;
pub mod wallet_send;
pub mod walletconnect;

pub use fluxion_protocol::{capability, swap};
