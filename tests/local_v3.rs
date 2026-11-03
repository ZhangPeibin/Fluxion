use alloy_primitives::U256;
use fluxion::{
    error::QuoteError,
    v3_local::{V3State, sqrt_ratio_at_tick},
};
use std::collections::BTreeMap;

#[test]
fn tick_prices_match_protocol_boundary_vectors() {
    // Integer reference vectors from the V3 TickMath definition.
    for (tick, expected) in [
        (-887272, "4295128739"),
        (-1, "79224201403219477170569942574"),
        (0, "79228162514264337593543950336"),
        (1, "79232123823359799118286999568"),
        (887272, "1461446703485210103287273052203988822378723970342"),
    ] {
        assert_eq!(sqrt_ratio_at_tick(tick).unwrap().to_string(), expected);
    }
}

#[test]
fn missing_bitmap_words_are_not_assumed_to_have_no_liquidity() {
    let state = V3State::new(
        sqrt_ratio_at_tick(0).unwrap(),
        0,
        1_000_000_000_000_000_000,
        3000,
        60,
        BTreeMap::from([(0, U256::ZERO)]),
        BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        state.quote(U256::from(1_000_000), true).unwrap_err(),
        QuoteError::IncompleteState
    );
}

#[test]
fn swaps_cross_signed_ticks_and_empty_liquidity_gaps() {
    let maps = BTreeMap::from([(-1, U256::from(1) << 255), (0, U256::from(1) << 1)]);
    let state = V3State::new(
        sqrt_ratio_at_tick(0).unwrap(),
        0,
        100_000,
        3000,
        60,
        maps,
        BTreeMap::from([(-60, 50_000), (60, -50_000)]),
    )
    .unwrap();
    for direction in [true, false] {
        let quote = state.quote(U256::from(2000), direction).unwrap();
        assert_eq!(quote.initialized_ticks_crossed, 1);
        assert!(quote.steps >= 2);
        assert!(quote.amount_out > U256::from(1800) && quote.amount_out < U256::from(2000));
    }
    let gap = V3State::new(
        sqrt_ratio_at_tick(0).unwrap(),
        0,
        0,
        3000,
        60,
        BTreeMap::from([(-1, U256::from(1) << 254), (0, U256::ZERO)]),
        BTreeMap::from([(-120, -100_000)]),
    )
    .unwrap();
    let quote = gap.quote(U256::from(1000), true).unwrap();
    assert_eq!(quote.initialized_ticks_crossed, 1);
    assert!(quote.amount_out > U256::ZERO);
}

#[test]
fn protocol_rounding_tiny_inputs_and_invalid_states_are_explicit() {
    let state = V3State::new(
        sqrt_ratio_at_tick(0).unwrap(),
        0,
        1_000_000_000_000_000_000,
        3000,
        60,
        BTreeMap::from([(-1, U256::ZERO), (0, U256::ZERO)]),
        BTreeMap::new(),
    )
    .unwrap();
    for direction in [true, false] {
        assert_eq!(
            state.quote(U256::from(1000), direction).unwrap().amount_out,
            U256::from(996)
        );
        assert_eq!(
            state.quote(U256::from(1), direction).unwrap().amount_out,
            U256::ZERO
        );
    }
    assert!(state.quote(U256::MAX, true).is_err());
    assert!(
        V3State::new(
            sqrt_ratio_at_tick(0).unwrap(),
            0,
            1000,
            3000,
            60,
            BTreeMap::from([(0, U256::from(1))]),
            BTreeMap::new()
        )
        .is_err()
    );
}

#[test]
fn wide_tick_spacing_and_large_inputs_keep_full_width_integer_arithmetic() {
    let liquidity = 1i128 << 120;
    let state = V3State::new(
        sqrt_ratio_at_tick(800_000).unwrap(),
        800_000,
        liquidity as u128,
        3000,
        16383,
        BTreeMap::from([(-1, U256::from(1) << 202), (0, U256::from(1) << 54)]),
        BTreeMap::from([(-884682, liquidity), (884682, -liquidity)]),
    )
    .unwrap();
    let first = state.quote(U256::from(1) << 120, true).unwrap();
    let second = state.quote(U256::from(1) << 121, true).unwrap();
    assert!(first.amount_out > U256::ZERO);
    assert!(second.amount_out > first.amount_out);
    assert!(first.sqrt_price_after < state.sqrt_price());
    assert!(second.sqrt_price_after < first.sqrt_price_after);
    assert_eq!(
        state.quote(U256::from(1) << 240, true).unwrap_err(),
        QuoteError::InsufficientLiquidity,
        "a global price limit cannot turn partially consumed input into a full exact-in quote"
    );
}
