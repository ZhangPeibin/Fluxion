//! Workspace facade kept for repository tests and backwards-compatible imports.
//! Production binaries live in `fluxion-remote` and `fluxion-client`.

pub use fluxion_client::{cli, client, mcp, routing, token};
pub use fluxion_protocol::{capability, swap};
pub use fluxion_remote::*;
