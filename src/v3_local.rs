//! Network-free exact-input V3 arithmetic. Q64.96 prices, full-width products,
//! protocol rounding, and bitmap-word boundaries are all kept in integers.
//! Formula reference: Uniswap V3 TickMath, SqrtPriceMath and SwapMath definitions.
use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::error::QuoteError;
use alloy_primitives::{U256, U512};

pub const MIN_TICK: i32 = -887272;
pub const MAX_TICK: i32 = 887272;
const Q96: U256 = U256::from_limbs([0, 1 << 32, 0, 0]);

pub fn sqrt_ratio_at_tick(tick: i32) -> Result<U256, QuoteError> {
    if !(MIN_TICK..=MAX_TICK).contains(&tick) {
        return Err(QuoteError::IncompleteState);
    }
    // floor(2^128 / sqrt(1.0001)^(2^bit)), with the protocol's fixed-point
    // constants. Multiplication rounds down at each binary exponent step.
    static FACTORS: OnceLock<Vec<U256>> = OnceLock::new();
    let factors = FACTORS.get_or_init(|| {
        [
            "fffcb933bd6fad37aa2d162d1a594001",
            "fff97272373d413259a46990580e213a",
            "fff2e50f5f656932ef12357cf3c7fdcc",
            "ffe5caca7e10e4e61c3624eaa0941cd0",
            "ffcb9843d60f6159c9db58835c926644",
            "ff973b41fa98c081472e6896dfb254c0",
            "ff2ea16466c96a3843ec78b326b52861",
            "fe5dee046a99a2a811c461f1969c3053",
            "fcbe86c7900a88aedcffc83b479aa3a4",
            "f987a7253ac413176f2b074cf7815e54",
            "f3392b0822b70005940c7a398e4b70f3",
            "e7159475a2c29b7443b29c7fa6e889d9",
            "d097f3bdfd2022b8845ad8f792aa5825",
            "a9f746462d870fdf8a65dc1f90e061e5",
            "70d869a156d2a1b890bb3df62baf32f7",
            "31be135f97d08fd981231505542fcfa6",
            "9aa508b5b7a84e1c677de54f3e99bc9",
            "5d6af8dedb81196699c329225ee604",
            "2216e584f5fa1ea926041bedfe98",
            "48a170391f7dc42444e8fa2",
        ]
        .iter()
        .map(|s| U256::from_str_radix(s, 16).unwrap())
        .collect()
    });
    let mut ratio: U256 = U256::from(1) << 128;
    for (bit, factor) in factors.iter().enumerate() {
        if tick.unsigned_abs() & (1 << bit) != 0 {
            ratio = ratio.checked_mul(*factor).ok_or(QuoteError::Overflow)? >> 128;
        }
    }
    if tick > 0 {
        ratio = U256::MAX / ratio;
    }
    Ok((ratio >> 32) + U256::from((ratio & U256::from(u32::MAX)) != U256::ZERO))
}

fn mul_div(a: U256, b: U256, den: U256, up: bool) -> Result<U256, QuoteError> {
    if den.is_zero() {
        return Err(QuoteError::InsufficientLiquidity);
    }
    let product = U512::from(a) * U512::from(b);
    let d = U512::from(den);
    let mut value = product / d;
    if up && product % d != U512::ZERO {
        value += U512::from(1);
    }
    if value.bit_len() > 256 {
        return Err(QuoteError::Overflow);
    }
    let limbs = value.as_limbs();
    Ok(U256::from_limbs([limbs[0], limbs[1], limbs[2], limbs[3]]))
}

fn div_up(a: U256, b: U256) -> Result<U256, QuoteError> {
    if b.is_zero() {
        return Err(QuoteError::InsufficientLiquidity);
    }
    Ok(a / b + U256::from(a % b != U256::ZERO))
}

fn delta0(a: U256, b: U256, liquidity: u128, up: bool) -> Result<U256, QuoteError> {
    let (low, high) = (a.min(b), a.max(b));
    let numerator = U256::from(liquidity) << 96;
    let first = mul_div(numerator, high - low, high, up)?;
    if up {
        div_up(first, low)
    } else if low.is_zero() {
        Err(QuoteError::InsufficientLiquidity)
    } else {
        Ok(first / low)
    }
}

fn delta1(a: U256, b: U256, liquidity: u128, up: bool) -> Result<U256, QuoteError> {
    mul_div(U256::from(liquidity), a.max(b) - a.min(b), Q96, up)
}

