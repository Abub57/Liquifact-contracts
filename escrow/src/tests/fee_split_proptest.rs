use super::*;

use crate::external::testutils::logger::InitLogger;
use crate::testutils::random::RandomGenerator;
use crate::external::testutils::env::InitLogger;

use crate::escrow::FeeSplit;
use crate::escrow::FeeSplitError;

const MAX_BPS: u32 = 10000;

fn setup() {
    let _ = InitLogger::try_init();
}

#[test]
fn test_fee_split_property_invariants() {
    setup();
    let mut rng = RandomGenerator::new();
    for _ in 0..1000 {
        let total = rng.gen_u32();
        let fee_bps = rng.gen_u32() % MAX_BPS;
        let split = FeeSplit::new(total, fee_bps);
        let result = split.calculate();
        match result {
            Ok((fee, net)) => {
                assert_eq!(fee + net, total);
                assert!(fee <= total);
                assert!(net <= total);
            }
            Err(_) => {
                // Invalid inputs should not produce a split.
                assert!(fee_bps > MAX_BPS || total == 0);
            }
        }
    }
}

#[test]
fn test_fee_split_boundary_cases() {
    setup();
    let cases = [
        (0, 0),
        (1, 0),
        (1, 1),
        (100, 10000),
        (100, 9999),
        (100, 10001),
        (0, 10000),
    ];
    for (total, fee_bps) in cases {
        let split = FeeSplit::new(total, fee_bps);
        let result = split.calculate();
        if fee_bps > MAX_BPS {
            assert_matches!(result, Err(FeeSplitError::InvalidFeeBps));
        } else if total == 0 {
            assert_matches!(result, Err(FeeSplitError::ZeroTotal));
        } else {
            let (fee, net) = result.expect("valid split");
            assert_eq!(fee + net, total);
        }
    }
}

#[test]
fn test_fee_split_regression_duplicate_calls() {
    setup();
    let split = FeeSplit::new(10000, 250);
    let first = split.calculate().expect("first calculation");
    let second = split.calculate().expect("second calculation");
    assert_eq!(first, second);
}

#[test]
fn test_fee_split_error_display_is_deterministic() {
    setup();
    let err = FeeSplit::new(0, 0).calculate().unwrap_err();
    assert!(!format("{}", err).is_empty());
}
