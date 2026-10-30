/// `atomcad-cli query --layout --image`: one area of one scope of the active
/// network, rendered as the canvas draws it, with labelled rulers.
///
/// The read-out describes the drawing as boxes; this is the same area as a
/// picture, so an agent can see what the boxes cannot say (a wire behind a
/// node, a crossing, a cramped corner) and act on it with the read-out's
/// numbers. The rendering is the export's (`NodeNetworkCanvasSnapshot` +
/// `capturePngOffscreen`), framed by a rectangle in the **scope's own**
/// coordinates — body-local inside a body, as the read-out prints them — which
/// is converted to canvas coordinates here.
///
/// Three choices, each so the picture and the read-out stay one thing:
///
/// * **Always at 100% zoom**, whatever the live canvas shows, so one image
///   pixel is one canvas pixel times the scale.
/// * **The rulers are labelled in the scope's coordinates**, the read-out's,
///   not the canvas's. They sit in a band outside the crop and never cover it.
/// * **Everything the build reads is read before the first `await`.**
///   `capturePngOffscreen` builds, lays out and paints synchronously and only
///   then awaits the rasterization, so a whole request stays in one
///   synchronous stretch up to the PNG encoding (D8 of
///   `doc/design_multiple_documents.md`).
library;

import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import 'package:flutter_cad/common/offscreen_capture.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_content.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';

/// Longest side of a region image, in pixels. A vision model scales anything
/// larger down before looking at it, so a bigger image only costs more.
const int NETWORK_REGION_MAX_SIDE = 2000;

/// Below this scale node titles are no longer legible: the area is refused
/// rather than drawn as an unreadable thumbnail.
const double NETWORK_REGION_MIN_SCALE = 0.25;

/// Thickness of each ruler band, in image pixels.
const double NETWORK_REGION_RULER_PX = 34.0;

/// A rendered region.
class NetworkRegionCapture {
  final Uint8List pngBytes;
  final int widthPx;
  final int heightPx;

  /// Image pixels per canvas pixel, after the [NETWORK_REGION_MAX_SIDE] cap.
  final double scale;

  /// Where the crop starts in the image: past the ruler bands, or `0`.
  final double contentOffsetPx;

  const NetworkRegionCapture({
    required this.pngBytes,
    required this.widthPx,
    required this.heightPx,
    required this.scale,
    required this.contentOffsetPx,
  });
}

/// Why a region could not be rendered, as one line for the read-out.
class NetworkRegionException implements Exception {
  final String message;
  const NetworkRegionException(this.message);

  @override
  String toString() => message;
}

/// The scale a region is actually drawn at: [requested], reduced so the
/// longest side fits [NETWORK_REGION_MAX_SIDE] with the rulers. Throws
/// [NetworkRegionException] when that is under [NETWORK_REGION_MIN_SCALE].
double networkRegionScale(Size region, double requested,
    {required bool rulers}) {
  final ruler = rulers ? NETWORK_REGION_RULER_PX : 0.0;
  final longest = math.max(region.width, region.height);
  final fit = (NETWORK_REGION_MAX_SIDE - ruler) / longest;
  final scale = math.min(requested, fit);
  if (scale < NETWORK_REGION_MIN_SCALE) {
    throw NetworkRegionException(
        'the area is ${region.width.round()}×${region.height.round()} px, too '
        'large to draw legibly within $NETWORK_REGION_MAX_SIDE px; pick a '
        'smaller --box or fewer --nodes');
  }
  return scale;
}

