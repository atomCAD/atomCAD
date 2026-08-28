import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_cad/common/number_format.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/field_distribution_api.dart';

/// Where the colour field's values sit **on the extracted surface**, and where
/// the colour domain cuts them.
///
/// Part 4 and Part 5 §The colour group of `doc/design_isosurface_level.md`:
///
/// - **x** is the signed colour value on a **linear** axis. Linear because the
///   domain crosses zero, which no log axis can, and because the surface range
///   is narrow — a real ESP envelope spans about `±0.08`.
/// - **y** is **surface area** per bin, not a vertex count: marching cubes puts
///   vertices where the geometry is busy, not where the area is.
/// - the domain is drawn as a **span** with two draggable handles, one per end.
///
/// # It is a sibling of `IsosurfaceHistogram`, not a reskin of it
///
/// An earlier draft of the design called this "the same widget with three
/// substitutions". That was true of the level histogram as *designed* and is not
/// true of it as *built*: it has since acquired a `PlotRange` mapping position ⇄
/// magnitude in `log10` space, a mass-based crop of the leading tail, bar
/// heights normalised over the plotted bins, and a cumulative
/// mass-at-or-above overlay whose height *is* the enclosed fraction. A signed,
/// linear, span-with-two-handles plot inherits none of that, and a paint has no
/// counterpart to an enclosed fraction at all. So this is written beside it
/// rather than parameterising one widget over two coordinate systems and two
/// parameter shapes.
///
/// # The axis spans the percentile band, widened
///
/// The level plot's rule — "drop the leading tail once it holds under
/// `PLOT_MASS_CROP` of `∫|v|`" — is a statement about a *log* axis and about
/// *mass*, and does not transfer. The equivalent question here is which
/// percentile band the axis should cover, and the answer is `[p2, p98]` widened
/// by [BAND_MARGIN], so a domain set outside the band is still visible rather
/// than silently off-plot.
///
/// **One property does carry over unchanged, because it is not about logs:** the
/// axis must never crop a handle out of view, and must be **stable under its own
/// output**. The handles map the pointer's x through the very range they set, so
/// a range that widened for a handle already on the plot would walk away from a
/// stationary finger. [spanRange] therefore widens for a handle only when that
/// handle would otherwise fall *outside*, and a drag can never push one out
/// (the pointer's x is clamped into the range before it is mapped), so a drag
/// never moves the axis at all.
///
/// It is **stateful for one reason**: which of the two handles a drag owns is
/// decided at drag start and must survive the rebuilds the drag itself causes.
/// The editor calls `setState` on every preview tick, so a local in `build`
/// would reset to "the min handle" mid-gesture and the max handle could never
/// be moved.
class IsosurfaceSpanHistogram extends StatefulWidget {
  /// Plot height. Shorter than the level histogram's: this plot carries no
  /// cumulative overlay, and height is the scarce dimension in the sidebar.
  static const double PLOT_HEIGHT = 84.0;

  /// How far past `[p2, p98]` the axis reaches, as a share of that band.
  ///
  /// Wide enough that the conventional `±0.05` ESP domain stays on the plot for
  /// a surface whose own band is a little narrower, and narrow enough that the
  /// bars still fill the width.
  static const double BAND_MARGIN = 0.35;

  /// Extra room left beyond a handle that forced the axis open, as a share of
  /// the band. Without it a handle just outside would land exactly on the edge
  /// and be half-clipped.
  static const double HANDLE_MARGIN = 0.08;

  /// How close (in fractional x) the pointer must be to a handle to grab it
  /// rather than the other one. Only used to pick *which* handle a drag owns.
  static const double GRAB_RADIUS = 0.5;

  final APISurfaceValueDistribution distribution;

  /// The domain's two ends as the panel currently shows them — the node's
  /// stored values, or the in-flight preview during a drag.
  final double colorMin;
  final double colorMax;

  /// Whether the handles can be dragged. False with `color_field` unwired,
  /// where the whole colour group is dormant.
  final bool draggable;

