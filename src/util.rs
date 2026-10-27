use alloy_primitives::U256;

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
