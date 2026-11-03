use alloy_primitives::U256;
use fluxion::{error::QuoteError, wsteth_local::WstEthState};

#[test]
fn lido_v4_uses_internal_ratio_and_protocol_rounding_not_total_ratio() {
    // Internal ETH/shares = 10/7; external shares=2 represents floor(20/7)=2
    // external ETH. Using the public totals 12/9 would unwrap 7 into 9, not 10.
    let state = WstEthState::new(
        4,
        U256::from(12),
        U256::from(9),
        U256::from(2),
        U256::from(2),
    )
    .unwrap();
    assert_eq!(
        state.amount_out(U256::from(10), true).unwrap(),
        U256::from(7)
    );
    assert_eq!(
        state.amount_out(U256::from(7), false).unwrap(),
        U256::from(10)
    );
    assert_eq!(
        state.amount_out(U256::from(9), true).unwrap(),
        U256::from(6)
    );
    assert_eq!(
        state.amount_out(U256::from(6), false).unwrap(),
        U256::from(8)
    );
    assert_eq!(state.amount_out(U256::from(1), true).unwrap(), U256::ZERO);
    assert_eq!(
        state.amount_out(U256::from(1), false).unwrap(),
        U256::from(1)
    );
    assert_eq!(
        state.amount_out(U256::from(u128::MAX), true),
        Err(QuoteError::Overflow)
    );
    assert!(state.amount_out(U256::from(u128::MAX - 1), false).is_ok());
    assert_eq!(
        state.amount_out(U256::ZERO, true),
        Err(QuoteError::ZeroAmount)
    );
    assert_eq!(
        state.amount_out(U256::MAX, false),
        Err(QuoteError::Overflow)
    );
}

#[test]
fn invalid_or_unknown_protocol_state_cannot_become_a_pricing_context() {
    for (version, total_ether, total_shares, external_ether, external_shares) in [
        (3, 12, 9, 2, 2),
        (4, 2, 9, 2, 2),
        (4, 12, 2, 2, 2),
        (4, 1, 9, 2, 2),
    ] {
        assert!(
            WstEthState::new(
                version,
                U256::from(total_ether),
                U256::from(total_shares),
                U256::from(external_ether),
                U256::from(external_shares)
            )
            .is_err()
        );
    }
}
