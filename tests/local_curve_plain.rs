use alloy_primitives::U256;
use fluxion::curve_plain::{Plain2Pool, PlainProfile};

#[test]
fn both_deployment_models_preserve_normalized_fees_and_two_coin_precision() {
    for profile in [PlainProfile::TwoCrv, PlainProfile::FraxUsdc] {
        let units = profile.units();
        let state = Plain2Pool::new(
            profile,
            [
                units[0] * U256::from(1_000_000),
                units[1] * U256::from(1_000_000),
            ],
            U256::from(150_000),
            U256::from(3_000_000),
        )
        .unwrap();
        for i in 0..2 {
            let out = state.amount_out(i, 1 - i, units[i]).unwrap();
            assert!(out < units[1 - i]);
            assert!(out > units[1 - i] * U256::from(999) / U256::from(1000));
        }
    }
}

#[test]
fn independent_historical_vectors_match_each_deployment_without_tolerance() {
    let mut count = 0;
    for text in [
        include_str!("fixtures/curve-2crv-20000000.json"),
        include_str!("fixtures/curve-2crv-26331147.json"),
        include_str!("fixtures/curve-frax-usdc-20000000.json"),
        include_str!("fixtures/curve-frax-usdc-26331147.json"),
    ] {
        let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        let parse = |v: &serde_json::Value| U256::from_str_radix(v.as_str().unwrap(), 10).unwrap();
        let profile = if fixture["profile"] == "plain-2crv" {
            PlainProfile::TwoCrv
        } else {
            PlainProfile::FraxUsdc
        };
        let state = Plain2Pool::new(
            profile,
            std::array::from_fn(|i| parse(&fixture["balances"][i])),
            parse(&fixture["amplification"]),
            parse(&fixture["fee"]),
        )
        .unwrap();
        for q in fixture["quotes"].as_array().unwrap() {
            let actual = state.amount_out(
                q["i"].as_u64().unwrap() as usize,
                q["j"].as_u64().unwrap() as usize,
                parse(&q["amount_in"]),
            );
            if q["success"] == true {
                assert_eq!(
                    actual.unwrap(),
                    parse(&q["amount_out"]),
                    "{} {q}",
                    fixture["profile"]
                );
            } else {
                assert!(actual.is_err());
            }
            count += 1;
        }
    }
    assert_eq!(count, 58);
}

#[test]
fn precise_a_is_not_truncated_and_zero_or_overflow_inputs_are_explicit() {
    use fluxion::error::QuoteError;
    let profile = PlainProfile::FraxUsdc;
    let units = profile.units();
    let balances = units.map(|u| u * U256::from(1_000_000));
    let first = Plain2Pool::new(
        profile,
        balances,
        U256::from(150_000),
        U256::from(1_000_000),
    )
    .unwrap();
    let fractional = Plain2Pool::new(
        profile,
        balances,
        U256::from(150_099),
        U256::from(1_000_000),
    )
    .unwrap();
    let amount = units[0] * U256::from(500_000);
    assert_ne!(
        first.amount_out(0, 1, amount).unwrap(),
        fractional.amount_out(0, 1, amount).unwrap()
    );
    assert_eq!(
        first.amount_out(0, 1, U256::ZERO),
        Err(QuoteError::ZeroAmount)
    );
    assert_eq!(first.amount_out(0, 1, U256::MAX), Err(QuoteError::Overflow));
    let empty = Plain2Pool::new(profile, [U256::ZERO; 2], U256::from(150_000), U256::ZERO).unwrap();
    assert!(!empty.is_liquid());
    assert_eq!(
        empty.amount_out(0, 1, U256::from(1)),
        Err(QuoteError::InsufficientLiquidity)
    );
}
