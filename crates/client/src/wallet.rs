//! Local read-only wallet balances and asset discovery.
//!
//! `wallet.balance` is local-only: explicit tokens are resolved and queried
//! against one pinned block. `wallet.assets` may use the 1inch Balance API to
//! discover candidate tokens, but every returned balance is re-read locally.

use std::collections::{HashMap, HashSet};

use alloy_primitives::{Address, B256, U256};
use serde_json::{Value, json};

use crate::capability::{Amount, CapabilityError, TokenRef};
use crate::token;

pub use fluxion_protocol::wallet::{
    AssetSource, CollectibleItem, WalletAllowance, WalletAllowancesResponse,
    WalletBalancesResponse, WalletCollectible, WalletCollectiblesResponse, WalletTokenBalance,
};

const DEFAULT_RPC: &str = "https://eth.drpc.org";
const DEFAULT_ONEINCH_BALANCE: &str = "https://api.1inch.com/balance/v1.2";
const NATIVE: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const BALANCE_OF: &str = "70a08231";
const DECIMALS: &str = "313ce567";
const SYMBOL: &str = "95d89b41";
const ALLOWANCE: &str = "dd62ed3e";
const SUPPORTS_INTERFACE: &str = "01ffc9a7";
const ERC721_INTERFACE: &str = "80ac58cd";
const ERC1155_INTERFACE: &str = "d9b67a26";
const ERC721_ENUMERABLE: &str = "780e9d63";
const TOKEN_OF_OWNER_BY_INDEX: &str = "2f745c59";
const OWNER_OF: &str = "6352211e";
const ERC1155_BALANCE_OF: &str = "00fdd58e";
const NAME: &str = "06fdde03";

#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub rpc_url: String,
    pub oneinch_api_key: Option<String>,
    pub oneinch_balance_url: String,
    pub alchemy_api_key: Option<String>,
    pub alchemy_nft_url: String,
}

impl WalletConfig {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            oneinch_api_key: std::env::var("ONEINCH_API_KEY")
                .ok()
                .filter(|key| !key.trim().is_empty()),
            oneinch_balance_url: std::env::var("FLUXION_ONEINCH_BALANCE_URL")
                .unwrap_or_else(|_| DEFAULT_ONEINCH_BALANCE.to_string()),
            alchemy_api_key: std::env::var("ALCHEMY_API_KEY")
                .ok()
                .filter(|key| !key.trim().is_empty()),
            alchemy_nft_url: std::env::var("FLUXION_ALCHEMY_NFT_URL")
                .unwrap_or_else(|_| "https://eth-mainnet.g.alchemy.com/nft/v3".to_string()),
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("FLUXION_RPC_URL")
                .or_else(|_| std::env::var("RPC_URL"))
                .unwrap_or_else(|_| DEFAULT_RPC.to_string()),
        )
    }
}

