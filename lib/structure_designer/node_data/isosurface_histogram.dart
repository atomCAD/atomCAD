import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/field_distribution_api.dart';

/// Where the field's mass sits, and where the current level cuts it.
///
/// Part 5 §The histogram of `doc/design_isosurface_level.md`:
///
/// - **x** is `log10 |v|` over the plotted range. Linear shows one spike.
/// - **y** is **mass per bin**, not sample count — count-weighted it is one
///   vacuum spike, the same failure as a count-weighted percentile. The axis is
///   unlabelled and unscaled: only its shape is meaningful.
/// - the **cumulative** mass-at-or-above curve is drawn on its own `0..1` scale
///   pinned to the plot height, with a right-hand tick at `1.0` so it is not
///   read as mass. Its height at the marker *is* the enclosed fraction, which is
///   what makes `f` legible.
/// - the **marker** is a vertical line at the active level, with the enclosed
///   (higher-magnitude) side shaded.
/// - exact **zeros** are a count in a line beneath the plot, never a bin.
///
/// **The marker is draggable in `Fraction` and `Absolute`, and inert under
/// `Auto`.** A vertical line on a plot is the most draggable-looking object in
/// the panel; leaving it inert where the value *is* editable guarantees people
/// try it and conclude the panel is broken.
///
/// **The fraction slider's travel is not this plot's x axis** — one is in
/// fraction space, the other in `log10 |v|`. The cumulative curve is the only
/// thing that relates them, which is a second reason it is drawn.
///
/// # The axis is cropped by mass, not fitted to the extremes
///
/// The distribution's bins span `[nonzero_min, nonzero_max]`, and on a real
/// `.cube` that is a useless range to plot. A Gaussian-basis density decays
/// exponentially away from the nuclei and the writer prints every sample in
/// `%13.5E`, so the far corners of the box hold genuine values around `1e-16` —
/// **one** such sample stretches the axis by twelve decades in which nothing is
/// drawn and the cumulative curve sits pinned at 1.0.
///
/// So the *plotted* range starts at the tightest magnitude that still has
/// [PLOT_MASS_CROP] or less of `∫|v|` below it, never showing fewer than
/// [MIN_PLOT_DECADES], and never cropping past the marker. This is the same
/// argument the design already makes for the colour domain — "fitting to the
/// extremes paints the whole surface one flat colour" — applied to an axis.
///
/// **The crop is presentation only.** The distribution and both of its queries
/// still cover every sample, so no resolved number moves; the caption says when
/// a tail was left out. The `preview*` lookups below are deliberately **not**
/// cropped: they answer questions about values, not about pixels.
class IsosurfaceHistogram extends StatelessWidget {
  /// Plot height. Height is the scarce dimension in the horizontal arrangement,
  /// so the widget shrinks rather than overflowing.
  static const double PLOT_HEIGHT = 120.0;

  /// Share of `∫|v|` that may fall outside the plotted range — the near-zero
  /// tail that carries no meaningful mass but sets `nonzero_min`.
  static const double PLOT_MASS_CROP = 1e-4;

  /// Floor on the plotted span, so a field whose mass sits in one bin still gets
  /// an axis with context rather than a sliver.
  static const double MIN_PLOT_DECADES = 3.0;

  /// How far from the left edge the marker is kept when it is what forces the
  /// crop open, as a share of the plot width.
  static const double MARKER_MARGIN = 0.05;

  final APIValueDistribution distribution;

  /// Whether the marker can be dragged — false under `Auto`, with no field, or
  /// while a wire drives the level.
  final bool draggable;

  /// Called once when a marker drag begins, so the model can open an undo
  /// coalescing session.
  final VoidCallback onDragStart;

  /// Called on every tick of a marker drag with the magnitude under the pointer
  /// and the fraction that magnitude encloses. The editor writes whichever of
  /// the two its live mode owns.
  final void Function(double magnitude, double fraction) onDragUpdate;

