import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/blocking_aware_single_child_scroll_view.dart';
import 'package:flutter_cad/structure_designer/node_data/anchor_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/circle_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/cuboid_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/extrude_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/sphere_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/half_plane_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/half_space_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/geo_trans_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/geo_to_atom_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/atom_trans.dart';
import 'package:flutter_cad/structure_designer/node_data/edit_atom_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/rect_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/reg_poly_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/stamp_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/facet_shell_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/relax_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/parameter_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/ivec3_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/ivec2_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/vec3_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/int_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/float_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/vec2_editor.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/stamp_api.dart';

/// A widget that displays and allows editing of node-specific data
/// based on the currently selected node in the graph.
class NodeDataWidget extends StatelessWidget {
  final StructureDesignerModel graphModel;

  const NodeDataWidget({
    super.key,
    required this.graphModel,
  });

  @override
  Widget build(BuildContext context) {
    // Listen to changes in the graph model
    return ChangeNotifierProvider.value(
      value: graphModel,
      child: Consumer<StructureDesignerModel>(
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

          // Wrap the editor widget in a SingleChildScrollView to handle tall editors
          return Padding(
            padding: const EdgeInsets.all(2.0),
            child: BlockingAwareSingleChildScrollView(
              child: _buildNodeEditor(selectedNode, model),
            ),
          );
        },
      ),
    );
  }

  // Helper method to build the appropriate editor based on node type
  Widget _buildNodeEditor(NodeView selectedNode, StructureDesignerModel model) {
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
          model: model,
        );
      case 'sphere':
        // Fetch the sphere data here in the parent widget
        final sphereData = getSphereData(
          nodeId: selectedNode.id,
        );
        return SphereEditor(
          nodeId: selectedNode.id,
          data: sphereData,
          model: model,
        );
      case 'half_space':
        // Fetch the half space data here in the parent widget
        final halfSpaceData = getHalfSpaceData(
          nodeId: selectedNode.id,
        );

        return HalfSpaceEditor(
          nodeId: selectedNode.id,
          data: halfSpaceData,
          model: model,
        );
      case 'geo_trans':
        // Fetch the geo transformation data here in the parent widget
        final geoTransData = getGeoTransData(
          nodeId: selectedNode.id,
        );

        return GeoTransEditor(
          nodeId: selectedNode.id,
          data: geoTransData,
          model: model,
        );
      case 'atom_trans':
        // Fetch the atom transformation data here in the parent widget
        final atomTransData = getAtomTransData(
          nodeId: selectedNode.id,
        );

        return AtomTransEditor(
          nodeId: selectedNode.id,
          data: atomTransData,
          model: model,
        );
      case 'geo_to_atom':
        // Fetch the geo to atom data here in the parent widget
        final geoToAtomData = getGeoToAtomData(
          nodeId: selectedNode.id,
        );

        return GeoToAtomEditor(
          nodeId: selectedNode.id,
          data: geoToAtomData,
        );

      case 'edit_atom':
        // Fetch the edit atom data here in the parent widget
        final editAtomData = getEditAtomData(
          nodeId: selectedNode.id,
        );

        return EditAtomEditor(
          nodeId: selectedNode.id,
          data: editAtomData,
          model: model,
        );
      case 'anchor':
        // Fetch the anchor data here in the parent widget
        final anchorData = getAnchorData(
          nodeId: selectedNode.id,
        );

        return AnchorEditor(
          nodeId: selectedNode.id,
          data: anchorData,
        );
      case 'stamp':
        // Fetch the stamp data here in the parent widget
        final stampView = getStampView(
          nodeId: selectedNode.id,
        );

        return StampEditor(
          nodeId: selectedNode.id,
          data: stampView,
          model: model,
        );
      case 'rect':
        // Fetch the rectangle data here in the parent widget
        final rectData = getRectData(
          nodeId: selectedNode.id,
        );

        return RectEditor(
          nodeId: selectedNode.id,
          data: rectData,
          model: model,
        );
      case 'circle':
        // Fetch the circle data here in the parent widget
        final circleData = getCircleData(
          nodeId: selectedNode.id,
        );

        return CircleEditor(
          nodeId: selectedNode.id,
          data: circleData,
          model: model,
        );
      case 'extrude':
        // Fetch the extrude data here in the parent widget
        final extrudeData = getExtrudeData(
          nodeId: selectedNode.id,
        );

        return ExtrudeEditor(
          nodeId: selectedNode.id,
          data: extrudeData,
          model: model,
        );
      case 'half_plane':
        // Fetch the half plane data here in the parent widget
        final halfPlaneData = getHalfPlaneData(
          nodeId: selectedNode.id,
        );

        return HalfPlaneEditor(
          nodeId: selectedNode.id,
          data: halfPlaneData,
          model: model,
        );
      case 'reg_poly':
        // Fetch the polygon data here in the parent widget
        final regPolyData = getRegPolyData(
          nodeId: selectedNode.id,
        );

        return RegPolyEditor(
          nodeId: selectedNode.id,
          data: regPolyData,
          model: model,
        );
      case 'facet_shell':
        // Fetch the facet shell data here in the parent widget
        final facetShellData = model.getFacetShellData(
          selectedNode.id,
        );

        return FacetShellEditor(
          nodeId: selectedNode.id,
          data: facetShellData,
          model: model,
        );
      case 'relax':
        // Relax editor doesn't need to fetch data - it gets it from the API
        return RelaxEditor(
          nodeId: selectedNode.id,
          model: model,
        );
      case 'parameter':
        // Fetch the parameter data here in the parent widget
        final parameterData = model.getParameterData(selectedNode.id);

        return ParameterEditor(
          nodeId: selectedNode.id,
          data: parameterData,
          model: model,
        );
      case 'ivec3':
        // Fetch the ivec3 data here in the parent widget
        final ivec3Data = getIvec3Data(
          nodeId: selectedNode.id,
        );

        return IVec3Editor(
          nodeId: selectedNode.id,
          data: ivec3Data,
          model: model,
        );
      case 'ivec2':
        // Fetch the ivec2 data here in the parent widget
        final ivec2Data = getIvec2Data(
          nodeId: selectedNode.id,
        );

        return IVec2Editor(
          nodeId: selectedNode.id,
          data: ivec2Data,
          model: model,
        );
      case 'vec3':
        // Fetch the vec3 data here in the parent widget
        final vec3Data = getVec3Data(
          nodeId: selectedNode.id,
        );

        return Vec3Editor(
          nodeId: selectedNode.id,
          data: vec3Data,
          model: model,
        );
      case 'int':
        // Fetch the int data here in the parent widget
        final intData = getIntData(
          nodeId: selectedNode.id,
        );

        return IntEditor(
          nodeId: selectedNode.id,
          data: intData,
          model: model,
        );
      case 'float':
        // Fetch the float data here in the parent widget
        final floatData = getFloatData(
          nodeId: selectedNode.id,
        );

        return FloatEditor(
          nodeId: selectedNode.id,
          data: floatData,
          model: model,
        );
      case 'vec2':
        // Fetch the vec2 data here in the parent widget
        final vec2Data = getVec2Data(
          nodeId: selectedNode.id,
        );

        return Vec2Editor(
          nodeId: selectedNode.id,
          data: vec2Data,
          model: model,
        );
      default:
        return Center(
          child: Text('No editor available for ${selectedNode.nodeTypeName}'),
        );
    }
  }
}