#[derive(Debug, Clone)]
pub struct BalanceRequest {
    pub address: Address,
    /// Symbols or addresses. Empty means the built-in registry.
    pub tokens: Vec<String>,
    pub include_native: bool,
    pub block: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AssetsRequest {
    pub address: Address,
    pub include_native: bool,
    pub include_unknown: bool,
    pub block: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct AllowanceRequest {
    pub owner: Address,
    pub spender: Address,
    /// Symbols or addresses. Empty means the built-in registry.
    pub tokens: Vec<String>,
    pub block: Option<u64>,
}

pub async fn balance(
    config: &WalletConfig,
    request: BalanceRequest,
) -> Result<WalletBalancesResponse, CapabilityError> {
    let rpc = Rpc::new(&config.rpc_url);
    let (block_number, block_hash, tag) = rpc.block(request.block).await?;
    let mut tokens = if request.tokens.is_empty() {
        registry_tokens()
    } else {
        resolve_tokens(&rpc, &request.tokens, &tag).await?
    };
    dedupe_tokens(&mut tokens);

    let mut sources = vec![rpc_source(block_number)];
    let warnings = Vec::new();
    let (native, mut balances) =
        verify_tokens(&rpc, request.address, request.include_native, tokens, &tag).await?;
    sort_balances(&mut balances);
    Ok(WalletBalancesResponse {
        address: request.address,
        block_number,
        block_hash,
        native,
        tokens: balances,
        unknown_count: 0,
        coverage: "local".to_string(),
        sources: std::mem::take(&mut sources),
        warnings,
    })
}

pub async fn assets(
    config: &WalletConfig,
    request: AssetsRequest,
) -> Result<WalletBalancesResponse, CapabilityError> {
    let rpc = Rpc::new(&config.rpc_url);
    let (block_number, block_hash, tag) = rpc.block(request.block).await?;
    let mut warnings = Vec::new();
    let mut sources = vec![rpc_source(block_number)];

    let discovery = discover_oneinch(config, request.address).await;
    let (mut known, unknown, native_seen, coverage) = match discovery {
        Ok((known, unknown, native_seen)) => {
            sources.push(AssetSource {
                name: "1inch-balance".to_string(),
                scope: "candidate-discovery".to_string(),
                block: None,
            });
            (known, unknown, native_seen, "provider+local")
        }
        Err(error) => {
            warnings.push(format!(
                "asset discovery unavailable; returned built-in registry tokens only: {}",
                error.message
            ));
            (registry_tokens(), HashMap::new(), None, "registry+local")
        }
    };
    dedupe_tokens(&mut known);

    let mut unknown_count = unknown.len();
    if request.include_unknown && !unknown.is_empty() {
        let addresses = unknown.keys().copied().collect::<Vec<_>>();
        let resolved = resolve_unknown_tokens(&rpc, &addresses, &tag).await;
        for (address, token) in resolved {
            match token {
                Ok(token) => known.push(token),
                Err(error) => warnings.push(format!(
                    "skipped unknown token {address}: {}",
                    error.message
                )),
            }
        }
        unknown_count = 0;
    }
    let _ = native_seen; // The provider only identifies candidates; native is re-read locally.
    let (native, mut balances) =
        verify_tokens(&rpc, request.address, request.include_native, known, &tag).await?;
    sort_balances(&mut balances);
    Ok(WalletBalancesResponse {
        address: request.address,
        block_number,
        block_hash,
        native,
        tokens: balances,
        unknown_count,
        coverage: coverage.to_string(),
        sources,
        warnings,
    })
}

pub async fn allowance(
    config: &WalletConfig,
    request: AllowanceRequest,
) -> Result<WalletAllowancesResponse, CapabilityError> {
    let rpc = Rpc::new(&config.rpc_url);
    let (block_number, block_hash, tag) = rpc.block(request.block).await?;
    let mut tokens = if request.tokens.is_empty() {
        registry_tokens()
    } else {
        resolve_tokens(&rpc, &request.tokens, &tag).await?
    };
    dedupe_tokens(&mut tokens);
    let mut calls = Vec::with_capacity(tokens.len());
    for token in &tokens {
        calls.push((
            "eth_call".to_string(),
            json!([
                {
                    "to": token.address,
                    "data": format!(
                        "0x{ALLOWANCE}{:0>64}{:0>64}",
                        hex_address(request.owner),
                        hex_address(request.spender)
                    )
                },
                tag
            ]),
        ));
    }
    let results = rpc.batch_or_individual(calls).await?;
    let mut warnings = Vec::new();
    let mut allowances = Vec::new();
    for (token, result) in tokens.into_iter().zip(results) {
        let raw = match result {
            Ok(value) => value.as_str().and_then(decode_hex_u256),
            Err(error) => {
                warnings.push(format!(
                    "allowance read failed for {}: {}",
                    token.address, error.message
                ));
                None
            }
        };
        let Some(raw) = raw else {
            continue;
        };
        allowances.push(WalletAllowance {
            known: known_token(token.address).is_some(),
            amount: Amount::from_raw(raw, token.decimals),
            token,
            spender: request.spender,
        });
    }
    allowances.sort_by(|a, b| {
        b.known
            .cmp(&a.known)
            .then_with(|| a.token.symbol.cmp(&b.token.symbol))
            .then_with(|| a.token.address.cmp(&b.token.address))
    });
    Ok(WalletAllowancesResponse {
        owner: request.owner,
        spender: request.spender,
        block_number,
        block_hash,
        allowances,
        coverage: "local".to_string(),
        sources: vec![AssetSource {
            name: "ethereum-rpc".to_string(),
            scope: "verified-allowances".to_string(),
            block: Some(block_number),
        }],
        warnings,
    })
}

#[derive(Debug, Clone)]
pub struct CollectiblesRequest {
    pub owner: Address,
    /// Explicit ERC721 collection addresses. Empty means the curated registry
    /// (or Alchemy discovery when configured).
    pub collections: Vec<Address>,
    pub include_empty: bool,
    pub block: Option<u64>,
}

const CURATED_COLLECTIONS: &[&str] = &[
    "0xBC4CA0EdA7647A8aB7C2061c2E118A18a936f13D", // BAYC
    "0x60E4d786628Fea6478F785A6d7e704777c86a7c6", // MAYC
    "0xED5AF388653567Af2F388E6224dC7C4b3241C544", // Azuki
    "0xBd3531dA5CF5857e7CfAA92426877b022e612cf8", // Pudgy Penguins
    "0x8a90CAb2b38dba80c64b7734e58Ee1dB38B8992e", // Doodles
    "0x5Af0D9827E0c53E4799BB226655A1de152A425a5", // Milady
    "0x57f1887a8BF19b14fC0dF6Fd9B2acc9Af147eA85", // ENS
    "0xb47e3cd837dDF8e4c57F05d70Ab865de6e193BBB", // Wrapped CryptoPunks
    "0x23581767a106ae21c074b2276D25e5C3e136a68b", // Moonbirds
    "0x34d85c9CDeB23FA97cb08333b511ac86E1C4E258", // Otherdeed
];

pub async fn collectibles(
    config: &WalletConfig,
    request: CollectiblesRequest,
) -> Result<WalletCollectiblesResponse, CapabilityError> {
    let rpc = Rpc::new(&config.rpc_url);
    let (block_number, block_hash, tag) = rpc.block(request.block).await?;
    let mut sources = vec![rpc_source(block_number)];
    let mut warnings = Vec::new();
    let (collections, coverage) = if request.collections.is_empty() {
        if let Some(key) = config.alchemy_api_key.as_deref() {
            match discover_alchemy(config, key, request.owner).await {
                Ok(mut collections) => {
                    sources.push(AssetSource {
                        name: "alchemy-nft".to_string(),
                        scope: "candidate-discovery".to_string(),
                        block: None,
                    });
                    verify_alchemy_collectibles(
                        &rpc,
                        request.owner,
                        &mut collections,
                        &tag,
                        &mut warnings,
                    )
                    .await?;
                    (collections, "provider+local")
                }
                Err(error) => {
                    warnings.push(format!(
                        "NFT discovery unavailable; falling back to curated registry: {}",
                        error.message
                    ));
                    let collections = curated_collectibles();
                    let collections = load_many_collections(
                        &rpc,
                        request.owner,
                        &collections,
                        request.include_empty,
                        &tag,
                        &mut warnings,
                    )
                    .await?;
                    (collections, "registry+local")
                }
            }
        } else {
            let collections = curated_collectibles();
            let collections = load_many_collections(
                &rpc,
                request.owner,
                &collections,
                request.include_empty,
                &tag,
                &mut warnings,
            )
            .await?;
            warnings.push(
                "no NFT indexer configured; searched the curated collection registry only"
                    .to_string(),
            );
            (collections, "registry+local")
        }
    } else {
        let collections = load_many_collections(
            &rpc,
            request.owner,
            &request.collections,
            request.include_empty,
            &tag,
            &mut warnings,
        )
        .await?;
        (collections, "local")
    };
    Ok(WalletCollectiblesResponse {
        owner: request.owner,
        block_number,
        block_hash,
        collections,
        coverage: coverage.to_string(),
        sources,
        warnings,
    })
}

fn curated_collectibles() -> Vec<Address> {
    CURATED_COLLECTIONS
        .iter()
        .filter_map(|value| value.parse().ok())
        .collect()
}

async fn load_many_collections(
    rpc: &Rpc,
    owner: Address,
    collections: &[Address],
    include_empty: bool,
    tag: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<WalletCollectible>, CapabilityError> {
    let mut out = Vec::new();
    for &collection in collections {
        match load_collection(rpc, owner, collection, include_empty, tag, warnings).await {
            Ok(Some(value)) => out.push(value),
            Ok(None) => {}
            Err(error) => warnings.push(format!(
                "skipped collection {collection}: {}",
                error.message
            )),
        }
    }
    Ok(out)
}

async fn load_collection(
    rpc: &Rpc,
    owner: Address,
    collection: Address,
    include_empty: bool,
    tag: &str,
    warnings: &mut Vec<String>,
) -> Result<Option<WalletCollectible>, CapabilityError> {
    let erc721 = supports_interface(rpc, collection, ERC721_INTERFACE, tag)
        .await
        .unwrap_or(false);
    if !erc721 {
        let erc1155 = supports_interface(rpc, collection, ERC1155_INTERFACE, tag)
            .await
            .unwrap_or(false);
        if erc1155 {
            warnings.push(format!(
                "{collection} is ERC1155; on-chain fallback cannot enumerate token ids without an indexer"
            ));
        } else {
            warnings.push(format!("{collection} is not an ERC721/ERC1155 contract"));
        }
        return Ok(None);
    }
    let balance = token_balance(rpc, collection, owner, tag).await?;
    if balance.is_zero() && !include_empty {
        return Ok(None);
    }
    let enumerable = supports_interface(rpc, collection, ERC721_ENUMERABLE, tag)
        .await
        .unwrap_or(false);
    let name = read_token_string(rpc, collection, NAME, tag).await;
    let symbol = read_token_string(rpc, collection, SYMBOL, tag).await;
    let count = usize::try_from(balance).unwrap_or(usize::MAX);
    let mut items = Vec::new();
    if enumerable {
        let limit = count.min(200);
        let mut calls = Vec::with_capacity(limit);
        for index in 0..limit {
            calls.push((
                "eth_call".to_string(),
                json!([
                    {
                        "to": collection,
                        "data": format!(
                            "0x{TOKEN_OF_OWNER_BY_INDEX}{:0>64}{:0>64}",
                            hex_address(owner),
                            format!("{index:x}")
                        )
                    },
                    tag
                ]),
            ));
        }
        let results = rpc.batch_or_individual(calls).await?;
        for result in results {
            let token_id = result
                .ok()
                .and_then(|value| value.as_str().and_then(decode_hex_u256))
                .map(|value| value.to_string());
            if let Some(token_id) = token_id {
                items.push(CollectibleItem {
                    token_id,
                    balance: "1".to_string(),
                });
            }
        }
    } else if !balance.is_zero() {
        warnings.push(format!(
            "{collection} is ERC721 but not Enumerable; only the count is available"
        ));
    }
    Ok(Some(WalletCollectible {
        collection,
        name,
        symbol,
        standard: "erc721".to_string(),
        enumerable,
        item_count: count,
        items,
    }))
}

async fn supports_interface(
    rpc: &Rpc,
    collection: Address,
    interface: &str,
    tag: &str,
) -> Result<bool, CapabilityError> {
    let data = format!("0x{SUPPORTS_INTERFACE}{interface}{}", "0".repeat(56));
    let value = rpc.eth_call_data(collection, &data, tag).await?;
    Ok(decode_uint(&value).is_some_and(|value| !value.is_zero()))
}

async fn token_balance(
    rpc: &Rpc,
    collection: Address,
    owner: Address,
    tag: &str,
) -> Result<U256, CapabilityError> {
    let data = format!("0x{BALANCE_OF}{:0>64}", hex_address(owner));
    let value = rpc.eth_call_data(collection, &data, tag).await?;
    decode_uint(&value).ok_or_else(|| {
        CapabilityError::new(
            crate::capability::CapabilityErrorCode::RpcError,
            format!("cannot read ERC721 balance for {collection}"),
            true,
        )
    })
}

async fn read_token_string(rpc: &Rpc, token: Address, selector: &str, tag: &str) -> Option<String> {
    rpc.eth_call_data(token, &format!("0x{selector}"), tag)
        .await
        .ok()
        .and_then(|value| decode_string(&value))
}

async fn discover_alchemy(
    config: &WalletConfig,
    key: &str,
    owner: Address,
) -> Result<Vec<WalletCollectible>, CapabilityError> {
    let base = config.alchemy_nft_url.trim_end_matches('/');
    let mut page_key: Option<String> = None;
    let mut collections: Vec<WalletCollectible> = Vec::new();
    for _ in 0..5 {
        let mut url =
            format!("{base}/{key}/getNFTsForOwner?owner={owner}&withMetadata=false&pageSize=100");
        if let Some(page_key) = &page_key {
            url.push_str("&pageKey=");
            url.push_str(page_key);
        }
        let response = reqwest::Client::new()
            .get(url)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(|error| {
                CapabilityError::new(
                    crate::capability::CapabilityErrorCode::RpcError,
                    format!("Alchemy NFT request failed: {error}"),
                    true,
                )
            })?;
        if !response.status().is_success() {
            return Err(CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                format!("Alchemy NFT returned HTTP {}", response.status()),
                true,
            ));
        }
        let value: Value = response.json().await.map_err(|error| {
            CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                format!("Alchemy NFT response was invalid: {error}"),
                true,
            )
        })?;
        let Some(nfts) = value.get("ownedNfts").and_then(Value::as_array) else {
            return Err(CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                "Alchemy NFT response has no ownedNfts",
                true,
            ));
        };
        for nft in nfts {
            let Some(address) = nft
                .get("contract")
                .and_then(|contract| contract.get("address"))
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<Address>().ok())
            else {
                continue;
            };
            let token_id = nft
                .get("tokenId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if token_id.is_empty() {
                continue;
            }
            let standard = match nft.get("tokenType").and_then(Value::as_str) {
                Some("ERC1155") => "erc1155",
                _ => "erc721",
            };
            let balance = nft
                .get("balance")
                .and_then(Value::as_str)
                .unwrap_or("1")
                .to_string();
            let index = collections
                .iter()
                .position(|item| item.collection == address);
            let item = CollectibleItem { token_id, balance };
            if let Some(index) = index {
                collections[index].items.push(item);
                collections[index].item_count = collections[index].items.len();
                if collections[index].standard != standard {
                    collections[index].standard = standard.to_string();
                }
            } else {
                collections.push(WalletCollectible {
                    collection: address,
                    name: nft
                        .get("contract")
                        .and_then(|contract| contract.get("name"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    symbol: nft
                        .get("contract")
                        .and_then(|contract| contract.get("symbol"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    standard: standard.to_string(),
                    enumerable: false,
                    item_count: 1,
                    items: vec![item],
                });
            }
        }
        page_key = value
            .get("pageKey")
            .and_then(Value::as_str)
            .map(str::to_string);
        if page_key.is_none() {
            break;
        }
    }
    Ok(collections)
}

async fn verify_alchemy_collectibles(
    rpc: &Rpc,
    owner: Address,
    collections: &mut Vec<WalletCollectible>,
    tag: &str,
    warnings: &mut Vec<String>,
) -> Result<(), CapabilityError> {
    let mut total = 0usize;
    for collection in collections.iter_mut() {
        total += collection.items.len();
        if total > 500 {
            collection
                .items
                .truncate(collection.items.len().saturating_sub(total - 500));
            warnings.push("collectible verification capped at 500 items".to_string());
        }
        for item in collection.items.iter_mut() {
            let token_id = U256::from_str_radix(&item.token_id, 10).ok();
            let Some(token_id) = token_id else {
                item.balance = "0".to_string();
                continue;
            };
            let data = if collection.standard == "erc1155" {
                format!(
                    "0x{ERC1155_BALANCE_OF}{:0>64}{:0>64}",
                    hex_address(owner),
                    format!("{token_id:x}")
                )
            } else {
                format!("0x{OWNER_OF}{:0>64}", format!("{token_id:x}"))
            };
            let value = rpc.eth_call_data(collection.collection, &data, tag).await;
            match value {
                Ok(value) if collection.standard == "erc1155" => {
                    let balance = decode_uint(&value).unwrap_or(U256::ZERO);
                    item.balance = balance.to_string();
                }
                Ok(value) => {
                    let matches = value.len() >= 32
                        && Address::from_slice(&value[value.len() - 20..]) == owner;
                    if !matches {
                        item.balance = "0".to_string();
                    }
                }
                Err(error) => {
                    warnings.push(format!(
                        "verification failed for {} #{}: {}",
                        collection.collection, item.token_id, error.message
                    ));
                    item.balance = "0".to_string();
                }
            }
        }
        collection.items.retain(|item| item.balance != "0");
        collection.item_count = collection.items.len();
        // Provider-discovered token IDs are explicit, even for contracts that
        // do not implement ERC721Enumerable.
        collection.enumerable = true;
    }
    collections.retain(|collection| !collection.items.is_empty());
    Ok(())
}

fn rpc_source(block: u64) -> AssetSource {
    AssetSource {
        name: "ethereum-rpc".to_string(),
        scope: "verified-balances".to_string(),
        block: Some(block),
    }
}

fn registry_tokens() -> Vec<TokenRef> {
    token::list()
        .iter()
        .filter_map(|token| {
            token.address.parse::<Address>().ok().map(|address| {
                TokenRef::new(Some(token.symbol.to_string()), address, token.decimals)
            })
        })
        .collect()
}

fn known_token(address: Address) -> Option<TokenRef> {
    token::list().iter().find_map(|token| {
        (token.address.parse::<Address>().ok() == Some(address))
            .then(|| TokenRef::new(Some(token.symbol.to_string()), address, token.decimals))
    })
}

fn dedupe_tokens(tokens: &mut Vec<TokenRef>) {
    let mut seen = HashSet::new();
    tokens.retain(|token| seen.insert(token.address));
}

async fn resolve_tokens(
    rpc: &Rpc,
    inputs: &[String],
    tag: &str,
) -> Result<Vec<TokenRef>, CapabilityError> {
    let mut tokens = Vec::new();
    for input in inputs {
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if input.eq_ignore_ascii_case("ETH") {
            continue; // Native is controlled by include_native.
        }
        if input.starts_with("0x") || input.starts_with("0X") {
            let address = input.parse::<Address>().map_err(|_| {
                CapabilityError::invalid_argument(format!("invalid token address: {input}"))
            })?;
            if let Some(known) = known_token(address) {
                tokens.push(known);
            } else {
                let (decimals, symbol) = token_metadata(rpc, address, tag).await?;
                tokens.push(TokenRef::new(symbol, address, decimals));
            }
        } else {
            let known = token::resolve(input, None)?;
            tokens.push(known);
        }
    }
    Ok(tokens)
}

async fn resolve_unknown_tokens(
    rpc: &Rpc,
    addresses: &[Address],
    tag: &str,
) -> Vec<(Address, Result<TokenRef, CapabilityError>)> {
    let mut out = Vec::new();
    for &address in addresses {
        let result = token_metadata(rpc, address, tag)
            .await
            .map(|(decimals, symbol)| TokenRef::new(symbol, address, decimals));
        out.push((address, result));
    }
    out
}

async fn token_metadata(
    rpc: &Rpc,
    address: Address,
    tag: &str,
) -> Result<(u8, Option<String>), CapabilityError> {
    let data = rpc.eth_call(address, DECIMALS, tag).await?;
    let decimals = decode_uint(&data).ok_or_else(|| {
        CapabilityError::new(
            crate::capability::CapabilityErrorCode::RpcError,
            format!("cannot read decimals for token {address}"),
            false,
        )
    })?;
    if decimals > U256::from(u8::MAX) {
        return Err(CapabilityError::invalid_argument(format!(
            "token {address} has invalid decimals"
        )));
    }
    let symbol = rpc
        .eth_call(address, SYMBOL, tag)
        .await
        .ok()
        .and_then(|data| decode_string(&data));
    Ok((decimals.to::<u8>(), symbol))
}

async fn verify_tokens(
    rpc: &Rpc,
    address: Address,
    include_native: bool,
    tokens: Vec<TokenRef>,
    tag: &str,
) -> Result<(Option<WalletTokenBalance>, Vec<WalletTokenBalance>), CapabilityError> {
    let mut calls = Vec::with_capacity(tokens.len() + usize::from(include_native));
    if include_native {
        calls.push(("eth_getBalance".to_string(), json!([address, tag])));
    }
    for token in &tokens {
        calls.push((
            "eth_call".to_string(),
            json!([{ "to": token.address, "data": format!("0x{BALANCE_OF}{:0>64}", hex_address(address)) }, tag]),
        ));
    }
    let results = rpc.batch_or_individual(calls).await?;
    let mut index = 0;
    let native = if include_native {
        let result = results.get(index).cloned().transpose()?;
        index += 1;
        result
            .and_then(|value| value.as_str().and_then(decode_hex_u256))
            .map(|raw| WalletTokenBalance {
                token: TokenRef::new(Some("ETH".to_string()), native_address(), 18),
                amount: Amount::from_raw(raw, 18),
                known: true,
            })
    } else {
        None
    };
    let mut balances = Vec::new();
    for token in tokens {
        let result = results.get(index).cloned().transpose()?;
        index += 1;
        let Some(raw) = result.and_then(|value| value.as_str().and_then(decode_hex_u256)) else {
            continue;
        };
        if raw.is_zero() {
            continue;
        }
        balances.push(WalletTokenBalance {
            known: known_token(token.address).is_some(),
            amount: Amount::from_raw(raw, token.decimals),
            token,
        });
    }
    Ok((native, balances))
}

fn sort_balances(balances: &mut [WalletTokenBalance]) {
    balances.sort_by(|a, b| {
        b.known
            .cmp(&a.known)
            .then_with(|| a.token.symbol.cmp(&b.token.symbol))
            .then_with(|| a.token.address.cmp(&b.token.address))
    });
}

fn native_address() -> Address {
    NATIVE.parse().unwrap()
}

fn hex_address(address: Address) -> String {
    let text = format!("{address:#x}");
    text.trim_start_matches("0x").to_string()
}

async fn discover_oneinch(
    config: &WalletConfig,
    address: Address,
) -> Result<(Vec<TokenRef>, HashMap<Address, U256>, Option<U256>), CapabilityError> {
    let key = config.oneinch_api_key.as_deref().ok_or_else(|| {
        CapabilityError::new(
            crate::capability::CapabilityErrorCode::Unsupported,
            "ONEINCH_API_KEY is not configured",
            false,
        )
    })?;
    let url = format!(
        "{}/1/balances/{}",
        config.oneinch_balance_url.trim_end_matches('/'),
        address
    );
    let response = reqwest::Client::new()
        .get(url)
        .bearer_auth(key)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|error| {
            CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                format!("1inch balance request failed: {error}"),
                true,
            )
        })?;
    if !response.status().is_success() {
        return Err(CapabilityError::new(
            crate::capability::CapabilityErrorCode::RpcError,
            format!("1inch balance returned HTTP {}", response.status()),
            true,
        ));
    }
    let values: HashMap<String, String> = response.json().await.map_err(|error| {
        CapabilityError::new(
            crate::capability::CapabilityErrorCode::RpcError,
            format!("1inch balance response was not a balance map: {error}"),
            true,
        )
    })?;
    let mut known = Vec::new();
    let mut unknown = HashMap::new();
    let mut native = None;
    for (raw_address, raw_balance) in values {
        let Ok(balance) = U256::from_str_radix(&raw_balance, 10) else {
            continue;
        };
        if balance.is_zero() {
            continue;
        }
        let Ok(token_address) = raw_address.parse::<Address>() else {
            continue;
        };
        if raw_address.eq_ignore_ascii_case(NATIVE) {
            native = Some(balance);
        } else if let Some(token) = known_token(token_address) {
            known.push(token);
        } else {
            unknown.insert(token_address, balance);
        }
    }
    Ok((known, unknown, native))
}

fn decode_hex_u256(value: &str) -> Option<U256> {
    U256::from_str_radix(value.trim_start_matches("0x"), 16).ok()
}

fn decode_uint(bytes: &[u8]) -> Option<U256> {
    (bytes.len() >= 32).then(|| U256::from_be_slice(&bytes[..32]))
}

fn decode_string(bytes: &[u8]) -> Option<String> {
    if bytes.len() == 32 {
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(32);
        return String::from_utf8(bytes[..end].to_vec()).ok();
    }
    if bytes.len() < 64 {
        return None;
    }
    let offset = U256::from_be_slice(&bytes[..32]).to::<usize>();
    let length = U256::from_be_slice(bytes.get(offset..offset + 32)?).to::<usize>();
    let data = bytes.get(offset + 32..offset + 32 + length)?;
    String::from_utf8(data.to_vec()).ok()
}

struct Rpc {
    client: reqwest::Client,
    url: String,
}

impl Rpc {
    fn new(url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
        }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, CapabilityError> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(rpc_error)?;
        let value: Value = response.json().await.map_err(rpc_error)?;
        if let Some(error) = value.get("error") {
            return Err(CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                error.to_string(),
                true,
            ));
        }
        value.get("result").cloned().ok_or_else(|| {
            CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                "JSON-RPC response has no result",
                true,
            )
        })
    }

    async fn batch(
        &self,
        calls: Vec<(String, Value)>,
    ) -> Result<Vec<Result<Value, CapabilityError>>, CapabilityError> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }
        let requests = calls
            .into_iter()
            .enumerate()
            .map(|(idx, (method, params))| {
                json!({
                    "jsonrpc": "2.0",
                    "id": idx,
                    "method": method,
                    "params": params,
                })
            })
            .collect::<Vec<_>>();
        let response = self
            .client
            .post(&self.url)
            .json(&requests)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map_err(rpc_error)?;
        let value: Value = response.json().await.map_err(rpc_error)?;
        let items = value.as_array().ok_or_else(|| {
            CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                "JSON-RPC batch response was not an array",
                true,
            )
        })?;
        let mut results = vec![None; requests.len()];
        for item in items {
            let Some(index) = item.get("id").and_then(Value::as_u64).map(|id| id as usize) else {
                continue;
            };
            if index >= results.len() {
                continue;
            }
            let result = if let Some(error) = item.get("error") {
                Err(CapabilityError::new(
                    crate::capability::CapabilityErrorCode::RpcError,
                    error.to_string(),
                    true,
                ))
            } else {
                item.get("result").cloned().ok_or_else(|| {
                    CapabilityError::new(
                        crate::capability::CapabilityErrorCode::RpcError,
                        "JSON-RPC batch item has no result",
                        true,
                    )
                })
            };
            results[index] = Some(result);
        }
        Ok(results
            .into_iter()
            .map(|result| {
                result.unwrap_or_else(|| {
                    Err(CapabilityError::new(
                        crate::capability::CapabilityErrorCode::RpcError,
                        "JSON-RPC batch response missing item",
                        true,
                    ))
                })
            })
            .collect())
    }

    async fn batch_or_individual(
        &self,
        calls: Vec<(String, Value)>,
    ) -> Result<Vec<Result<Value, CapabilityError>>, CapabilityError> {
        match self.batch(calls.clone()).await {
            Ok(results) if results.iter().all(Result::is_ok) => Ok(results),
            _ => {
                let mut results = Vec::with_capacity(calls.len());
                for (method, params) in calls {
                    results.push(self.call(&method, params).await);
                }
                Ok(results)
            }
        }
    }

    async fn eth_call(
        &self,
        to: Address,
        selector: &str,
        tag: &str,
    ) -> Result<Vec<u8>, CapabilityError> {
        self.eth_call_data(to, &format!("0x{selector}"), tag).await
    }

    async fn eth_call_data(
        &self,
        to: Address,
        data: &str,
        tag: &str,
    ) -> Result<Vec<u8>, CapabilityError> {
        let value = self
            .call("eth_call", json!([{ "to": to, "data": data }, tag]))
            .await?;
        let Some(text) = value.as_str() else {
            return Err(CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                "eth_call returned a non-string result",
                true,
            ));
        };
        let text = text.trim_start_matches("0x");
        if !text.len().is_multiple_of(2) {
            return Err(CapabilityError::new(
                crate::capability::CapabilityErrorCode::RpcError,
                "eth_call returned invalid hex",
                true,
            ));
        }
        let mut bytes = Vec::with_capacity(text.len() / 2);
        for index in (0..text.len()).step_by(2) {
            let byte = u8::from_str_radix(&text[index..index + 2], 16).map_err(|_| {
                CapabilityError::new(
                    crate::capability::CapabilityErrorCode::RpcError,
                    "eth_call returned invalid hex",
                    true,
                )
            })?;
            bytes.push(byte);
        }
        Ok(bytes)
    }

    async fn block(&self, requested: Option<u64>) -> Result<(u64, B256, String), CapabilityError> {
        let number = match requested {
            Some(number) => number,
            None => {
                let value = self.call("eth_blockNumber", json!([])).await?;
                value
                    .as_str()
                    .and_then(decode_hex_u256)
                    .and_then(|value| u64::try_from(value).ok())
                    .ok_or_else(|| {
                        CapabilityError::new(
                            crate::capability::CapabilityErrorCode::RpcError,
                            "eth_blockNumber returned an invalid block",
                            true,
                        )
                    })?
            }
        };
        let tag = format!("0x{number:x}");
        let value = self
            .call("eth_getBlockByNumber", json!([tag, false]))
            .await?;
        let hash = value
            .get("hash")
            .and_then(Value::as_str)
            .and_then(|hash| hash.parse::<B256>().ok())
            .ok_or_else(|| {
                CapabilityError::new(
                    crate::capability::CapabilityErrorCode::RpcError,
                    "cannot read block hash",
                    true,
                )
            })?;
        Ok((number, hash, tag))
    }
}

