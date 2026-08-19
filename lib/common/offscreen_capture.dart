/// Rasterizing a widget **at its full size, off screen**.
///
/// The ordinary way to turn a widget into an image — a `RepaintBoundary` in the
/// live tree plus `RenderRepaintBoundary.toImage()` — can only ever capture
/// what layout gave that boundary, i.e. the visible viewport. To capture
/// content that is *larger* than the window (a whole node network, say), the
/// widget has to be laid out at the full size somewhere the window's
/// constraints don't apply.
///
/// So this file mounts a second, private render tree: our own [BuildOwner] +
/// [PipelineOwner] + [RenderView], driven by hand through one
/// build/layout/paint pass, with a standalone [RenderRepaintBoundary] as the
/// container. Nothing is ever registered with `WidgetsBinding`, so the app's
/// frames, focus, and gestures are untouched and the user sees no flicker.
///
/// ## What the captured subtree may depend on
///
/// It is a *detached* tree: it inherits nothing from the app's widget tree.
/// Whatever the subtree reads from context — `Directionality`, `MediaQuery`,
/// `Provider`s, `Theme` — must be supplied by the caller inside [child].
/// `Overlay` and `MaterialLocalizations` are the usual traps; they are only
/// needed by widgets that *show* something (menus, tooltips, text fields), so a
/// paint-only capture normally does not need them. Nothing here can be
/// "captured lazily" either: only what a single synchronous
/// build → layout → paint produces ends up in the image.
library;

import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// The ancestors a captured subtree cannot do without.
///
/// A detached tree inherits nothing, and three of the widgets ordinary app code
/// uses everywhere assert on an ancestor rather than falling back:
///
/// * `Text` / `RichText` need a [Directionality].
/// * `Tooltip.build` calls `debugCheckHasOverlay` — **even when no tooltip is
///   showing**, so any subtree containing one throws in a debug build without an
///   [Overlay]. (`Overlay.of` in a *handler* is fine; this is in `build`.)
/// * A handful of widgets read [MediaQuery] without the `maybe` variant.
///
/// Wrap capture content in this rather than rediscovering that list. It is
/// deliberately not applied inside [capturePngOffscreen]: a caller may need to
/// interpose its own providers, and silently wrapping would hide where the
/// scaffolding comes from.
class OffscreenCaptureScaffold extends StatelessWidget {
  /// Logical size of the capture, published through [MediaQuery].
  final Size size;
  final Widget child;

  const OffscreenCaptureScaffold({
    super.key,
    required this.size,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    return Directionality(
      textDirection: TextDirection.ltr,
      child: MediaQuery(
        data: MediaQueryData(size: size),
        child: Overlay(
          initialEntries: [OverlayEntry(builder: (context) => child)],
        ),
      ),
    );
  }
}

/// Largest raster dimension (in device pixels) an offscreen capture will ask
/// the engine for.
///
/// `ui.Scene.toImage` rasterizes on the GPU, so a request beyond the platform's
/// maximum texture size fails — silently on some backends. 8192 is the
/// conservative floor across the desktop backends atomCAD ships on, and is
/// enforced here rather than discovered as a blank PNG.
const int MAX_OFFSCREEN_CAPTURE_DIMENSION = 8192;

/// A finished offscreen capture: the encoded PNG plus what it actually came out
/// as (which may be smaller than requested — see [OffscreenCapture.pixelRatio]).
class OffscreenCapture {
  final Uint8List pngBytes;
  final int widthPx;
  final int heightPx;

  /// The pixel ratio actually used, after clamping to
  /// [MAX_OFFSCREEN_CAPTURE_DIMENSION]. Compare against the requested ratio to
  /// tell the user their image was rendered less crisply than they asked for.
  final double pixelRatio;

  const OffscreenCapture({
    required this.pngBytes,
    required this.widthPx,
    required this.heightPx,
    required this.pixelRatio,
  });
}

/// Thrown when even a 1:1 capture of [logicalSize] would exceed
/// [MAX_OFFSCREEN_CAPTURE_DIMENSION]. The caller has to shrink the *content*
/// (a smaller zoom level, a smaller region), not the pixel ratio.
class OffscreenCaptureTooLargeException implements Exception {
  final Size logicalSize;
  final int maxDimension;

  const OffscreenCaptureTooLargeException(this.logicalSize, this.maxDimension);

