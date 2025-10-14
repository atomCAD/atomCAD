import 'package:flutter/material.dart';
import 'package:flutter_resizable_container/flutter_resizable_container.dart';
import 'package:flutter_cad/structure_designer/structure_designer_viewport.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';

/// The main content area of the structure designer, containing the 3D viewport and node network.
class MainContentArea extends StatelessWidget {
  final StructureDesignerModel graphModel;
  final GlobalKey nodeNetworkKey;

  /// Whether the division between viewport and node network is vertical (true) or horizontal (false)
  final bool verticalDivision;

  const MainContentArea({
    required this.graphModel,
    required this.nodeNetworkKey,
    this.verticalDivision = true,
    Key? key,
  }) : super(key: key);

  @override
  Widget build(BuildContext context) {
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

  /// Builds the NodeNetwork widget with the global key
  Widget _buildNodeNetwork() {
    return NodeNetwork(key: nodeNetworkKey, graphModel: graphModel);
  }

  /// Builds the network panel for vertical layout (side-by-side network and data)
  Widget _buildVerticalNetworkPanel() {
    return Row(
      key: const ValueKey('vertical_layout'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          flex: 4,
          child: _buildNodeNetwork(),
        ),
        _buildNodeDataPanel(isVertical: true),
      ],
    );
  }

  /// Builds the network panel for horizontal layout (stacked network and data)
  Widget _buildHorizontalNetworkPanel() {
    return Column(
      key: const ValueKey('horizontal_layout'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(
          flex: 4,
          child: _buildNodeNetwork(),
        ),
        _buildNodeDataPanel(isVertical: false),
      ],
    );
  }

  /// Builds the node data panel with appropriate decoration based on orientation
  Widget _buildNodeDataPanel({required bool isVertical}) {
    return Container(
      width: isVertical ? 400 : double.infinity,
      height: isVertical ? double.infinity : 240,
      padding: const EdgeInsets.all(8.0),
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
      child: NodeDataWidget(graphModel: graphModel),
    );
  }
}
