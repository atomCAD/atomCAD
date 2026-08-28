//! `format_natural` — plain decimal where a person reads it at a glance,
//! scientific outside that range.
//!
//! The Flutter twin is `lib/common/number_format.dart` and must agree with
//! this, because the same quantity is shown in a pin readout (here) and in the
//! isosurface panel (there). `test/isosurface_level_slider_test.dart` mirrors
//! the table below; a change to either belongs in both.

use atomcad_util::number_format::format_natural;

#[test]
fn ordinary_magnitudes_are_plain_decimals() {
    // The complaint this exists for: `1.2233e-2` says what `0.012233` says,
    // with an exponent the reader has to decode first.
    assert_eq!(format_natural(0.012233, 5), "0.012233");
    assert_eq!(format_natural(0.002, 4), "0.002");
    assert_eq!(format_natural(4.364e-3, 4), "0.004364");
    assert_eq!(format_natural(1.64e-3, 4), "0.00164");
    assert_eq!(format_natural(3.02e-4, 4), "0.000302");
    assert_eq!(format_natural(0.5, 4), "0.5");
    assert_eq!(format_natural(97.3, 4), "97.3");
    assert_eq!(format_natural(2985.0, 4), "2985");
}

#[test]
fn trailing_zeros_are_trimmed_in_both_forms() {
    // Padding a typed `0.002` out to the requested precision claims a precision
    // the number does not have.
    assert_eq!(format_natural(0.002, 6), "0.002");
    assert_eq!(format_natural(1.0, 5), "1");
    assert_eq!(format_natural(2e-9, 5), "2e-9");
}

#[test]
fn only_the_mantissa_is_trimmed() {
    // `1e-10`'s exponent ends in a zero, and eating it would change the value
    // by nine decades.
    assert_eq!(format_natural(1e-10, 4), "1e-10");
    assert_eq!(format_natural(1.5e-20, 4), "1.5e-20");
}

#[test]
fn very_small_and_very_large_stay_scientific() {
    assert_eq!(format_natural(1e-16, 4), "1e-16");
    assert_eq!(format_natural(1.2345e-7, 5), "1.2345e-7");
    assert_eq!(format_natural(1e6, 4), "1e6");
    assert_eq!(format_natural(1.5e12, 4), "1.5e12");
}

#[test]
fn the_decade_boundaries_land_on_the_right_side() {
    // `log10` is approximate and being one decade out flips the notation, so
    // the exponents either side of each threshold are pinned.
    assert_eq!(format_natural(1e-4, 4), "0.0001", "1e-4 is still readable");
    assert_eq!(format_natural(9.9e-5, 4), "9.9e-5");
    assert_eq!(format_natural(999999.0, 6), "999999");
    assert_eq!(format_natural(1e5, 4), "100000");
}

#[test]
fn significant_digits_are_significant_not_decimal_places() {
    // Four significant figures means four, wherever the point falls.
    assert_eq!(format_natural(0.012233, 4), "0.01223");
    assert_eq!(format_natural(1.2233, 4), "1.223");
    assert_eq!(format_natural(122.33, 4), "122.3");
}

#[test]
fn signs_zero_and_non_finite_survive() {
    assert_eq!(format_natural(-0.002, 4), "-0.002");
    assert_eq!(format_natural(-1e-16, 4), "-1e-16");
    assert_eq!(format_natural(0.0, 4), "0");
    // Zero has no exponent to switch on, and `-0.0` must not read as a negative
    // level.
    assert_eq!(format_natural(-0.0, 4), "0");
    assert_eq!(format_natural(f64::NAN, 4), "NaN");
    assert_eq!(format_natural(f64::INFINITY, 4), "inf");
}