fn next_price(
    price: U256,
    liquidity: u128,
    amount: U256,
    zero_for_one: bool,
) -> Result<U256, QuoteError> {
    if amount.is_zero() {
        return Ok(price);
    }
    if liquidity == 0 {
        return Err(QuoteError::InsufficientLiquidity);
    }
    if zero_for_one {
        let numerator = U256::from(liquidity) << 96;
        if let Some(denominator) = amount
            .checked_mul(price)
            .and_then(|p| p.checked_add(numerator))
        {
            mul_div(numerator, price, denominator, true)
        } else {
            // Preserve the overflow fallback's division/rounding order, rather
            // than using a mathematically equivalent but differently rounded ratio.
            div_up(
                numerator,
                (numerator / price)
                    .checked_add(amount)
                    .ok_or(QuoteError::Overflow)?,
            )
        }
    } else {
        price
            .checked_add(mul_div(amount, Q96, U256::from(liquidity), false)?)
            .ok_or(QuoteError::Overflow)
    }
}

#[derive(Debug)]
pub struct V3Quote {
    pub amount_out: U256,
    pub sqrt_price_after: U256,
    pub initialized_ticks_crossed: usize,
    pub steps: usize,
}

pub struct V3State {
    sqrt_price: U256,
    tick: i32,
    liquidity: u128,
    fee: u32,
    spacing: i32,
    bitmaps: BTreeMap<i16, U256>,
    ticks: BTreeMap<i32, i128>,
}

impl V3State {
    pub fn new(
        sqrt_price: U256,
        tick: i32,
        liquidity: u128,
        fee: u32,
        spacing: i32,
        bitmaps: BTreeMap<i16, U256>,
        ticks: BTreeMap<i32, i128>,
    ) -> Result<Self, QuoteError> {
        if spacing <= 0
            || spacing > 32767
            || fee >= 1_000_000
            || !(MIN_TICK..MAX_TICK).contains(&tick)
            || sqrt_price < sqrt_ratio_at_tick(tick)?
            || sqrt_price > sqrt_ratio_at_tick(tick + 1)?
            || sqrt_price >= sqrt_ratio_at_tick(MAX_TICK)?
        {
            return Err(QuoteError::IncompleteState);
        }
        let center = i16::try_from(tick.div_euclid(spacing).div_euclid(256))
            .map_err(|_| QuoteError::IncompleteState)?;
        if !bitmaps.contains_key(&center) {
            return Err(QuoteError::IncompleteState);
        }
        // Every set bitmap bit must have its corresponding liquidityNet.
        for (&word, &bitmap) in &bitmaps {
            if i32::from(word) < MIN_TICK.div_euclid(spacing).div_euclid(256)
                || i32::from(word) > MAX_TICK.div_euclid(spacing).div_euclid(256)
            {
                return Err(QuoteError::IncompleteState);
            }
            for bit in 0..256 {
                if (bitmap >> bit) & U256::from(1) != U256::ZERO {
                    let t = (i32::from(word) * 256 + bit) * spacing;
                    if !(MIN_TICK..=MAX_TICK).contains(&t) || !ticks.contains_key(&t) {
                        return Err(QuoteError::IncompleteState);
                    }
                }
            }
        }
        for &t in ticks.keys() {
            if t % spacing != 0 {
                return Err(QuoteError::IncompleteState);
            }
            let compressed = t.div_euclid(spacing);
            let word = i16::try_from(compressed.div_euclid(256))
                .map_err(|_| QuoteError::IncompleteState)?;
            if bitmaps.get(&word).is_none_or(|map| {
                (*map >> compressed.rem_euclid(256)) & U256::from(1) == U256::ZERO
            }) {
                return Err(QuoteError::IncompleteState);
            }
        }
        Ok(Self {
            sqrt_price,
            tick,
            liquidity,
            fee,
            spacing,
            bitmaps,
            ticks,
        })
    }

    pub fn sqrt_price(&self) -> U256 {
        self.sqrt_price
    }
    pub fn covered_words(&self) -> (i16, i16) {
        (
            *self.bitmaps.first_key_value().unwrap().0,
            *self.bitmaps.last_key_value().unwrap().0,
        )
    }
    pub fn initialized_tick_count(&self) -> usize {
        self.ticks.len()
    }

