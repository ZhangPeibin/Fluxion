use alloy_primitives::U256;

pub(crate) fn u256_to_f64(x: U256) -> f64 {
    x.to_string().parse().unwrap_or(f64::INFINITY)
}
