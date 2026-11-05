//! Internal execution routing. `swap.quote` always uses the remote Fluxion
//! service; agents and users do not choose an execution location.
//!
//! Development and tests may point `FLUXION_REMOTE_URL` (or `--url`) at a mock
//! or locally running server. That is an endpoint override, not a mode.

use std::env;

use crate::capability::{CapabilityError, CapabilityErrorCode};

#[derive(Debug, Clone)]
pub struct RemoteBackend {
    pub base_url: String,
    pub api_key: Option<String>,
}

pub fn resolve(
    remote_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<RemoteBackend, CapabilityError> {
    let url = remote_url
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| {
            CapabilityError::new(
                CapabilityErrorCode::RpcError,
                "Fluxion remote service is not configured; set FLUXION_REMOTE_URL",
                false,
            )
        })?;
    Ok(RemoteBackend {
        base_url: url.trim_end_matches('/').to_string(),
        api_key: api_key
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_string),
    })
}

pub fn from_env(remote_url: Option<&str>) -> Result<RemoteBackend, CapabilityError> {
    let env_url = env::var("FLUXION_REMOTE_URL").ok();
    let api_key = env::var("FLUXION_API_KEY").ok();
    resolve(remote_url.or(env_url.as_deref()), api_key.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_a_remote_endpoint() {
        let error = resolve(None, None).unwrap_err();
        assert_eq!(error.code, CapabilityErrorCode::RpcError);
        assert!(error.message.contains("FLUXION_REMOTE_URL"));
    }

    #[test]
    fn trims_url_and_keeps_api_key() {
        let backend = resolve(Some("https://api.example.com/"), Some("key")).unwrap();
        assert_eq!(backend.base_url, "https://api.example.com");
        assert_eq!(backend.api_key.as_deref(), Some("key"));
    }

    #[test]
    fn explicit_url_overrides_env_style_input() {
        let backend = resolve(Some("http://127.0.0.1:8080"), None).unwrap();
        assert_eq!(backend.base_url, "http://127.0.0.1:8080");
        assert!(backend.api_key.is_none());
    }
}
