import 'dart:async';

import 'package:flutter/foundation.dart';

/// Asks for the viewport's refined frame once it is due
/// (`doc/design_viewport_antialiasing.md` D5).
///
/// The viewport renders on demand, so after the last frame of a camera drag
/// nothing would ever ask for the refined one. Each frame reports how long
/// until it is due (`APIFrameInfo.refineIn`); [onFrame] (re)arms a single
/// timer for that long, whose callback requests a frame. A new frame cancels
/// and re-arms it, so it only fires once the view has rested.
class RefinementTimer {
  RefinementTimer(this.onDue);

  /// Requests a frame (`CadViewportState.renderingNeeded`).
  final VoidCallback onDue;

  Timer? _timer;

  /// Called after every frame with its `refineIn`, in seconds; `null` when
  /// no refined frame is owed.
  void onFrame(double? refineInSeconds) {
    _timer?.cancel();
    _timer = null;
    if (refineInSeconds == null) return;
    _timer = Timer(
      Duration(microseconds: (refineInSeconds * 1e6).ceil()),
      () {
        _timer = null;
        onDue();
      },
    );
  }

  void dispose() {
    _timer?.cancel();
    _timer = null;
  }
}
