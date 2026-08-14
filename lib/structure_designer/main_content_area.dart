import 'package:flutter/material.dart';
import 'package:flutter_resizable_container/flutter_resizable_container.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/structure_designer_viewport.dart';
import 'package:flutter_cad/structure_designer/node_network/network_editor_tabs.dart';
import 'package:flutter_cad/structure_designer/schema_editor.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';
import 'package:flutter_cad/structure_designer/qualified_name_header.dart';

/// The main content area of the structure designer, containing the 3D viewport and node network.
class MainContentArea extends StatelessWidget {
  final StructureDesignerModel graphModel;
  final GlobalKey nodeNetworkKey;

  /// Whether the division between viewport and node network is vertical (true) or horizontal (false)
  final bool verticalDivision;

  /// When true, render only the viewport (no node network editor or node data panel).
  final bool directEditingMode;

  const MainContentArea({
    required this.graphModel,
    required this.nodeNetworkKey,
    this.verticalDivision = true,
    this.directEditingMode = false,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    if (directEditingMode) {
      return Expanded(
        child: StructureDesignerViewport(graphModel: graphModel),
      );
    }

    return Expanded(
      child: ResizableContainer(
        direction: verticalDivision ? Axis.vertical : Axis.horizontal,
        children: [
          // Viewport panel - initially 65% of height/width
          ResizableChild(
            size: ResizableSize.ratio(0.65, min: 100),
            // Custom divider that appears below/beside this panel
            divider: ResizableDivider(
              thickness: 8,
              color: Colors.grey.shade300,
              cursor: verticalDivision
                  ? SystemMouseCursors.resizeRow
                  : SystemMouseCursors.resizeColumn,
            ),
            child: StructureDesignerViewport(graphModel: graphModel),
          ),
          // Node Network panel - initially 35% of height/width
          ResizableChild(
            size: ResizableSize.ratio(0.35, min: verticalDivision ? 100 : 300),
            child: verticalDivision
                ? _buildVerticalNetworkPanel()
                : _buildHorizontalNetworkPanel(),
          ),
        ],
      ),
    );
  }

  /// Builds the bottom-of-main-area editor: either the network editor tabs
  /// or the schema editor for the active record def. The choice is driven
  /// by `model.activeRecordDefName`; when non-null, the schema editor takes
  /// over (the node-data side panel is hidden because record defs have no
  /// per-node properties).
  Widget _buildBottomEditor() {
    return Consumer<StructureDesignerModel>(
      builder: (context, model, _) {
        if (model.activeRecordDefName != null) {
          return SchemaEditor(
            model: model,
            defName: model.activeRecordDefName!,
          );
        }
        return NetworkEditorTabs(
          graphModel: graphModel,
          nodeNetworkKey: nodeNetworkKey,
        );
      },
    );
  }

  /// Builds the network panel for vertical layout (side-by-side network and data)
  Widget _buildVerticalNetworkPanel() {
    return Consumer<StructureDesignerModel>(
      builder: (context, model, _) {
        final isSchemaEditor = model.activeRecordDefName != null;
        return Row(
          key: const ValueKey('vertical_layout'),
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Expanded(
              flex: 4,
              child: _buildBottomEditor(),
            ),
            // Hide the node-data side panel while the schema editor is shown
            // — record defs have no per-node properties to edit there.
            if (!isSchemaEditor)
              _buildNodeDataPanel(isVertical: true, model: model),
          ],
        );
      },
    );
  }

  /// Builds the network panel for horizontal layout (stacked network and data)
  Widget _buildHorizontalNetworkPanel() {
    return Consumer<StructureDesignerModel>(
      builder: (context, model, _) {
        final isSchemaEditor = model.activeRecordDefName != null;
        return Column(
          key: const ValueKey('horizontal_layout'),
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Expanded(
              flex: 4,
              child: _buildBottomEditor(),
            ),
            if (!isSchemaEditor)
              _buildNodeDataPanel(isVertical: false, model: model),
          ],
        );
      },
    );
  }

  /// Builds the node data panel with appropriate decoration based on
  /// orientation, headed by the active network's qualified name (issue #207).
  ///
  /// The header sits here rather than inside `NetworkDescriptionEditor` so it
  /// is present for **every** state of the panel — with a node selected just as
  /// much as without one. Which network you are editing is context for the
  /// whole panel, not a property of the no-selection case; before this, the
  /// qualified name was not displayed anywhere in the application.
  ///
  /// The panel is hidden outright while the schema editor is active (see the
  /// callers), so there is no record-def case to handle: `SchemaEditor` carries
  /// its own name header.
  Widget _buildNodeDataPanel(
      {required bool isVertical, required StructureDesignerModel model}) {
    final networkName = model.nodeNetworkView?.name;
    return Container(
      width: isVertical ? 400 : double.infinity,
      height: isVertical ? double.infinity : 240,
      decoration: BoxDecoration(
        border: Border(
          // Apply different border based on orientation
          left: isVertical
              ? const BorderSide(color: Colors.grey, width: 1)
              : BorderSide.none,
          top: !isVertical
              ? const BorderSide(color: Colors.grey, width: 1)
              : BorderSide.none,
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
              child: NodeDataWidget(graphModel: graphModel),
            ),
          ),
        ],
      ),
    );
  }
}
