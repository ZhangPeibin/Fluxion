use alloy_primitives::U256;
use fluxion::{curve_local::Classic3Pool, error::QuoteError};

#[test]
fn classic_three_coin_quotes_preserve_decimals_fees_and_all_directions() {
    // Balanced virtual reserves: 1,000,000 whole tokens of each denomination.
    let state = Classic3Pool::new(
        [
            U256::from(10u128.pow(24)),
            U256::from(10u128.pow(12)),
            U256::from(10u128.pow(12)),
        ],
        U256::from(4000),
        U256::from(1_500_000),
    )
    .unwrap();
    for i in 0..3 {
        for j in 0..3 {
            if i == j {
                continue;
            }
            let input = if i == 0 {
                U256::from(10u128.pow(18))
            } else {
                U256::from(1_000_000)
            };
            let unit = if j == 0 {
                U256::from(10u128.pow(18))
            } else {
                U256::from(1_000_000)
            };
            let out = state.amount_out(i, j, input).unwrap();
            assert!(out > unit * U256::from(999) / U256::from(1000) && out < unit);
        }
    }
    assert_eq!(
        state.amount_out(0, 0, U256::from(1)),
        Err(QuoteError::UnknownPool)
    );
    assert_eq!(
        state.amount_out(0, 1, U256::ZERO),
        Err(QuoteError::ZeroAmount)
    );
}

#[test]
fn all_recorded_mainnet_vectors_match_raw_integer_outputs_offline() {
    for text in [
        include_str!("fixtures/curve-3pool-20000000.json"),
        include_str!("fixtures/curve-3pool-26331147.json"),
    ] {
        let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        let parse = |v: &serde_json::Value| U256::from_str_radix(v.as_str().unwrap(), 10).unwrap();
        let balances = std::array::from_fn(|i| parse(&fixture["balances"][i]));
        let state = Classic3Pool::new(
            balances,
            parse(&fixture["amplification"]),
            parse(&fixture["fee"]),
        )
        .unwrap();
        let quotes = fixture["quotes"].as_array().unwrap();
        assert_eq!(quotes.len(), 32);
        for q in quotes {
            let out = state
                .amount_out(
                    q["i"].as_u64().unwrap() as usize,
                    q["j"].as_u64().unwrap() as usize,
                    parse(&q["amount_in"]),
                )
                .unwrap();
            assert_eq!(out, parse(&q["amount_out"]), "{q}");
        }
    }
}

#[test]
fn empty_liquidity_invalid_parameters_and_overflow_are_distinct() {
    let state =
        Classic3Pool::new([U256::ZERO; 3], U256::from(4000), U256::from(1_500_000)).unwrap();
    assert!(!state.is_liquid());
    assert_eq!(
        state.amount_out(0, 2, U256::from(1000)),
        Err(QuoteError::InsufficientLiquidity)
    );
    assert!(Classic3Pool::new([U256::from(1000); 3], U256::ZERO, U256::ZERO).is_err());
    let state = Classic3Pool::new([U256::from(1000); 3], U256::from(4000), U256::ZERO).unwrap();
    assert_eq!(state.amount_out(0, 1, U256::MAX), Err(QuoteError::Overflow));
}
