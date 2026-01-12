import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/blocking_aware_single_child_scroll_view.dart';
import 'package:flutter_cad/structure_designer/node_data/circle_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/cuboid_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/extrude_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/sphere_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/half_plane_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/half_space_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/drawing_plane_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/geo_trans_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/lattice_symop_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/lattice_move_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/lattice_rot_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/atom_trans.dart';
import 'package:flutter_cad/structure_designer/node_data/edit_atom_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/rect_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/reg_poly_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/facet_shell_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/relax_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/parameter_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/map_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/ivec3_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/ivec2_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/vec3_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/int_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/range_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/string_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/bool_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/float_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/vec2_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/expr_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/motif_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/atom_fill_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/import_xyz_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/export_xyz_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/atom_cut_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/unit_cell_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/network_description_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/comment_editor.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

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
            return Padding(
              padding: const EdgeInsets.all(2.0),
              child: BlockingAwareSingleChildScrollView(
                child: NetworkDescriptionEditor(
                  key: ValueKey(nodeNetworkView.name),
                ),
              ),
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
      case 'Comment':
        final commentData = getCommentData(nodeId: selectedNode.id);
        return CommentEditor(
          nodeId: selectedNode.id,
          data: commentData,
          model: model,
        );
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
      case 'drawing_plane':
        // Fetch the drawing plane data here in the parent widget
        final drawingPlaneData = getDrawingPlaneData(
          nodeId: selectedNode.id,
        );

        return DrawingPlaneEditor(
          nodeId: selectedNode.id,
          data: drawingPlaneData,
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
      case 'lattice_symop':
        // Fetch the lattice symmetry operation data here in the parent widget
        final latticeSymopData = model.getLatticeSymopData(selectedNode.id);

        return LatticeSymopEditor(
          nodeId: selectedNode.id,
          data: latticeSymopData,
          model: model,
        );
      case 'lattice_move':
        // Fetch the lattice move data here in the parent widget
        final latticeMoveData = model.getLatticeMoveData(selectedNode.id);

        return LatticeMoveEditor(
          nodeId: selectedNode.id,
          data: latticeMoveData,
          model: model,
        );
      case 'lattice_rot':
        // Fetch the lattice rotation data here in the parent widget
        final latticeRotData = model.getLatticeRotData(selectedNode.id);

        return LatticeRotEditor(
          nodeId: selectedNode.id,
          data: latticeRotData,
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
      case 'map':
        // Fetch the map data here in the parent widget
        final mapData = getMapData(
          nodeId: selectedNode.id,
        );

        return MapEditor(
          nodeId: selectedNode.id,
          data: mapData,
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
      case 'range':
        // Fetch the range data here in the parent widget
        final rangeData = getRangeData(
          nodeId: selectedNode.id,
        );

        return RangeEditor(
          nodeId: selectedNode.id,
          data: rangeData,
          model: model,
        );
      case 'string':
        // Fetch the string data here in the parent widget
        final stringData = getStringData(
          nodeId: selectedNode.id,
        );

        return StringEditor(
          nodeId: selectedNode.id,
          data: stringData,
          model: model,
        );
      case 'bool':
        // Fetch the bool data here in the parent widget
        final boolData = getBoolData(
          nodeId: selectedNode.id,
        );

        return BoolEditor(
          nodeId: selectedNode.id,
          data: boolData,
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
      case 'expr':
        // Fetch the expr data here in the parent widget
        final exprData = model.getExprData(selectedNode.id);

        return ExprEditor(
          nodeId: selectedNode.id,
          data: exprData,
          model: model,
        );
      case 'motif':
        // Fetch the motif data here in the parent widget
        final motifData = model.getMotifData(selectedNode.id);

        return MotifEditor(
          nodeId: selectedNode.id,
          data: motifData,
          model: model,
        );
      case 'atom_fill':
        // Fetch the atom_fill data here in the parent widget
        final atomFillData = model.getAtomFillData(selectedNode.id);

        return AtomFillEditor(
          nodeId: selectedNode.id,
          data: atomFillData,
          model: model,
        );
      case 'import_xyz':
        // Fetch the import_xyz data here in the parent widget
        final importXyzData = model.getImportXyzData(selectedNode.id);

        return ImportXyzEditor(
          nodeId: selectedNode.id,
          data: importXyzData,
          model: model,
        );
      case 'export_xyz':
        // Fetch the export_xyz data here in the parent widget
        final exportXyzData = getExportXyzData(
          nodeId: selectedNode.id,
        );

        return ExportXyzEditor(
          nodeId: selectedNode.id,
          data: exportXyzData,
          model: model,
        );
      case 'atom_cut':
        // Fetch the atom_cut data here in the parent widget
        final atomCutData = getAtomCutData(
          nodeId: selectedNode.id,
        );

        return AtomCutEditor(
          nodeId: selectedNode.id,
          data: atomCutData,
          model: model,
        );
      case 'unit_cell':
        // Fetch the unit_cell data here in the parent widget
        final unitCellData = getUnitCellData(
          nodeId: selectedNode.id,
        );

        return UnitCellEditor(
          nodeId: selectedNode.id,
          data: unitCellData,
          model: model,
        );
      default:
        return Center(
          child: Text('No editor available for ${selectedNode.nodeTypeName}'),
        );
    }
  }
}
