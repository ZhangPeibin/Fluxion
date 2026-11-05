//! Exact view model for the deployed Tricrypto2 pool/Views/Math combination.
//! Uses stored D and post-trade dynamic fee; no RPC, floats or invariant refit.
use crate::{
    error::QuoteError,
    model::{Pool, PoolKind},
    util::u256_to_f64,
};
use alloy_primitives::{Address, U256};

pub const POOL: &str = "0xd51a44d3fae010294c616388b506acda1bfaae46";
pub const VIEWS: &str = "0x40745803c2faa8e8402e2ae935933d07ca8f355c";
pub const MATH: &str = "0x8f68f4810cce3194b6cb6f3d50fa58c2c9bdd1d5";
pub const COINS: [&str; 3] = [
    "0xdac17f958d2ee523a2206206994597c13d831ec7",
    "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
];
pub const PROFILE: &str = "tricrypto2";
const P: U256 = U256::from_limbs([1_000_000_000_000_000_000, 0, 0, 0]);

pub fn pool() -> Pool {
    Pool {
        kind: PoolKind::Curve,
        address: POOL.parse().unwrap(),
        coins: COINS.map(|c| c.parse().unwrap()).to_vec(),
    }
}
pub(crate) fn supported(p: &Pool) -> bool {
    p.kind == PoolKind::Curve
        && p.address == POOL.parse::<Address>().unwrap()
        && p.coins == COINS.map(|c| c.parse::<Address>().unwrap())
}
pub fn units() -> [U256; 3] {
    [U256::from(1_000_000), U256::from(100_000_000), P]
}
fn precisions() -> [U256; 3] {
    [
        U256::from(1_000_000_000_000u64),
        U256::from(10_000_000_000u64),
        U256::from(1),
    ]
}
fn add(a: U256, b: U256) -> Result<U256, QuoteError> {
    a.checked_add(b).ok_or(QuoteError::Overflow)
}
fn sub(a: U256, b: U256) -> Result<U256, QuoteError> {
    a.checked_sub(b).ok_or(QuoteError::Overflow)
}
fn mul(a: U256, b: U256) -> Result<U256, QuoteError> {
    a.checked_mul(b).ok_or(QuoteError::Overflow)
}
fn div(a: U256, b: U256) -> Result<U256, QuoteError> {
    if b.is_zero() {
        Err(QuoteError::InsufficientLiquidity)
    } else {
        Ok(a / b)
    }
}
fn safe_fraction(value: U256, d: U256) -> Result<(), QuoteError> {
    let fraction = div(mul(value, P)?, d)?;
    if fraction < U256::from(10u128.pow(16)) || fraction > U256::from(10u128.pow(20)) {
        return Err(QuoteError::InsufficientLiquidity);
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Tricrypto2Parameters {
    /// Already ANN as returned by A(); do not multiply by N or N^N again.
    pub amplification: U256,
    pub gamma: U256,
    pub stored_d: U256,
    pub price_scale: [U256; 2],
    pub mid_fee: U256,
    pub out_fee: U256,
    pub fee_gamma: U256,
}

pub struct Tricrypto2State {
    balances: [U256; 3],
    parameters: Tricrypto2Parameters,
}
#[derive(Debug)]
pub struct Tricrypto2Quote {
    pub amount_out: U256,
    pub fee_rate: U256,
    pub iterations: usize,
}

fn newton_y(
    p: &Tricrypto2Parameters,
    xp: [U256; 3],
    output: usize,
) -> Result<(U256, usize), QuoteError> {
    let d = p.stored_d;
    let n = U256::from(3);
    let one = U256::from(1);
    let limit_divisor = U256::from(10u128.pow(14));
    for (k, value) in xp.iter().enumerate() {
        if k != output {
            safe_fraction(*value, d)?;
        }
    }
    let mut sorted = xp;
    sorted[output] = U256::ZERO;
    sorted.sort_by(|a, b| b.cmp(a));
    let convergence_limit = (sorted[0] / limit_divisor)
        .max(d / limit_divisor)
        .max(U256::from(100));
    let mut y = d / n;
    let mut k_i = P;
    let mut sum_i = U256::ZERO;
    // Small value first for y; large value first for K_i. Preserve both orders.
    for x in [sorted[1], sorted[0]] {
        y = div(mul(y, d)?, mul(x, n)?)?;
        sum_i = add(sum_i, x)?;
    }
    for x in &sorted[..2] {
        k_i = div(mul(mul(k_i, *x)?, n)?, d)?;
    }
    for iteration in 0..255 {
        let previous = y;
        let k0 = div(mul(mul(k_i, y)?, n)?, d)?;
        let sum = add(sum_i, y)?;
        let gamma_plus = add(p.gamma, P)?;
        let g = add(gamma_plus.max(k0) - gamma_plus.min(k0), one)?;
        let mut mul1 = div(mul(P, d)?, p.gamma)?;
        mul1 = div(mul(mul1, g)?, p.gamma)?;
        mul1 = mul(mul1, g)?;
        mul1 = div(mul(mul1, U256::from(10000))?, p.amplification)?;
        let mul2 = add(P, div(mul(mul(U256::from(2), P)?, k0)?, g)?)?;
        let mut yfprime = add(add(mul(P, y)?, mul(sum, mul2)?)?, mul1)?;
        let dyfprime = mul(d, mul2)?;
        if yfprime < dyfprime {
            y = previous / U256::from(2);
            continue;
        }
        yfprime = sub(yfprime, dyfprime)?;
        let fprime = div(yfprime, y)?;
        let mut minus = div(mul1, fprime)?;
        let plus = add(
            div(add(yfprime, mul(P, d)?)?, fprime)?,
            div(mul(minus, P)?, k0)?,
        )?;
        minus = add(minus, div(mul(P, sum)?, fprime)?)?;
        y = if plus < minus {
            previous / U256::from(2)
        } else {
            sub(plus, minus)?
        };
        let difference = y.max(previous) - y.min(previous);
        if difference < convergence_limit.max(y / limit_divisor) {
            safe_fraction(y, d)?;
            return Ok((y, iteration + 1));
        }
    }
    Err(QuoteError::InsufficientLiquidity)
}

impl Tricrypto2State {
    pub fn new(balances: [U256; 3], parameters: Tricrypto2Parameters) -> Result<Self, QuoteError> {
        let p = &parameters;
        if p.amplification < U256::from(2700)
            || p.amplification > U256::from(270_000_000)
            || p.gamma < U256::from(10_000_000_000u64)
            || p.gamma > U256::from(10_000_000_000_000_000u64)
            || p.stored_d > U256::from(10u128.pow(33))
            || p.price_scale.iter().any(U256::is_zero)
            || p.price_scale.iter().any(|price| price.bit_len() > 128)
            || p.mid_fee > U256::from(10_000_000_000u64)
            || p.out_fee > U256::from(10_000_000_000u64)
            || p.out_fee < U256::from(500_000)
            || p.mid_fee > p.out_fee
            || p.fee_gamma.is_zero()
            || p.fee_gamma >= P
        {
            return Err(QuoteError::IncompleteState);
        }
        Ok(Self {
            balances,
            parameters,
        })
    }
    pub fn balances(&self) -> &[U256; 3] {
        &self.balances
    }
    pub fn parameters(&self) -> &Tricrypto2Parameters {
        &self.parameters
    }
    pub fn is_liquid(&self) -> bool {
        self.parameters.stored_d >= U256::from(10u128.pow(17))
            && self.balances.iter().any(|v| !v.is_zero())
    }
    fn normalize(&self, balances: [U256; 3]) -> Result<[U256; 3], QuoteError> {
        let prec = precisions();
        let mut xp = balances;
        xp[0] = mul(xp[0], prec[0])?;
        for k in 1..3 {
            xp[k] = div(
                mul(mul(xp[k], self.parameters.price_scale[k - 1])?, prec[k])?,
                P,
            )?;
        }
        Ok(xp)
    }
    fn dynamic_fee(&self, xp: [U256; 3]) -> Result<U256, QuoteError> {
        let sum = xp.iter().try_fold(U256::ZERO, |s, x| add(s, *x))?;
        let mut weight = P;
        for x in xp {
            weight = div(mul(mul(weight, U256::from(3))?, x)?, sum)?;
        }
        let p = &self.parameters;
        if !p.fee_gamma.is_zero() {
            weight = div(mul(p.fee_gamma, P)?, sub(add(p.fee_gamma, P)?, weight)?)?;
        }
        div(
            add(mul(p.mid_fee, weight)?, mul(p.out_fee, sub(P, weight)?)?)?,
            P,
        )
    }
    pub fn quote(&self, i: usize, j: usize, amount: U256) -> Result<Tricrypto2Quote, QuoteError> {
        if i >= 3 || j >= 3 || i == j {
            return Err(QuoteError::UnknownPool);
        }
        if amount.is_zero() {
            return Err(QuoteError::ZeroAmount);
        }
        if !self.is_liquid() {
            return Err(QuoteError::InsufficientLiquidity);
        }
        let mut balances = self.balances;
        balances[i] = add(balances[i], amount)?;
        // Normalize the whole post-input balance, not cached xp + rounded dx.
        let mut xp = self.normalize(balances)?;
        let (y, iterations) = newton_y(&self.parameters, xp, j)?;
        let mut dy = sub(sub(xp[j], y)?, U256::from(1))?;
        xp[j] = y;
        if j > 0 {
            dy = div(mul(dy, P)?, self.parameters.price_scale[j - 1])?;
        }
        dy = div(dy, precisions()[j])?;
        let fee_rate = self.dynamic_fee(xp)?;
        let fee = div(mul(fee_rate, dy)?, U256::from(10_000_000_000u64))?;
        Ok(Tricrypto2Quote {
            amount_out: sub(dy, fee)?,
            fee_rate,
            iterations,
        })
    }
    pub fn amount_out(&self, i: usize, j: usize, amount: U256) -> Result<U256, QuoteError> {
        Ok(self.quote(i, j, amount)?.amount_out)
    }
    pub fn mid_price(&self, i: usize, j: usize) -> Result<f64, QuoteError> {
        Ok(u256_to_f64(self.amount_out(i, j, U256::from(1_000_000))?) / 1_000_000.0)
    }
}
