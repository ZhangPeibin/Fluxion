//! Built-in Ethereum token registry for the CLI/MCP surface.
//!
//! Addresses remain accepted inputs; symbols are conveniences. Unknown
//! addresses require an explicit decimals override.

use alloy_primitives::{Address, U256};

use crate::capability::{CapabilityError, TokenRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenInfo {
    pub symbol: &'static str,
    pub address: &'static str,
    pub decimals: u8,
}

macro_rules! token {
    ($symbol:literal, $address:literal, $decimals:expr) => {
        TokenInfo {
            symbol: $symbol,
            address: $address,
            decimals: $decimals,
        }
    };
}

static TOKENS: &[TokenInfo] = &[
    token!("WETH", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", 18),
    token!("USDC", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", 6),
    token!("USDT", "0xdac17f958d2ee523a2206206994597c13d831ec7", 6),
    token!("DAI", "0x6b175474e89094c44da98b954eedeac495271d0f", 18),
    token!("WBTC", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599", 8),
    token!("stETH", "0xae7ab96520de3a18e5e111b5eaab095312d7fe84", 18),
    token!("wstETH", "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0", 18),
    token!("LDO", "0x5a98fcbea516cf06857215779fd812ca3bef1b32", 18),
    token!("PEPE", "0x6982508145454ce325ddbe47a25d4ec3d2311933", 18),
    token!("LINK", "0x514910771af9ca656af840dff83e8264ecf986ca", 18),
    token!("UNI", "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984", 18),
    token!("AAVE", "0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9", 18),
    token!("SHIB", "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce", 18),
    token!("MKR", "0x9f8f72aa9304c8b593d555f12ef6589cc3a579a2", 18),
    token!("CRV", "0xd533a949740bb3306d119cc777fa900ba034cd52", 18),
    token!("USDS", "0xdc035d45d973e3ec169d2276ddab16f1e407384f", 18),
    token!("LUSD", "0x5f98805a4e8be255a32880fdec7f6728c6568ba0", 18),
    token!("FRAX", "0x853d955acef822db058eb8505911ed77f175b99e", 18),
    token!("USDe", "0x4c9edd5852cd905f086c759e8383e09bff1e68b3", 18),
    token!("ENA", "0x57e114b691db790c35207b2e685d4a43181e6061", 18),
    token!("rETH", "0xae78736cd615f374d3085123a210448e74fc6393", 18),
    token!("cbETH", "0xbe9895146f7af43049ca1c1ae358b0541ea49704", 18),
    token!("GNO", "0x6810e776880c02933d47db1b9fc05908e5386b96", 18),
    token!("RPL", "0xd33526068d116ce69f19a9ee46f0bd304f21a51f", 18),
    token!("YFI", "0x0bc529c00c6401aef6d220be8c6ea1667f6ad93e", 18),
    token!("SUSHI", "0x6b3595068778dd592e39a122f4f5a5cf09c90fe2", 18),
    token!("1INCH", "0x111111111117dc0aa78b770fa6a738034120c302", 18),
    token!("BAL", "0xba100000625a3754423978a60c9317c58a424e3d", 18),
    token!("COMP", "0xc00e94cb662c3520282e6f5717214004a7f26888", 18),
    token!("SNX", "0xc011a73ee8576fb46f5e1c5751ca3b9fe0af2a6f", 18),
    token!("FET", "0xaea46a60368a7bd060eec7df8cba43b7ef41ad85", 18),
    token!("GRT", "0xc944e90c64b2c07662a292be6244bdf05cda44a7", 18),
    token!("APE", "0x4d224452801aced8b2f0aebe155379bb5d594381", 18),
    token!("ARB", "0xb50721bcf8d664c30412cfbc6cf7a15145234ad1", 18),
    token!("ENS", "0xc18360217d8f7ab5e7c516566761ea12ce7f9d72", 18),
];

pub fn list() -> &'static [TokenInfo] {
    TOKENS
}

pub fn decimals_for(address: Address) -> Option<u8> {
    TOKENS
        .iter()
        .find(|token| token.address.parse::<Address>().ok() == Some(address))
        .map(|token| token.decimals)
}

pub fn resolve(input: &str, decimals_override: Option<u8>) -> Result<TokenRef, CapabilityError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(CapabilityError::invalid_argument("empty token"));
    }
    if input.starts_with("0x") || input.starts_with("0X") {
        let address = input.parse::<Address>().map_err(|_| {
            CapabilityError::invalid_argument(format!("invalid token address: {input}"))
        })?;
        let known = TOKENS
            .iter()
            .find(|token| token.address.parse::<Address>().ok() == Some(address));
        let (symbol, decimals) = match (known, decimals_override) {
            (Some(token), Some(decimals)) => (Some(token.symbol.to_string()), decimals),
            (Some(token), None) => (Some(token.symbol.to_string()), token.decimals),
            (None, Some(decimals)) => (None, decimals),
            (None, None) => {
                return Err(CapabilityError::invalid_argument(format!(
                    "unknown token address {input}; pass decimals explicitly"
                )));
            }
        };
        return Ok(TokenRef::new(symbol, address, decimals));
    }

    let upper = input.to_ascii_uppercase();
    let symbol = if upper == "ETH" { "WETH" } else { input };
    let token = TOKENS
        .iter()
        .find(|token| token.symbol.eq_ignore_ascii_case(symbol))
        .ok_or_else(|| {
            CapabilityError::invalid_argument(format!("unknown token symbol: {input}"))
        })?;
    let address = token.address.parse::<Address>().map_err(|_| {
        CapabilityError::internal(format!("invalid built-in token address: {}", token.address))
    })?;
    Ok(TokenRef::new(
        Some(token.symbol.to_string()),
        address,
        decimals_override.unwrap_or(token.decimals),
    ))
}

pub fn parse_amount(input: &str, decimals: u8) -> Result<U256, CapabilityError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(CapabilityError::invalid_argument("empty amount"));
    }
    if input.starts_with('-') {
        return Err(CapabilityError::invalid_argument("amount must be positive"));
    }
    let input = input.strip_prefix('+').unwrap_or(input);
    let mut parts = input.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some() {
        return Err(CapabilityError::invalid_argument(format!(
            "invalid amount: {input}"
        )));
    }
    if !integer.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit())
    {
        return Err(CapabilityError::invalid_argument(format!(
            "invalid amount: {input}"
        )));
    }
    let integers = if integer.is_empty() { "0" } else { integer };
    let decimals = usize::from(decimals);
    if fraction.len() > decimals {
        return Err(CapabilityError::invalid_argument(format!(
            "amount has more than {decimals} decimal places"
        )));
    }
    let mut raw = String::with_capacity(integers.len() + decimals);
    raw.push_str(integers);
    raw.push_str(fraction);
    raw.push_str(&"0".repeat(decimals - fraction.len()));
    U256::from_str_radix(&raw, 10)
        .map_err(|_| CapabilityError::invalid_argument(format!("invalid amount: {input}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_symbols_aliases_and_addresses() {
        let ldo = resolve("ldo", None).unwrap();
        assert_eq!(ldo.symbol.as_deref(), Some("LDO"));
        assert_eq!(ldo.decimals, 18);
        let eth = resolve("ETH", None).unwrap();
        assert_eq!(eth.symbol.as_deref(), Some("WETH"));
        let usdc = resolve("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", None).unwrap();
        assert_eq!(usdc.decimals, 6);
        assert!(resolve("0x0000000000000000000000000000000000000001", None).is_err());
        assert_eq!(
            resolve("0x0000000000000000000000000000000000000001", Some(18))
                .unwrap()
                .decimals,
            18
        );
    }

    #[test]
    fn parses_decimal_amounts_without_float() {
        assert_eq!(
            parse_amount("100", 18).unwrap(),
            U256::from(100u64) * U256::from(10u64).pow(U256::from(18))
        );
        assert_eq!(
            parse_amount("0.5", 18).unwrap(),
            U256::from(5u64) * U256::from(10u64).pow(U256::from(17))
        );
        assert_eq!(
            parse_amount("1.000001", 6).unwrap(),
            U256::from(1_000_001u64)
        );
        assert_eq!(parse_amount(".5", 2).unwrap(), U256::from(50u64));
        assert!(parse_amount("1.1234567", 6).is_err());
        assert!(parse_amount("-1", 18).is_err());
    }
}
