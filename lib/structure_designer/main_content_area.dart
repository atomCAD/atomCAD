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

  const MainContentArea({
    required this.graphModel,
    required this.nodeNetworkKey,
    Key? key,
  }) : super(key: key);

  @override
  Widget build(BuildContext context) {
    return Expanded(
      child: ResizableContainer(
        direction: Axis.vertical,
        children: [
          // 3D Viewport panel - initially 65% of height
          ResizableChild(
            size: ResizableSize.ratio(0.65),
            // Custom divider that appears below this panel
            divider: ResizableDivider(
              thickness: 8, // Height of the divider
              color: Colors.grey.shade300,
              cursor: SystemMouseCursors.resizeRow,
            ),
            child: StructureDesignerViewport(graphModel: graphModel),
          ),
          // Node Network panel - initially 35% of height
          ResizableChild(
            size: ResizableSize.ratio(0.35),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(
                  flex: 4,
                  child:
                      NodeNetwork(key: nodeNetworkKey, graphModel: graphModel),
                ),
                Container(
                  width: 300,
                  padding: const EdgeInsets.all(8.0),
                  decoration: const BoxDecoration(
                    border: Border(
                      left: BorderSide(
                        color: Colors.grey,
                        width: 1,
                      ),
                    ),
                  ),
                  child: NodeDataWidget(graphModel: graphModel),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