/// Renders [scopeRect] — in the coordinates of the scope [scopeChain] names
/// (empty for the root) — of the model's active network to a PNG.
Future<NetworkRegionCapture> captureNetworkRegion({
  required StructureDesignerModel model,
  required ui.FlutterView flutterView,
  required List<BigInt> scopeChain,
  required Rect scopeRect,
  double scale = 1.0,
  bool rulers = true,
}) {
  final view = model.nodeNetworkView;
  if (view == null) {
    throw const NetworkRegionException('no node network is shown');
  }

  // Body-local → canvas: the body's top-left as the canvas lays it out at
  // 100%, with no pan, where screen and canvas coordinates coincide.
  var canvasRect = scopeRect;
  if (scopeChain.isNotEmpty) {
    final resolver = ScopeResolver(
      root: view,
      panOffset: Offset.zero,
      scale: 1.0,
      zoomLevel: ZoomLevel.normal,
    );
    final origin = resolver.layout.lookupOrigin(scopeChain);
    if (origin == null || resolver.isBodyCollapsed(scopeChain)) {
      throw const NetworkRegionException(
          'the body is not drawn on the canvas (collapsed)');
    }
    canvasRect = scopeRect.shift(origin);
  }

  final effective = networkRegionScale(scopeRect.size, scale, rulers: rulers);
  // Laid out in canvas units and rasterized at `effective`, so the bands are
  // sized in canvas units that come out NETWORK_REGION_RULER_PX wide.
  final band = rulers ? NETWORK_REGION_RULER_PX / effective : 0.0;
  final logicalSize = scopeRect.size + Offset(band, band);

  final snapshot = NodeNetworkCanvasSnapshot(
    model: model,
    view: view,
    logicalBounds: canvasRect,
    zoomLevel: ZoomLevel.normal,
  );
  final child = Directionality(
    textDirection: TextDirection.ltr,
    child: Stack(
      children: [
        if (rulers)
          Positioned.fill(
            child: CustomPaint(
              painter: NetworkRegionRulerPainter(
                region: scopeRect,
                band: band,
                scale: effective,
              ),
            ),
          ),
        Positioned(
          left: band,
          top: band,
          width: scopeRect.width,
          height: scopeRect.height,
          child: snapshot,
        ),
      ],
    ),
  );

  return capturePngOffscreen(
    child: child,
    logicalSize: logicalSize,
    view: flutterView,
    pixelRatio: effective,
  ).then((capture) => NetworkRegionCapture(
        pngBytes: capture.pngBytes,
        widthPx: capture.widthPx,
        heightPx: capture.heightPx,
        scale: capture.pixelRatio,
        contentOffsetPx: band * capture.pixelRatio,
      ));
}

/// The ruler bands along the top and left edges, labelled in the scope's
/// coordinates. Painted in canvas units; [scale] is what they will be
/// rasterized at, so text and ticks are sized to come out a fixed number of
/// image pixels.
class NetworkRegionRulerPainter extends CustomPainter {
  /// The area the crop shows, in scope coordinates.
  final Rect region;

  /// Thickness of a band, in canvas units.
  final double band;
  final double scale;

  const NetworkRegionRulerPainter({
    required this.region,
    required this.band,
    required this.scale,
  });

  /// Labelled steps tried in order; the first at least this many image pixels
  /// apart wins, so labels never collide.
  static const List<double> _steps = [
    10,
    20,
    50,
    100,
    200,
    500,
    1000,
    2000,
    5000,
    10000
  ];
  static const double _minLabelSpacingPx = 70.0;

  /// The labelled step for [scale]: the first of [_steps] whose labels land
  /// at least [_minLabelSpacingPx] apart.
  static double labelStep(double scale) =>
      _steps.firstWhere((step) => step * scale >= _minLabelSpacingPx,
          orElse: () => _steps.last);

  @override
  void paint(Canvas canvas, Size size) {
    final px = 1.0 / scale; // one image pixel, in canvas units
    final bandPaint = Paint()..color = const Color(0xFFEFEFEF);
    canvas.drawRect(Rect.fromLTWH(0, 0, size.width, band), bandPaint);
    canvas.drawRect(Rect.fromLTWH(0, 0, band, size.height), bandPaint);
    final edge = Paint()
      ..color = const Color(0xFF9E9E9E)
      ..strokeWidth = px;
    canvas.drawLine(Offset(band, band), Offset(size.width, band), edge);
    canvas.drawLine(Offset(band, band), Offset(band, size.height), edge);

    final step = labelStep(scale);
    final minor = step / 2;
    final tick = Paint()
      ..color = const Color(0xFF616161)
      ..strokeWidth = px;

    void ticks(double from, double to, void Function(double v, bool major) at) {
      for (var v = (from / minor).ceil() * minor; v <= to; v += minor) {
        at(v, (v / step).roundToDouble() * step == v);
      }
    }

    ticks(region.left, region.right, (v, major) {
      final x = band + (v - region.left);
      canvas.drawLine(
          Offset(x, band), Offset(x, band - (major ? 10 : 5) * px), tick);
      if (major) _label(canvas, '${v.round()}', Offset(x + 3 * px, 3 * px));
    });
    ticks(region.top, region.bottom, (v, major) {
      final y = band + (v - region.top);
      canvas.drawLine(
          Offset(band, y), Offset(band - (major ? 10 : 5) * px, y), tick);
      if (major) _label(canvas, '${v.round()}', Offset(3 * px, y + 2 * px));
    });
  }

  void _label(Canvas canvas, String text, Offset at) {
    final painter = TextPainter(
      text: TextSpan(
        text: text,
        style: TextStyle(
          color: const Color(0xFF212121),
          fontSize: 11 / scale,
          height: 1.0,
        ),
      ),
      textDirection: TextDirection.ltr,
    )..layout();
    painter.paint(canvas, at);
    painter.dispose();
  }

  @override
  bool shouldRepaint(NetworkRegionRulerPainter oldDelegate) =>
      oldDelegate.region != region ||
      oldDelegate.band != band ||
      oldDelegate.scale != scale;
}