fn rpc_error(error: reqwest::Error) -> CapabilityError {
    CapabilityError::new(
        crate::capability::CapabilityErrorCode::RpcError,
        format!("local RPC request failed: {error}"),
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_erc20_string_and_bytes32_symbols() {
        let mut abi = vec![0u8; 64];
        abi[31] = 32;
        abi[63] = 4;
        abi.extend_from_slice(b"USDC");
        assert_eq!(decode_string(&abi).as_deref(), Some("USDC"));

        let mut bytes32 = vec![0u8; 32];
        bytes32[..3].copy_from_slice(b"DAI");
        assert_eq!(decode_string(&bytes32).as_deref(), Some("DAI"));
    }

    #[test]
    fn parses_decimal_balance_map_without_float() {
        assert_eq!(
            decode_hex_u256("0xde0b6b3a7640000"),
            Some(U256::from(1_000_000_000_000_000_000u128))
        );
        assert_eq!(decode_hex_u256("0x0"), Some(U256::ZERO));
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::extract::Json;
    use axum::routing::{get, post};
    use axum::{Router, http::StatusCode};
    use serde_json::json;

    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const LDO: &str = "0x5a98fcbea516cf06857215779fd812ca3bef1b32";
    const UNKNOWN: &str = "0x0000000000000000000000000000000000000003";
    const NATIVE: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    const COLLECTION: &str = "0x0000000000000000000000000000000000000010";
    const PROVIDER_COLLECTION: &str = "0x0000000000000000000000000000000000000020";

    fn word(value: U256) -> String {
        format!("0x{value:064x}")
    }

    fn abi_string(value: &str) -> String {
        let mut bytes = Vec::new();
        let mut word = [0u8; 32];
        word[31] = 32;
        bytes.extend_from_slice(&word);
        let mut length = [0u8; 32];
        let len = value.len();
        length[24..].copy_from_slice(&(len as u64).to_be_bytes());
        bytes.extend_from_slice(&length);
        let mut data = vec![0u8; 32];
        data[..len].copy_from_slice(value.as_bytes());
        bytes.extend_from_slice(&data);
        format!(
            "0x{}",
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }

    async fn rpc(Json(body): Json<Value>) -> Json<Value> {
        let is_batch = body.is_array();
        let requests = body.as_array().cloned().unwrap_or_else(|| vec![body]);
        let items = requests
            .iter()
            .map(|request| {
                let id = request.get("id").cloned().unwrap_or(Value::Null);
                let method = request["method"].as_str().unwrap_or_default();
                let result = match method {
                    "eth_blockNumber" => json!("0x64"),
                    "eth_getBlockByNumber" => json!({
                        "number": "0x64",
                        "hash": "0x0000000000000000000000000000000000000000000000000000000000000064"
                    }),
                    "eth_getBalance" => json!(word(U256::from(1_000_000_000_000_000_000u128))),
                    "eth_call" => {
                        let call = &request["params"][0];
                        let to = call["to"].as_str().unwrap_or_default().to_ascii_lowercase();
                        let data = call["data"].as_str().unwrap_or_default();
                        if data.starts_with("0x313ce567") {
                            if to == UNKNOWN.to_ascii_lowercase() {
                                json!(word(U256::from(18u64)))
                            } else if to == USDC.to_ascii_lowercase() {
                                json!(word(U256::from(6u64)))
                            } else {
                                json!(word(U256::from(18u64)))
                            }
                        } else if data.starts_with("0x95d89b41") {
                            if to == UNKNOWN.to_ascii_lowercase() {
                                json!(abi_string("TEST"))
                            } else if to == USDC.to_ascii_lowercase() {
                                json!(abi_string("USDC"))
                            } else if to == COLLECTION.to_ascii_lowercase()
                                || to == PROVIDER_COLLECTION.to_ascii_lowercase()
                            {
                                json!(abi_string("NFT"))
                            } else {
                                json!(abi_string("TOKEN"))
                            }
                        } else if data.starts_with("0x01ffc9a7") {
                            json!(word(U256::from(1u64)))
                        } else if data.starts_with("0x2f745c59") {
                            let index = U256::from_str_radix(
                                data.rsplit_once(
                                    "000000000000000000000000000000000000000000000000000000000000",
                                )
                                .map(|(_, tail)| tail)
                                .unwrap_or("0"),
                                16,
                            )
                            .unwrap_or(U256::ZERO);
                            json!(word(index + U256::from(1u64)))
                        } else if data.starts_with("0x6352211e") {
                            json!(format!(
                                "0x{:0>64}",
                                "0000000000000000000000000000000000000001"
                            ))
                        } else if data.starts_with("0x00fdd58e") {
                            json!(word(U256::from(1u64)))
                        } else if data.starts_with("0x06fdde03") {
                            json!(abi_string("Collection"))
                        } else if data.starts_with("0xdd62ed3e") {
                            let amount = if to == USDC.to_ascii_lowercase() {
                                U256::from(1_000_000_000u64)
                            } else {
                                U256::ZERO
                            };
                            json!(word(amount))
                        } else if data.starts_with("0x70a08231") {
                            let amount = if to == COLLECTION.to_ascii_lowercase()
                                || to == PROVIDER_COLLECTION.to_ascii_lowercase()
                            {
                                U256::from(2u64)
                            } else if to == USDC.to_ascii_lowercase() {
                                U256::from(123_456_789u64)
                            } else if to == LDO.to_ascii_lowercase() {
                                U256::from(5_000_000_000_000_000_000u128)
                            } else if to == UNKNOWN.to_ascii_lowercase() {
                                U256::from(7_000_000_000_000_000_000u128)
                            } else {
                                U256::ZERO
                            };
                            json!(word(amount))
                        } else {
                            json!("0x")
                        }
                    }
                    _ => json!("0x0"),
                };
                json!({"jsonrpc": "2.0", "id": id, "result": result})
            })
            .collect::<Vec<_>>();
        if is_batch {
            Json(Value::Array(items))
        } else {
            Json(items.into_iter().next().unwrap_or(Value::Null))
        }
    }

    async fn oneinch() -> Json<Value> {
        let mut map = serde_json::Map::new();
        map.insert(USDC.to_string(), json!("123456789"));
        map.insert(LDO.to_string(), json!("5000000000000000000"));
        map.insert(UNKNOWN.to_string(), json!("7000000000000000000"));
        map.insert(
            "0x0000000000000000000000000000000000000004".to_string(),
            json!("0"),
        );
        map.insert(NATIVE.to_string(), json!("1000000000000000000"));
        Json(Value::Object(map))
    }

    async fn alchemy() -> Json<Value> {
        Json(json!({
            "ownedNfts": [
                {
                    "contract": {"address": PROVIDER_COLLECTION, "name": "Provider Collection", "symbol": "PC"},
                    "tokenType": "ERC721",
                    "tokenId": "1",
                    "balance": "1"
                },
                {
                    "contract": {"address": PROVIDER_COLLECTION, "name": "Provider Collection", "symbol": "PC"},
                    "tokenType": "ERC721",
                    "tokenId": "2",
                    "balance": "1"
                }
            ]
        }))
    }

    async fn unavailable() -> (StatusCode, Json<Value>) {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "unavailable"})),
        )
    }

    async fn test_server(provider_ok: bool) -> String {
        let app = Router::new()
            .route("/rpc", post(rpc))
            .route(
                "/balance/v1.2/1/balances/{address}",
                if provider_ok {
                    get(oneinch)
                } else {
                    get(|| async { unavailable().await })
                },
            )
            .route("/nft/v3/test/getNFTsForOwner", get(alchemy));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn config(base: &str) -> WalletConfig {
        WalletConfig {
            rpc_url: format!("{base}/rpc"),
            oneinch_api_key: Some("test".to_string()),
            oneinch_balance_url: format!("{base}/balance/v1.2"),
            alchemy_api_key: None,
            alchemy_nft_url: "http://unused".to_string(),
        }
    }

    #[tokio::test]
    async fn balance_reads_native_known_and_unknown_tokens() {
        let base = test_server(true).await;
        let response = balance(
            &config(&base),
            BalanceRequest {
                address: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                tokens: vec![USDC.to_string(), UNKNOWN.to_string()],
                include_native: true,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.coverage, "local");
        assert_eq!(response.native.as_ref().unwrap().amount.decimal, "1");
        assert_eq!(response.tokens.len(), 2);
        let usdc = response
            .tokens
            .iter()
            .find(|token| token.token.address.to_string().eq_ignore_ascii_case(USDC))
            .unwrap();
        assert_eq!(usdc.amount.decimal, "123.456789");
        assert!(usdc.known);
        let unknown = response
            .tokens
            .iter()
            .find(|token| {
                token
                    .token
                    .address
                    .to_string()
                    .eq_ignore_ascii_case(UNKNOWN)
            })
            .unwrap();
        assert_eq!(unknown.token.symbol.as_deref(), Some("TEST"));
        assert_eq!(unknown.amount.decimal, "7");
        assert!(!unknown.known);
    }

    #[tokio::test]
    async fn allowance_reads_known_tokens_and_keeps_zero_values() {
        let base = test_server(true).await;
        let response = allowance(
            &config(&base),
            AllowanceRequest {
                owner: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                spender: "0x0000000000000000000000000000000000000002"
                    .parse()
                    .unwrap(),
                tokens: vec![USDC.to_string(), LDO.to_string()],
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.coverage, "local");
        assert_eq!(response.allowances.len(), 2);
        let usdc = response
            .allowances
            .iter()
            .find(|entry| entry.token.address.to_string().eq_ignore_ascii_case(USDC))
            .unwrap();
        assert_eq!(usdc.amount.decimal, "1000");
        let ldo = response
            .allowances
            .iter()
            .find(|entry| entry.token.address.to_string().eq_ignore_ascii_case(LDO))
            .unwrap();
        assert_eq!(ldo.amount.decimal, "0");
    }

    #[tokio::test]
    async fn collectibles_falls_back_to_onchain_enumerable_collection() {
        let base = test_server(true).await;
        let owner: Address = "0x0000000000000000000000000000000000000001"
            .parse()
            .unwrap();
        let collection: Address = COLLECTION.parse().unwrap();
        let response = collectibles(
            &WalletConfig {
                rpc_url: format!("{base}/rpc"),
                oneinch_api_key: None,
                oneinch_balance_url: "http://unused".to_string(),
                alchemy_api_key: None,
                alchemy_nft_url: "http://unused".to_string(),
            },
            CollectiblesRequest {
                owner,
                collections: vec![collection],
                include_empty: false,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.coverage, "local");
        assert_eq!(response.collections.len(), 1);
        let entry = &response.collections[0];
        assert_eq!(entry.standard, "erc721");
        assert!(entry.enumerable);
        assert_eq!(entry.item_count, 2);
        assert_eq!(entry.items.len(), 2);
    }

    #[tokio::test]
    async fn collectibles_uses_alchemy_then_verifies_locally() {
        let base = test_server(true).await;
        let owner: Address = "0x0000000000000000000000000000000000000001"
            .parse()
            .unwrap();
        let response = collectibles(
            &WalletConfig {
                rpc_url: format!("{base}/rpc"),
                oneinch_api_key: None,
                oneinch_balance_url: "http://unused".to_string(),
                alchemy_api_key: Some("test".to_string()),
                alchemy_nft_url: format!("{base}/nft/v3"),
            },
            CollectiblesRequest {
                owner,
                collections: Vec::new(),
                include_empty: false,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.coverage, "provider+local");
        assert_eq!(response.collections.len(), 1);
        assert_eq!(response.collections[0].items.len(), 2);
    }

    #[tokio::test]
    async fn assets_uses_provider_then_verifies_locally() {
        let base = test_server(true).await;
        let response = assets(
            &config(&base),
            AssetsRequest {
                address: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                include_native: true,
                include_unknown: false,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            response.coverage, "provider+local",
            "{:?}",
            response.warnings
        );
        assert_eq!(response.unknown_count, 1);
        assert_eq!(response.native.as_ref().unwrap().amount.decimal, "1");
        assert_eq!(response.tokens.len(), 2);
        assert!(
            response
                .tokens
                .iter()
                .all(|token| token.token.address.to_string() != UNKNOWN)
        );
    }

    #[tokio::test]
    async fn assets_can_expand_unknown_tokens_and_falls_back() {
        let base = test_server(true).await;
        let response = assets(
            &config(&base),
            AssetsRequest {
                address: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                include_native: false,
                include_unknown: true,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(response.unknown_count, 0);
        assert!(response.tokens.iter().any(|token| {
            token
                .token
                .address
                .to_string()
                .eq_ignore_ascii_case(UNKNOWN)
        }));

        let fallback_base = test_server(false).await;
        // The RPC server is still available; only the provider route fails.
        let fallback = assets(
            &config(&fallback_base),
            AssetsRequest {
                address: "0x0000000000000000000000000000000000000001"
                    .parse()
                    .unwrap(),
                include_native: false,
                include_unknown: false,
                block: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(fallback.coverage, "registry+local");
        assert!(!fallback.warnings.is_empty());
    }
}
