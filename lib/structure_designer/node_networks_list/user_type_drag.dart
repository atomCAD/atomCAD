/// What a drag out of the user-types panel offers to drop targets outside the
/// panel.
///
/// The panel's rows drag their own (private) tree node, which the panel's
/// folder drop targets consume to move a type. The node network editor cannot
/// name that type, so the tree node implements this interface and the canvas
/// accepts `DragTarget<UserTypeDragData>` — Flutter matches drag data with
/// `is`, so one drag serves both kinds of target.
abstract interface class UserTypeDragData {
  /// The fully qualified name of the node network this drag would place as a
  /// node, or null when the dragged row cannot be placed (a folder, a record
  /// type def).
  String? get placeableNetworkName;
}
