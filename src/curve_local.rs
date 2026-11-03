//! Verified deployment dispatch and unchanged classic 3pool arithmetic.
//! The two-coin models live separately; no generic ABI-based crypto acceptance.
use crate::{
    curve_plain::{Plain2Pool, PlainProfile},
    error::QuoteError,
    model::{Pool, PoolKind},
    util::u256_to_f64,
};
use alloy_primitives::{Address, U256};

pub const THREE_POOL: &str = "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7";
pub const PROFILE: &str = "classic-3pool";
pub const COINS: [&str; 3] = [
    "0x6b175474e89094c44da98b954eedeac495271d0f",
    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
    "0xdac17f958d2ee523a2206206994597c13d831ec7",
];

pub(crate) fn supported(pool: &Pool) -> bool {
    profile_for(pool).is_some()
}

pub(crate) fn profile_for(pool: &Pool) -> Option<CurveProfile> {
    if classic_supported(pool) {
        Some(CurveProfile::Classic3)
    } else {
        PlainProfile::for_pool(pool).map(CurveProfile::Plain)
    }
}

fn classic_supported(pool: &Pool) -> bool {
    pool.kind == PoolKind::Curve
        && pool.address == THREE_POOL.parse::<Address>().unwrap()
        && pool.coins == COINS.map(|c| c.parse::<Address>().unwrap())
}

#[derive(Debug, Clone, Copy)]
pub enum CurveProfile {
    Classic3,
    Plain(PlainProfile),
}

impl CurveProfile {
    pub fn pool(self) -> Pool {
        match self {
            Self::Classic3 => Pool {
                kind: PoolKind::Curve,
                address: THREE_POOL.parse().unwrap(),
                coins: COINS.map(|c| c.parse().unwrap()).to_vec(),
            },
            Self::Plain(profile) => profile.pool(),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Classic3 => PROFILE,
            Self::Plain(p) => p.name(),
        }
    }
    pub fn a_precision(self) -> u64 {
        match self {
            Self::Classic3 => 1,
            Self::Plain(_) => 100,
        }
    }
    pub fn a_getter(self) -> &'static str {
        match self {
            Self::Classic3 => "A()",
            Self::Plain(_) => "A_precise()",
        }
    }
    pub fn balance_source(self) -> &'static str {
        match self {
            Self::Classic3 => "stored",
            Self::Plain(p) => p.balance_source(),
        }
    }
    pub fn units(self) -> Vec<U256> {
        match self {
            Self::Classic3 => vec![
                U256::from(10u128.pow(18)),
                U256::from(1_000_000),
                U256::from(1_000_000),
            ],
            Self::Plain(p) => p.units().to_vec(),
        }
    }
    pub(crate) fn build(
        self,
        balances: &[U256],
        a: U256,
        fee: U256,
    ) -> Result<CurveState, QuoteError> {
        match self {
            Self::Classic3 => Classic3Pool::new(
                balances
                    .try_into()
                    .map_err(|_| QuoteError::IncompleteState)?,
                a,
                fee,
            )
            .map(CurveState::Classic3),
            Self::Plain(p) => Plain2Pool::new(
                p,
                balances
                    .try_into()
                    .map_err(|_| QuoteError::IncompleteState)?,
                a,
                fee,
            )
            .map(CurveState::Plain),
        }
    }
}