  /// Called once when the drag ends, closing the undo session.
  final VoidCallback onDragEnd;

  /// Where to draw the marker, overriding the distribution's resolved level.
  /// Set while a drag is in flight: nothing has been written to the kernel yet,
  /// so `distribution.resolvedLevel` still describes the pre-drag surface.
  final double? markerMagnitude;

  const IsosurfaceHistogram({
    super.key,
    required this.distribution,
    required this.draggable,
    required this.onDragStart,
    required this.onDragUpdate,
    required this.onDragEnd,
    this.markerMagnitude,
  });

  /// Fractional x position (`0..1`) of a magnitude over the **whole** binned
  /// range, or null when there is no span to place it on.
  static double? positionOfMagnitude(APIValueDistribution d, double magnitude) {
    if (d.binEdges.length < 2 || magnitude <= 0) return null;
    final lo = math.log(d.binEdges.first) / math.ln10;
    final hi = math.log(d.binEdges.last) / math.ln10;
    if (!(hi > lo)) return null;
    return ((math.log(magnitude) / math.ln10) - lo) / (hi - lo);
  }

  /// Enclosed fraction at a fractional position over the whole binned range,
  /// read off the same cumulative curve the plot draws.
  static double fractionAtPosition(APIValueDistribution d, double position) {
    final curve = d.cumulativeAtOrAbove;
    if (curve.isEmpty) return 0.0;
    final t = position.clamp(0.0, 1.0) * (curve.length - 1);
    final low = t.floor();
    final high = math.min(low + 1, curve.length - 1);
    return curve[low] + (curve[high] - curve[low]) * (t - low);
  }

  /// What share of the field lies at or above `magnitude`, or null when there is
  /// no curve to read.
  ///
  /// **A preview, not the answer.** It is accurate to one bin, while the kernel
  /// resolves against the exact sorted samples. The editor uses it only to keep
  /// the readout and the marker moving *during* a drag; the authoritative pair
  /// arrives from the kernel on release. Never write one of these numbers into
  /// node data.
  static double? previewFractionForMagnitude(
      APIValueDistribution d, double magnitude) {
    final position = positionOfMagnitude(d, magnitude);
    if (position == null) return null;
    return fractionAtPosition(d, position.clamp(0.0, 1.0));
  }

  /// The isovalue enclosing `fraction`, previewed off the cumulative curve.
  ///
  /// Mirrors `LogHistogram::iso_for_fraction`: the lower edge of the tightest
  /// bin that still encloses `fraction`. Same preview-only caveat as
  /// [previewFractionForMagnitude].
  static double? previewMagnitudeForFraction(
      APIValueDistribution d, double fraction) {
    final curve = d.cumulativeAtOrAbove;
    if (curve.length < 2 || d.binEdges.length != curve.length) return null;
    // `curve` is non-increasing, so the entries at or above `fraction` form a
    // prefix; the last of them is the tightest threshold that still encloses it.
    var last = 0;
    for (var index = 0; index < curve.length; index++) {
      if (curve[index] < fraction) break;
      last = index;
    }
    return d.binEdges[last];
  }

