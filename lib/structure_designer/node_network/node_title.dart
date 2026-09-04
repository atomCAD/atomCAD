/// What a node's title bar says — the *type* it is, or the *name* it has.
///
/// The canvas has always written the type (`expr`, nine times over on a real
/// design). The name is the identifier the text format, the AI, the error paths
/// and the AI History panel all use, and `NodeTitleMode` is the two-state switch
/// between them (`doc/design_node_names_in_ui.md` D4).
///
/// Two rules make this file worth having rather than inlining the ternary:
///
/// * **The mode never changes a node's footprint** (D5). Nothing here is
///   consulted by `ScopeResolver.effectiveNodeSizeLogical` or `getNodeSize`;
///   the header ellipsizes inside the width the *type* determined. Flipping the
///   mode is a repaint, not a layout.
/// * **Every node kind uses the same rule.** A builtin, a custom-network
///   instance, a closure, a comment, an HOF owner and a collapsed HOF all reach
///   the name through [nodeTitleText] or [nodeTitleLabel], so no kind can be
///   left showing a type while its neighbours show names.
library;

import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/structure_designer/node_network/node_name_path.dart';

/// The header text for [node] under [mode].
///
/// In `type` mode this is the simple type name — `xray` for `builtin.xray`,
/// `0_styling` for a custom-network instance. In `name` mode it is the node's
/// own `custom_name` (`xray1`, `0_styling1`), unqualified and unquoted: the
/// path form belongs to the property-panel strip and the picker, not to a title
/// bar that has one node's width to work with.
String nodeTitleText(NodeView node, NodeTitleMode mode) {
  return mode == NodeTitleMode.name
      ? nodeDisplayName(node)
      : getSimpleName(node.nodeTypeName);
}

/// The **user-supplied label** segment of a title bar — a closure's
/// `custom_label`, a comment's title — under [mode], or `''` when there is none
/// to show.
///
/// In `name` mode the label is replaced by the name, deliberately: the label is
/// that node's own choice of title, and the point of the mode is to see the one
/// identifier the AI and the text format share. Since every node has a name,
/// the `name`-mode result is never empty — which is also what makes a comment's
/// normally-hidden header appear in `name` mode.
String nodeTitleLabel(NodeView node, NodeTitleMode mode, String label) {
  return mode == NodeTitleMode.name ? nodeDisplayName(node) : label;
}
