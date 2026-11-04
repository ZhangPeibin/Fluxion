use crate::local_state::{LocalStateConfig, PricingMode};
use crate::model::{ConversionKind, Pool, PoolKind};
use crate::split::{SplitConfig, SplitOptimizer};
use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid config: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolConversionConfig {
    pub kind: ConversionKind,
    pub contract: Address,
    pub underlying: Address,
    pub wrapped: Address,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default = "enabled")]
    pub wrap: bool,
    #[serde(default = "enabled")]
    pub unwrap: bool,
}

fn enabled() -> bool {
    true
}

impl ProtocolConversionConfig {
    pub fn pool(&self) -> Pool {
        Pool {
            kind: PoolKind::Conversion {
                kind: self.kind,
                wrap: self.wrap,
                unwrap: self.unwrap,
            },
            address: self.contract,
            coins: vec![self.underlying, self.wrapped],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct V2ForkConfig {
    pub name: String,
    pub factory: Address,
    #[serde(default)]
    pub shibaswap: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct V3SubgraphConfig {
    pub name: String,
    pub subgraph_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub rpc_url: String,
    pub uniswap_v2_subgraph_id: String,
    pub uniswap_v3_subgraph_id: String,
    pub curve_address_provider: String,
    pub curve_registry_ids: Vec<u64>,
    pub enabled_platforms: Vec<String>,
    pub max_hops: usize,
    pub top_k: usize,
    pub max_pools: usize,
    pub min_liquidity: u128,
    pub min_volume_24h: u128,
    pub min_pool_age_days: u64,
    pub reserve_ratio_k: u128,
    pub slippage_penalty_lambda: f64,
    pub refresh_interval_secs: u64,
    pub snapshot_refresh_secs: u64,
    /// Re-run pool discovery at this interval; `0` disables runtime refresh.
    pub universe_refresh_secs: u64,
    pub request_timeout_ms: u64,
    pub token_allowlist: Vec<String>,
    /// Explicit deployment exclusions, visible in public experiment settings.
    pub excluded_pools: Vec<Address>,
    pub shibaswap_v2_factory: Address,
    /// Additional same-interface V2 deployments discovered through their factory.
    pub v2_forks: Vec<V2ForkConfig>,
    /// Additional same-interface V3 deployments discovered through a subgraph.
    pub v3_subgraphs: Vec<V3SubgraphConfig>,
    pub protocol_conversions: Vec<ProtocolConversionConfig>,
    #[serde(deserialize_with = "service_split_config")]
    pub split: SplitConfig,
    pub pricing_mode: PricingMode,
    pub local_state: LocalStateConfig,
}

fn service_split_config<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SplitConfig, D::Error> {
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    let incoming = value
        .as_object()
        .ok_or_else(|| D::Error::custom("split must be a mapping"))?;
    let mut merged = serde_json::to_value(SplitConfig::multi_pool()).map_err(D::Error::custom)?;
    merged.as_object_mut().unwrap().extend(incoming.clone());
    serde_json::from_value(merged).map_err(D::Error::custom)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            rpc_url: "https://eth.drpc.org".to_string(),
            uniswap_v2_subgraph_id: "EYCKATKGBKLWvSfwvBjzfCBmGwYNdVkduYXVivCsLRFu".to_string(),
            uniswap_v3_subgraph_id: "5zvR82QoaXYFyDEKLZ9t6v9adgnptxYpKpSbxtgVENFV".to_string(),
            curve_address_provider: "0x0000000022D53366457F9d5E68Ec105046FC4383".to_string(),
            curve_registry_ids: vec![0, 5],
            enabled_platforms: vec![
                "uniswap-v2".to_string(),
                "uniswap-v3".to_string(),
                "curve".to_string(),
                "shibaswap-v2".to_string(),
            ],
            max_hops: 3,
            top_k: 8,
            max_pools: 10_000,
            min_liquidity: 50_000,
            min_volume_24h: 0,
            min_pool_age_days: 0,
            reserve_ratio_k: 0,
            slippage_penalty_lambda: 0.0,
            refresh_interval_secs: 30,
            snapshot_refresh_secs: 120,
            universe_refresh_secs: 300,
            request_timeout_ms: 10000,
            token_allowlist: Vec::new(),
            excluded_pools: Vec::new(),
            split: SplitConfig::multi_pool(),
            pricing_mode: PricingMode::default(),
            local_state: LocalStateConfig::default(),
            shibaswap_v2_factory: "0x115934131916C8b277DD010Ee02de363c09d037c"
                .parse()
                .unwrap(),
            v2_forks: Vec::new(),
            v3_subgraphs: Vec::new(),
            protocol_conversions: vec![ProtocolConversionConfig {
                kind: ConversionKind::WstEth,
                contract: "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
                    .parse()
                    .unwrap(),
                underlying: "0xae7ab96520de3a18e5e111b5eaab095312d7fe84"
                    .parse()
                    .unwrap(),
                wrapped: "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
                    .parse()
                    .unwrap(),
                enabled: true,
                wrap: true,
                unwrap: true,
            }],
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        SplitOptimizer::new(self.split.clone()).map_err(ConfigError::Invalid)?;
        self.local_state.validate().map_err(ConfigError::Invalid)?;
        let mut contracts = std::collections::HashSet::new();
        for conversion in &self.protocol_conversions {
            if conversion.contract == Address::ZERO
                || conversion.underlying == Address::ZERO
                || conversion.wrapped != conversion.contract
                || conversion.underlying == conversion.wrapped
                || (conversion.enabled && !conversion.wrap && !conversion.unwrap)
                || !contracts.insert(conversion.contract)
            {
                return Err(ConfigError::Invalid("conversion must have a unique nonzero wrapper contract, distinct underlying, and an enabled direction".into()));
            }
        }
        for token in &self.token_allowlist {
            token
                .parse::<Address>()
                .map_err(|_| ConfigError::Invalid(format!("invalid allowlist token: {token}")))?;
        }
        Ok(())
    }
    /// Only a missing file uses defaults; malformed configuration is an error.
    pub fn load(path: &str) -> Result<Self, ConfigError> {
        match Self::from_file(path) {
            Err(ConfigError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::default())
            }
            result => result,
        }
    }

    /// Public experiment settings. RPC URLs may embed credentials and are omitted.
    pub fn public_settings(&self) -> serde_json::Value {
        let mut value = serde_json::to_value(self).expect("serializable configuration");
        value.as_object_mut().unwrap().remove("rpc_url");
        value
    }

    pub fn from_file(path: &str) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&text)?;
        config.validate()?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_yaml_uses_defaults() {
        let c: Config = serde_yaml::from_str("{}").unwrap();
        assert_eq!(c.max_hops, 3);
        assert_eq!(c.top_k, 8);
        assert_eq!(c.rpc_url, "https://eth.drpc.org");
        assert_eq!(c.reserve_ratio_k, 0);
    }

    #[test]
    fn example_config_parses() {
        let c = Config::from_file("config.example.yaml").unwrap();
        assert_eq!(c.max_hops, 3);
        assert!(!c.enabled_platforms.is_empty());
    }

    #[test]
    fn same_interface_forks_are_config_only() {
        let config: Config = serde_yaml::from_str(
            "v2_forks:\n  - name: fork\n    factory: '0x0000000000000000000000000000000000000010'\n    shibaswap: false\nv3_subgraphs:\n  - name: fork-v3\n    subgraph_id: abc\n",
        )
        .unwrap();
        assert_eq!(config.v2_forks.len(), 1);
        assert!(!config.v2_forks[0].shibaswap);
        assert_eq!(config.v3_subgraphs[0].subgraph_id, "abc");
        assert!(serde_yaml::from_str::<Config>("v2_forks:\n  - name: x\n").is_err());
    }

    #[test]
    fn missing_file_errors() {
        assert!(Config::from_file("does-not-exist.yaml").is_err());
    }

    #[test]
    fn unknown_field_errors() {
        let r: Result<Config, _> = serde_yaml::from_str("bogus: 1");
        assert!(r.is_err());
    }

    #[test]
    fn partial_service_split_settings_preserve_multi_pool_defaults() {
        let config: Config = serde_yaml::from_str("split:\n  timeout_ms: 1234\n").unwrap();
        assert_eq!(config.split.max_branches, 4);
        assert_eq!(config.split.max_quote_calls, 1024);
        assert_eq!(config.split.timeout_ms, 1234);
        assert!(serde_yaml::from_str::<Config>("split:\n  unknown_setting: 1\n").is_err());
    }

    #[test]
    fn local_profile_is_explicit_and_rejects_invalid_sync_policy() {
        let config = Config::from_file("config.local-v2.example.yaml").unwrap();
        assert_eq!(config.pricing_mode, PricingMode::LocalSnapshot);
        assert!(config.protocol_conversions.is_empty());
        assert_eq!(Config::default().pricing_mode, PricingMode::Rpc);
        let mut invalid = config;
        invalid.local_state.poll_interval_ms = 0;
        assert!(invalid.validate().is_err());
        assert!(serde_yaml::from_str::<Config>("local_state:\n  undocumented: 1\n").is_err());
    }
}
