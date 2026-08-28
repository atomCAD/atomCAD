/// Human-readable formatting of a magnitude for a readout.
///
/// Scientific notation is right for `1e-16` and wrong for `0.012233`, and a
/// readout that always reaches for it makes ordinary numbers harder to read
/// than they need to be: `1.2233e-2` says exactly what `0.012233` says, with an
/// exponent the reader has to decode first.
///
/// So [formatNatural] switches on the exponent rather than always using one
/// form: plain decimal while the number sits in a range a person reads at a
/// glance, scientific outside it.
///
/// **This is the Flutter twin of `atomcad_util::number_format`, and the two
/// must agree** — the same quantity is shown in a pin readout (Rust) and in the
/// isosurface panel (here), and a level that reads `0.002` on one and `2e-3` on
/// the other would look like two different numbers.
library;

import 'dart:math' as math;

/// Smallest exponent still printed as a plain decimal. `1e-4` is `0.0001` —
/// three leading zeros, still countable at a glance; `1e-5` is not.
const int MIN_PLAIN_EXPONENT = -4;

/// First exponent printed in scientific notation. Below it a number is at most
/// six digits before the point, which reads fine unseparated.
const int MAX_PLAIN_EXPONENT = 6;

/// Format [value] to [significantDigits] significant digits, choosing plain
/// decimal or scientific notation by magnitude.
///
/// Trailing zeros are trimmed in both forms: a level typed as `0.002` should
/// read back as `0.002`, not `0.002000`, and padding it to the requested
/// precision claims a precision the number does not have.
String formatNatural(double value, int significantDigits) {
  if (value.isNaN) return 'NaN';
  if (value.isInfinite) return value.isNegative ? '-inf' : 'inf';
  if (value == 0.0) return '0';
  final digits = significantDigits < 1 ? 1 : significantDigits;
  final exponent = _floorLog10(value.abs());
  if (exponent >= MIN_PLAIN_EXPONENT && exponent < MAX_PLAIN_EXPONENT) {
    // `digits - 1 - exponent` decimals puts exactly `digits` significant
    // figures after rounding; clamping at zero covers the integers.
    final decimals = (digits - 1 - exponent).clamp(0, 20);
    return _trimTrailingZeros(value.toStringAsFixed(decimals));
  }
  final formatted = value.toStringAsExponential(digits - 1);
  // Trim inside the mantissa only — `2.000e-3` becomes `2e-3`, and the
  // exponent's own digits must survive untouched.
  final split = formatted.indexOf('e');
  if (split < 0) return formatted;
  final mantissa = _trimTrailingZeros(formatted.substring(0, split));
  // **Dart writes `1e+6` where Rust's `{:e}` writes `1e6`.** Dropping the plus
  // is what keeps the two twins byte-identical; without it the same level reads
  // differently in the pin readout and in the panel.
  var exponentText = formatted.substring(split + 1);
  if (exponentText.startsWith('+')) exponentText = exponentText.substring(1);
  return '${mantissa}e$exponentText';
}

/// `floor(log10(x))` for `x > 0`, corrected against the power itself.
///
/// `log10` is approximate — `log(1e-4) / ln10` can land a hair below `-4` — and
/// being one decade out here changes the notation, so the result is checked
/// against an exact comparison rather than trusted.
int _floorLog10(double x) {
  final approx = (math.log(x) / math.ln10).floor();
  if (math.pow(10.0, approx + 1) <= x) return approx + 1;
  if (math.pow(10.0, approx) > x) return approx - 1;
  return approx;
}

/// Drop trailing fractional zeros, and the point if nothing survives it. Leaves
/// a string with no `.` alone, so an integer keeps every digit.
String _trimTrailingZeros(String text) {
  if (!text.contains('.')) return text;
  var end = text.length;
  while (end > 0 && text[end - 1] == '0') {
    end--;
  }
  if (end > 0 && text[end - 1] == '.') end--;
  return text.substring(0, end);
}
