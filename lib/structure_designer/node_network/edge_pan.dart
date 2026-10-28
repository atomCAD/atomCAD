/// Edge auto-pan (`doc/design_move_wires.md`, Phase 3): while a wire, a node
/// or a selection rectangle is dragged, holding the pointer near the canvas
/// edge, or past it, pans the canvas that way.
///
/// The arithmetic and the release rule are pure functions, so
/// `test/edge_pan_test.dart` pins their signs and cases with no widget; the
/// ticker in `NodeNetworkState` only calls them. [pinUnderPointer] is the one
/// helper that needs real widgets (`test/pin_under_pointer_test.dart`).
library;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

import 'package:flutter_cad/structure_designer/node_network/wire_move.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Depth of the edge band, in screen px: the pointer pans the canvas once it
/// is this close to an edge.
const double EDGE_PAN_BAND = 30.0;

/// How far past the edge, in screen px, the pointer must go for the full
/// speed. Past the edge has to count, or a fast pointer flies over the band
/// and stops panning (D12).
const double EDGE_PAN_OVERSHOOT = 20.0;

/// The pan speed cap, in screen px/s — the same at every zoom level.
const double EDGE_PAN_MAX_SPEED = 900.0;

/// The longest frame a pan step accounts for, in seconds. A stall (a slow
/// refresh, a breakpoint) must not turn into a jump.
const double EDGE_PAN_MAX_DT = 0.05;

/// One axis of [edgePanVelocity]: 0 deeper than the band, rising linearly to
/// 1 at [EDGE_PAN_OVERSHOOT] past the edge, held at 1 beyond.
double _edgeDepth(double distanceInsideEdge) {
  final depth = (EDGE_PAN_BAND - distanceInsideEdge) /
      (EDGE_PAN_BAND + EDGE_PAN_OVERSHOOT);
  return depth.clamp(0.0, 1.0);
}

/// Pan velocity, in screen px/s, for a pointer at [pointer] (canvas-local)
/// on a canvas of [size]. It points toward the edge the pointer is at, and
/// is zero outside the edge band. Each axis is independent, so a corner pans
/// diagonally.
Offset edgePanVelocity(Offset pointer, Size size) {
  double axis(double p, double extent) =>
      EDGE_PAN_MAX_SPEED * (_edgeDepth(extent - p) - _edgeDepth(p));
  return Offset(axis(pointer.dx, size.width), axis(pointer.dy, size.height));
}

/// What one tick of edge auto-pan does to the canvas and to what is dragged.
class EdgePanStep {
  const EdgePanStep(
      {required this.panDelta,
      required this.nodeDelta,
      required this.rectStartShift});

  /// Added to the canvas's logical pan offset. Points away from the edge the
  /// pointer is at: the content slides away from it, and what lies beyond
  /// comes into view.
  final Offset panDelta;

  /// Added to the dragged nodes' logical positions (D13), so they stay under
  /// a still pointer: the opposite of [panDelta].
  final Offset nodeDelta;

  /// Added to a selection rectangle's start corner, in screen px, so the
  /// corner stays on the content it started on.
  final Offset rectStartShift;
}

/// The step of one tick: [velocity] (screen px/s, from [edgePanVelocity])
/// over [dt] seconds at zoom [scale]. The logical step is the screen step
/// divided by [scale], since `screen = (logical + pan) * scale`.
EdgePanStep edgePanStep(Offset velocity, double dt, double scale) {
  final screenStep = velocity * dt;
  final logicalStep = screenStep / scale;
  return EdgePanStep(
    panDelta: -logicalStep,
    nodeDelta: logicalStep,
    rectStartShift: -screenStep,
  );
}

/// What a wire release does after the D19 / D14 checks.
enum ReleaseDecision {
  /// Release as usual: connect, move, or open the Add Node popup.
  proceed,

  /// Do nothing: no connect, no move, no Add Node popup.
  ignore,
}

/// The release rule of a wire drag (new or moved), applied in order:
///
/// - D19: a release with the pointer outside the canvas ([insideCanvas]
///   false) is ignored, pan or no pan.
/// - D14: if the canvas panned since the last pointer move, the pin
///   `Draggable` chose ([chosenPin], null for no pin) must be the pin under
///   the release position ([pinUnderPointer], null for no pin), or the
///   release is ignored. `Draggable` re-checks its targets only on pointer
///   moves, so after a pan its choice may be a pin that has scrolled away.
ReleaseDecision releaseDecision({
  required bool insideCanvas,
  required bool pannedSinceMove,
  required PinReference? chosenPin,
  required PinReference? pinUnderPointer,
}) {
  if (!insideCanvas) return ReleaseDecision.ignore;
  if (!pannedSinceMove) return ReleaseDecision.proceed;
  final chosenKey = chosenPin == null ? null : pinKey(chosenPin);
  final underKey = pinUnderPointer == null ? null : pinKey(pinUnderPointer);
  return chosenKey == underKey
      ? ReleaseDecision.proceed
      : ReleaseDecision.ignore;
}

/// The pin under [globalPosition] in the view [viewId], found with the hit
/// test `Draggable` itself runs to pick its drop target: every `PinWidget`
/// wraps its `DragTarget` in a translucent `MetaData` carrying its
/// [PinReference], and the first one on the hit-test path wins. The answer
/// follows the real hit box, and a pin that is not built (far zoom) is never
/// found. Other metadata on the path — the `DragTarget`'s own entry — is
/// skipped.
PinReference? pinUnderPointer(Offset globalPosition, int viewId) {
  final result = HitTestResult();
  WidgetsBinding.instance.hitTestInView(result, globalPosition, viewId);
  for (final entry in result.path) {
    final target = entry.target;
    if (target is RenderMetaData && target.metaData is PinReference) {
      return target.metaData as PinReference;
    }
  }
  return null;
}
