/// Tests for `query --layout --image`'s Flutter half
/// (`lib/structure_designer/node_network/network_region_capture.dart`): the
/// scale cap, the ruler steps, and the rulers rendered through the same
/// offscreen capture the region image uses.
///
/// The region itself is the image export's `NodeNetworkCanvasSnapshot`, which
/// needs a live model and so the Rust library; that part is the manual
/// walkthrough's. `tester.runAsync` is needed around every capture (see
/// `offscreen_capture_test.dart`).
library;

import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:flutter_cad/common/offscreen_capture.dart';
import 'package:flutter_cad/structure_designer/node_network/network_region_capture.dart';

void main() {
  group('networkRegionScale', () {
    test('keeps the requested scale when the image fits', () {
      expect(networkRegionScale(const Size(700, 600), 1.0, rulers: true), 1.0);
      expect(networkRegionScale(const Size(700, 600), 2.0, rulers: true), 2.0);
    });

    test('reduces it so the longest side fits, rulers included', () {
      final scale =
          networkRegionScale(const Size(3000, 1000), 1.0, rulers: true);
      expect(3000 * scale + NETWORK_REGION_RULER_PX,
          closeTo(NETWORK_REGION_MAX_SIDE, 1e-9));
      expect(networkRegionScale(const Size(3000, 1000), 1.0, rulers: false),
          closeTo(NETWORK_REGION_MAX_SIDE / 3000, 1e-12));
    });

    test('refuses an area that would be drawn too small to read', () {
      expect(
        () => networkRegionScale(const Size(20000, 500), 1.0, rulers: true),
        throwsA(isA<NetworkRegionException>().having(
            (e) => e.message, 'message', contains('too large'))),
      );
    });
  });

  test('ruler labels stay at least 70 image pixels apart', () {
    expect(NetworkRegionRulerPainter.labelStep(1.0), 100);
    expect(NetworkRegionRulerPainter.labelStep(2.0), 50);
    expect(NetworkRegionRulerPainter.labelStep(0.5), 200);
    expect(NetworkRegionRulerPainter.labelStep(0.3), 500);
  });

  testWidgets('the ruler bands come out NETWORK_REGION_RULER_PX wide',
      (tester) async {
    // A region of 400×300 canvas px at scale 0.5: the bands are laid out in
    // canvas units and must still rasterize to the fixed pixel width.
    const scale = 0.5;
    const region = Rect.fromLTWH(-100, 200, 400, 300);
    const band = NETWORK_REGION_RULER_PX / scale;
    final size = region.size + const Offset(band, band);

    final capture = await tester.runAsync(() => capturePngOffscreen(
          child: Directionality(
            textDirection: TextDirection.ltr,
            child: Stack(children: [
              const Positioned.fill(
                child: CustomPaint(
                  painter: NetworkRegionRulerPainter(
                      region: region, band: band, scale: scale),
                ),
              ),
              Positioned(
                left: band,
                top: band,
                width: region.width,
                height: region.height,
                child: const ColoredBox(color: Color(0xFFFFFFFF)),
              ),
            ]),
          ),
          logicalSize: size,
          view: tester.view,
          pixelRatio: scale,
        ));

    expect(capture, isNotNull);
    expect(capture!.widthPx, (400 * scale + NETWORK_REGION_RULER_PX).round());
    expect(capture.heightPx, (300 * scale + NETWORK_REGION_RULER_PX).round());

    final pixels = await tester.runAsync(() async {
      final codec = await ui.instantiateImageCodec(capture.pngBytes);
      final image = (await codec.getNextFrame()).image;
      final data = await image.toByteData();
      final width = image.width;
      image.dispose();
      return (data!, width);
    });
    final (bytes, width) = pixels!;
    int red(int x, int y) => bytes.getUint8((y * width + x) * 4);
    // Inside the left band, clear of labels and ticks: the band's grey.
    expect(red(2, capture.heightPx - 2), 0xEF);
    // Just inside the content: white.
    final inside = NETWORK_REGION_RULER_PX.round() + 4;
    expect(red(inside, inside), 0xFF);
  });
}
