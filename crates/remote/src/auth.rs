//! Minimal installation-token authentication for the remote quote service.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use alloy_primitives::keccak256;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Default)]
pub struct AuthStore {
    tokens: RwLock<HashMap<String, String>>,
}

impl AuthStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, install_id: &str) -> String {
        let mut tokens = self.tokens.write().unwrap();
        if let Some(token) = tokens.get(install_id) {
            return token.clone();
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let token = format!(
            "{:#x}",
            keccak256(format!("{install_id}:{nanos}:{}", std::process::id()).as_bytes())
        );
        tokens.insert(install_id.to_string(), token.clone());
        token
    }

    pub fn authorize(&self, header: Option<&str>) -> bool {
        let Some(value) = header else {
            return false;
        };
        let Some(token) = value.strip_prefix("Bearer ") else {
            return false;
        };
        let tokens = self.tokens.read().unwrap();
        tokens.values().any(|issued| issued == token)
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub install_id: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub token: String,
}

pub async fn register(
    Extension(auth): Extension<Arc<AuthStore>>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, StatusCode> {
    if request.install_id.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(Json(RegisterResponse {
        token: auth.register(request.install_id.trim()),
    }))
}

pub async fn require_auth(
    State(auth): State<Arc<AuthStore>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if request.uri().path() == "/quote" {
        let header = request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok());
        if !auth.authorize(header) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Unauthorized"})),
            )
                .into_response();
        }
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_is_idempotent_and_authorizes_bearer_tokens() {
        let auth = AuthStore::new();
        let token = auth.register("install-1");
        assert_eq!(token, auth.register("install-1"));
        assert_ne!(token, auth.register("install-2"));
        assert!(auth.authorize(Some(&format!("Bearer {token}"))));
        assert!(!auth.authorize(Some("Bearer nope")));
        assert!(!auth.authorize(Some(token.as_str())));
        assert!(!auth.authorize(None));
    }
}
