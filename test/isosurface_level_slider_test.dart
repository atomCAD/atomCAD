/// Unit tests for the `isosurface` fraction slider's coordinate map
/// (`doc/design_isosurface_level.md` Part 5 §The fraction slider).
///
/// A **pure Dart** test — the two functions are static and touch nothing but
/// `dart:math`, so this runs in well under a second and is deliberately not an
/// integration test (`AGENTS.md`: the Flutter smoke test is human-only).
///
/// Widget *behaviour* is covered by the design's manual walkthrough. What is
/// tested here is not widget behaviour: the slider's window is **smaller than
/// the property's legal range** of `0 < f < 1`, and the whole point of the map
/// returning null outside it is that a value like `0.1` must not render
/// indistinguishably from `0.30` and get silently tripled by the next drag.
library;

import 'dart:math' as math;

import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/field_distribution_api.dart';
import 'package:flutter_cad/structure_designer/node_data/isosurface_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/isosurface_histogram.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Float64List;
import 'package:flutter_test/flutter_test.dart';

/// Four log-spaced bins over `1e-4 .. 1e0`, with a cumulative curve chosen so
/// every expected answer is readable by hand.
APIValueDistribution _distribution() => APIValueDistribution(
      state: APIDistributionState.available,
      message: '',
      binEdges: Float64List.fromList([1e-4, 1e-3, 1e-2, 1e-1, 1e0]),
      binMass: Float64List.fromList([0.1, 0.3, 0.4, 0.2]),
      cumulativeAtOrAbove: Float64List.fromList([1.0, 0.9, 0.6, 0.2, 0.0]),
      nonzeroMin: 1e-4,
      nonzeroMax: 1e0,
      zeroCount: BigInt.zero,
      isExact: true,
      resolvedLevel: 1e-2,
      resolvedFraction: 0.6,
      hasResolvedLevel: true,
      hasResolvedFraction: true,
      autoBasis: '',
      storedLevelMatches: false,
      storedFractionMatches: false,
    );

