use alloy_primitives::{Address, U256};

pub(crate) fn u256_to_f64(x: U256) -> f64 {
    x.to_string().parse().unwrap_or(f64::INFINITY)
}

pub(crate) fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s
        .trim()
        .trim_start_matches("0x")
        .replace([' ', '\n', '\t'], "");
    if !s.len().is_multiple_of(2) {
        return Err("odd hex length".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

pub(crate) fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// ABI-encode an address as a 32-byte word (left-padded).
pub(crate) fn pad_address(a: Address) -> String {
    format!("{}{}", "0".repeat(24), bytes_to_hex(a.as_slice()))
}

/// ABI-encode a U256 as a 32-byte word.
pub(crate) fn u256_word(x: U256) -> String {
    bytes_to_hex(&x.to_be_bytes::<32>())
}

/// ABI-encode a u32 as a 32-byte word (left-padded).
pub(crate) fn u32_word(x: u32) -> String {
    format!("{x:064x}")
}

pub(crate) fn u256_to_usize(x: U256) -> usize {
    x.to_string().parse().unwrap_or(usize::MAX)
}