  /// Called once when a handle drag begins, so the model can open an undo
  /// coalescing session.
  final VoidCallback onDragStart;

  /// Called on every tick of a handle drag with the whole domain. The editor
  /// holds it in its own state and writes **once**, on release — a colour-domain
  /// edit re-runs the display conversion exactly as a level edit does.
  final void Function(double min, double max) onDragUpdate;

  /// Called once when the drag ends, closing the undo session.
  final VoidCallback onDragEnd;

  /// Replaces the kernel's own empty-state caption. Used for the one case the
  /// kernel cannot phrase best: with `color_field` unwired *and* the node
  /// hidden, the scene reports "display the node", while the actionable advice
  /// is to make the wire. The caption lives inside the plot's box so the group
  /// keeps its height and the panel says a thing once.
  final String? emptyMessage;

  const IsosurfaceSpanHistogram({
    super.key,
    required this.distribution,
    required this.colorMin,
    required this.colorMax,
    required this.draggable,
    required this.onDragStart,
    required this.onDragUpdate,
    required this.onDragEnd,
    this.emptyMessage,
  });

  /// The value span the axis covers — see the class doc.
  ///
  /// Exposed for testing: "stable under its own output" is a property that can
  /// only be checked by feeding the range its own handle positions, and it is
  /// exactly the property whose loss makes a plot creep under a stationary
  /// finger.
  static SpanRange spanRange(
      APISurfaceValueDistribution d, double lo, double hi) {
    var low = d.p2;
    var high = d.p98;
    if (!(high > low)) {
      // A constant colour field over the surface, or a degenerate band. Fall
      // back to the observed range, then to a unit interval, so the axis always
      // has a finite span to map through.
      low = d.valueMin;
      high = d.valueMax;
    }
    if (!(high > low)) {
      low -= 0.5;
      high += 0.5;
    }
    final band = high - low;
    var axisLo = low - band * BAND_MARGIN;
    var axisHi = high + band * BAND_MARGIN;

    // Never crop a handle out of view — and only widen for one that is already
    // outside, which is what keeps the range stable under its own output.
    final handleMargin = band * HANDLE_MARGIN;
    for (final handle in [lo, hi]) {
      if (!handle.isFinite) continue;
      if (handle < axisLo) axisLo = handle - handleMargin;
      if (handle > axisHi) axisHi = handle + handleMargin;
    }
    return SpanRange(axisLo, axisHi);
  }

  @override
  State<IsosurfaceSpanHistogram> createState() =>
      _IsosurfaceSpanHistogramState();
}

class _IsosurfaceSpanHistogramState extends State<IsosurfaceSpanHistogram> {
  /// Which handle the in-flight drag owns. Held here rather than in `build`
  /// because the drag's own preview ticks rebuild this widget — see the class
  /// doc.
  bool _movingMin = true;