    fn next_tick(&self, tick: i32, zero_for_one: bool) -> Result<(i32, bool), QuoteError> {
        let compressed = tick.div_euclid(self.spacing) + i32::from(!zero_for_one);
        let word =
            i16::try_from(compressed.div_euclid(256)).map_err(|_| QuoteError::IncompleteState)?;
        let bit = compressed.rem_euclid(256) as usize;
        let bitmap = *self.bitmaps.get(&word).ok_or(QuoteError::IncompleteState)?;
        let masked = bitmap
            & if zero_for_one {
                U256::MAX >> (255 - bit)
            } else {
                U256::MAX << bit
            };
        let initialized = !masked.is_zero();
        let position = if zero_for_one {
            if initialized {
                masked.bit_len() as i32 - 1
            } else {
                0
            }
        } else if initialized {
            masked.trailing_zeros() as i32
        } else {
            255
        };
        let next = (i32::from(word) * 256 + position) * self.spacing;
        Ok((next.clamp(MIN_TICK, MAX_TICK), initialized))
    }

    pub fn quote(&self, amount: U256, zero_for_one: bool) -> Result<V3Quote, QuoteError> {
        if amount.is_zero() {
            return Err(QuoteError::ZeroAmount);
        }
        if amount.bit_len() > 255 {
            return Err(QuoteError::Overflow);
        } // protocol uses signed amountSpecified
        // A complete, empty global bitmap proves that no liquidity can be
        // reached. A partial window (including one with a hole) cannot prove it.
        if self.liquidity == 0 && self.ticks.is_empty() {
            let low = MIN_TICK.div_euclid(self.spacing).div_euclid(256) as i16;
            let high = MAX_TICK.div_euclid(self.spacing).div_euclid(256) as i16;
            if self.covered_words() == (low, high)
                && self.bitmaps.len() == usize::from((high - low) as u16) + 1
            {
                return Err(QuoteError::InsufficientLiquidity);
            }
        }
        let limit = if zero_for_one {
            sqrt_ratio_at_tick(MIN_TICK)? + U256::from(1)
        } else {
            sqrt_ratio_at_tick(MAX_TICK)? - U256::from(1)
        };
        let mut price = self.sqrt_price;
        let mut tick = self.tick;
        let mut liquidity = self.liquidity;
        let mut remaining = amount;
        let mut out = U256::ZERO;
        let mut crossed = 0;
        let mut steps = 0;
        while !remaining.is_zero() && price != limit {
            let (next_tick, initialized) = self.next_tick(tick, zero_for_one)?;
            let boundary = sqrt_ratio_at_tick(next_tick)?;
            let target = if zero_for_one {
                boundary.max(limit)
            } else {
                boundary.min(limit)
            };
            let available = mul_div(
                remaining,
                U256::from(1_000_000 - self.fee),
                U256::from(1_000_000),
                false,
            )?;
            let required = if zero_for_one {
                delta0(target, price, liquidity, true)?
            } else {
                delta1(price, target, liquidity, true)?
            };
            let next = if available >= required {
                target
            } else {
                next_price(price, liquidity, available, zero_for_one)?
            };
            let used = if next == target {
                required
            } else if zero_for_one {
                delta0(next, price, liquidity, true)?
            } else {
                delta1(price, next, liquidity, true)?
            };
            let received = if zero_for_one {
                delta1(next, price, liquidity, false)?
            } else {
                delta0(price, next, liquidity, false)?
            };
            let fee = if next != target {
                remaining.checked_sub(used).ok_or(QuoteError::Overflow)?
            } else {
                mul_div(
                    used,
                    U256::from(self.fee),
                    U256::from(1_000_000 - self.fee),
                    true,
                )?
            };
            remaining = remaining
                .checked_sub(used.checked_add(fee).ok_or(QuoteError::Overflow)?)
                .ok_or(QuoteError::Overflow)?;
            out = out.checked_add(received).ok_or(QuoteError::Overflow)?;
            steps += 1;
            price = next;
            if next == boundary {
                if initialized {
                    let net = *self
                        .ticks
                        .get(&next_tick)
                        .ok_or(QuoteError::IncompleteState)?;
                    let subtract = (net >= 0) == zero_for_one;
                    liquidity = if subtract {
                        liquidity.checked_sub(net.unsigned_abs())
                    } else {
                        liquidity.checked_add(net.unsigned_abs())
                    }
                    .ok_or(QuoteError::IncompleteState)?;
                    crossed += 1;
                }
                tick = next_tick - i32::from(zero_for_one);
            }
            // A partial step spends all remaining input (fee included). An
            // explicit global sqrt limit also ends the pool's swap loop.
        }
        if !remaining.is_zero() {
            // QuoterV2 can report a partial pool swap at the global limit. A
            // local exact-in allocation must actually consume its full input.
            return Err(QuoteError::InsufficientLiquidity);
        }
        Ok(V3Quote {
            amount_out: out,
            sqrt_price_after: price,
            initialized_ticks_crossed: crossed,
            steps,
        })
    }
}