pub enum CurveState {
    Classic3(Classic3Pool),
    Plain(Plain2Pool),
}
impl CurveState {
    pub fn profile(&self) -> CurveProfile {
        match self {
            Self::Classic3(_) => CurveProfile::Classic3,
            Self::Plain(p) => CurveProfile::Plain(p.profile()),
        }
    }
    pub fn balances(&self) -> &[U256] {
        match self {
            Self::Classic3(s) => s.balances(),
            Self::Plain(s) => s.balances(),
        }
    }
    pub fn amplification(&self) -> U256 {
        match self {
            Self::Classic3(s) => s.amplification(),
            Self::Plain(s) => s.amplification(),
        }
    }
    pub fn fee(&self) -> U256 {
        match self {
            Self::Classic3(s) => s.fee(),
            Self::Plain(s) => s.fee(),
        }
    }
    pub fn is_liquid(&self) -> bool {
        match self {
            Self::Classic3(s) => s.is_liquid(),
            Self::Plain(s) => s.is_liquid(),
        }
    }
    pub fn amount_out(&self, i: usize, j: usize, amount: U256) -> Result<U256, QuoteError> {
        match self {
            Self::Classic3(s) => s.amount_out(i, j, amount),
            Self::Plain(s) => s.amount_out(i, j, amount),
        }
    }
    pub fn mid_price(&self, i: usize, j: usize) -> Result<f64, QuoteError> {
        match self {
            Self::Classic3(s) => s.mid_price(i, j),
            Self::Plain(s) => s.mid_price(i, j),
        }
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

fn invariant(xp: &[U256; 3], ann: U256) -> Result<U256, QuoteError> {
    let n = U256::from(3);
    let sum = xp.iter().try_fold(U256::ZERO, |s, x| add(s, *x))?;
    let mut d = sum;
    for _ in 0..255 {
        let mut product = d;
        for x in xp {
            product = div(mul(product, d)?, mul(*x, n)?)?;
        }
        let previous = d;
        let numerator = mul(add(mul(ann, sum)?, mul(product, n)?)?, d)?;
        let denominator = add(
            mul(sub(ann, U256::from(1))?, d)?,
            mul(U256::from(4), product)?,
        )?;
        d = div(numerator, denominator)?;
        if close(d, previous) {
            break;
        }
    }
    // The deployed classic profile returns its final iterate after the bound.
    Ok(d)
}

pub struct Classic3Pool {
    balances: [U256; 3],
    rates: [U256; 3],
    xp: [U256; 3],
    amp: U256,
    fee: U256,
    d: Option<U256>,
}

impl Classic3Pool {
    pub fn new(balances: [U256; 3], amp: U256, fee: U256) -> Result<Self, QuoteError> {
        if amp.is_zero() || fee >= U256::from(10_000_000_000u64) {
            return Err(QuoteError::IncompleteState);
        }
        let precision = U256::from(10u128.pow(18));
        let rates = [
            precision,
            U256::from(10u128.pow(30)),
            U256::from(10u128.pow(30)),
        ];
        let mut xp = [U256::ZERO; 3];
        for i in 0..3 {
            xp[i] = div(mul(rates[i], balances[i])?, precision)?;
        }
        // Empty reserves are known state, not missing data. Such a pool cannot
        // quote swaps; it should not poison other fully loaded candidates.
        let d = if xp.iter().any(U256::is_zero) {
            None
        } else {
            Some(invariant(&xp, mul(amp, U256::from(3))?)?)
        };
        Ok(Self {
            balances,
            rates,
            xp,
            amp,
            fee,
            d,
        })
    }
    pub fn balances(&self) -> &[U256; 3] {
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

    fn y(&self, i: usize, j: usize, x: U256) -> Result<U256, QuoteError> {
        let d = self.d.ok_or(QuoteError::InsufficientLiquidity)?;
        let n = U256::from(3);
        let ann = mul(self.amp, n)?;
        let mut sum = U256::ZERO;
        let mut c = d;
        for k in 0..3 {
            if k == j {
                continue;
            }
            let value = if k == i { x } else { self.xp[k] };
            sum = add(sum, value)?;
            c = div(mul(c, d)?, mul(value, n)?)?;
        }
        c = div(mul(c, d)?, mul(ann, n)?)?;
        let b = add(sum, div(d, ann)?)?;
        let mut y = d;
        for _ in 0..255 {
            let previous = y;
            y = div(
                add(mul(y, y)?, c)?,
                sub(add(mul(U256::from(2), y)?, b)?, d)?,
            )?;
            if close(y, previous) {
                break;
            }
        }
        Ok(y)
    }

    pub fn amount_out(&self, i: usize, j: usize, amount: U256) -> Result<U256, QuoteError> {
        if i >= 3 || j >= 3 || i == j {
            return Err(QuoteError::UnknownPool);
        }
        if amount.is_zero() {
            return Err(QuoteError::ZeroAmount);
        }
        let precision = U256::from(10u128.pow(18));
        let x = add(self.xp[i], div(mul(amount, self.rates[i])?, precision)?)?;
        let y = self.y(i, j, x)?;
        // Preserve the deployed *view*'s order: subtract 1, convert units,
        // then deduct fee. Its exchange function has a different order.
        let dy = div(
            mul(sub(sub(self.xp[j], y)?, U256::from(1))?, precision)?,
            self.rates[j],
        )?;
        let fee = div(mul(self.fee, dy)?, U256::from(10_000_000_000u64))?;
        sub(dy, fee)
    }

    pub fn mid_price(&self, i: usize, j: usize) -> Result<f64, QuoteError> {
        // Keep the existing RPC adapter's coarse-price sampling convention.
        let input = U256::from(1_000_000);
        Ok(u256_to_f64(self.amount_out(i, j, input)?) / u256_to_f64(input))
    }
}