  void _emit(double localX, double width, SpanRange range) {
    if (width <= 0) return;
    final value = range.valueAt((localX / width).clamp(0.0, 1.0));
    // The two ends are not allowed to cross: `max > min` is the extractor's
    // guard, and a domain that fails it flattens the whole surface to the
    // ramp's midpoint. Pushing one handle past the other pins it instead.
    if (_movingMin) {
      widget.onDragUpdate(math.min(value, widget.colorMax), widget.colorMax);
    } else {
      widget.onDragUpdate(widget.colorMin, math.max(value, widget.colorMin));
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final captionStyle = theme.textTheme.bodySmall;
    final distribution = widget.distribution;
    final colorMin = widget.colorMin;
    final colorMax = widget.colorMax;

    if (distribution.state != APISurfaceDistributionState.available ||
        distribution.binWeight.isEmpty) {
      // The group keeps its height so the panel does not jump when the node is
      // displayed or a wire is made. The caption names which empty state it is.
      return SizedBox(
        height: IsosurfaceSpanHistogram.PLOT_HEIGHT,
        child: Center(
          child: Text(
            widget.emptyMessage ??
                (distribution.message.isEmpty
                    ? 'No surface distribution to plot'
                    : distribution.message),
            style: captionStyle,
            textAlign: TextAlign.center,
          ),
        ),
      );
    }

    final range =
        IsosurfaceSpanHistogram.spanRange(distribution, colorMin, colorMax);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        LayoutBuilder(
          builder: (context, constraints) {
            final width = constraints.maxWidth;
            final plot = CustomPaint(
              size: Size(width, IsosurfaceSpanHistogram.PLOT_HEIGHT),
              painter: _SpanPainter(
                distribution: distribution,
                range: range,
                minPosition: range.positionOf(colorMin),
                maxPosition: range.positionOf(colorMax),
                zeroPosition: range.positionOf(0.0),
                barColor: theme.colorScheme.primary.withValues(alpha: 0.55),
                spanShade: theme.colorScheme.primary.withValues(alpha: 0.12),
                handleColor: theme.colorScheme.error,
                axisColor: theme.dividerColor,
                textColor:
                    captionStyle?.color ?? theme.textTheme.bodySmall!.color!,
              ),
            );
            if (!widget.draggable) {
              return SizedBox(
                width: width,
                height: IsosurfaceSpanHistogram.PLOT_HEIGHT,
                child: plot,
              );
            }
            return SizedBox(
              width: width,
              height: IsosurfaceSpanHistogram.PLOT_HEIGHT,
              child: MouseRegion(
                cursor: SystemMouseCursors.resizeLeftRight,
                child: GestureDetector(
                  key: const Key('isosurface_span_handles'),
                  behavior: HitTestBehavior.opaque,
                  onHorizontalDragStart: (details) {
                    final position =
                        (details.localPosition.dx / width).clamp(0.0, 1.0);
                    final toMin =
                        (position - (range.positionOf(colorMin) ?? 0.0)).abs();
                    final toMax =
                        (position - (range.positionOf(colorMax) ?? 1.0)).abs();
                    _movingMin = toMin <= toMax &&
                        toMin < IsosurfaceSpanHistogram.GRAB_RADIUS;
                    widget.onDragStart();
                    _emit(details.localPosition.dx, width, range);
                  },
                  onHorizontalDragUpdate: (details) =>
                      _emit(details.localPosition.dx, width, range),
                  onHorizontalDragEnd: (_) => widget.onDragEnd(),
                  onHorizontalDragCancel: widget.onDragEnd,
                  child: plot,
                ),
              ),
            );
          },
        ),
        const SizedBox(height: 2),
        // Every printed number goes through `formatNatural`: a fitted `±0.04`
        // must not come back as `4.0000e-2`.
        Text(
          'Surface: ${formatNatural(distribution.valueMin, 3)} … '
          '${formatNatural(distribution.valueMax, 3)}  ·  '
          '${distribution.vertexCount} vertices',
          style: captionStyle,
        ),
      ],
    );
  }
}

/// The value span the plot's x axis covers.
class SpanRange {
  final double low;
  final double high;

  const SpanRange(this.low, this.high);

  double get span => high - low;

  /// Fractional x of a value, or null when it falls outside the axis.
  double? positionOf(double value) {
    if (!(span > 0) || !value.isFinite) return null;
    final position = (value - low) / span;
    if (position < 0.0 || position > 1.0) return null;
    return position;
  }

  /// The value at a fractional x.
  double valueAt(double position) => low + position.clamp(0.0, 1.0) * span;
}

class _SpanPainter extends CustomPainter {
  final APISurfaceValueDistribution distribution;
  final SpanRange range;
  final double? minPosition;
  final double? maxPosition;

  /// Fractional x of zero, when the axis crosses it. A diverging ramp's white
  /// sits here, so it is worth a tick of its own.
  final double? zeroPosition;
  final Color barColor;
  final Color spanShade;
  final Color handleColor;
  final Color axisColor;
  final Color textColor;

  _SpanPainter({
    required this.distribution,
    required this.range,
    required this.minPosition,
    required this.maxPosition,
    required this.zeroPosition,
    required this.barColor,
    required this.spanShade,
    required this.handleColor,
    required this.axisColor,
    required this.textColor,
  });