  /// The sub-range of bins actually drawn — see the class doc.
  ///
  /// Exposed for testing: the crop is where an axis could quietly start lying
  /// about what it shows, so the rule is worth pinning.
  static PlotRange plotRange(APIValueDistribution d, double? marker) {
    final edges = d.binEdges;
    final curve = d.cumulativeAtOrAbove;
    final bins = d.binMass.length;
    if (bins < 1 || edges.length != bins + 1 || curve.length != edges.length) {
      return const PlotRange(0, 0, 0.0, 0.0);
    }
    final logLo = math.log(edges.first) / math.ln10;
    final logHi = math.log(edges.last) / math.ln10;
    if (!(logHi > logLo)) return PlotRange(0, bins, logLo, logHi);
    final decadesPerBin = (logHi - logLo) / bins;

    // `curve` is non-increasing, so the edges still holding essentially all of
    // the mass form a prefix; the last of them is the tightest crop that leaves
    // no more than PLOT_MASS_CROP outside the plot.
    final threshold = 1.0 - PLOT_MASS_CROP;
    var start = 0;
    for (var index = 0; index < curve.length; index++) {
      if (curve[index] < threshold) break;
      start = index;
    }
    // Never crop to a sliver: a field whose mass sits in one bin still needs an
    // axis with context around it.
    final minBins = math.min(bins, (MIN_PLOT_DECADES / decadesPerBin).ceil());
    start = math.min(start, bins - minBins);
    // Never crop past the marker. An absolute level typed out in the tail is
    // exactly when the user needs to see where it falls.
    //
    // **Only when it would otherwise fall outside.** Widening for a marker that
    // is already on the plot makes the axis creep: a marker drag maps the
    // pointer's x through this very range, so at the left edge the marker would
    // land on `start`, push it a margin lower, and do it again on the next
    // frame — the plot rescaling under a stationary finger. The guard is what
    // makes the range stable under its own output.
    if (marker != null) {
      final position = positionOfMagnitude(d, marker);
      if (position != null && position < 1.0) {
        final markerBin = (position * bins).floor();
        if (markerBin < start) {
          final margin = (MARKER_MARGIN * (bins - start)).ceil();
          start = math.max(0, markerBin - margin);
        }
      }
    }
    start = start.clamp(0, bins - 1);
    return PlotRange(
      start,
      bins,
      logLo + start * decadesPerBin,
      logHi,
    );
  }

  void _emit(double localX, double width, PlotRange range) {
    if (width <= 0) return;
    final magnitude = range.magnitudeAt((localX / width).clamp(0.0, 1.0));
    onDragUpdate(
      magnitude,
      previewFractionForMagnitude(distribution, magnitude) ?? 0.0,
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final captionStyle = theme.textTheme.bodySmall;

    if (distribution.binMass.isEmpty) {
      // The group keeps its height so the panel does not jump when a wire is
      // made; the caption above the plot already names which empty state it is.
      return SizedBox(
        height: PLOT_HEIGHT,
        child: Center(
          child: Text('No distribution to plot', style: captionStyle),
        ),
      );
    }

    final marker = markerMagnitude ??
        (distribution.hasResolvedLevel ? distribution.resolvedLevel : null);
    final range = plotRange(distribution, marker);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        LayoutBuilder(
          builder: (context, constraints) {
            final width = constraints.maxWidth;
            final plot = CustomPaint(
              size: Size(width, PLOT_HEIGHT),
              painter: _HistogramPainter(
                distribution: distribution,
                range: range,
                markerPosition:
                    marker == null ? null : range.positionOf(marker),
                barColor: theme.colorScheme.primary.withValues(alpha: 0.55),
                curveColor: theme.colorScheme.secondary,
                markerColor: theme.colorScheme.error,
                axisColor: theme.dividerColor,
                textColor:
                    captionStyle?.color ?? theme.textTheme.bodySmall!.color!,
                enclosedShade:
                    theme.colorScheme.primary.withValues(alpha: 0.12),
              ),
            );
            if (!draggable) {
              return SizedBox(width: width, height: PLOT_HEIGHT, child: plot);
            }
            return SizedBox(
              width: width,
              height: PLOT_HEIGHT,
              child: MouseRegion(
                cursor: SystemMouseCursors.resizeLeftRight,
                child: GestureDetector(
                  key: const Key('isosurface_histogram_marker'),
                  behavior: HitTestBehavior.opaque,
                  onHorizontalDragStart: (details) {
                    onDragStart();
                    _emit(details.localPosition.dx, width, range);
                  },
                  onHorizontalDragUpdate: (details) =>
                      _emit(details.localPosition.dx, width, range),
                  onHorizontalDragEnd: (_) => onDragEnd(),
                  onHorizontalDragCancel: onDragEnd,
                  child: plot,
                ),
              ),
            );
          },
        ),
        // All three lines are **conditional**: each reports something the plot
        // is not showing, and none of them has anything to say in the ordinary
        // case. "No samples are exactly zero" is not news.
        if (distribution.zeroCount > BigInt.zero) ...[
          const SizedBox(height: 2),
          Text(
            '${distribution.zeroCount} zero samples (not plotted)',
            style: captionStyle,
          ),
        ],
        // An axis that starts above the field's smallest value is a zoom, and a
        // zoom the reader did not ask for should announce itself.
        if (range.isCropped)
          Text(
            'Axis cropped below ${formatNatural(range.lowMagnitude, 2)} '
            '(< ${(PLOT_MASS_CROP * 100).toStringAsFixed(2)}% of ∫|v|)',
            style: captionStyle,
          ),
        if (!distribution.isExact)
          Text('Binned — accurate to about one bin', style: captionStyle),
      ],
    );
  }
}

