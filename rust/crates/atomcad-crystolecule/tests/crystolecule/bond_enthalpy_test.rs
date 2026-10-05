//! Tests for `bond_enthalpy`: the tables the chemisorption search used to
//! score with, kept for a later consumer.

use atomcad_crystolecule::bond_enthalpy::{
    bond_enthalpy_kcal, is_tabulated_element, pauling_estimate_kj, tabulated_enthalpy_kj,
};

const H: i16 = 1;
const B: i16 = 5;
const C: i16 = 6;
const N: i16 = 7;
const O: i16 = 8;
const SI: i16 = 14;

#[test]
fn the_enthalpy_table_is_symmetric_and_converted_once() {
    assert_eq!(tabulated_enthalpy_kj(SI, O), Some(452.0));
    assert_eq!(tabulated_enthalpy_kj(O, SI), Some(452.0));
    assert_eq!(tabulated_enthalpy_kj(C, SI), Some(318.0));
    assert_eq!(tabulated_enthalpy_kj(32, 32), Some(186.0));
    let (d, estimated) = bond_enthalpy_kcal(SI, O).unwrap();
    assert!((d - 452.0 / 4.184).abs() < 1e-12);
    assert!(!estimated);
}

#[test]
fn pauling_estimates_track_the_table() {
    // A self-check that catches a mistyped value: for pairs the table has,
    // Pauling's estimate from the homonuclear values is within 15 %.
    for (a, b) in [(SI, O), (SI, H), (SI, 17), (C, H), (O, H), (C, O)] {
        let table = tabulated_enthalpy_kj(a, b).unwrap();
        let estimate = pauling_estimate_kj(a, b).unwrap();
        let rel = (estimate - table).abs() / table;
        assert!(
            rel < 0.15,
            "{a}–{b}: estimate {estimate:.1} vs table {table}"
        );
    }
    let si_o = pauling_estimate_kj(SI, O).unwrap();
    assert!((si_o - 412.9).abs() < 0.5, "Si–O estimate {si_o}");
}

#[test]
fn a_missing_pair_is_estimated_and_an_unknown_element_has_none() {
    assert_eq!(tabulated_enthalpy_kj(SI, N), None);
    let (d, estimated) = bond_enthalpy_kcal(SI, N).unwrap();
    assert!(estimated);
    assert!(d > 0.0);
    assert!(!is_tabulated_element(B));
    assert_eq!(bond_enthalpy_kcal(SI, B), None);
}
