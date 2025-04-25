import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/graph_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer_api.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/structure_designer/node_data/cuboid_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/sphere_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/half_space_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/geo_trans_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/atom_trans.dart';

/// A widget that displays and allows editing of node-specific data
/// based on the currently selected node in the graph.
class NodeDataWidget extends StatelessWidget {
  final GraphModel graphModel;

  const NodeDataWidget({
    super.key,
    required this.graphModel,
  });

  @override
  Widget build(BuildContext context) {
    // Listen to changes in the graph model
    return ChangeNotifierProvider.value(
      value: graphModel,
      child: Consumer<GraphModel>(
        builder: (context, model, child) {
          final nodeNetworkView = model.nodeNetworkView;
          if (nodeNetworkView == null) return const SizedBox.shrink();

          // Find the selected node
          final selectedNode = nodeNetworkView.nodes.entries
              .where((entry) => entry.value.selected)
              .map((entry) => entry.value)
              .firstOrNull;

          if (selectedNode == null) {
            return const Center(
              child: Text('No node selected'),
            );
          }

          // Based on the node type, show the appropriate editor
          switch (selectedNode.nodeTypeName) {
            case 'cuboid':
              // Fetch the cuboid data here in the parent widget
              final cuboidData = getCuboidData(
                nodeId: selectedNode.id,
              );

              return CuboidEditor(
                nodeId: selectedNode.id,
                data: cuboidData,
              );
            case 'sphere':
              // Fetch the sphere data here in the parent widget
              final sphereData = getSphereData(
                nodeId: selectedNode.id,
              );
              return SphereEditor(
                nodeId: selectedNode.id,
                data: sphereData,
              );
            case 'half_space':
              // Fetch the half space data here in the parent widget
              final halfSpaceData = getHalfSpaceData(
                nodeId: selectedNode.id,
              );

              return HalfSpaceEditor(
                nodeId: selectedNode.id,
                data: halfSpaceData,
              );
            case 'geo_trans':
              // Fetch the geo transformation data here in the parent widget
              final geoTransData = getGeoTransData(
                nodeId: selectedNode.id,
              );

              return GeoTransEditor(
                nodeId: selectedNode.id,
                data: geoTransData,
              );
            case 'atom_trans':
              // Fetch the atom transformation data here in the parent widget
              final atomTransData = getAtomTransData(
                nodeId: selectedNode.id,
              );

              return AtomTransEditor(
                nodeId: selectedNode.id,
                data: atomTransData,
              );
            default:
              return Center(
                child: Text(
                    'No editor available for ${selectedNode.nodeTypeName}'),
              );
          }
        },
      ),
    );
  }
}
