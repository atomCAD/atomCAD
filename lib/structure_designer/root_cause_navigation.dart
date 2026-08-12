import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// "Go to root cause" — shared by both entry points (the node context menu and
/// the panel's error picker), so the landing behaves identically from either
/// (`doc/design_error_management.md` D7).
///
/// A failure fans out downstream as chained error text; the root cause is the
/// node at the end of the origin-link walk. Jumping there can cross a
/// custom-network boundary, and that hop needs care: activating another network
/// re-evaluates it **standalone** — with its own parameter defaults and display
/// set — so the node we land on may legitimately show *no* live badge. The
/// error existed only under the originating instance's arguments. The jump
/// therefore carries the original error text (and, across a boundary, its
/// provenance) into a transient SnackBar, so the user lands with the context in
/// hand even when the live badge cannot reproduce it.
///
/// [screenAnchor] is threaded into the jump unchanged: the source node's center
/// at right-click time, so the landing is anchored the way a Find Usages jump
/// is. Callers with no meaningful source position omit it.
void goToRootCause(
  BuildContext context,
  StructureDesignerModel model,
  APIErrorRootCause root, {
  Offset? screenAnchor,
}) {
  final messenger = ScaffoldMessenger.maybeOf(context);
  // Read the provenance *before* the jump — afterwards the active network is
  // the root's own, and every root would look same-network.
  final crossNetwork = model.nodeNetworkView?.name != root.hostNetwork;

  model.jumpToRootCause(root, screenAnchor: screenAnchor);

  final label = crossNetwork
      ? '${getSimpleName(root.hostNetwork)} — ${root.nodeLabel}'
      : root.nodeLabel;
  messenger?.showSnackBar(
    SnackBar(content: Text('Root cause: $label — ${root.errorText}')),
  );
}