/// The bins the plot actually draws, and the `log10` span they cover.
///
/// `startBin` is the first bin shown; `binCount` is the total the distribution
/// holds, so the plotted count is `binCount - startBin`.
class PlotRange {
  final int startBin;
  final int binCount;
  final double logLo;
  final double logHi;

  const PlotRange(this.startBin, this.binCount, this.logLo, this.logHi);

  bool get isCropped => startBin > 0;
  int get plottedBins => binCount - startBin;
  double get lowMagnitude => math.pow(10.0, logLo).toDouble();

  /// Fractional x of a magnitude within the plotted span, or null when it falls
  /// outside it (or there is no span).
  double? positionOf(double magnitude) {
    if (magnitude <= 0 || !(logHi > logLo)) return null;
    final position =
        ((math.log(magnitude) / math.ln10) - logLo) / (logHi - logLo);
    if (position < 0.0 || position > 1.0) return null;
    return position;
  }

  /// The magnitude at a fractional x within the plotted span.
  double magnitudeAt(double position) => math
      .pow(10.0, logLo + position.clamp(0.0, 1.0) * (logHi - logLo))
      .toDouble();
}

class _HistogramPainter extends CustomPainter {
  final APIValueDistribution distribution;
  final PlotRange range;

  /// Fractional x of the active level, or null when nothing is resolved or it
  /// falls outside the plotted span.
  final double? markerPosition;
  final Color barColor;
  final Color curveColor;
  final Color markerColor;
  final Color axisColor;
  final Color textColor;
  final Color enclosedShade;

  _HistogramPainter({
    required this.distribution,
    required this.range,
    required this.markerPosition,
    required this.barColor,
    required this.curveColor,
    required this.markerColor,
    required this.axisColor,
    required this.textColor,
    required this.enclosedShade,
  });

  /// Room under the plot for the decade labels.
  static const double AXIS_LABEL_HEIGHT = 14.0;

