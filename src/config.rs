use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid config: {0}")]
    Parse(#[from] serde_yaml::Error),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub rpc_url: String,
    pub uniswap_v2_subgraph_id: String,
    pub uniswap_v3_subgraph_id: String,
    pub curve_address_provider: String,
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
    pub token_allowlist: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: "https://eth.drpc.org".to_string(),
            uniswap_v2_subgraph_id: "EYCKATKGBKLWvSfwvBjzfCBmGwYNdVkduYXVivCsLRFu".to_string(),
            uniswap_v3_subgraph_id: "5zvR82QoaXYFyDEKLZ9t6v9adgnptxYpKpSbxtgVENFV".to_string(),
            curve_address_provider: "0x0000000022D53366457F9d5E68Ec105046FC4383".to_string(),
            enabled_platforms: vec![
                "uniswap-v2".to_string(),
                "uniswap-v3".to_string(),
                "curve".to_string(),
            ],
            max_hops: 3,
            top_k: 8,
            max_pools: 100,
            min_liquidity: 50_000,
            min_volume_24h: 0,
            min_pool_age_days: 0,
            reserve_ratio_k: 10,
            slippage_penalty_lambda: 0.0,
            refresh_interval_secs: 30,
            token_allowlist: Vec::new(),
        }
    }
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&text)?)
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
        assert_eq!(c.reserve_ratio_k, 10);
    }

    #[test]
    fn example_config_parses() {
        let c = Config::from_file("config.example.yaml").unwrap();
        assert_eq!(c.max_hops, 3);
        assert!(!c.enabled_platforms.is_empty());
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
}
