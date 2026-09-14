/// The viewport camera basis (`getCameraTransform`, `lib/common/cad_viewport.dart`).
///
/// Everything that turns a pointer position into a world ray, or a world
/// position into a pixel, is built from this basis, and it has to agree with
/// the renderer's view matrix — `look_at_rh(eye, target, up)`, which
/// orthonormalizes internally. The regression this pins is the case where they
/// disagreed: a stored `up` that is a *world* axis rather than the view-plane
/// up (what the CLI's `camera --up`, a loaded project and the initial pose all
/// write) left `right` short by `cos φ` and `up` sheared along the view
/// direction, so every pick landed on the wrong atom, further off the further
/// the pointer was from the viewport centre.
library;

import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:vector_math/vector_math.dart' as vector_math;

import 'package:flutter_cad/common/cad_viewport.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';

APIVec3 _v(double x, double y, double z) => APIVec3(x: x, y: y, z: z);

APICamera _camera({
  required APIVec3 eye,
  required APIVec3 target,
  required APIVec3 up,
}) =>
    APICamera(
      eye: eye,
      target: target,
      up: up,
      aspect: 1330.0 / 477.0,
      fovy: pi * 0.15,
      znear: 0.1,
      zfar: 10000.0,
      orthographic: false,
      orthoHalfHeight: 16.0,
      pivotPoint: target,
      navUp: _v(0.0, 0.0, 1.0),
    );

/// `vector_math.Vector3` is float32, so the basis is only good to ~1e-6 —
/// tolerances here are about the arithmetic being right, not the precision.
void _expectOrthonormal(CameraTransform ct) {
  expect(ct.right.length, closeTo(1.0, 1e-5));
  expect(ct.up.length, closeTo(1.0, 1e-5));
  expect(ct.forward.length, closeTo(1.0, 1e-5));
  expect(ct.right.dot(ct.forward), closeTo(0.0, 1e-5));
  expect(ct.up.dot(ct.forward), closeTo(0.0, 1e-5));
  expect(ct.right.dot(ct.up), closeTo(0.0, 1e-5));
}

void main() {
  test('a level camera is unchanged — right = forward x up, up = world up', () {
    final ct = getCameraTransform(_camera(
      eye: _v(0.0, -50.0, 0.0),
      target: _v(0.0, 0.0, 0.0),
      up: _v(0.0, 0.0, 1.0),
    ))!;

    _expectOrthonormal(ct);
    expect(ct.forward.y, closeTo(1.0, 1e-5));
    expect(ct.right.x, closeTo(1.0, 1e-5));
    expect(ct.up.z, closeTo(1.0, 1e-5));
  });

  test('a tilted camera orthonormalizes against forward', () {
    // Looking down at 45 degrees with the world +Z still stored as `up`: the
    // pose the CLI and a loaded project write. `forward . up` is -sin(45).
    final ct = getCameraTransform(_camera(
      eye: _v(0.0, -30.0, 30.0),
      target: _v(0.0, 0.0, 0.0),
      up: _v(0.0, 0.0, 1.0),
    ))!;

    _expectOrthonormal(ct);

    // The bug: `right` used to come out with length cos(45) = 0.7071, which
    // scaled every horizontal pick offset by that factor.
    expect(ct.right.x, closeTo(1.0, 1e-5));
    expect(ct.up.y, closeTo(sqrt1_2, 1e-5));
    expect(ct.up.z, closeTo(sqrt1_2, 1e-5));
  });

  test('an unnormalized stored up does not scale the basis', () {
    final ct = getCameraTransform(_camera(
      eye: _v(0.0, -50.0, 0.0),
      target: _v(0.0, 0.0, 0.0),
      up: _v(0.0, 0.0, 7.5),
    ))!;

    _expectOrthonormal(ct);
  });

  test('up parallel to forward falls back to a usable basis', () {
    // Straight down with `up` left at +Z: every roll is equally valid and
    // `look_at_rh` is degenerate too, but the basis must stay finite or the
    // ray collapses onto the view axis and picking dies.
    final ct = getCameraTransform(_camera(
      eye: _v(0.0, 0.0, 50.0),
      target: _v(0.0, 0.0, 0.0),
      up: _v(0.0, 0.0, 1.0),
    ))!;

    _expectOrthonormal(ct);
  });

  test('projection and ray agree with the renderer for a tilted camera', () {
    // The measurement that found the bug, as arithmetic: a point 20 A along
    // the true screen-right from the target must project to
    // `20 * d / L` pixels right of centre, with `d = H/2 / tan(fovy/2)`.
    const viewportHeight = 477.0;
    final camera = _camera(
      eye: _v(42.09, 6.66, 40.0),
      target: _v(42.09, 36.66, 10.0),
      up: _v(0.0, 0.0, 1.0),
    );
    final ct = getCameraTransform(camera)!;

    final target = vector_math.Vector3(42.09, 36.66, 10.0);
    final point = target + ct.right * 20.0;

    final d = viewportHeight * 0.5 / tan(camera.fovy * 0.5);
    final distance = (target - ct.eye).length;

    final offset = point - ct.eye;
    final xView = offset.dot(ct.right);
    final zView = offset.dot(ct.forward);

    expect(zView, closeTo(distance, 1e-4));
    expect(xView / zView * d, closeTo(20.0 * d / distance, 1e-3));
    // Measured against the renderer at 1330x477: 468 px right of centre.
    expect(xView / zView * d, closeTo(468.3, 0.5));
  });
}
