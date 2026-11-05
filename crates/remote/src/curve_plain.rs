//! Deployment-bound two-coin StableSwap views. A_PRECISION=100 is shared;
//! balance acquisition and D-product rounding order are not interchangeable.
use crate::{
    error::QuoteError,
    model::{Pool, PoolKind},
    util::u256_to_f64,
};
use alloy_primitives::U256;

pub const TWO_CRV: &str = "0x1005f7406f32a61bd760cfa14accd2737913d546";
pub const FRAX_USDC: &str = "0xdcef968d416a41cdac0ed8702fac8128a64241a2";
const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const USDT: &str = "0xdac17f958d2ee523a2206206994597c13d831ec7";
const FRAX: &str = "0x853d955acef822db058eb8505911ed77f175b99e";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlainProfile {
    TwoCrv,
    FraxUsdc,
}

impl PlainProfile {
    pub fn name(self) -> &'static str {
        match self {
            Self::TwoCrv => "plain-2crv",
            Self::FraxUsdc => "plain-frax-usdc",
        }
    }
    pub fn balance_source(self) -> &'static str {
        match self {
            Self::TwoCrv => "erc20-minus-admin",
            Self::FraxUsdc => "stored",
        }
    }
    pub fn pool(self) -> Pool {
        let (address, coins) = match self {
            Self::TwoCrv => (TWO_CRV, [USDC, USDT]),
            Self::FraxUsdc => (FRAX_USDC, [FRAX, USDC]),
        };
        Pool {
            kind: PoolKind::Curve,
            address: address.parse().unwrap(),
            coins: coins.map(|c| c.parse().unwrap()).to_vec(),
        }
    }
    pub fn units(self) -> [U256; 2] {
        match self {
            Self::TwoCrv => [U256::from(1_000_000); 2],
            Self::FraxUsdc => [U256::from(10u128.pow(18)), U256::from(1_000_000)],
        }
    }
    pub fn rates(self) -> [U256; 2] {
        match self {
            Self::TwoCrv => [U256::from(10u128.pow(30)); 2],
            Self::FraxUsdc => [U256::from(10u128.pow(18)), U256::from(10u128.pow(30))],
        }
    }
    pub(crate) fn for_pool(pool: &Pool) -> Option<Self> {
        [Self::TwoCrv, Self::FraxUsdc].into_iter().find(|profile| {
            let expected = profile.pool();
            pool.kind == PoolKind::Curve
                && pool.address == expected.address
                && pool.coins == expected.coins
        })
    }
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
fn close(a: U256, b: U256) -> bool {
    a.max(b) - a.min(b) <= U256::from(1)
}

fn invariant(profile: PlainProfile, xp: &[U256; 2], amp: U256) -> Result<U256, QuoteError> {
    let sum = add(xp[0], xp[1])?;
    let ann = mul(amp, U256::from(2))?;
    let precision = U256::from(100);
    let mut d = sum;
    for _ in 0..255 {
        let product = match profile {
            PlainProfile::TwoCrv => {
                div(div(mul(div(mul(d, d)?, xp[0])?, d)?, xp[1])?, U256::from(4))?
            }
            PlainProfile::FraxUsdc => {
                let first = div(mul(d, d)?, mul(xp[0], U256::from(2))?)?;
                div(mul(first, d)?, mul(xp[1], U256::from(2))?)?
            }
        };
        let previous = d;
        let top = mul(
            add(
                div(mul(ann, sum)?, precision)?,
                mul(product, U256::from(2))?,
            )?,
            d,
        )?;
        let bottom = add(
            div(mul(sub(ann, precision)?, d)?, precision)?,
            mul(U256::from(3), product)?,
        )?;
        d = div(top, bottom)?;
        if close(previous, d) {
            return Ok(d);
        }
    }
    // Both deployments revert instead of returning an unconverged iterate.
    Err(QuoteError::InsufficientLiquidity)
}

pub struct Plain2Pool {
    profile: PlainProfile,
    balances: [U256; 2],
    xp: [U256; 2],
    amp: U256,
    fee: U256,
    d: Option<U256>,
}

impl Plain2Pool {
    pub fn new(
        profile: PlainProfile,
        balances: [U256; 2],
        precise_a: U256,
        fee: U256,
    ) -> Result<Self, QuoteError> {
        if precise_a < U256::from(100) || fee >= U256::from(10_000_000_000u64) {
            return Err(QuoteError::IncompleteState);
        }
        let rates = profile.rates();
        let precision = U256::from(10u128.pow(18));
        let xp = [
            div(mul(rates[0], balances[0])?, precision)?,
            div(mul(rates[1], balances[1])?, precision)?,
        ];
        let d = if xp.iter().any(U256::is_zero) {
            None
        } else {
            Some(invariant(profile, &xp, precise_a)?)
        };
        Ok(Self {
            profile,
            balances,
            xp,
            amp: precise_a,
            fee,
            d,
        })
    }
    pub fn profile(&self) -> PlainProfile {
        self.profile
    }
    pub fn balances(&self) -> &[U256; 2] {
        &self.balances
    }
    pub fn amplification(&self) -> U256 {
        self.amp
    }
    pub fn fee(&self) -> U256 {
        self.fee
    }
    pub fn is_liquid(&self) -> bool {
        self.d.is_some()
    }

    pub fn amount_out(&self, i: usize, j: usize, amount: U256) -> Result<U256, QuoteError> {
        if i >= 2 || j >= 2 || i == j {
            return Err(QuoteError::UnknownPool);
        }
        if amount.is_zero() {
            return Err(QuoteError::ZeroAmount);
        }
        let d = self.d.ok_or(QuoteError::InsufficientLiquidity)?;
        let rates = self.profile.rates();
        let precision = U256::from(10u128.pow(18));
        let x = add(self.xp[i], div(mul(amount, rates[i])?, precision)?)?;
        let ann = mul(self.amp, U256::from(2))?;
        let mut c = div(mul(d, d)?, mul(x, U256::from(2))?)?;
        c = div(mul(mul(c, d)?, U256::from(100))?, mul(ann, U256::from(2))?)?;
        let b = add(x, div(mul(d, U256::from(100))?, ann)?)?;
        let mut y = d;
        let mut converged = false;
        for _ in 0..255 {
            let previous = y;
            y = div(
                add(mul(y, y)?, c)?,
                sub(add(mul(U256::from(2), y)?, b)?, d)?,
            )?;
            if close(previous, y) {
                converged = true;
                break;
            }
        }
        if !converged {
            return Err(QuoteError::InsufficientLiquidity);
        }
        let dy = sub(sub(self.xp[j], y)?, U256::from(1))?;
        let fee = div(mul(self.fee, dy)?, U256::from(10_000_000_000u64))?;
        // The two-coin views deduct fee in normalized units *before* converting.
        div(mul(sub(dy, fee)?, precision)?, rates[j])
    }
    pub fn mid_price(&self, i: usize, j: usize) -> Result<f64, QuoteError> {
        Ok(u256_to_f64(self.amount_out(i, j, U256::from(1_000_000))?) / 1_000_000.0)
    }
}
