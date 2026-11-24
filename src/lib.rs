//! Workspace facade kept for repository tests and backwards-compatible imports.
//! Production binaries live in `eollo-remote` and `eollo-client`.

pub use eollo_client::{cli, client, mcp, routing, token};
pub use eollo_protocol::{capability, swap};
pub use eollo_remote::*;
