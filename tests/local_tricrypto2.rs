use alloy_primitives::U256;
use fluxion::tricrypto2_local::{Tricrypto2Parameters, Tricrypto2State};
use serde_json::Value;

fn number(v: &Value) -> U256 {
    U256::from_str_radix(v.as_str().unwrap(), 10).unwrap()
}
fn state(f: &Value) -> Tricrypto2State {
    let p = &f["parameters"];
    Tricrypto2State::new(
        std::array::from_fn(|i| number(&f["balances"][i])),
        Tricrypto2Parameters {
            amplification: number(&p["amplification"]),
            gamma: number(&p["gamma"]),
            stored_d: number(&p["stored_d"]),
            price_scale: std::array::from_fn(|i| number(&p["price_scale"][i])),
            mid_fee: number(&p["mid_fee"]),
            out_fee: number(&p["out_fee"]),
            fee_gamma: number(&p["fee_gamma"]),
        },
    )
    .unwrap()
}

#[test]
fn independent_deployed_views_match_exactly_including_reverts() {
    let mut count = 0;
    let mut rejected = 0;
    for text in [
        include_str!("fixtures/tricrypto2-20000000.json"),
        include_str!("fixtures/tricrypto2-26331640.json"),
    ] {
        let f: Value = serde_json::from_str(text).unwrap();
        let state = state(&f);
        for q in f["quotes"].as_array().unwrap() {
            let result = state.amount_out(
                q["i"].as_u64().unwrap() as usize,
                q["j"].as_u64().unwrap() as usize,
                number(&q["amount_in"]),
            );
            if q["success"] == true {
                assert_eq!(
                    result.unwrap(),
                    number(&q["amount_out"]),
                    "block={} {q}",
                    f["block"]["number"]
                );
            } else {
                assert!(
                    result.is_err(),
                    "unexpected success at block={} {q}",
                    f["block"]["number"]
                );
                rejected += 1;
            }
            count += 1;
        }
    }
    assert_eq!(count, 104);
    assert_eq!(rejected, 14);
}

#[test]
fn fees_are_amount_dependent_and_stored_d_is_not_refitted() {
    let f: Value = serde_json::from_str(include_str!("fixtures/tricrypto2-26331640.json")).unwrap();
    let model = state(&f);
    let small = model.quote(0, 2, U256::from(1_000_000_000u64)).unwrap();
    let large = model.quote(0, 2, U256::from(100_000_000_000u64)).unwrap();
    assert_ne!(small.fee_rate, large.fee_rate);
    for quote in [&small, &large] {
        assert!(
            quote.fee_rate >= model.parameters().mid_fee
                && quote.fee_rate <= model.parameters().out_fee
        );
        assert!((1..=255).contains(&quote.iterations));
    }
    let mut parameters = model.parameters().clone();
    parameters.stored_d += U256::from(10u128.pow(23));
    let changed = Tricrypto2State::new(*model.balances(), parameters).unwrap();
    assert_ne!(
        changed.amount_out(0, 2, U256::from(1_000_000_000u64)),
        Ok(small.amount_out)
    );
}

#[test]
fn bounds_empty_state_and_overflow_fail_without_fabricated_outputs() {
    use fluxion::error::QuoteError;
    let f: Value = serde_json::from_str(include_str!("fixtures/tricrypto2-26331640.json")).unwrap();
    let model = state(&f);
    assert_eq!(
        model.amount_out(0, 2, U256::ZERO),
        Err(QuoteError::ZeroAmount)
    );
    assert_eq!(model.amount_out(0, 2, U256::MAX), Err(QuoteError::Overflow));
    assert_eq!(
        model.amount_out(3, 0, U256::from(1)),
        Err(QuoteError::UnknownPool)
    );
    for invalid in 0..9 {
        let mut p = model.parameters().clone();
        match invalid {
            0 => p.amplification = U256::from(2699),
            1 => p.gamma = U256::from(1),
            2 => p.price_scale[0] = U256::ZERO,
            3 => p.stored_d = U256::MAX,
            4 => p.fee_gamma = U256::ZERO,
            5 => p.fee_gamma = U256::from(10u128.pow(18)),
            6 => p.mid_fee = p.out_fee + U256::from(1),
            7 => p.out_fee = U256::from(499_999),
            _ => p.gamma = U256::from(10_000_000_000_000_001u64),
        }
        assert!(Tricrypto2State::new(*model.balances(), p).is_err());
    }
    let mut p = model.parameters().clone();
    p.stored_d = U256::ZERO;
    let empty = Tricrypto2State::new([U256::ZERO; 3], p).unwrap();
    assert!(!empty.is_liquid());
    assert_eq!(
        empty.amount_out(0, 2, U256::from(1)),
        Err(QuoteError::InsufficientLiquidity)
    );
}
