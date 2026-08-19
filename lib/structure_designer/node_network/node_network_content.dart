/// What the node canvas *contains*, independent of who is looking at it.
///
/// [NodeNetworkState] renders the interactive canvas; the image export renders
/// the same content off screen at full size (see [NodeNetworkCanvasSnapshot]).
/// Both must show the same thing, so the pieces they share live here:
///
/// * [appendCanvasNodeWidgets] — the scope walk that turns a network view into
///   node widgets (top-level nodes first, then each visible body's nodes as
///   siblings; see `AGENTS.md` → "Zones (inline HOF bodies)").
/// * [canvasNodeWidget] — the Comment-vs-generic node widget switch.
/// * [nodeNetworkContentBounds] — the logical bounding box of everything drawn.
///
/// The interactive canvas keeps what is genuinely its own: pan/zoom state,
/// pointer handling, the selection rectangle, and the drag fast path (the
/// per-node `ListenableBuilder` — see `AGENTS.md` → "Performance invariants").
library;

import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import 'package:flutter_cad/common/offscreen_capture.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/comment_node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';

/// Builds the Stack child for one node. The live canvas wraps the result in its
/// drag-fast-path `ListenableBuilder`; the image export uses it directly.
typedef CanvasNodeWidgetBuilder = Widget Function(
    NodeView node, List<BigInt> scopeChain, ScopeResolver resolver);

/// The widget for a single node in [scopeChain]. Comment nodes get
/// [CommentNodeWidget] at every scope (top level *and* inside HOF/closure
/// bodies); everything else gets [NodeWidget]. Both are scope-aware
/// (positioning + key + API calls), so the same routing serves the top-level
/// walk and the recursive body walk.
Widget canvasNodeWidget({
  required NodeView node,
  required List<BigInt> scopeChain,
  required NodeNetworkView rootView,
  required ScopeResolver resolver,
  required Offset panOffset,
  required ZoomLevel zoomLevel,
  bool hideSelection = false,
}) {
  if (node.nodeTypeName == 'Comment') {
    return CommentNodeWidget(
      key: NodeWidgetKeys.nodeWidget(node.id, scopeChain: scopeChain),
      node: node,
      panOffset: panOffset,
      zoomLevel: zoomLevel,
      resolver: resolver,
      scopeChain: scopeChain,
      hideSelection: hideSelection,
    );
  }
  return NodeWidget(
    node: node,
    panOffset: panOffset,
    zoomLevel: zoomLevel,
    rootView: rootView,
    resolver: resolver,
    scopeChain: scopeChain,
    hideSelection: hideSelection,
  );
}

/// Appends a widget for every node reachable from [rootView] — first the outer
/// scope's nodes (HOFs included), then each HOF's body nodes recursively. Body
/// nodes appear *above* their HOF in the Stack so they receive pointer events
/// first.
///
/// [resolver] is consulted to decide whether to descend into each HOF's body: a
/// body that's collapsed (rendered too small to be readable — see U6) is
/// skipped, since the HOF widget itself already swaps in the `[N nodes]`
/// placeholder for that case.
void appendCanvasNodeWidgets({
  required List<Widget> children,
  required NodeNetworkView rootView,
  required ScopeResolver resolver,
  required CanvasNodeWidgetBuilder builder,
}) {
  _appendNodesRecursive(
      children, rootView, const <BigInt>[], resolver, builder);
}

void _appendNodesRecursive(
  List<Widget> children,
  NodeNetworkView view,
  List<BigInt> scopeChain,
  ScopeResolver resolver,
  CanvasNodeWidgetBuilder builder,
) {
  for (final entry in view.nodes.entries) {
    children.add(builder(entry.value, scopeChain, resolver));
  }
  // Then walk into each HOF's body — body nodes are drawn after their owner HOF
  // so they layer on top. Skip if the body is collapsed.
  for (final entry in view.nodes.entries) {
    final node = entry.value;
    final zone = node.zone;
    if (zone == null) continue;
    final bodyChain = [...scopeChain, node.id];
    if (resolver.isBodyCollapsed(bodyChain)) continue;
    _appendZoneNodesRecursive(children, zone, bodyChain, resolver, builder);
  }
}

void _appendZoneNodesRecursive(
  List<Widget> children,
  ZoneView zone,
  List<BigInt> scopeChain,
  ScopeResolver resolver,
  CanvasNodeWidgetBuilder builder,
) {
  for (final entry in zone.nodes.entries) {
    children.add(builder(entry.value, scopeChain, resolver));
  }
  for (final entry in zone.nodes.entries) {
    final node = entry.value;
    final inner = node.zone;
    if (inner == null) continue;
    final innerChain = [...scopeChain, node.id];
    if (resolver.isBodyCollapsed(innerChain)) continue;
    _appendZoneNodesRecursive(children, inner, innerChain, resolver, builder);
  }
}

/// Slack (logical px) added around the node bounding box when framing the whole
/// canvas for an image export.
///
/// Wires are Bezier curves whose control points reach
/// [BASE_CUBIC_SPLINE_HORIZ_OFFSET] beyond their pins, so a right-to-left wire
/// bulges outside the nodes' own bounds. This is also the visual breathing room
/// around the content.
const double NODE_NETWORK_CONTENT_MARGIN = 60.0;

/// How many **top-level** nodes are selected — the ones a region export can
/// frame.
///
/// Selection inside an HOF/closure body is deliberately not counted: a body
/// node's position lives in its body's own coordinate frame, and the export
/// frames the top-level canvas. A body selection therefore reads as "nothing to
/// frame", which is what gates the export dialog's checkbox.
int countSelectedTopLevelNodes(NodeNetworkView view) =>
    view.nodes.values.where((node) => node.selected).length;

