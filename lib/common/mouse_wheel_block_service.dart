import 'package:flutter/foundation.dart';

/// A singleton-ish service that holds whether we should block scrolling.
class MouseWheelBlockService extends ChangeNotifier {
  bool _block = false;
  bool get isBlocked => _block;

  /// Call this to ask "please block all scrolling" (e.g. when my field is hovered).
  void block() {
    if (!_block) {
      _block = true;
      notifyListeners();
    }
  }

  /// Call this to clear the block (e.g. when the mouse leaves my field).
  void unblock() {
    if (_block) {
      _block = false;
      notifyListeners();
    }
  }
}
