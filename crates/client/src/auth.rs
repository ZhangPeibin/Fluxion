//! Installation identity and token storage for the remote Fluxion service.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::capability::{CapabilityError, CapabilityErrorCode};

fn fluxion_dir() -> PathBuf {
    let home = env::var_os("FLUXION_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(env::temp_dir);
    home.join(".fluxion")
}

fn install_id_path() -> PathBuf {
    fluxion_dir().join("install-id")
}

fn token_path() -> PathBuf {
    fluxion_dir().join("auth.json")
}

fn write_private(path: &PathBuf, contents: &str) -> Result<(), CapabilityError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CapabilityError::internal(format!("cannot create {}: {error}", parent.display()))
        })?;
    }
    fs::write(path, contents).map_err(|error| {
        CapabilityError::internal(format!("cannot write {}: {error}", path.display()))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn install_id() -> String {
    let path = install_id_path();
    if let Ok(id) = fs::read_to_string(&path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let id = format!("{:032x}", nanos ^ u128::from(std::process::id()));
    let _ = write_private(&path, &id);
    id
}

fn read_tokens() -> HashMap<String, String> {
    fs::read_to_string(token_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn load_token(base_url: &str) -> Option<String> {
    read_tokens()
        .get(base_url.trim_end_matches('/'))
        .filter(|token| !token.trim().is_empty())
        .cloned()
}

pub fn save_token(base_url: &str, token: &str) {
    let mut tokens = read_tokens();
    tokens.insert(
        base_url.trim_end_matches('/').to_string(),
        token.to_string(),
    );
    if let Ok(text) = serde_json::to_string_pretty(&tokens) {
        let _ = write_private(&token_path(), &text);
    }
}

pub async fn register(http: &reqwest::Client, base_url: &str) -> Result<String, CapabilityError> {
    let response = http
        .post(format!("{}/register", base_url.trim_end_matches('/')))
        .json(&json!({"install_id": install_id()}))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|error| {
            CapabilityError::new(
                CapabilityErrorCode::RpcError,
                format!("installation registration failed: {error}"),
                true,
            )
        })?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(CapabilityError::new(
            CapabilityErrorCode::RpcError,
            format!("installation registration rejected ({status}): {text}"),
            status.is_server_error(),
        ));
    }
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        CapabilityError::internal(format!("invalid registration response: {error}"))
    })?;
    value
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .ok_or_else(|| CapabilityError::internal("registration response missing token"))
}