  @override
  void paint(Canvas canvas, Size size) {
    final plotHeight = size.height - AXIS_LABEL_HEIGHT;
    if (plotHeight <= 0 || size.width <= 0) return;

    final mass = distribution.binMass;
    final plotted = range.plottedBins;
    final logSpan = range.logHi - range.logLo;
    if (plotted < 1 || !(logSpan > 0)) return;

    /// x of bin edge `index`, indexed into the **full** arrays.
    double xOfEdge(int index) =>
        size.width * (index - range.startBin) / plotted;

    // --- mass bars -------------------------------------------------------
    // Scaled to the tallest **plotted** bin, which is the other half of what
    // cropping buys: a peak set by an off-plot bin would flatten everything on
    // screen. The axis is unlabelled and unscaled on purpose — only its shape
    // carries meaning.
    var peak = 0.0;
    for (var index = range.startBin; index < range.binCount; index++) {
      if (mass[index] > peak) peak = mass[index];
    }
    if (peak > 0) {
      final bars = Path()..moveTo(0, plotHeight);
      for (var index = range.startBin; index < range.binCount; index++) {
        final y = plotHeight * (1.0 - mass[index] / peak);
        bars.lineTo(xOfEdge(index), y);
        bars.lineTo(xOfEdge(index + 1), y);
      }
      bars.lineTo(size.width, plotHeight);
      bars.close();
      canvas.drawPath(bars, Paint()..color = barColor);
    }

    // --- enclosed side ---------------------------------------------------
    // Everything at or above the level is what the surface encloses, so the
    // shading goes to the marker's right.
    final marker = markerPosition;
    if (marker != null && marker < 1.0) {
      final left = marker.clamp(0.0, 1.0) * size.width;
      canvas.drawRect(
        Rect.fromLTRB(left, 0, size.width, plotHeight),
        Paint()..color = enclosedShade,
      );
    }

    // --- cumulative mass-at-or-above curve, on its own 0..1 scale ---------
    final curve = distribution.cumulativeAtOrAbove;
    if (curve.length == range.binCount + 1) {
      final path = Path();
      for (var index = range.startBin; index <= range.binCount; index++) {
        final y = plotHeight * (1.0 - curve[index].clamp(0.0, 1.0));
        if (index == range.startBin) {
          path.moveTo(xOfEdge(index), y);
        } else {
          path.lineTo(xOfEdge(index), y);
        }
      }
      canvas.drawPath(
        path,
        Paint()
          ..color = curveColor
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5,
      );
      // Right-hand tick at 1.0, so the curve is not read as mass.
      _label(canvas, '1.0', Offset(size.width - 20, 0), 20);
      canvas.drawLine(
        Offset(size.width - 22, 0.75),
        Offset(size.width, 0.75),
        Paint()
          ..color = curveColor
          ..strokeWidth = 0.75,
      );
    }

    // --- marker ----------------------------------------------------------
    if (marker != null && marker >= 0.0 && marker <= 1.0) {
      final x = marker * size.width;
      canvas.drawLine(
        Offset(x, 0),
        Offset(x, plotHeight),
        Paint()
          ..color = markerColor
          ..strokeWidth = 1.5,
      );
    }

    // --- axis ------------------------------------------------------------
    canvas.drawLine(
      Offset(0, plotHeight),
      Offset(size.width, plotHeight),
      Paint()
        ..color = axisColor
        ..strokeWidth = 1.0,
    );
    // One tick per decade, thinned so labels never collide on a narrow panel.
    final firstDecade = range.logLo.ceil();
    final lastDecade = range.logHi.floor();
    final decades = lastDecade - firstDecade + 1;
    if (decades > 0) {
      final step = math.max(1, (decades / 8).ceil());
      for (var decade = firstDecade; decade <= lastDecade; decade += step) {
        final x = size.width * (decade - range.logLo) / logSpan;
        canvas.drawLine(
          Offset(x, plotHeight),
          Offset(x, plotHeight + 3),
          Paint()
            ..color = axisColor
            ..strokeWidth = 1.0,
        );
        _label(canvas, '$decade', Offset(x - 10, plotHeight + 2), 20);
      }
    }
  }

  void _label(Canvas canvas, String text, Offset at, double width) {
    final painter = TextPainter(
      text: TextSpan(
        text: text,
        style: TextStyle(color: textColor, fontSize: 9),
      ),
      textAlign: TextAlign.center,
      textDirection: TextDirection.ltr,
    )..layout(minWidth: width, maxWidth: width);
    painter.paint(canvas, at);
  }

  @override
  bool shouldRepaint(_HistogramPainter old) =>
      old.distribution != distribution ||
      old.markerPosition != markerPosition ||
      old.range.startBin != range.startBin;
}