  /// Room under the plot for the two end labels.
  static const double AXIS_LABEL_HEIGHT = 14.0;

  @override
  void paint(Canvas canvas, Size size) {
    final plotHeight = size.height - AXIS_LABEL_HEIGHT;
    if (plotHeight <= 0 || size.width <= 0 || !(range.span > 0)) return;

    final edges = distribution.binEdges;
    final weight = distribution.binWeight;
    if (weight.isEmpty || edges.length != weight.length + 1) return;

    /// Fractional x of a value, unclamped — bars are drawn by their edges and a
    /// bar straddling the axis end must still be clipped rather than folded.
    double xOf(double value) => size.width * ((value - range.low) / range.span);

    // --- area bars -------------------------------------------------------
    // Scaled to the tallest **visible** bin, the same rule the level histogram
    // follows: a peak set by an off-plot bin would flatten everything on screen.
    var peak = 0.0;
    for (var index = 0; index < weight.length; index++) {
      final mid = (edges[index] + edges[index + 1]) * 0.5;
      if (mid < range.low || mid > range.high) continue;
      if (weight[index] > peak) peak = weight[index];
    }
    if (peak > 0) {
      final bars = Paint()..color = barColor;
      for (var index = 0; index < weight.length; index++) {
        if (weight[index] <= 0) continue;
        final left = xOf(edges[index]).clamp(0.0, size.width);
        final right = xOf(edges[index + 1]).clamp(0.0, size.width);
        if (right <= left) continue;
        final height = plotHeight * (weight[index] / peak).clamp(0.0, 1.0);
        canvas.drawRect(
          Rect.fromLTRB(left, plotHeight - height, right, plotHeight),
          bars,
        );
      }
    }

    // --- the domain, as a shaded span ------------------------------------
    // Clamped rather than skipped: a domain reaching past the axis still covers
    // everything on screen, and drawing nothing would read as "no domain set".
    final left = (minPosition ?? 0.0) * size.width;
    final right = (maxPosition ?? 1.0) * size.width;
    if (right > left) {
      canvas.drawRect(
        Rect.fromLTRB(left, 0, right, plotHeight),
        Paint()..color = spanShade,
      );
    }

    // --- zero, where a diverging ramp's white sits -----------------------
    final zero = zeroPosition;
    if (zero != null) {
      canvas.drawLine(
        Offset(zero * size.width, 0),
        Offset(zero * size.width, plotHeight),
        Paint()
          ..color = axisColor
          ..strokeWidth = 1.0,
      );
    }

    // --- handles ---------------------------------------------------------
    final handle = Paint()
      ..color = handleColor
      ..strokeWidth = 1.5;
    for (final position in [minPosition, maxPosition]) {
      if (position == null) continue;
      final x = position * size.width;
      canvas.drawLine(Offset(x, 0), Offset(x, plotHeight), handle);
      // A small foot, so a line at the very edge still reads as a grabbable
      // control rather than as the plot's border.
      canvas.drawRect(
        Rect.fromLTWH(x - 2.5, plotHeight - 6, 5, 6),
        Paint()..color = handleColor,
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
    _label(canvas, formatNatural(range.low, 2), Offset(0, plotHeight + 2), 42,
        TextAlign.left);
    _label(canvas, formatNatural(range.high, 2),
        Offset(size.width - 42, plotHeight + 2), 42, TextAlign.right);
  }

  void _label(
      Canvas canvas, String text, Offset at, double width, TextAlign align) {
    final painter = TextPainter(
      text: TextSpan(
        text: text,
        style: TextStyle(color: textColor, fontSize: 9),
      ),
      textAlign: align,
      textDirection: TextDirection.ltr,
    )..layout(minWidth: width, maxWidth: width);
    painter.paint(canvas, at);
  }

  @override
  bool shouldRepaint(_SpanPainter old) =>
      old.distribution != distribution ||
      old.minPosition != minPosition ||
      old.maxPosition != maxPosition ||
      old.range.low != range.low ||
      old.range.high != range.high;
}
