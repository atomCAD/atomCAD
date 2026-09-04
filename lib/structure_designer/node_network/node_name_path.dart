import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Composing a node's **name path** — `map4/e1` — from a `NodeNetworkView`.
///
/// This is the spelling the AI is handed everywhere: error paths, the AI
/// History panel's diff hunk titles and moved-node rows, the Layout log. It is
/// the stored `custom_name`s of the scope chain joined with `/`, unquoted —
/// backtick quoting is a text-format concern and never appears in a path, and
/// a `/` inside a name is refused at the source so the join stays unambiguous
/// (`doc/design_node_names_in_ui.md` D1/D8).
///
/// The **authoritative** walk is Rust's (`find_nodes_by_name` /
/// `resolve_node_path`, D8): anything that has to *match* a path — the Find
/// Node picker, a jump from the AI History panel — goes there, so there is one
/// path rule. These helpers stay for the surfaces that merely *display* a path
/// for a node they already hold (the property panel's name strip, *Copy node
/// name*), where a `NodeNetworkView` is in hand and an FFI round trip would buy
/// nothing. They must keep composing it exactly the way Rust does, or a copied
/// name would stop finding its node.

/// The node's stored `custom_name`, with `#<id>` standing in for a node that
/// somehow has none. Every node has one in practice — `add_node` mints one and
/// the `.cnnd` loader backfills — so the fallback exists only so a path is
/// never silently truncated and a title bar is never blank.
String nodeDisplayName(NodeView node) {
  final custom = node.customName;
  return (custom != null && custom.isNotEmpty) ? custom : '#${node.id}';
}

/// The names of the HOF owners along [scopeChain], outermost first. An empty
/// chain (a top-level node) yields an empty list. A chain that does not
/// resolve — a stale selection against a freshly loaded view — stops early
/// rather than throwing.
List<String> scopeOwnerNames(NodeNetworkView root, List<BigInt> scopeChain) {
  final names = <String>[];
  Map<BigInt, NodeView> current = root.nodes;
  for (final hofId in scopeChain) {
    final hof = current[hofId];
    if (hof == null) break;
    names.add(nodeDisplayName(hof));
    final zone = hof.zone;
    if (zone == null) break;
    current = zone.nodes;
  }
  return names;
}

/// The full name path of [node], which lives at [scopeChain] in [root]:
/// `e1` at the top level, `map4/e1` one body down.
String nodeNamePath(
    NodeNetworkView root, List<BigInt> scopeChain, NodeView node) {
  return [...scopeOwnerNames(root, scopeChain), nodeDisplayName(node)]
      .join('/');
}
