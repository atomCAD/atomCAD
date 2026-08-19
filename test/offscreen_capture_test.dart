/// Tests for the offscreen full-size capture harness
/// (`lib/common/offscreen_capture.dart`), which backs *File → Export node
/// network image*.
///
/// The harness stands up its own `BuildOwner` / `PipelineOwner` / `RenderView`
/// and drives one build → layout → paint pass by hand, which is exactly the
/// kind of code that breaks silently on a Flutter upgrade (the APIs it uses have
/// been reshuffled before). These tests are cheap and headless: they prove a
/// widget *larger than any viewport* renders at its full size, that the pixel
/// ratio is clamped rather than handed to the engine as an impossible texture
/// size, and that an impossible request fails loudly.
///
/// `tester.runAsync` is required around every capture: `toImage` and the PNG
/// encode are real engine work, which the widget tester's fake async would
/// otherwise never let complete.
library;

import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart' show RendererBinding;
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/common/offscreen_capture.dart';

/// The eight bytes every PNG file starts with.
const List<int> _pngMagic = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

void main() {
  testWidgets('captures a widget far larger than the viewport', (tester) async {
    // Deliberately bigger than the 800x600 test viewport in both dimensions:
    // this is the whole point of the harness.
    const size = Size(2400, 1600);

    final capture = await tester.runAsync(() => capturePngOffscreen(
          child: const ColoredBox(color: Color(0xFF112233)),
          logicalSize: size,
          view: tester.view,
          pixelRatio: 1.0,
        ));

    expect(capture, isNotNull);
    expect(capture!.widthPx, 2400);
    expect(capture.heightPx, 1600);
    expect(capture.pixelRatio, 1.0);
    expect(capture.pngBytes.take(8), _pngMagic);
  });

  testWidgets('renders text, which needs a Directionality of its own',
      (tester) async {
    // The captured tree is detached, so it inherits nothing — a bare `Text`
    // would assert on the missing `Directionality`. Any content widget the
    // export renders has to bring its own, which is why
    // `NodeNetworkCanvasSnapshot` supplies one.
    final capture = await tester.runAsync(() => capturePngOffscreen(
          child: const Directionality(
            textDirection: TextDirection.ltr,
            child: Text('offscreen'),
          ),
          logicalSize: const Size(400, 200),
          view: tester.view,
          pixelRatio: 1.0,
        ));

    expect(capture, isNotNull);
    expect(capture!.widthPx, 400);
    expect(capture.heightPx, 200);
  });

  testWidgets('renders the widget kinds the node canvas is built from',
      (tester) async {
    // A detached tree inherits no `Overlay`, `MaterialLocalizations`, or
    // `MediaQuery`, and the node canvas uses all of these widget kinds:
    // `Tooltip` (pin and error tooltips), `Icon` (eyes, badges),
    // `SingleChildScrollView` (a comment note's body), `CustomPaint` (grid and
    // wires) and `Stack`/`Positioned` (every node). If any of them starts
    // requiring an ancestor the export cannot supply, it fails here rather than
    // in front of the user.
    //
    // `Tooltip` is the one that already did: its `build` asserts an `Overlay`
    // ancestor whether or not a tooltip is showing, which is why
    // [OffscreenCaptureScaffold] exists and why this test wraps content in it
    // exactly as `NodeNetworkCanvasSnapshot` does.
    const size = Size(900, 700);
    final capture = await tester.runAsync(() => capturePngOffscreen(
          child: OffscreenCaptureScaffold(
            size: size,
            child: Stack(
              children: [
                CustomPaint(painter: _BoxPainter(), child: Container()),
                const Positioned(
                  left: 20,
                  top: 20,
                  child: Tooltip(
                    message: 'a pin tooltip',
                    child: Icon(IconData(0xe800), size: 16),
                  ),
                ),
                Positioned(
                  left: 60,
                  top: 60,
                  width: 120,
                  height: 40,
                  child: SingleChildScrollView(
                    child: Text('a note body long enough to scroll' * 4),
                  ),
                ),
              ],
            ),
          ),
          logicalSize: size,
          view: tester.view,
          pixelRatio: 1.0,
        ));

    expect(capture, isNotNull);
    expect(capture!.widthPx, 900);
    expect(capture.heightPx, 700);
  });

  testWidgets('actually paints the content, at the right place',
      (tester) async {
    // The size assertions above would all pass for a blank image, so this one
    // reads pixels back — from the same shape `NodeNetworkCanvasSnapshot` uses:
    // a background `ColoredBox`, a `CustomPaint` layer (the grid and wires) and
    // a `Positioned` child (a node). The sampled node sits at (1800, 700),
    // outside the test viewport's 800x600, so this also proves content beyond
    // the viewport is rendered rather than clipped away.
    const size = Size(2000, 900);

    final pixels = await tester.runAsync(() async {
      final capture = await capturePngOffscreen(
        child: OffscreenCaptureScaffold(
          size: size,
          child: ColoredBox(
            color: const Color(0xFFFFFFFF),
            child: Stack(
              children: [
                CustomPaint(painter: _BandPainter(), child: Container()),
                const Positioned(
                  left: 1800,
                  top: 700,
                  width: 100,
                  height: 100,
                  child: ColoredBox(color: Color(0xFF0000FF)),
                ),
              ],
            ),
          ),
        ),
        logicalSize: size,
        view: tester.view,
        pixelRatio: 1.0,
      );
      final codec = await ui.instantiateImageCodec(capture.pngBytes);
      final frame = await codec.getNextFrame();
      return frame.image.toByteData(format: ui.ImageByteFormat.rawRgba);
    });

    expect(pixels, isNotNull);
    // rawRgba packs R,G,B,A in ascending byte order, which `getUint32`
    // (big-endian by default) reads back as 0xRRGGBBAA.
    int pixelAt(int x, int y) => pixels!.getUint32((y * 2000 + x) * 4);
    expect(pixelAt(1000, 400), 0xFFFFFFFF,
        reason: 'background should be painted');
    expect(pixelAt(50, 50), 0xFF0000FF,
        reason: 'the CustomPaint layer should be painted, at full canvas size');
    expect(pixelAt(1850, 750), 0x0000FFFF,
        reason: 'a node far outside the viewport should be painted');
  });

  testWidgets('applies the pixel ratio to the raster, not to layout',
      (tester) async {
    final capture = await tester.runAsync(() => capturePngOffscreen(
          child: const ColoredBox(color: Color(0xFF445566)),
          logicalSize: const Size(300, 150),
          view: tester.view,
          pixelRatio: 2.0,
        ));

    expect(capture!.widthPx, 600);
    expect(capture.heightPx, 300);
    expect(capture.pixelRatio, 2.0);
  });

  testWidgets('clamps the pixel ratio to the maximum image dimension',
      (tester) async {
    // 5000 logical px at the requested 3x would be 15000 px — past what the
    // engine will rasterize. The capture must come back smaller rather than
    // blank.
    const size = Size(5000, 1000);
    final capture = await tester.runAsync(() => capturePngOffscreen(
          child: const ColoredBox(color: Color(0xFF778899)),
          logicalSize: size,
          view: tester.view,
          pixelRatio: 3.0,
        ));

    expect(capture, isNotNull);
    expect(capture!.pixelRatio, lessThan(3.0));
    expect(capture.pixelRatio, MAX_OFFSCREEN_CAPTURE_DIMENSION / 5000);
    expect(capture.widthPx, MAX_OFFSCREEN_CAPTURE_DIMENSION);
  });

  testWidgets('refuses content that cannot fit even at 1:1', (tester) async {
    final size = Size(MAX_OFFSCREEN_CAPTURE_DIMENSION + 1.0, 500);

    // No `runAsync` here: the size check happens before any engine work, so the
    // returned future is already an error — and `runAsync` would report it as an
    // unexpected test exception instead of handing it to the matcher.
    await expectLater(
      capturePngOffscreen(
        child: const ColoredBox(color: Color(0xFFAABBCC)),
        logicalSize: size,
        view: tester.view,
        pixelRatio: 1.0,
      ),
      throwsA(isA<OffscreenCaptureTooLargeException>()),
    );
  });

  testWidgets('leaves the live binding alone', (tester) async {
    // Nothing about the capture may register with the binding: a render view
    // that did would fight the app for the window surface, and a leftover
    // focus manager would steal keyboard focus.
    await tester.pumpWidget(const ColoredBox(color: Color(0xFF000000)));
    final viewsBefore = RendererBinding.instance.renderViews.length;
    final focusBefore = FocusManager.instance;

    await tester.runAsync(() => capturePngOffscreen(
          child: const ColoredBox(color: Color(0xFF010203)),
          logicalSize: const Size(1200, 900),
          view: tester.view,
          pixelRatio: 1.0,
        ));

    expect(RendererBinding.instance.renderViews.length, viewsBefore);
    expect(FocusManager.instance, same(focusBefore));
  });
}

/// A trivial painter, standing in for the canvas's grid/wire layers.
class _BoxPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(
        Offset.zero & size, Paint()..color = const Color(0xFFEEEEEE));
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

/// Paints a red band in the canvas's top-left corner, standing in for the
/// grid/wire layers. Its `size` is the whole canvas — which the real grid
/// painter relies on, since it clips to it.
class _BandPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(const Rect.fromLTWH(0, 0, 200, 200),
        Paint()..color = const Color(0xFFFF0000));
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}
