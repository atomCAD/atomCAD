/// What a drag off a pin does (`doc/design_move_wires.md`, "The rule").
///
/// Pure and widget-free, so `test/wire_drag_mode_test.dart` can walk every
/// cell of the rule table.
library;

enum WireDragMode {
  /// Draw a new wire from the pin (today's drag).
  newWire,

  /// Pick up every wire of the pin and drop them on another pin.
  move,
}

/// The mode of a drag starting on a pin.
///
/// [isOutput] is true for every wire source (output, function pin, zone
/// input); [multi] for an array input; [wireCount] is the number of wires
/// the pin holds (`sd_api.pinWires`); [ctrl] whether Ctrl was down when the
/// drag started — read once, so pressing or releasing it mid-drag changes
/// nothing.
///
/// An unconnected pin always starts a new wire. Ctrl moves the wires of any
/// connected pin. Without Ctrl only a connected **single input** moves its
/// wire: a new wire from there could only replace the one already in it.
WireDragMode wireDragMode({
  required bool isOutput,
  required bool multi,
  required int wireCount,
  required bool ctrl,
}) {
  if (wireCount == 0) return WireDragMode.newWire;
  if (ctrl) return WireDragMode.move;
  if (!isOutput && !multi) return WireDragMode.move;
  return WireDragMode.newWire;
}
