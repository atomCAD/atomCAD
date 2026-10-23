/// The per-drag plan memo of the drag-onto-wire gesture
/// (`doc/design_insert_node_on_wire.md` Phase 4).
///
/// While an unconnected node is dragged, the canvas hit-tests its title bar
/// against the wires on every drag tick, and highlights a wire only if the
/// node can be spliced into it. That question is Rust's insert plan, an FFI
/// call: asked per tick it would still work, only slowly, so nothing would
/// ever show the mistake. This memo asks it **once per wire the drag
/// touches** and remembers the answer until the drag ends.
///
/// Pure and widget-free, with the planner injected, so
/// `test/splice_drop_memo_test.dart` can count the questions.
class PerDragPlanMemo<W> {
  /// [planner] answers "can the dragged node splice into this wire?";
  /// [keyOf] names a wire by value, since a fresh hit-test result is a new
  /// object on every tick.
  PerDragPlanMemo(this._planner, this._keyOf);

  final bool Function(W wire) _planner;
  final Object Function(W wire) _keyOf;
  final Map<Object, bool> _answers = {};

  /// Whether the dragged node fits [wire]. The planner runs only the first
  /// time a wire is asked about during this drag.
  bool accepts(W wire) =>
      _answers.putIfAbsent(_keyOf(wire), () => _planner(wire));

  /// The drag ended: forget every answer. The next drag may be of another
  /// node, over a network that has changed.
  void endDrag() => _answers.clear();

  /// How many wires have been planned during this drag.
  int get plannedWireCount => _answers.length;
}