void main() {
  group('fraction slider window', () {
    test('the ends are 0.30 and 0.999', () {
      // The bottom stop is the `0.30` the density-viz handoff's flood animation
      // starts from; the top is three nines. Neither end may be `0` or `1` —
      // both are rejected by validation, which is why the log offset cannot be
      // zero.
      expect(IsosurfaceEditor.sliderFractionMin, closeTo(0.3002, 1e-4));
      expect(IsosurfaceEditor.sliderFractionMax, closeTo(0.999, 1e-9));
      expect(IsosurfaceEditor.sliderFractionMin, greaterThan(0.0));
      expect(IsosurfaceEditor.sliderFractionMax, lessThan(1.0));
      expect(IsosurfaceEditor.fractionForSlider(0.0),
          closeTo(IsosurfaceEditor.sliderFractionMin, 1e-12));
      expect(IsosurfaceEditor.fractionForSlider(1.0),
          closeTo(IsosurfaceEditor.sliderFractionMax, 1e-12));
    });

    test('most of the travel lands in the density band 0.9 - 0.999', () {
      // The travel is linear in the number of nines, which is the whole point:
      // spent evenly in `f` it would put almost nothing where densities live.
      final low = IsosurfaceEditor.sliderPositionForFraction(0.9)!;
      final high = IsosurfaceEditor.sliderPositionForFraction(0.999)!;
      expect(high - low, greaterThan(0.65));
    });

    test('the orbital band is reachable and gets real travel', () {
      // 0.5 - 0.9 is where orbitals and spin densities sit. Moving the bottom
      // stop down to 0.30 is what this test exists to protect.
      final low = IsosurfaceEditor.sliderPositionForFraction(0.5)!;
      final high = IsosurfaceEditor.sliderPositionForFraction(0.9)!;
      expect(low, greaterThan(0.0));
      expect(high - low, greaterThan(0.15));
    });

    test('the map round-trips', () {
      for (final f in [0.31, 0.5, 0.72, 0.9, 0.95, 0.99, 0.998]) {
        final s = IsosurfaceEditor.sliderPositionForFraction(f);
        expect(s, isNotNull, reason: '$f is inside the window');
        expect(IsosurfaceEditor.fractionForSlider(s!), closeTo(f, 1e-9));
      }
    });

    test('a fraction outside the window has no position, rather than a stop',
        () {
      // Both of these are legal property values that arrive easily — from the
      // text format, the CLI, an older `.cnnd`. The slider disables itself on a
      // null; rendering them at a stop is the failure this guards.
      expect(IsosurfaceEditor.sliderPositionForFraction(0.1), isNull);
      expect(IsosurfaceEditor.sliderPositionForFraction(0.9995), isNull);
    });

    test('a value outside the legal range has no position either', () {
      for (final f in [0.0, 1.0, -0.5, 1.5, double.nan]) {
        expect(IsosurfaceEditor.sliderPositionForFraction(f), isNull,
            reason: '$f is not a legal enclosed fraction');
      }
    });
  });

  // The panel previews the *other* coordinate off the cumulative curve while a
  // drag is in flight, because nothing has been written to the kernel yet and
  // its exact answer only arrives on release. These are bin-accurate by
  // construction; what must not drift is which bin.
  group('drag preview lookups', () {
    test('a magnitude reads its enclosed share off the curve', () {
      final d = _distribution();
      expect(IsosurfaceHistogram.previewFractionForMagnitude(d, 1e-2),
          closeTo(0.6, 1e-9));
      expect(IsosurfaceHistogram.previewFractionForMagnitude(d, 1e-4),
          closeTo(1.0, 1e-9));
      expect(IsosurfaceHistogram.previewFractionForMagnitude(d, 1e0),
          closeTo(0.0, 1e-9));
      // Between two edges the curve interpolates rather than stepping:
      // 10^-1.5 sits halfway between the 1e-2 and 1e-1 edges in log space, so
      // halfway between their 0.6 and 0.2.
      expect(IsosurfaceHistogram.previewFractionForMagnitude(d, 0.0316227766),
          closeTo(0.4, 1e-6));
    });

    test('a fraction reads back the tightest edge that still encloses it', () {
      // Mirrors `LogHistogram::iso_for_fraction`: the lower edge of the
      // tightest bin still enclosing the target, never the next one up.
      final d = _distribution();
      expect(IsosurfaceHistogram.previewMagnitudeForFraction(d, 0.6),
          closeTo(1e-2, 1e-12));
      expect(IsosurfaceHistogram.previewMagnitudeForFraction(d, 0.7),
          closeTo(1e-3, 1e-12));
      expect(IsosurfaceHistogram.previewMagnitudeForFraction(d, 0.2),
          closeTo(1e-1, 1e-12));
    });

    test('the two are consistent at an edge', () {
      final d = _distribution();
      final magnitude =
          IsosurfaceHistogram.previewMagnitudeForFraction(d, 0.6)!;
      expect(IsosurfaceHistogram.previewFractionForMagnitude(d, magnitude),
          closeTo(0.6, 1e-9));
    });

    test('an empty distribution previews nothing rather than throwing', () {
      final empty = APIValueDistribution(
        state: APIDistributionState.noField,
        message: '',
        binEdges: Float64List.fromList([]),
        binMass: Float64List.fromList([]),
        cumulativeAtOrAbove: Float64List.fromList([]),
        nonzeroMin: 0.0,
        nonzeroMax: 0.0,
        zeroCount: BigInt.zero,
        isExact: true,
        resolvedLevel: 0.0,
        resolvedFraction: 0.0,
        hasResolvedLevel: false,
        hasResolvedFraction: false,
        autoBasis: '',
        storedLevelMatches: false,
        storedFractionMatches: false,
      );
      expect(
          IsosurfaceHistogram.previewFractionForMagnitude(empty, 1e-2), isNull);
      expect(
          IsosurfaceHistogram.previewMagnitudeForFraction(empty, 0.5), isNull);
    });
  });

  // The axis is cropped by mass rather than fitted to the extremes: one sample
  // at 1e-16 in a real `.cube` otherwise stretches it by twelve decades in which
  // nothing is drawn. The crop is presentation only — no resolved number moves —
  // but an axis that quietly starts above the field's smallest value is exactly
  // where a plot could begin lying, so the rule is pinned here.
  group('mass-cropped axis', () {
    /// 20 log-spaced bins over `1e-16 .. 1e4`, with all of the mass in the top
    /// five — the shape a density on a generous box actually has.
    APIValueDistribution vacuumTailed({double resolvedLevel = 1e-1}) {
      const bins = 20;
      final edges = <double>[
        for (var i = 0; i <= bins; i++) math.pow(10.0, -16 + i).toDouble()
      ];
      final mass = <double>[
        for (var i = 0; i < bins; i++) i >= 15 ? 20.0 : 0.0
      ];
      final total = mass.reduce((a, b) => a + b);
      final cumulative = <double>[];
      var above = total;
      for (var i = 0; i < bins; i++) {
        cumulative.add(above / total);
        above -= mass[i];
      }
      cumulative.add(0.0);
      return APIValueDistribution(
        state: APIDistributionState.available,
        message: '',
        binEdges: Float64List.fromList(edges),
        binMass: Float64List.fromList(mass),
        cumulativeAtOrAbove: Float64List.fromList(cumulative),
        nonzeroMin: 1e-16,
        nonzeroMax: 1e4,
        zeroCount: BigInt.zero,
        isExact: true,
        resolvedLevel: resolvedLevel,
        resolvedFraction: 0.5,
        hasResolvedLevel: true,
        hasResolvedFraction: true,
        autoBasis: '',
        storedLevelMatches: false,
        storedFractionMatches: false,
      );
    }

    test('the empty tail is dropped, and the axis starts where the mass does',
        () {
      final range = IsosurfaceHistogram.plotRange(vacuumTailed(), 1e-1);
      expect(range.isCropped, isTrue);
      // Asserted as a property, not as arithmetic: the twelve empty decades are
      // gone and the top of the range is untouched. The exact left edge depends
      // on the marker margin, which is a separate rule with its own test.
      expect(range.logLo, greaterThan(-5.0),
          reason: 'the vacuum tail must not set the axis');
      expect(range.logHi, closeTo(4.0, 1e-9));
      expect(range.plottedBins, lessThan(range.binCount));
    });

    test('an uncropped distribution reports itself as such', () {
      final d = _distribution(); // mass spread across every bin
      final range = IsosurfaceHistogram.plotRange(d, 1e-2);
      expect(range.isCropped, isFalse);
      expect(range.startBin, 0);
    });

    test('the crop never hides the marker', () {
      // An absolute level typed out in the vacuum tail is exactly when the user
      // needs to see where it falls, so the crop opens back up to include it.
      final range = IsosurfaceHistogram.plotRange(vacuumTailed(), 1e-12);
      expect(range.positionOf(1e-12), isNotNull,
          reason: 'the marker must land inside the plotted span');
      expect(range.logLo, lessThan(-12.0));
    });

    test('a mass spike does not crop the axis to a sliver', () {
      // All the mass in one bin would otherwise crop to a single bin's width.
      const bins = 20;
      final edges = <double>[
        for (var i = 0; i <= bins; i++) math.pow(10.0, -16 + i).toDouble()
      ];
      final mass = <double>[for (var i = 0; i < bins; i++) i == 19 ? 1.0 : 0.0];
      final cumulative = <double>[
        for (var i = 0; i <= bins; i++) i <= 19 ? 1.0 : 0.0
      ];
      final d = APIValueDistribution(
        state: APIDistributionState.available,
        message: '',
        binEdges: Float64List.fromList(edges),
        binMass: Float64List.fromList(mass),
        cumulativeAtOrAbove: Float64List.fromList(cumulative),
        nonzeroMin: 1e-16,
        nonzeroMax: 1e4,
        zeroCount: BigInt.zero,
        isExact: true,
        resolvedLevel: 1e3,
        resolvedFraction: 1.0,
        hasResolvedLevel: true,
        hasResolvedFraction: true,
        autoBasis: '',
        storedLevelMatches: false,
        storedFractionMatches: false,
      );
      final range = IsosurfaceHistogram.plotRange(d, 1e3);
      expect(range.logHi - range.logLo,
          greaterThanOrEqualTo(IsosurfaceHistogram.MIN_PLOT_DECADES - 1e-9));
    });

    test('positions map back to magnitudes within the plotted span', () {
      final range = IsosurfaceHistogram.plotRange(vacuumTailed(), 1e-1);
      // What a marker drag relies on: the pointer's x has to mean the magnitude
      // the axis labels under it claim, over the *cropped* span.
      expect(range.magnitudeAt(0.0), closeTo(range.lowMagnitude, 1e-15));
      expect(range.magnitudeAt(1.0), closeTo(1e4, 1e-3));
      final middle = range.magnitudeAt(0.5);
      expect(range.positionOf(middle), closeTo(0.5, 1e-9));
    });

    test('the range is stable under its own output', () {
      // A marker drag maps the pointer's x through the range it produced. If a
      // marker already on the plot could still widen it, holding the pointer at
      // the left edge would rescale the axis every frame — the plot walking away
      // from a stationary finger.
      var range = IsosurfaceHistogram.plotRange(vacuumTailed(), 1e-1);
      for (var i = 0; i < 5; i++) {
        final atLeftEdge = range.magnitudeAt(0.0);
        final next = IsosurfaceHistogram.plotRange(vacuumTailed(), atLeftEdge);
        expect(next.startBin, range.startBin,
            reason: 'iteration $i moved the axis under a stationary pointer');
        range = next;
      }
    });

    test('a magnitude outside the plotted span has no position', () {
      final range = IsosurfaceHistogram.plotRange(vacuumTailed(), 1e-1);
      expect(range.positionOf(1e-16), isNull);
      expect(range.positionOf(0.0), isNull);
    });
  });

  // `formatNatural` is the Flutter twin of `atomcad_util::number_format`, and
  // the two must agree: the same level is printed in a pin readout (Rust) and
  // in this panel (Dart), and `0.002` on one against `2e-3` on the other reads
  // as two different numbers. **This table mirrors
  // `rust/crates/atomcad-util/tests/util/number_format_test.rs` case for case;
  // a change to either belongs in both.**
  group('natural number formatting', () {
    test('ordinary magnitudes are plain decimals', () {
      expect(formatNatural(0.012233, 5), '0.012233');
      expect(formatNatural(0.002, 4), '0.002');
      expect(formatNatural(4.364e-3, 4), '0.004364');
      expect(formatNatural(1.64e-3, 4), '0.00164');
      expect(formatNatural(3.02e-4, 4), '0.000302');
      expect(formatNatural(0.5, 4), '0.5');
      expect(formatNatural(97.3, 4), '97.3');
      expect(formatNatural(2985.0, 4), '2985');
    });

    test('trailing zeros are trimmed in both forms', () {
      expect(formatNatural(0.002, 6), '0.002');
      expect(formatNatural(1.0, 5), '1');
      expect(formatNatural(2e-9, 5), '2e-9');
    });

    test('only the mantissa is trimmed', () {
      // `1e-10`'s exponent ends in a zero, and eating it would change the value
      // by nine decades.
      expect(formatNatural(1e-10, 4), '1e-10');
      expect(formatNatural(1.5e-20, 4), '1.5e-20');
    });

    test('very small and very large stay scientific', () {
      expect(formatNatural(1e-16, 4), '1e-16');
      expect(formatNatural(1.2345e-7, 5), '1.2345e-7');
      expect(formatNatural(1e6, 4), '1e6');
      expect(formatNatural(1.5e12, 4), '1.5e12');
    });

    test('the decade boundaries land on the right side', () {
      // `log10` is approximate and being one decade out flips the notation, so
      // the exponents either side of each threshold are pinned.
      expect(formatNatural(1e-4, 4), '0.0001');
      expect(formatNatural(9.9e-5, 4), '9.9e-5');
      expect(formatNatural(999999.0, 6), '999999');
      expect(formatNatural(1e5, 4), '100000');
    });

    test('a positive exponent carries no plus sign', () {
      // Dart's `toStringAsExponential` writes `1e+6`; Rust's `{:e}` writes
      // `1e6`. The twins have to agree, so the plus is stripped here.
      expect(formatNatural(1e6, 4), isNot(contains('+')));
      expect(formatNatural(1.5e12, 4), isNot(contains('+')));
    });

    test('significant digits are significant, not decimal places', () {
      expect(formatNatural(0.012233, 4), '0.01223');
      expect(formatNatural(1.2233, 4), '1.223');
      expect(formatNatural(122.33, 4), '122.3');
    });

    test('signs, zero and non-finite survive', () {
      expect(formatNatural(-0.002, 4), '-0.002');
      expect(formatNatural(-1e-16, 4), '-1e-16');
      expect(formatNatural(0.0, 4), '0');
      expect(formatNatural(-0.0, 4), '0');
      expect(formatNatural(double.nan, 4), 'NaN');
      expect(formatNatural(double.infinity, 4), 'inf');
    });
  });
}
