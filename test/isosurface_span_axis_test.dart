/// Unit tests for the colour-domain span plot's axis
/// (`doc/design_isosurface_level.md` Part 5 §The colour group).
///
/// A **pure Dart** test: `spanRange` is static and touches nothing but
/// arithmetic, so this runs in well under a second and is deliberately not an
/// integration test (`AGENTS.md`: the Flutter smoke test is human-only).
///
/// Widget *behaviour* is covered by the design's manual walkthrough. Two things
/// here are not widget behaviour:
///
/// - **the axis must be stable under its own output.** The handles map the
///   pointer's x through the very range they set, so a range that widened for a
///   handle already on the plot would walk away from a stationary finger. This
///   is the one property the level histogram's crop rule contributes to its
///   sibling, and it is invisible until someone drags a handle to the edge.
/// - **the fitted numbers read plainly.** `±0.04` must come back as `0.04`, not
///   `4.0000e-2`.
library;

import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/field_distribution_api.dart';
import 'package:flutter_cad/structure_designer/node_data/isosurface_span_histogram.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Float64List;
import 'package:flutter_test/flutter_test.dart';

/// A surface band of `[-0.05, 0.05]` at the 2nd/98th percentile, inside an
/// observed range of `[-0.08, 0.08]` — the shape a real ESP envelope has.
APISurfaceValueDistribution _distribution() => APISurfaceValueDistribution(
      state: APISurfaceDistributionState.available,
      message: '',
      binEdges: Float64List.fromList([-0.08, -0.04, 0.0, 0.04, 0.08]),
      binWeight: Float64List.fromList([1.0, 4.0, 4.0, 1.0]),
      valueMin: -0.08,
      valueMax: 0.08,
      p2: -0.05,
      p98: 0.05,
      vertexCount: BigInt.from(4000),
      isSigned: true,
      fitMin: -0.04,
      fitMax: 0.04,
      canFit: true,
      fitBlockedReason: '',
    );

void main() {
  group('spanRange', () {
    test('covers the percentile band, widened by the margin', () {
      final d = _distribution();
      final range = IsosurfaceSpanHistogram.spanRange(d, -0.04, 0.04);
      final margin = 0.1 * IsosurfaceSpanHistogram.BAND_MARGIN;
      expect(range.low, closeTo(-0.05 - margin, 1e-12));
      expect(range.high, closeTo(0.05 + margin, 1e-12));
    });

    test('a domain inside the band does not move the axis', () {
      final d = _distribution();
      final wide = IsosurfaceSpanHistogram.spanRange(d, -0.05, 0.05);
      final narrow = IsosurfaceSpanHistogram.spanRange(d, -0.001, 0.001);
      expect(narrow.low, wide.low);
      expect(narrow.high, wide.high);
    });

    test('a domain outside the band opens the axis to keep it visible', () {
      final d = _distribution();
      final range = IsosurfaceSpanHistogram.spanRange(d, -0.5, 0.5);
      expect(range.positionOf(-0.5), isNotNull,
          reason: 'a handle must never be cropped out of view');
      expect(range.positionOf(0.5), isNotNull);
    });

    test('is stable under its own output', () {
      // The property that matters: feed the range a handle position it itself
      // produced, and it must not move. Without this the plot rescales under a
      // stationary finger — the axis widens, the handle maps further in, the
      // axis widens again.
      final d = _distribution();
      var range = IsosurfaceSpanHistogram.spanRange(d, -0.04, 0.04);
      for (var iteration = 0; iteration < 8; iteration++) {
        // A drag can only produce values inside the current range: the pointer's
        // x is clamped to `[0, 1]` before it is mapped.
        final atLeftStop = range.valueAt(0.0);
        final atRightStop = range.valueAt(1.0);
        final next =
            IsosurfaceSpanHistogram.spanRange(d, atLeftStop, atRightStop);
        expect(next.low, closeTo(range.low, 1e-12),
            reason: 'iteration $iteration widened the axis');
        expect(next.high, closeTo(range.high, 1e-12),
            reason: 'iteration $iteration widened the axis');
        range = next;
      }
    });

    test('survives a degenerate distribution with a finite span', () {
      final flat = APISurfaceValueDistribution(
        state: APISurfaceDistributionState.available,
        message: '',
        binEdges: Float64List.fromList([0.25, 0.25]),
        binWeight: Float64List.fromList([1.0]),
        valueMin: 0.25,
        valueMax: 0.25,
        p2: 0.25,
        p98: 0.25,
        vertexCount: BigInt.from(4000),
        isSigned: false,
        fitMin: 0.0,
        fitMax: 0.0,
        canFit: false,
        fitBlockedReason: 'constant',
      );
      final range = IsosurfaceSpanHistogram.spanRange(flat, 0.0, 1.0);
      expect(range.span, greaterThan(0.0),
          reason: 'a zero-width axis would divide by zero in every mapping');
      expect(range.positionOf(0.0), isNotNull);
      expect(range.positionOf(1.0), isNotNull);
    });
  });

  group('SpanRange', () {
    test('maps position and value inversely inside the axis', () {
      const range = SpanRange(-0.1, 0.1);
      expect(range.valueAt(0.5), closeTo(0.0, 1e-12));
      expect(range.positionOf(0.0), closeTo(0.5, 1e-12));
      expect(range.positionOf(-0.2), isNull, reason: 'outside the axis');
    });
  });

  group('the fitted numbers read plainly', () {
    test('a symmetric ESP domain does not come back in scientific notation',
        () {
      expect(formatNatural(0.04, 3), '0.04');
      expect(formatNatural(-0.04, 3), '-0.04');
      expect(formatNatural(0.0909, 3), '0.0909');
      // Still scientific where a plain decimal would be unreadable.
      expect(formatNatural(4.0e-7, 2), '4e-7');
    });
  });
}
