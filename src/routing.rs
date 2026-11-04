//! Internal execution routing. Agents call capabilities; Fluxion decides whether
//! a capability runs against the hosted remote service or the local runtime.

use std::env;

use crate::capability::{CapabilityError, CapabilityErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Auto,
    Managed,
    SelfHosted,
}

impl ExecutionMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "managed" | "remote" => Some(Self::Managed),
            "self_hosted" | "self-hosted" | "local" => Some(Self::SelfHosted),
            _ => None,
        }
    }

    pub fn from_env() -> Self {
        match env::var("FLUXION_MODE") {
            Ok(value) => Self::parse(&value).unwrap_or_else(|| {
                tracing::warn!(mode = %value, "unknown FLUXION_MODE; using auto");
                Self::Auto
            }),
            Err(_) => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionLocation {
    Local,
    Remote,
}

#[derive(Debug, Clone)]
pub struct QuoteBackend {
    pub base_url: String,
    pub api_key: Option<String>,
    pub location: ExecutionLocation,
    /// Diagnostic label written to `provenance.provider`.
    pub provider: &'static str,
}

impl QuoteBackend {
    pub fn is_local(&self) -> bool {
        self.location == ExecutionLocation::Local
    }
}

pub fn resolve(
    mode: ExecutionMode,
    local_provider: &'static str,
    local_url: &str,
    remote_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<QuoteBackend, CapabilityError> {
    let remote = remote_url.filter(|url| !url.trim().is_empty());
    let use_remote = match mode {
        ExecutionMode::Managed => true,
        ExecutionMode::SelfHosted => false,
        ExecutionMode::Auto => remote.is_some(),
    };
    if use_remote {
        let Some(url) = remote else {
            return Err(CapabilityError::new(
                CapabilityErrorCode::RpcError,
                "FLUXION_MODE=managed requires FLUXION_REMOTE_URL",
                false,
            ));
        };
        return Ok(QuoteBackend {
            base_url: url.trim_end_matches('/').to_string(),
            api_key: api_key.map(str::to_string),
            location: ExecutionLocation::Remote,
            provider: "fluxion-remote",
        });
    }
    Ok(QuoteBackend {
        base_url: local_url.trim_end_matches('/').to_string(),
        api_key: None,
        location: ExecutionLocation::Local,
        provider: local_provider,
    })
}

pub fn from_env(
    local_provider: &'static str,
    local_url: &str,
) -> Result<QuoteBackend, CapabilityError> {
    let remote_url = env::var("FLUXION_REMOTE_URL").ok();
    let api_key = env::var("FLUXION_API_KEY").ok();
    resolve(
        ExecutionMode::from_env(),
        local_provider,
        local_url,
        remote_url.as_deref(),
        api_key.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_prefers_remote_when_configured() {
        let backend = resolve(
            ExecutionMode::Auto,
            "local-mcp",
            "http://127.0.0.1:8080",
            Some("https://api.example.com/"),
            Some("key"),
        )
        .unwrap();
        assert_eq!(backend.location, ExecutionLocation::Remote);
        assert_eq!(backend.base_url, "https://api.example.com");
        assert_eq!(backend.provider, "fluxion-remote");
        assert_eq!(backend.api_key.as_deref(), Some("key"));
    }

    #[test]
    fn auto_falls_back_to_local_without_remote() {
        let backend = resolve(
            ExecutionMode::Auto,
            "local-cli",
            "http://127.0.0.1:8080",
            None,
            None,
        )
        .unwrap();
        assert_eq!(backend.location, ExecutionLocation::Local);
        assert_eq!(backend.provider, "local-cli");
    }

    #[test]
    fn managed_requires_remote_url() {
        let error = resolve(
            ExecutionMode::Managed,
            "local-mcp",
            "http://127.0.0.1:8080",
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(error.code, CapabilityErrorCode::RpcError);
    }

    #[test]
    fn self_hosted_ignores_remote_url() {
        let backend = resolve(
            ExecutionMode::SelfHosted,
            "local-mcp",
            "http://127.0.0.1:8080",
            Some("https://api.example.com"),
            None,
        )
        .unwrap();
        assert_eq!(backend.location, ExecutionLocation::Local);
    }
}
