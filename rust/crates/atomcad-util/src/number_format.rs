//! Human-readable formatting of a magnitude for a readout.
//!
//! Scientific notation is right for `1e-16` and wrong for `0.012233`, and a
//! readout that always reaches for it makes ordinary numbers harder to read
//! than they need to be: `1.2233e-2` says exactly what `0.012233` says, with an
//! exponent the reader has to decode first.
//!
//! So [`format_natural`] switches on the exponent rather than always using one
//! form: plain decimal while the number sits in a range a person reads at a
//! glance, scientific outside it. This is the **only** place that rule lives on
//! the Rust side — the Flutter twin is `lib/common/number_format.dart`, and the
//! two must agree, because the same quantity is shown in a pin readout and in
//! the isosurface panel.

/// Smallest exponent still printed as a plain decimal. `1e-4` is `0.0001` —
/// three leading zeros, still countable at a glance; `1e-5` is not.
const MIN_PLAIN_EXPONENT: i32 = -4;

/// First exponent printed in scientific notation. Below it a number is at most
/// six digits before the point, which reads fine unseparated.
const MAX_PLAIN_EXPONENT: i32 = 6;

/// Format `value` to `significant_digits` significant digits, choosing plain
/// decimal or scientific notation by magnitude.
///
/// Trailing zeros are trimmed in both forms: a level typed as `0.002` should
/// read back as `0.002`, not `0.002000`, and padding it to the requested
/// precision claims a precision the number does not have.
///
/// Non-finite values are passed through as Rust prints them (`NaN`, `inf`,
/// `-inf`) rather than being formatted into something that looks numeric.
pub fn format_natural(value: f64, significant_digits: usize) -> String {
    if !value.is_finite() {
        return format!("{}", value);
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let digits = significant_digits.max(1);
    let exponent = value.abs().log10().floor() as i32;
    if (MIN_PLAIN_EXPONENT..MAX_PLAIN_EXPONENT).contains(&exponent) {
        // `digits - 1 - exponent` decimals puts exactly `digits` significant
        // figures after rounding; saturating at zero covers the integers.
        let decimals = (digits as i32 - 1 - exponent).max(0) as usize;
        trim_trailing_zeros(&format!("{:.*}", decimals, value))
    } else {
        let formatted = format!("{:.*e}", digits - 1, value);
        // Trim inside the mantissa only — `2.000e-3` becomes `2e-3`, and the
        // exponent's own digits must survive untouched.
        match formatted.split_once('e') {
            Some((mantissa, exp)) => format!("{}e{}", trim_trailing_zeros(mantissa), exp),
            None => formatted,
        }
    }
}

/// Drop trailing fractional zeros, and the point if nothing survives it. Leaves
/// a string with no `.` alone, so an integer keeps every digit.
fn trim_trailing_zeros(text: &str) -> String {
    if !text.contains('.') {
        return text.to_string();
    }
    let trimmed = text.trim_end_matches('0');
    trimmed.strip_suffix('.').unwrap_or(trimmed).to_string()
}