/// The logical-space bounding box of what the canvas draws for [view] at
/// [zoomLevel], including [NODE_NETWORK_CONTENT_MARGIN] of slack.
///
/// With [selectedOnly] the box is drawn around the **selected** top-level nodes
/// instead of all of them — the region export. The nodes need not be adjacent or
/// even near each other: the box is their common bounding box, so a shift-drag
/// selection spanning several screens frames exactly that span. Everything
/// inside the box still renders, selected or not, which is what makes the crop
/// look like the canvas rather than like a filtered subset.
///
/// Only top-level nodes are measured either way: a body's nodes live inside
/// their HOF's footprint, and `ScopeResolver.effectiveNodeSizeLogical` already
/// reports the grown-to-fit size of an HOF whose body outgrew its stored size
/// (that is what the resolver's bottom-up size pass is for).
///
/// Returns [Rect.zero] for an empty network, and for [selectedOnly] with no
/// top-level selection — callers gate on [countSelectedTopLevelNodes] first.
Rect nodeNetworkContentBounds({
  required NodeNetworkView view,
  required ZoomLevel zoomLevel,
  double margin = NODE_NETWORK_CONTENT_MARGIN,
  bool selectedOnly = false,
}) {
  if (view.nodes.isEmpty) return Rect.zero;

  // Pan-invariant: sizes come out of the layout pass in logical units, so the
  // pan offset this resolver is built with does not matter.
  final resolver = ScopeResolver(
    root: view,
    panOffset: Offset.zero,
    scale: getZoomScale(zoomLevel),
    zoomLevel: zoomLevel,
  );

  Rect? bounds;
  for (final node in view.nodes.values) {
    if (selectedOnly && !node.selected) continue;
    final size = resolver.effectiveNodeSizeLogical(node, const <BigInt>[]);
    final rect = Rect.fromLTWH(
        node.position.x, node.position.y, size.width, size.height);
    bounds = bounds == null ? rect : bounds.expandToInclude(rect);
  }
  return bounds == null ? Rect.zero : bounds.inflate(margin);
}

/// A non-interactive rendering of an entire node network, for offscreen image
/// capture (`export_network_image.dart`).
///
/// Deliberately *not* a stripped-down copy of the live canvas: it builds the
/// same painter layers and the same node widgets through the same walk. What it
/// leaves out is only what has no meaning in a still image — pointer handling,
/// the selection rectangle, and the drag fast path.
///
/// It supplies its own [OffscreenCaptureScaffold] (directionality, media query,
/// overlay) and [ChangeNotifierProvider], because a captured tree is detached
/// and inherits nothing from the app's tree — see
/// `lib/common/offscreen_capture.dart`.
class NodeNetworkCanvasSnapshot extends StatelessWidget {
  final StructureDesignerModel model;
  final NodeNetworkView view;

  /// Logical-space region to render, normally [nodeNetworkContentBounds].
  final Rect logicalBounds;
  final ZoomLevel zoomLevel;

  /// Painted behind the grid. The live canvas has no background of its own and
  /// simply sits on the app's white surface, which the grid colors assume.
  final Color backgroundColor;

  /// Draw nodes and wires as neither selected nor active — the default, because
  /// selection is editing state that a reader of the picture cannot tell apart
  /// from meaning, and a region export is framed *by* a selection that would
  /// otherwise light up the whole image. Nothing is moved or omitted; only the
  /// highlight styling is dropped. See `NodeWidget.hideSelection`.
  final bool hideSelection;

  const NodeNetworkCanvasSnapshot({
    super.key,
    required this.model,
    required this.view,
    required this.logicalBounds,
    required this.zoomLevel,
    this.backgroundColor = Colors.white,
    this.hideSelection = true,
  });

  /// Size in logical pixels of the image this widget renders into.
  Size get pixelSize => logicalBounds.size * getZoomScale(zoomLevel);

  @override
  Widget build(BuildContext context) {
    // screen = (logical + panOffset) * scale, so this puts the region's
    // top-left corner at the image's origin.
    final panOffset = -logicalBounds.topLeft;
    final resolver = ScopeResolver(
      root: view,
      panOffset: panOffset,
      scale: getZoomScale(zoomLevel),
      zoomLevel: zoomLevel,
    );

    final children = <Widget>[
      // Bottom layer: grid + top-level wires, under the node widgets.
      NodeNetworkInteractionLayer(
          model: model,
          panOffset: panOffset,
          zoomLevel: zoomLevel,
          hideSelection: hideSelection),
    ];
    appendCanvasNodeWidgets(
      children: children,
      rootView: view,
      resolver: resolver,
      builder: (node, scopeChain, r) => canvasNodeWidget(
        node: node,
        scopeChain: scopeChain,
        rootView: view,
        resolver: r,
        panOffset: panOffset,
        zoomLevel: zoomLevel,
        hideSelection: hideSelection,
      ),
    );
    // Top layer: body wires, which would otherwise be hidden by the HOF node
    // widget's opaque body background.
    children.add(NodeNetworkInteractionLayer(
        model: model,
        panOffset: panOffset,
        zoomLevel: zoomLevel,
        overlay: true,
        hideSelection: hideSelection));

    return OffscreenCaptureScaffold(
      size: pixelSize,
      child: ChangeNotifierProvider<StructureDesignerModel>.value(
        value: model,
        child: ColoredBox(
          color: backgroundColor,
          child: Stack(children: children),
        ),
      ),
    );
  }
}
