/// The open-port `+` tabs (`doc/design_wire_declared_inputs.md`).
///
/// A node that declares its own inputs (`NodeView.hasOpenInputPort`: `expr`,
/// an anonymous `record_construct`) gets a `+` tab hanging **below** its
/// bottom edge at the input-pin column while a wire is being dragged.
/// Dropping the wire on it appends an input of the wire's type and connects
/// the wire (`StructureDesignerModel.addInputFromWire`).
///
/// Three properties are load-bearing:
///
/// * **The tabs live in their own canvas layer, outside every node's
///   footprint.** Flutter does not hit-test a child outside its parent's
///   bounds, so a tab hanging out of the node widget would never receive the
///   drop; and the node's size, its pin offsets and every saved layout stay
///   as they are — `getNodeSize` does not know the tab exists.
/// * **The layer rebuilds twice per drag, never per tick.** It listens to
///   `model.wireDragSource`, which changes only when a drag starts and ends
///   (`node_network/AGENTS.md` → "Performance invariants").
/// * **The layer is on top of the nodes**, so a tab covering the top of a
///   node beneath it still wins the hit test.
///
/// Placement and eligibility are the pure [openPortTabs], unit-tested in
/// `test/open_port_tabs_test.dart`.
library;

import 'package:flutter/material.dart';

import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Tab size and inset from the node's left edge, in logical pixels (scaled by
/// the zoom level like everything else on the canvas).
const double OPEN_PORT_TAB_WIDTH = 28.0;
const double OPEN_PORT_TAB_HEIGHT = 16.0;
const double OPEN_PORT_TAB_LEFT_INSET = 4.0;

/// One tab: the node it adds inputs to, and where it is drawn (screen
/// coordinates, canvas-local).
class OpenPortTabPlacement {
  final BigInt nodeId;
  final List<BigInt> scopeChain;
  final Rect rect;

  const OpenPortTabPlacement(this.nodeId, this.scopeChain, this.rect);
}

/// Every open-port tab to show for the network [resolver] lays out: one per
/// visible node with `hasOpenInputPort`, directly below the node's bottom
/// edge at its input-pin column. Walks the same scopes the canvas draws
/// (`appendCanvasNodeWidgets`), so a node inside a collapsed body — whose
/// widget is not on screen — gets no tab.
List<OpenPortTabPlacement> openPortTabs(ScopeResolver resolver) {
  final tabs = <OpenPortTabPlacement>[];
  void walk(Map<BigInt, NodeView> nodes, List<BigInt> scopeChain) {
    for (final node in nodes.values) {
      if (!node.hasOpenInputPort) continue;
      final topLeft =
          resolver.scopedToScreen(scopeChain, apiVec2ToOffset(node.position));
      final size = resolver.effectiveNodeSizeScreen(node, scopeChain);
      final scale = resolver.scale;
      tabs.add(OpenPortTabPlacement(
        node.id,
        scopeChain,
        Rect.fromLTWH(
          topLeft.dx + OPEN_PORT_TAB_LEFT_INSET * scale,
          topLeft.dy + size.height,
          OPEN_PORT_TAB_WIDTH * scale,
          OPEN_PORT_TAB_HEIGHT * scale,
        ),
      ));
    }
    for (final node in nodes.values) {
      final zone = node.zone;
      if (zone == null) continue;
      final bodyChain = [...scopeChain, node.id];
      if (resolver.isBodyCollapsed(bodyChain)) continue;
      walk(zone.nodes, bodyChain);
    }
  }

  walk(resolver.root.nodes, const <BigInt>[]);
  return tabs;
}

/// The canvas layer holding the tabs. Empty (and transparent to the pointer)
/// unless a wire is being dragged.
class OpenPortTabLayer extends StatelessWidget {
  final StructureDesignerModel model;

  /// The build's shared resolver: the layer rebuilds only at the start and
  /// end of a drag, and whenever the canvas itself rebuilds, so the layout it
  /// was built from is current.
  final ScopeResolver resolver;

  const OpenPortTabLayer(
      {super.key, required this.model, required this.resolver});

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<PinReference?>(
      valueListenable: model.wireDragSource,
      builder: (context, source, _) {
        if (source == null) return const SizedBox.shrink();
        return Stack(
          clipBehavior: Clip.none,
          children: [
            for (final tab in openPortTabs(resolver))
              Positioned(
                left: tab.rect.left,
                top: tab.rect.top,
                child: OpenPortTab(
                  key: ValueKey(
                      'open_port_tab_${tab.scopeChain.join('/')}_${tab.nodeId}'),
                  model: model,
                  nodeId: tab.nodeId,
                  scopeChain: tab.scopeChain,
                  size: tab.rect.size,
                ),
              ),
          ],
        );
      },
    );
  }
}

/// One `+` tab: a drop target for a dragged wire. Highlighted while an
/// acceptable wire hovers it; drawn in a refused style with the reason beside
/// it otherwise (Flutter shows no tooltips during a drag, so the reason is a
/// label).
class OpenPortTab extends StatefulWidget {
  final StructureDesignerModel model;
  final BigInt nodeId;
  final List<BigInt> scopeChain;
  final Size size;

  const OpenPortTab({
    super.key,
    required this.model,
    required this.nodeId,
    required this.scopeChain,
    required this.size,
  });

  @override
  State<OpenPortTab> createState() => _OpenPortTabState();
}

class _OpenPortTabState extends State<OpenPortTab> {
  /// Why the hovering wire is refused, from the last hover check.
  String? _refusal;

  @override
  Widget build(BuildContext context) {
    return DragTarget<PinReference>(
      onWillAcceptWithDetails: (details) {
        final reason = widget.model.canAddInputFromWire(
            details.data, widget.nodeId,
            scopeChain: widget.scopeChain);
        if (reason != _refusal) setState(() => _refusal = reason);
        return reason == null;
      },
      onAcceptWithDetails: (details) {
        final error = widget.model.addInputFromWire(details.data, widget.nodeId,
            scopeChain: widget.scopeChain);
        if (error != null && mounted) showErrorSnackBar(context, error);
      },
      builder: (context, candidates, rejected) {
        final colors = Theme.of(context).colorScheme;
        final accepted = candidates.isNotEmpty;
        final refused = rejected.isNotEmpty;
        final Color border = refused
            ? colors.error
            : (accepted ? colors.primary : colors.outline);
        final Color fill = refused
            ? colors.errorContainer
            : (accepted ? colors.primary : colors.surfaceContainerHighest);
        final Color glyph = refused
            ? colors.onErrorContainer
            : (accepted ? colors.onPrimary : colors.onSurfaceVariant);
        final radius = Radius.circular(widget.size.height / 3);
        final tab = Container(
          width: widget.size.width,
          height: widget.size.height,
          decoration: BoxDecoration(
            color: fill,
            border: Border.all(color: border, width: accepted ? 1.5 : 1.0),
            borderRadius:
                BorderRadius.only(bottomLeft: radius, bottomRight: radius),
          ),
          alignment: Alignment.center,
          child: Icon(refused ? Icons.block : Icons.add,
              size: widget.size.height * 0.85, color: glyph),
        );
        if (!refused || _refusal == null) return tab;
        return Row(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            tab,
            const SizedBox(width: 4),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              decoration: BoxDecoration(
                color: colors.errorContainer,
                borderRadius: BorderRadius.circular(4),
              ),
              child: Text(
                _refusal!,
                style: TextStyle(fontSize: 11, color: colors.onErrorContainer),
              ),
            ),
          ],
        );
      },
    );
  }
}
