import 'package:flutter/material.dart';
import 'package:flutter_resizable_container/flutter_resizable_container.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/structure_designer_viewport.dart';
import 'package:flutter_cad/structure_designer/node_network/network_editor_tabs.dart';
import 'package:flutter_cad/structure_designer/schema_editor.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';
import 'package:flutter_cad/structure_designer/qualified_name_header.dart';

/// The main content area of the structure designer: the 3D viewport, the node
/// properties panel docked to its right, and the node network editor.
///
/// ## How the space is divided
///
/// The properties panel is glued to the **viewport**, not to the network
/// editor. The nesting, outermost first, is:
///
/// 1. the left sidebar (split off in `structure_designer.dart` — it wants full
///    height),
/// 2. the network editor (split off here by [ResizableContainer] — it wants a
///    lot of room, and the full width, because node graphs run left to right),
/// 3. the properties panel (split off last, sharing the viewport's row).
///
/// That ordering is what makes a tall properties panel possible. Before it, the
/// panel lived *inside* the network editor's share of the window and could
/// never be taller than that — which is exactly the case a mechanosynth build
/// walkthrough needs. Now it gets the viewport's height (65% by default), and
/// with the network editor collapsed it gets the whole window.
class MainContentArea extends StatefulWidget {
  final StructureDesignerModel graphModel;
  final GlobalKey nodeNetworkKey;

  /// Whether the division between viewport and node network is vertical (true)
  /// or horizontal (false). In either case the properties panel stays on the
  /// viewport's right edge.
  final bool verticalDivision;

  /// When true, render only the viewport (no node network editor or node data panel).
  final bool directEditingMode;

  /// Whether the node network editor (the bottom/right share) is shown.
  final bool networkEditorVisible;

  /// Whether the node properties panel (the viewport's right dock) is shown.
  final bool nodeDataPanelVisible;

  const MainContentArea({
    required this.graphModel,
    required this.nodeNetworkKey,
    this.verticalDivision = true,
    this.directEditingMode = false,
    this.networkEditorVisible = true,
    this.nodeDataPanelVisible = true,
    super.key,
  });

  @override
  State<MainContentArea> createState() => _MainContentAreaState();
}

class _MainContentAreaState extends State<MainContentArea> {
  /// Width of the node properties dock. Local UI state, like the left
  /// sidebar's width in `structure_designer.dart`.
  double _nodeDataPanelWidth = 400;

  static const double _nodeDataPanelMinWidth = 250;
  static const double _nodeDataPanelMaxWidth = 800;

  @override
  Widget build(BuildContext context) {
    if (widget.directEditingMode) {
      return Expanded(
        child: StructureDesignerViewport(graphModel: widget.graphModel),
      );
    }

    return Expanded(
      child: Consumer<StructureDesignerModel>(
        builder: (context, model, _) {
          // Record defs have no per-node properties, so the schema editor
          // takes the panel's place entirely (it carries its own name header).
          final isSchemaEditor = model.activeRecordDefName != null;
          final showNodeData = widget.nodeDataPanelVisible && !isSchemaEditor;

          final viewportArea = Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(
                child: StructureDesignerViewport(graphModel: widget.graphModel),
              ),
              if (showNodeData) ...[
                _buildNodeDataResizeHandle(),
                _buildNodeDataPanel(model: model),
              ],
            ],
          );

          if (!widget.networkEditorVisible) {
            return viewportArea;
          }

          return ResizableContainer(
            // The key forces a fresh subtree when the orientation flips, and
            // is what the integration tests look for.
            key: ValueKey(widget.verticalDivision
                ? 'vertical_layout'
                : 'horizontal_layout'),
            direction:
                widget.verticalDivision ? Axis.vertical : Axis.horizontal,
            children: [
              // Viewport + properties panel - initially 65% of height/width.
              ResizableChild(
                size: ResizableSize.ratio(0.65, min: 200),
                // Custom divider that appears below/beside this panel
                divider: ResizableDivider(
                  thickness: 8,
                  color: Colors.grey.shade300,
                  cursor: widget.verticalDivision
                      ? SystemMouseCursors.resizeRow
                      : SystemMouseCursors.resizeColumn,
                ),
                child: viewportArea,
              ),
              // Node network editor - initially 35% of height/width.
              ResizableChild(
                size: ResizableSize.ratio(0.35,
                    min: widget.verticalDivision ? 100 : 300),
                child: _buildNetworkEditor(),
              ),
            ],
          );
        },
      ),
    );
  }

  /// Builds the network editor: either the network editor tabs or the schema
  /// editor for the active record def. The choice is driven by
  /// `model.activeRecordDefName`; when non-null, the schema editor takes over.
  Widget _buildNetworkEditor() {
    return Consumer<StructureDesignerModel>(
      builder: (context, model, _) {
        if (model.activeRecordDefName != null) {
          return SchemaEditor(
            model: model,
            defName: model.activeRecordDefName!,
          );
        }
        return NetworkEditorTabs(
          graphModel: widget.graphModel,
          nodeNetworkKey: widget.nodeNetworkKey,
        );
      },
    );
  }

  /// Drag handle between the viewport and the properties dock. Dragging left
  /// widens the panel, hence the subtraction.
  Widget _buildNodeDataResizeHandle() {
    return GestureDetector(
      onHorizontalDragUpdate: (details) {
        setState(() {
          _nodeDataPanelWidth = (_nodeDataPanelWidth - details.delta.dx)
              .clamp(_nodeDataPanelMinWidth, _nodeDataPanelMaxWidth);
        });
      },
      child: MouseRegion(
        cursor: SystemMouseCursors.resizeColumn,
        child: Container(
          width: 6,
          color: Colors.grey.shade300,
        ),
      ),
    );
  }

  /// Builds the node data panel, headed by the active network's qualified name
  /// (issue #207).
  ///
  /// The header sits here rather than inside `NetworkDescriptionEditor` so it
  /// is present for **every** state of the panel — with a node selected just as
  /// much as without one. Which network you are editing is context for the
  /// whole panel, not a property of the no-selection case.
  Widget _buildNodeDataPanel({required StructureDesignerModel model}) {
    final networkName = model.nodeNetworkView?.name;
    return Container(
      width: _nodeDataPanelWidth,
      height: double.infinity,
      decoration: const BoxDecoration(
        border: Border(
          left: BorderSide(color: Colors.grey, width: 1),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Full-bleed strip: the panel's own padding moved onto the body below
          // so the header spans edge to edge like the schema editor's.
          if (networkName != null)
            QualifiedNameHeader(
              qualifiedName: networkName,
              icon: Icons.account_tree,
              copyTooltip: 'Copy qualified network name',
              copyConfirmation: 'Network name copied to clipboard',
            ),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.all(8.0),
              child: NodeDataWidget(graphModel: widget.graphModel),
            ),
          ),
        ],
      ),
    );
  }
}