  @override
  String toString() =>
      'Capture area is ${logicalSize.width.round()}×${logicalSize.height.round()} '
      'logical pixels, which exceeds the maximum image dimension of '
      '$maxDimension. Try a smaller zoom level.';
}

/// Lays [child] out at [logicalSize] in a private render tree and returns it
/// encoded as a PNG.
///
/// [view] is only used for the [RenderView]'s required `FlutterView` handle —
/// pass `View.of(context)`. The render view is never registered with the
/// binding, so nothing is drawn into that window; its
/// `devicePixelRatio` is deliberately fixed at 1.0 so layout happens in logical
/// pixels and the whole scaling job is done by the [pixelRatio] passed to
/// `toImage` (which rasterizes the recorded vectors, so text and lines stay
/// crisp rather than being upscaled).
///
/// [pixelRatio] is clamped down so the result fits
/// [MAX_OFFSCREEN_CAPTURE_DIMENSION]; if it cannot fit even at 1.0, this throws
/// [OffscreenCaptureTooLargeException].
Future<OffscreenCapture> capturePngOffscreen({
  required Widget child,
  required Size logicalSize,
  required ui.FlutterView view,
  double pixelRatio = 2.0,
}) async {
  if (logicalSize.isEmpty) {
    throw ArgumentError.value(
        logicalSize, 'logicalSize', 'must have a positive width and height');
  }

  final double longestSide =
      math.max(logicalSize.width, logicalSize.height).ceilToDouble();
  final double maxRatio = MAX_OFFSCREEN_CAPTURE_DIMENSION / longestSide;
  if (maxRatio < 1.0) {
    throw OffscreenCaptureTooLargeException(
        logicalSize, MAX_OFFSCREEN_CAPTURE_DIMENSION);
  }
  final double effectiveRatio = math.min(pixelRatio, maxRatio);

  final boundary = RenderRepaintBoundary();
  final renderView = RenderView(
    view: view,
    configuration: ViewConfiguration(
      logicalConstraints: BoxConstraints.tight(logicalSize),
      physicalConstraints: BoxConstraints.tight(logicalSize),
      devicePixelRatio: 1.0,
    ),
    // topLeft rather than the default centering: the boundary is sized exactly
    // to `logicalSize` anyway, so alignment is cosmetic — but a top-left origin
    // keeps the (harmless) sub-pixel rounding out of the captured edges.
    child: RenderPositionedBox(alignment: Alignment.topLeft, child: boundary),
  );

  final pipelineOwner = PipelineOwner();
  pipelineOwner.rootNode = renderView;
  renderView.prepareInitialFrame();

  final focusManager = FocusManager();
  final buildOwner = BuildOwner(focusManager: focusManager);
  RenderObjectToWidgetElement<RenderBox>? element;

  try {
    element = RenderObjectToWidgetAdapter<RenderBox>(
      container: boundary,
      debugShortDescription: '[offscreen capture root]',
      child: SizedBox.fromSize(size: logicalSize, child: child),
    ).attachToRenderTree(buildOwner);
    buildOwner.buildScope(element);
    buildOwner.finalizeTree();
    pipelineOwner.flushLayout();
    pipelineOwner.flushCompositingBits();
    pipelineOwner.flushPaint();

    final ui.Image image = await boundary.toImage(pixelRatio: effectiveRatio);
    try {
      final ByteData? data =
          await image.toByteData(format: ui.ImageByteFormat.png);
      if (data == null) {
        throw StateError('PNG encoding of the captured image failed');
      }
      return OffscreenCapture(
        pngBytes: data.buffer.asUint8List(),
        widthPx: image.width,
        heightPx: image.height,
        pixelRatio: effectiveRatio,
      );
    } finally {
      image.dispose();
    }
  } finally {
    // Unmount the subtree by re-attaching the same root with no child. Skipping
    // this would leave every `Provider.of` / `ListenableBuilder` in the
    // captured tree subscribed to the live model, so each later
    // `notifyListeners()` would rebuild a tree nobody looks at (and keep the
    // whole capture tree alive).
    if (element != null) {
      RenderObjectToWidgetAdapter<RenderBox>(
        container: boundary,
        child: null,
      ).attachToRenderTree(buildOwner, element);
      buildOwner.buildScope(element);
      buildOwner.finalizeTree();
    }
    pipelineOwner.rootNode = null;
    renderView.dispose();
    pipelineOwner.dispose();
    focusManager.dispose();
  }
}
