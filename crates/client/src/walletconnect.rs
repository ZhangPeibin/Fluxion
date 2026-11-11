//! Local WalletConnect v2 session management.
//!
//! The dApp side of WalletConnect is implemented directly: relay JWT auth,
//! pairing envelopes, session proposal/settle and session persistence. Private
//! keys never enter Fluxion; the wallet signs on the user's device.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alloy_primitives::{Address, U256};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use hkdf::Hkdf;
use qrcode::render::unicode::Dense1x2;
use qrcode::{Color, EcLevel, QrCode};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};
use url::Url;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::capability::{CapabilityError, CapabilityErrorCode};
use fluxion_protocol::wallet::{
    WalletConnectResponse, WalletDisconnectResponse, WalletPeer, WalletSignResponse,
    WalletStatusResponse,
};

const DEFAULT_RELAY_URL: &str = "wss://relay.walletconnect.com";
const PAIRING_TIMEOUT_SECS: u64 = 300;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type SocketWrite = SplitSink<Socket, Message>;
type SocketRead = SplitStream<Socket>;

#[derive(Debug, Clone)]
pub struct ConnectConfig {
    pub project_id: String,
    pub relay_url: String,
}

impl ConnectConfig {
    pub fn from_env() -> Result<Self, CapabilityError> {
        Ok(Self {
            project_id: env::var("WALLETCONNECT_PROJECT_ID")
                .map_err(|_| invalid("WALLETCONNECT_PROJECT_ID is not set"))?,
            relay_url: env::var("WALLETCONNECT_RELAY_URL")
                .unwrap_or_else(|_| DEFAULT_RELAY_URL.to_string()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub topic: String,
    pub sym_key: String,
    pub accounts: Vec<Address>,
    pub chain_id: u64,
    pub expiry: i64,
    pub responder_public_key: String,
    #[serde(default)]
    pub wallet_name: Option<String>,
    #[serde(default)]
    pub wallet_url: Option<String>,
    #[serde(default)]
    pub wallet_icons: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub relay_secret: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SignPayload {
    Message(String),
    TypedData(Value),
}

#[derive(Debug, Clone)]
pub struct SignRequest {
    pub address: Option<Address>,
    pub payload: SignPayload,
    pub timeout: Duration,
}

impl Session {
    pub async fn sign(&self, request: SignRequest) -> Result<WalletSignResponse, CapabilityError> {
        let address = request
            .address
            .or_else(|| self.accounts.first().copied())
            .ok_or_else(|| {
                CapabilityError::new(
                    CapabilityErrorCode::Unsupported,
                    "WalletConnect session has no account",
                    false,
                )
            })?;
        if !self.accounts.contains(&address) {
            return Err(invalid("address is not in the WalletConnect session"));
        }
        let (method, params) = match request.payload {
            SignPayload::Message(message) => {
                if message.len() > 64 * 1024 {
                    return Err(invalid("message is too large"));
                }
                (
                    "personal_sign",
                    json!([format!("0x{}", hex(message.as_bytes())), address]),
                )
            }
            SignPayload::TypedData(typed_data) => {
                let typed_json = validate_typed_data(&typed_data, self.chain_id)?;
                ("eth_signTypedData_v4", json!([address, typed_json]))
            }
        };
        let result = self.request_wallet(method, params, request.timeout).await?;
        let signature = result
            .as_str()
            .ok_or_else(|| rpc("wallet returned no signature"))?;
        if !signature.starts_with("0x") || !signature[2..].chars().all(|ch| ch.is_ascii_hexdigit())
        {
            return Err(rpc("wallet returned an invalid signature"));
        }
        Ok(WalletSignResponse {
            chain: crate::chain::Chain::from_id(self.chain_id)
                .map(|chain| chain.as_str().to_string())
                .unwrap_or_else(|_| format!("eip155:{}", self.chain_id)),
            address,
            chain_id: self.chain_id,
            method: method.to_string(),
            signature: signature.to_string(),
        })
    }

    pub async fn send_transaction(
        &self,
        from: Address,
        to: Address,
        value: U256,
        data: String,
        timeout: Duration,
    ) -> Result<String, CapabilityError> {
        if !self.accounts.contains(&from) {
            return Err(invalid("address is not in the WalletConnect session"));
        }
        let transaction = json!({
            "from": from,
            "to": to,
            "value": format!("0x{:x}", value),
            "data": data,
        });
        let result = self
            .request_wallet("eth_sendTransaction", json!([transaction]), timeout)
            .await?;
        let hash = result
            .as_str()
            .ok_or_else(|| rpc("wallet returned no transaction hash"))?;
        if !hash.starts_with("0x")
            || hash.len() != 66
            || !hash[2..].chars().all(|ch| ch.is_ascii_hexdigit())
        {
            return Err(rpc("wallet returned an invalid transaction hash"));
        }
        Ok(hash.to_string())
    }

    async fn request_wallet(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, CapabilityError> {
        if self.expiry <= unix_timestamp() {
            return Err(CapabilityError::new(
                CapabilityErrorCode::StaleState,
                "WalletConnect session has expired; run wallet.connect again",
                false,
            ));
        }
        if !self.methods.is_empty() && !self.methods.iter().any(|item| item == method) {
            return Err(CapabilityError::new(
                CapabilityErrorCode::Unsupported,
                format!("wallet did not approve {method} for this session"),
                false,
            ));
        }
        let key = session_key(&self.sym_key)?;
        let config = ConnectConfig::from_env()?;
        let relay_secret = self.relay_secret.as_deref().ok_or_else(|| {
            CapabilityError::new(
                CapabilityErrorCode::Unsupported,
                "WalletConnect session was created by an older Fluxion version; run wallet.disconnect and wallet.connect again",
                false,
            )
        })?;
        let (mut relay, _) = connect_relay(&config, Some(relay_secret)).await?;
        let subscribe_id = relay
            .request("irn_subscribe", json!({"topic": self.topic}))
            .await?;
        wait_for_id(&mut relay, subscribe_id).await?;

        let id = relay.next_id();
        let payload = json!({
            "id": id,
            "jsonrpc": "2.0",
            "method": "wc_sessionRequest",
            "params": {
                "request": {
                    "method": method,
                    "params": params
                },
                "chainId": format!("eip155:{}", self.chain_id)
            }
        });
        publish(&mut relay, &self.topic, &key, &payload, 1108, true).await?;

        let timeout = timeout.max(Duration::from_secs(1));
        match tokio::time::timeout(
            timeout,
            wait_for_response(&mut relay, &self.topic, &key, id),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(CapabilityError::new(
                CapabilityErrorCode::Timeout,
                "wallet request timed out",
                true,
            )),
        }
    }

    pub fn load() -> Result<Option<Self>, CapabilityError> {
        Self::load_from(&session_path())
    }

    pub fn load_from(path: &PathBuf) -> Result<Option<Self>, CapabilityError> {
        match fs::read(path) {
            Ok(bytes) => {
                let mut session: Self = serde_json::from_slice(&bytes)
                    .map_err(|error| internal(format!("invalid WalletConnect session: {error}")))?;
                dedupe_accounts(&mut session.accounts);
                Ok(Some(session))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(internal(format!("cannot read {}: {error}", path.display()))),
        }
    }

    pub fn save(&self) -> Result<(), CapabilityError> {
        self.save_to(&session_path())
    }

    pub fn save_to(&self, path: &PathBuf) -> Result<(), CapabilityError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                internal(format!("cannot create {}: {error}", parent.display()))
            })?;
        }
        fs::write(
            path,
            serde_json::to_vec_pretty(self).map_err(|error| internal(error.to_string()))?,
        )
        .map_err(|error| internal(format!("cannot write {}: {error}", path.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn peer(&self) -> Option<WalletPeer> {
        self.wallet_name.as_ref().map(|name| WalletPeer {
            name: name.clone(),
            url: self.wallet_url.clone().unwrap_or_default(),
            icons: self.wallet_icons.clone(),
        })
    }
}

fn walletconnect_dir() -> PathBuf {
    let home = env::var_os("FLUXION_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(env::temp_dir);
    home.join(".fluxion").join("walletconnect")
}

pub fn session_path() -> PathBuf {
    walletconnect_dir().join("session.json")
}

pub fn open_qr(path: &str) -> Result<(), CapabilityError> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return Err(internal(
        "opening a QR image is not supported on this platform",
    ));

    command
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| internal(format!("cannot open QR image: {error}")))
}

fn qr_png_path(connection_id: &str) -> PathBuf {
    walletconnect_dir().join(format!("qr-{connection_id}.png"))
}

fn write_private_bytes(path: &PathBuf, bytes: &[u8]) -> Result<(), CapabilityError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| internal(format!("cannot create {}: {error}", parent.display())))?;
    }
    fs::write(path, bytes)
        .map_err(|error| internal(format!("cannot write {}: {error}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn parse_chain_id(value: &str) -> Result<u64, CapabilityError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "ethereum" | "eth" | "mainnet" | "1" => Ok(1),
        "bsc" | "bnb" | "binance" | "56" => Ok(56),
        other => Err(invalid(format!(
            "unsupported chain `{other}`; expected ethereum or bsc"
        ))),
    }
}

pub async fn sign(request: SignRequest) -> Result<WalletSignResponse, CapabilityError> {
    let session = Session::load()?.ok_or_else(|| {
        CapabilityError::new(
            CapabilityErrorCode::Unsupported,
            "wallet is not connected; run wallet.connect first",
            false,
        )
    })?;
    session.sign(request).await
}

pub async fn send_transaction(
    from: Address,
    to: Address,
    value: U256,
    data: String,
    timeout: Duration,
) -> Result<String, CapabilityError> {
    let session = Session::load()?.ok_or_else(|| {
        CapabilityError::new(
            CapabilityErrorCode::Unsupported,
            "wallet is not connected; run wallet.connect first",
            false,
        )
    })?;
    session
        .send_transaction(from, to, value, data, timeout)
        .await
}

pub fn status() -> Result<WalletStatusResponse, CapabilityError> {
    let Some(session) = Session::load()? else {
        return Ok(disconnected_status(None));
    };
    Ok(status_from_session(session, None))
}

pub async fn wait_for_session(wait: Duration) -> Result<WalletStatusResponse, CapabilityError> {
    if wait.is_zero() {
        return status();
    }
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let current = status()?;
        if current.status != "disconnected" || tokio::time::Instant::now() >= deadline {
            return Ok(current);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

pub fn disconnect() -> Result<WalletDisconnectResponse, CapabilityError> {
    let path = session_path();
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(internal(format!(
                "cannot remove {}: {error}",
                path.display()
            )));
        }
    }
    Ok(WalletDisconnectResponse { disconnected: true })
}

async fn connect_relay(
    config: &ConnectConfig,
    secret: Option<&str>,
) -> Result<(Relay, String), CapabilityError> {
    let relay_key = match secret {
        Some(secret) => {
            let bytes = unhex(secret)?;
            let bytes: [u8; 32] = bytes
                .as_slice()
                .try_into()
                .map_err(|_| internal("invalid WalletConnect relay secret"))?;
            SigningKey::from_bytes(&bytes)
        }
        None => SigningKey::generate(&mut OsRng),
    };
    let relay_secret = hex(relay_key.to_bytes().as_slice());
    let auth = auth_token(&relay_key, &config.relay_url);
    let mut url = Url::parse(&config.relay_url)
        .map_err(|error| invalid(format!("invalid relay URL: {error}")))?;
    url.query_pairs_mut()
        .append_pair("projectId", &config.project_id)
        .append_pair("auth", &auth);

    let (socket, _) = connect_async(url.as_str())
        .await
        .map_err(|error| rpc(format!("WalletConnect relay connection failed: {error}")))?;
    let (write, read) = socket.split();
    Ok((
        Relay {
            write,
            read,
            next_id: 1_000_000_000,
        },
        relay_secret,
    ))
}

pub struct Pairing {
    relay: Relay,
    relay_secret: String,
    pairing_secret: StaticSecret,
    pairing_topic: String,
    pairing_key: [u8; 32],
    uri: String,
    proposal: Value,
    chain_id: u64,
    expires_at: i64,
}

impl Pairing {
    pub async fn start(config: &ConnectConfig, chain_id: u64) -> Result<Self, CapabilityError> {
        let (mut relay, relay_secret) = connect_relay(config, None).await?;

        let pairing_secret = StaticSecret::random_from_rng(OsRng);
        let pairing_key = pairing_secret.to_bytes();
        let mut topic = [0u8; 32];
        OsRng.fill_bytes(&mut topic);
        let pairing_topic = hex(&topic);
        let proposer_public = PublicKey::from(&pairing_secret);
        let uri = format!(
            "wc:{pairing_topic}@2?relay-protocol=irn&symKey={}",
            hex(&pairing_key)
        );

        let subscribe_id = relay
            .request("irn_subscribe", json!({"topic": pairing_topic}))
            .await?;
        wait_for_id(&mut relay, subscribe_id).await?;

        let proposal_id = relay.next_id();
        let proposal = json!({
            "id": proposal_id,
            "jsonrpc": "2.0",
            "method": "wc_sessionPropose",
            "params": {
                "relays": [{"protocol": "irn"}],
                "requiredNamespaces": {
                    "eip155": {
                        "chains": [format!("eip155:{chain_id}")],
                        "methods": ["eth_signTransaction", "eth_signTypedData_v4"],
                        "events": ["chainChanged", "accountsChanged"]
                    }
                },
                "optionalNamespaces": {
                    "eip155": {
                        "chains": [format!("eip155:{chain_id}")],
                        "methods": ["eth_sendTransaction", "personal_sign", "eth_signTypedData"],
                        "events": []
                    }
                },
                "proposer": {
                    "publicKey": hex(&proposer_public.to_bytes()),
                    "metadata": {
                        "name": "Fluxion",
                        "description": "Fluxion local wallet session",
                        "url": "https://github.com/ZhangPeibin/Fluxion",
                        "icons": []
                    }
                }
            }
        });
        let publish_id = publish(
            &mut relay,
            &pairing_topic,
            &pairing_key,
            &proposal,
            1100,
            true,
        )
        .await?;
        wait_for_id(&mut relay, publish_id).await?;

        Ok(Self {
            relay,
            pairing_secret,
            pairing_topic,
            pairing_key,
            uri,
            proposal,
            chain_id,
            relay_secret,
            expires_at: unix_timestamp() + PAIRING_TIMEOUT_SECS as i64,
        })
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }

    pub fn qr_ascii(&self) -> Result<String, CapabilityError> {
        render_qr(&self.uri)
    }

    pub fn qr_png_base64(&self) -> Result<String, CapabilityError> {
        qr_png_base64(&self.uri)
    }

    pub fn qr_png_bytes(&self) -> Result<Vec<u8>, CapabilityError> {
        qr_png(&self.uri)
    }

    pub async fn wait(self, timeout: Duration) -> Result<Session, CapabilityError> {
        match tokio::time::timeout(timeout, self.wait_inner()).await {
            Ok(result) => result,
            Err(_) => Err(CapabilityError::new(
                CapabilityErrorCode::Timeout,
                "WalletConnect pairing timed out",
                true,
            )),
        }
    }

    async fn wait_inner(mut self) -> Result<Session, CapabilityError> {
        let mut session_topic: Option<String> = None;
        let mut session_key: Option<[u8; 32]> = None;

        loop {
            let message =
                match tokio::time::timeout(Duration::from_secs(8), self.relay.next()).await {
                    Ok(result) => result?,
                    Err(_) => {
                        if session_topic.is_none() {
                            publish(
                                &mut self.relay,
                                &self.pairing_topic,
                                &self.pairing_key,
                                &self.proposal,
                                1100,
                                true,
                            )
                            .await?;
                        }
                        continue;
                    }
                };
            if message.get("method").and_then(Value::as_str) == Some("irn_subscription") {
                if let Some(id) = message.get("id").cloned() {
                    self.relay
                        .send_raw(json!({"jsonrpc": "2.0", "id": id, "result": true}))
                        .await?;
                }
                let Some(data) = message.get("params").and_then(|params| params.get("data")) else {
                    continue;
                };
                let Some(topic) = data.get("topic").and_then(Value::as_str) else {
                    continue;
                };
                let Some(encrypted) = data.get("message").and_then(Value::as_str) else {
                    continue;
                };
                let key = if topic == self.pairing_topic {
                    &self.pairing_key
                } else if session_topic.as_deref() == Some(topic) {
                    session_key
                        .as_ref()
                        .ok_or_else(|| internal("received session message before key derivation"))?
                } else {
                    continue;
                };
                let decrypted = decrypt(key, encrypted)?;
                let payload: Value = serde_json::from_slice(&decrypted)
                    .map_err(|error| internal(format!("invalid WalletConnect message: {error}")))?;

                if let Some(error) = payload.get("error") {
                    return Err(rpc(format!("wallet rejected the session: {error}")));
                }

                if let Some(responder_public_key) = payload
                    .get("result")
                    .and_then(|result| result.get("responderPublicKey"))
                    .and_then(Value::as_str)
                {
                    let (topic, key) = derive_session(&self.pairing_secret, responder_public_key)?;
                    self.relay
                        .request("irn_subscribe", json!({"topic": topic}))
                        .await?;
                    session_topic = Some(topic);
                    session_key = Some(key);
                    continue;
                }

                if payload.get("method").and_then(Value::as_str) == Some("wc_sessionSettle") {
                    let topic = session_topic
                        .as_deref()
                        .ok_or_else(|| internal("session settled without a session topic"))?;
                    let key = session_key
                        .as_ref()
                        .ok_or_else(|| internal("session settled without a session key"))?;
                    let accounts = session_accounts(&payload)?;
                    let expiry = payload
                        .get("params")
                        .and_then(|params| params.get("expiry"))
                        .and_then(Value::as_i64)
                        .unwrap_or_default();
                    let controller = payload
                        .get("params")
                        .and_then(|params| params.get("controller"));
                    let wallet_name = controller
                        .and_then(|controller| controller.get("metadata"))
                        .and_then(|metadata| metadata.get("name"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let wallet_url = controller
                        .and_then(|controller| controller.get("metadata"))
                        .and_then(|metadata| metadata.get("url"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let wallet_icons = controller
                        .and_then(|controller| controller.get("metadata"))
                        .and_then(|metadata| metadata.get("icons"))
                        .and_then(Value::as_array)
                        .map(|icons| {
                            icons
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    let methods = payload
                        .get("params")
                        .and_then(|params| params.get("namespaces"))
                        .and_then(|namespaces| namespaces.get("eip155"))
                        .and_then(|namespace| namespace.get("methods"))
                        .and_then(Value::as_array)
                        .map(|methods| {
                            methods
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();

                    let ack = json!({
                        "id": payload.get("id").cloned().unwrap_or(Value::Null),
                        "jsonrpc": "2.0",
                        "result": true
                    });
                    publish(&mut self.relay, topic, key, &ack, 1103, false).await?;

                    let session = Session {
                        topic: topic.to_string(),
                        sym_key: hex(key),
                        accounts,
                        chain_id: self.chain_id,
                        expiry,
                        responder_public_key: controller
                            .and_then(|controller| controller.get("publicKey"))
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        wallet_name,
                        wallet_url,
                        wallet_icons,
                        methods,
                        relay_secret: Some(self.relay_secret.clone()),
                    };
                    session.save()?;
                    return Ok(session);
                }
            } else if let Some(error) = message.get("error") {
                return Err(rpc(format!("WalletConnect relay error: {error}")));
            }
        }
    }
}

#[derive(Clone)]
pub struct Manager {
    inner: Arc<Mutex<ManagerState>>,
}

#[derive(Default)]
struct ManagerState {
    pending: Option<Pending>,
}

struct Pending {
    connection_id: String,
    uri: String,
    qr_ascii: String,
    qr_png_base64: Option<String>,
    qr_png_path: Option<String>,
    chain_id: u64,
    expires_at: i64,
    status: String,
    error: Option<String>,
    handle: Option<JoinHandle<()>>,
}

impl Manager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ManagerState::default())),
        }
    }

    pub async fn connect(&self, chain_id: u64) -> Result<WalletConnectResponse, CapabilityError> {
        if let Some(session) = Session::load()?
            && session.expiry > unix_timestamp()
        {
            if session.chain_id != chain_id {
                return Err(CapabilityError::new(
                    CapabilityErrorCode::Unsupported,
                    format!(
                        "active WalletConnect session is eip155:{}; disconnect before connecting to eip155:{chain_id}",
                        session.chain_id
                    ),
                    false,
                ));
            }
            return Ok(connected_response(session));
        }

        {
            let state = self.inner.lock().await;
            if let Some(pending) = &state.pending
                && pending.status == "pending"
                && pending.chain_id == chain_id
                && pending.expires_at > unix_timestamp()
            {
                return Ok(pending_response(pending));
            }
        }

        if let Some(old) = self.inner.lock().await.pending.take() {
            if let Some(path) = &old.qr_png_path {
                let _ = fs::remove_file(path);
            }
            if let Some(handle) = old.handle {
                handle.abort();
            }
        }

        let config = ConnectConfig::from_env()?;
        let pairing =
            tokio::time::timeout(Duration::from_secs(20), Pairing::start(&config, chain_id))
                .await
                .map_err(|_| {
                    CapabilityError::new(
                        CapabilityErrorCode::Timeout,
                        "WalletConnect relay connection timed out",
                        true,
                    )
                })??;
        let connection_id = pairing.uri()[3..35].to_string();
        let uri = pairing.uri().to_string();
        let qr_ascii = pairing.qr_ascii()?;
        let qr_png_bytes = pairing.qr_png_bytes()?;
        let qr_png_base64 = Some(data_encoding::BASE64.encode(&qr_png_bytes));
        let qr_png_path = qr_png_path(&connection_id);
        write_private_bytes(&qr_png_path, &qr_png_bytes)?;
        let qr_png_path = Some(qr_png_path.to_string_lossy().to_string());
        let expires_at = pairing.expires_at();

        let manager = self.clone();
        let task_id = connection_id.clone();
        let handle = tokio::spawn(async move {
            let result = pairing
                .wait(Duration::from_secs(PAIRING_TIMEOUT_SECS))
                .await;
            manager.finish(task_id, result).await;
        });

        let pending = Pending {
            connection_id,
            uri,
            qr_ascii,
            qr_png_base64,
            qr_png_path,
            chain_id,
            expires_at,
            status: "pending".to_string(),
            error: None,
            handle: Some(handle),
        };
        let response = pending_response(&pending);
        self.inner.lock().await.pending = Some(pending);
        Ok(response)
    }

    pub async fn status(&self) -> Result<WalletStatusResponse, CapabilityError> {
        let pending = {
            let state = self.inner.lock().await;
            state.pending.as_ref().map(|pending| PendingStatus {
                connection_id: pending.connection_id.clone(),
                expires_at: pending.expires_at,
                status: pending.status.clone(),
                error: pending.error.clone(),
            })
        };
        if let Some(pending) = pending {
            if pending.status == "pending" && pending.expires_at <= unix_timestamp() {
                return Ok(WalletStatusResponse {
                    status: "expired".to_string(),
                    accounts: Vec::new(),
                    chain_id: None,
                    expires_at: Some(pending.expires_at),
                    connection_id: Some(pending.connection_id),
                    wallet: None,
                    error: None,
                });
            }
            return Ok(WalletStatusResponse {
                status: pending.status,
                accounts: Vec::new(),
                chain_id: None,
                expires_at: Some(pending.expires_at),
                connection_id: Some(pending.connection_id),
                wallet: None,
                error: pending.error,
            });
        }

        let Some(session) = Session::load()? else {
            return Ok(disconnected_status(None));
        };
        Ok(status_from_session(session, None))
    }

    pub async fn wait_status(
        &self,
        wait: Duration,
    ) -> Result<WalletStatusResponse, CapabilityError> {
        if wait.is_zero() {
            return self.status().await;
        }
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let status = self.status().await?;
            if status.status != "pending" || tokio::time::Instant::now() >= deadline {
                return Ok(status);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub async fn disconnect(&self) -> Result<WalletDisconnectResponse, CapabilityError> {
        let pending = {
            let mut state = self.inner.lock().await;
            state.pending.take()
        };
        if let Some(pending) = pending {
            if let Some(path) = &pending.qr_png_path {
                let _ = fs::remove_file(path);
            }
            if let Some(handle) = pending.handle {
                handle.abort();
            }
        }
        disconnect()
    }

    async fn finish(&self, connection_id: String, result: Result<Session, CapabilityError>) {
        let mut state = self.inner.lock().await;
        let Some(pending) = state.pending.as_mut() else {
            return;
        };
        if pending.connection_id != connection_id {
            return;
        }
        match result {
            Ok(_) => {
                if let Some(path) = &pending.qr_png_path {
                    let _ = fs::remove_file(path);
                }
                state.pending = None;
            }
            Err(error) => {
                if let Some(path) = &pending.qr_png_path {
                    let _ = fs::remove_file(path);
                }
                pending.status = "error".to_string();
                pending.error = Some(error.message);
                pending.handle = None;
            }
        }
    }
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
    }
}

struct PendingStatus {
    connection_id: String,
    expires_at: i64,
    status: String,
    error: Option<String>,
}

fn pending_response(pending: &Pending) -> WalletConnectResponse {
    WalletConnectResponse {
        status: "pending".to_string(),
        connection_id: pending.connection_id.clone(),
        uri: pending.uri.clone(),
        qr_ascii: pending.qr_ascii.clone(),
        expires_at: pending.expires_at,
        accounts: Vec::new(),
        chain_id: Some(pending.chain_id),
        qr_png_base64: pending.qr_png_base64.clone(),
        qr_png_path: pending.qr_png_path.clone(),
    }
}

fn connected_response(session: Session) -> WalletConnectResponse {
    WalletConnectResponse {
        status: "connected".to_string(),
        connection_id: session.topic.clone(),
        uri: String::new(),
        qr_ascii: String::new(),
        expires_at: session.expiry,
        accounts: session.accounts,
        chain_id: Some(session.chain_id),
        qr_png_base64: None,
        qr_png_path: None,
    }
}

fn disconnected_status(connection_id: Option<String>) -> WalletStatusResponse {
    WalletStatusResponse {
        status: "disconnected".to_string(),
        accounts: Vec::new(),
        chain_id: None,
        expires_at: None,
        connection_id,
        wallet: None,
        error: None,
    }
}

fn status_from_session(session: Session, error: Option<String>) -> WalletStatusResponse {
    let status = if session.expiry > unix_timestamp() {
        "connected"
    } else {
        "expired"
    };
    WalletStatusResponse {
        status: status.to_string(),
        accounts: session.accounts.clone(),
        chain_id: Some(session.chain_id),
        expires_at: Some(session.expiry),
        connection_id: Some(session.topic.clone()),
        wallet: session.peer(),
        error,
    }
}

fn sign_error(error: &Value) -> CapabilityError {
    if error.get("code").and_then(Value::as_i64) == Some(4001) {
        CapabilityError::new(
            CapabilityErrorCode::UserRejected,
            "wallet rejected the signing request",
            false,
        )
    } else {
        rpc(format!("wallet signing request failed: {error}"))
    }
}

fn session_key(value: &str) -> Result<[u8; 32], CapabilityError> {
    let bytes = unhex(value)?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| internal("invalid WalletConnect session key"))
}

fn validate_typed_data(value: &Value, chain_id: u64) -> Result<String, CapabilityError> {
    let text = serde_json::to_string(value)
        .map_err(|error| invalid(format!("invalid typed data: {error}")))?;
    if text.len() > 64 * 1024 {
        return Err(invalid("typed data is too large"));
    }
    if let Some(domain_chain) = value.get("domain").and_then(|domain| domain.get("chainId")) {
        let parsed = match domain_chain {
            Value::Number(number) => number.as_u64(),
            Value::String(text) => {
                let text = text.trim();
                if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                    u64::from_str_radix(hex, 16).ok()
                } else {
                    text.parse::<u64>().ok()
                }
            }
            _ => None,
        };
        if parsed != Some(chain_id) {
            return Err(invalid(format!(
                "typed data domain.chainId must be {chain_id}"
            )));
        }
    }
    Ok(text)
}

async fn wait_for_response(
    relay: &mut Relay,
    topic: &str,
    key: &[u8; 32],
    request_id: u64,
) -> Result<Value, CapabilityError> {
    loop {
        let message = relay.next().await?;
        if message.get("method").and_then(Value::as_str) == Some("irn_subscription") {
            if let Some(id) = message.get("id").cloned() {
                relay
                    .send_raw(json!({"jsonrpc": "2.0", "id": id, "result": true}))
                    .await?;
            }
            let Some(data) = message.get("params").and_then(|params| params.get("data")) else {
                continue;
            };
            if data.get("topic").and_then(Value::as_str) != Some(topic) {
                continue;
            }
            let Some(encrypted) = data.get("message").and_then(Value::as_str) else {
                continue;
            };
            let payload: Value = serde_json::from_slice(&decrypt(key, encrypted)?)
                .map_err(|error| internal(format!("invalid WalletConnect message: {error}")))?;
            if payload.get("id").and_then(Value::as_u64) != Some(request_id) {
                continue;
            }
            if let Some(error) = payload.get("error") {
                return Err(sign_error(error));
            }
            return payload
                .get("result")
                .cloned()
                .ok_or_else(|| rpc("wallet returned no result"));
        } else if let Some(error) = message.get("error") {
            return Err(rpc(format!("WalletConnect relay error: {error}")));
        }
    }
}

struct Relay {
    write: SocketWrite,
    read: SocketRead,
    next_id: u64,
}

impl Relay {
    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<u64, CapabilityError> {
        let id = self.next_id();
        self.send_raw(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))
        .await?;
        Ok(id)
    }

    async fn send_raw(&mut self, payload: Value) -> Result<(), CapabilityError> {
        self.write
            .send(Message::Text(payload.to_string().into()))
            .await
            .map_err(|error| rpc(format!("WalletConnect relay write failed: {error}")))
    }

    async fn next(&mut self) -> Result<Value, CapabilityError> {
        loop {
            match self.read.next().await {
                Some(Ok(Message::Text(text))) => {
                    return serde_json::from_str(text.as_str()).map_err(|error| {
                        rpc(format!("invalid WalletConnect relay message: {error}"))
                    });
                }
                Some(Ok(Message::Ping(payload))) => {
                    self.write
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|error| {
                            rpc(format!("WalletConnect relay write failed: {error}"))
                        })?;
                }
                Some(Ok(Message::Close(frame))) => {
                    return Err(rpc(format!("WalletConnect relay closed: {frame:?}")));
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => {
                    return Err(rpc(format!("WalletConnect relay error: {error}")));
                }
                None => return Err(rpc("WalletConnect relay disconnected")),
            }
        }
    }
}

async fn wait_for_id(relay: &mut Relay, id: u64) -> Result<(), CapabilityError> {
    loop {
        let message = relay.next().await?;
        if message.get("id").and_then(Value::as_u64) == Some(id) {
            if let Some(error) = message.get("error") {
                return Err(rpc(format!("WalletConnect relay request failed: {error}")));
            }
            return Ok(());
        }
    }
}

async fn publish(
    relay: &mut Relay,
    topic: &str,
    key: &[u8; 32],
    payload: &Value,
    tag: u32,
    prompt: bool,
) -> Result<u64, CapabilityError> {
    let message = encrypt(
        key,
        &serde_json::to_vec(payload).map_err(|error| internal(error.to_string()))?,
    )?;
    relay
        .request(
            "irn_publish",
            json!({
                "topic": topic,
                "message": message,
                "ttl": 300,
                "tag": tag,
                "prompt": prompt
            }),
        )
        .await
}

fn derive_session(
    pairing_secret: &StaticSecret,
    responder_public_key: &str,
) -> Result<(String, [u8; 32]), CapabilityError> {
    let bytes = unhex(responder_public_key)?;
    let peer_bytes: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| invalid("invalid WalletConnect responder public key"))?;
    let peer = PublicKey::from(peer_bytes);
    let shared = pairing_secret.diffie_hellman(&peer);
    let hkdf = Hkdf::<Sha256>::new(None, shared.as_bytes());
    let mut key = [0u8; 32];
    hkdf.expand(&[], &mut key)
        .map_err(|_| internal("WalletConnect session key expansion failed"))?;
    let topic = Sha256::digest(key);
    Ok((hex(&topic), key))
}

fn dedupe_accounts(accounts: &mut Vec<Address>) {
    let mut unique = Vec::with_capacity(accounts.len());
    for account in accounts.drain(..) {
        if !unique.contains(&account) {
            unique.push(account);
        }
    }
    *accounts = unique;
}

fn session_accounts(payload: &Value) -> Result<Vec<Address>, CapabilityError> {
    let accounts = payload
        .get("params")
        .and_then(|params| params.get("namespaces"))
        .and_then(|namespaces| namespaces.get("eip155"))
        .and_then(|namespace| namespace.get("accounts"))
        .and_then(Value::as_array)
        .ok_or_else(|| rpc("WalletConnect session has no eip155 accounts"))?;

    let mut parsed = Vec::new();
    for account in accounts {
        let Some(account) = account.as_str() else {
            continue;
        };
        let Some(address) = account.rsplit(':').next() else {
            continue;
        };
        if let Ok(address) = address.parse::<Address>()
            && !parsed.contains(&address)
        {
            parsed.push(address);
        }
    }
    if parsed.is_empty() {
        return Err(rpc("WalletConnect session returned no usable accounts"));
    }
    Ok(parsed)
}

fn encrypt(key: &[u8; 32], payload: &[u8]) -> Result<String, CapabilityError> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| internal("invalid WalletConnect session key"))?;
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let encrypted = cipher
        .encrypt(&nonce, payload)
        .map_err(|_| internal("WalletConnect encryption failed"))?;
    let mut envelope = Vec::with_capacity(1 + 12 + encrypted.len());
    envelope.push(0);
    envelope.extend_from_slice(&nonce);
    envelope.extend_from_slice(&encrypted);
    Ok(data_encoding::BASE64.encode(&envelope))
}

#[allow(deprecated)]
fn decrypt(key: &[u8; 32], envelope: &str) -> Result<Vec<u8>, CapabilityError> {
    let bytes = data_encoding::BASE64
        .decode(envelope.as_bytes())
        .map_err(|_| rpc("invalid WalletConnect encrypted envelope"))?;
    let (kind, body) = bytes
        .split_first()
        .ok_or_else(|| rpc("empty WalletConnect envelope"))?;
    let body = match kind {
        0 => body,
        1 => body
            .get(32..)
            .ok_or_else(|| rpc("truncated WalletConnect type-1 envelope"))?,
        other => {
            return Err(rpc(format!(
                "unsupported WalletConnect envelope type {other}"
            )));
        }
    };
    if body.len() < 12 {
        return Err(rpc("truncated WalletConnect envelope"));
    }
    let nonce = Nonce::from_slice(&body[..12]);
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| internal("invalid WalletConnect session key"))?;
    cipher
        .decrypt(nonce, &body[12..])
        .map_err(|_| rpc("WalletConnect decryption failed"))
}

fn auth_token(key: &SigningKey, relay_url: &str) -> String {
    let now = unix_timestamp();
    let header = json!({"typ": "JWT", "alg": "EdDSA"});
    let claims = json!({
        "iss": did_key(&key.verifying_key()),
        "sub": "https://github.com/ZhangPeibin/Fluxion",
        "aud": relay_url,
        "iat": now,
        "exp": now + 3600
    });
    let header = base64url(&serde_json::to_vec(&header).expect("JWT header"));
    let claims = base64url(&serde_json::to_vec(&claims).expect("JWT claims"));
    let message = format!("{header}.{claims}");
    let signature = key.sign(message.as_bytes());
    format!("{message}.{}", base64url(&signature.to_bytes()))
}

fn render_qr(uri: &str) -> Result<String, CapabilityError> {
    let code = QrCode::with_error_correction_level(uri.as_bytes(), EcLevel::L)
        .map_err(|error| internal(format!("cannot render WalletConnect QR: {error}")))?;
    let raw = code
        .render::<Dense1x2>()
        .quiet_zone(false)
        .module_dimensions(1, 1)
        .build();
    let lines = raw.lines().collect::<Vec<_>>();
    let width = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or_default();
    let mut out = String::new();
    out.push('\n');
    for line in lines {
        out.push_str("  ");
        out.push_str(line);
        for _ in line.chars().count()..width {
            out.push(' ');
        }
        out.push_str("  ");
        out.push('\n');
    }
    out.push('\n');
    Ok(out)
}

fn qr_png_base64(uri: &str) -> Result<String, CapabilityError> {
    Ok(data_encoding::BASE64.encode(&qr_png(uri)?))
}

fn qr_png(uri: &str) -> Result<Vec<u8>, CapabilityError> {
    let code = QrCode::with_error_correction_level(uri.as_bytes(), EcLevel::L)
        .map_err(|error| internal(format!("cannot render WalletConnect QR: {error}")))?;
    let modules = code.width();
    let colors = code.to_colors();
    let quiet = 4usize;
    let scale = 4usize;
    let side = (modules + quiet * 2) * scale;
    let mut raw = Vec::with_capacity((side + 1) * side);
    for y in 0..side {
        raw.push(0);
        let my = y / scale;
        for x in 0..side {
            let mx = x / scale;
            let dark = mx >= quiet
                && my >= quiet
                && mx < quiet + modules
                && my < quiet + modules
                && colors[(my - quiet) * modules + (mx - quiet)] == Color::Dark;
            raw.push(if dark { 0 } else { 255 });
        }
    }

    let mut png = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(side as u32).to_be_bytes());
    ihdr.extend_from_slice(&(side as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
    write_png_chunk(&mut png, b"IHDR", &ihdr);
    write_png_chunk(&mut png, b"IDAT", &zlib_store(&raw));
    write_png_chunk(&mut png, b"IEND", &[]);
    Ok(png)
}

fn write_png_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    png.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 6);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut offset = 0;
    while offset < data.len() {
        let len = (data.len() - offset).min(65535);
        let last = offset + len == data.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(len as u16).to_le_bytes());
        out.extend_from_slice(&(!(len as u16)).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + len]);
        offset += len;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn did_key(key: &ed25519_dalek::VerifyingKey) -> String {
    let mut bytes = vec![0xed, 0x01];
    bytes.extend_from_slice(key.as_bytes());
    format!("did:key:z{}", bs58::encode(bytes).into_string())
}

fn base64url(bytes: &[u8]) -> String {
    data_encoding::BASE64URL_NOPAD.encode(bytes)
}

fn hex(bytes: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

fn unhex(value: &str) -> Result<Vec<u8>, CapabilityError> {
    data_encoding::HEXLOWER_PERMISSIVE
        .decode(value.as_bytes())
        .map_err(|_| invalid("invalid WalletConnect hex value"))
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn invalid(message: impl Into<String>) -> CapabilityError {
    CapabilityError::invalid_argument(message)
}

fn internal(message: impl Into<String>) -> CapabilityError {
    CapabilityError::internal(message)
}

fn rpc(message: impl Into<String>) -> CapabilityError {
    CapabilityError::new(CapabilityErrorCode::RpcError, message, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypts_and_decrypts_session_payloads() {
        let key = [7u8; 32];
        let payload = br#"{"jsonrpc":"2.0","result":true}"#;
        let encrypted = encrypt(&key, payload).unwrap();
        assert_eq!(decrypt(&key, &encrypted).unwrap(), payload);
    }

    #[test]
    fn parses_eip155_accounts() {
        let payload = json!({
            "params": {
                "namespaces": {
                    "eip155": {
                        "accounts": [
                            "eip155:1:0x5ed10018795d667050e996833c60ff02b1388058",
                            "eip155:1:0x5ed10018795d667050e996833c60ff02b1388058"
                        ]
                    }
                }
            }
        });
        let accounts = session_accounts(&payload).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(
            accounts[0],
            "0x5ED10018795d667050e996833C60fF02b1388058"
                .parse::<Address>()
                .unwrap()
        );
    }

    #[test]
    fn renders_a_compact_square_qr_block() {
        let uri = "wc:93ca62389f414fae145ca1b411ed989d805fd833592238095c3641e557a43119@2?relay-protocol=irn&symKey=1552f4a10b0a3b8a2b116edbb90de4ce25b415d2180e1186ad3d1139dbfeccb8";
        let rendered = render_qr(uri).unwrap();
        let lines = rendered.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 27);
        let qr_lines = lines
            .iter()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(qr_lines.len(), 25);
        assert!(
            qr_lines
                .iter()
                .all(|line| line.chars().count() == qr_lines[0].chars().count())
        );
        assert_eq!(
            qr_lines[0].chars().count(),
            53,
            "QR should be 53 columns wide, got {}",
            qr_lines[0].chars().count()
        );
    }

    #[test]
    fn renders_a_png_qr() {
        let png = qr_png("wc:test").unwrap();
        assert!(png.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
        assert!(png.len() > 100);
    }

    #[test]
    fn validates_typed_data_chain_and_size() {
        let typed = json!({
            "types": {},
            "primaryType": "Person",
            "domain": {"chainId": 1},
            "message": {}
        });
        assert!(validate_typed_data(&typed, 1).is_ok());
        assert!(validate_typed_data(&typed, 56).is_err());
        let hex_chain = json!({"domain": {"chainId": "0x1"}, "message": {}});
        assert!(validate_typed_data(&hex_chain, 1).is_ok());
    }

    #[test]
    fn maps_wallet_rejection_to_user_rejected() {
        let error = sign_error(&json!({"code": 4001, "message": "User rejected"}));
        assert_eq!(error.code, CapabilityErrorCode::UserRejected);
        assert!(!error.retryable);
    }

    #[test]
    fn saves_and_loads_a_session() {
        let path = env::temp_dir().join(format!(
            "fluxion-walletconnect-test-{}.json",
            std::process::id()
        ));
        let session = Session {
            topic: "topic".to_string(),
            sym_key: "key".to_string(),
            accounts: vec![
                "0x5ED10018795d667050e996833C60fF02b1388058"
                    .parse()
                    .unwrap(),
            ],
            chain_id: 1,
            expiry: unix_timestamp() + 60,
            responder_public_key: "responder".to_string(),
            wallet_name: Some("MetaMask".to_string()),
            wallet_url: Some("https://metamask.io".to_string()),
            wallet_icons: Vec::new(),
            methods: vec![],
            relay_secret: Some("00".repeat(32)),
        };
        session.save_to(&path).unwrap();
        let loaded = Session::load_from(&path).unwrap().unwrap();
        assert_eq!(loaded.accounts, session.accounts);
        assert_eq!(loaded.wallet_name.as_deref(), Some("MetaMask"));
        let _ = fs::remove_file(path);
    }
}
