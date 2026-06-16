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
import 'package:flutter_cad/structure_designer/node_data/structure_move_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/structure_rot_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/free_move_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/free_rot_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/edit_atom_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/atom_edit_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/rect_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/reg_poly_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/facet_shell_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/relax_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/parameter_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/print_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/map_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/array_at_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/collect_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/filter_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/foreach_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/fold_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/closure_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/apply_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/sequence_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/ivec3_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/ivec2_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/vec3_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/int_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/range_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/product_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/record_construct_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/record_destructure_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/string_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/bool_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/float_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/vec2_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/expr_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/motif_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/motif_sub_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/materialize_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/import_xyz_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/import_cif_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/infer_bonds_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/atom_replace_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/export_xyz_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/apply_diff_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/atom_composediff_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/atom_cut_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/lattice_vecs_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/supercell_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/imat2_rows_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/imat2_cols_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/imat2_diag_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/plane_tiling_vectors_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/imat3_rows_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/imat3_cols_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/imat3_diag_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/mat3_rows_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/mat3_cols_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/mat3_diag_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/network_description_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/comment_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/custom_node_editor.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Keys for property editor widgets used in integration testing.
class PropertyEditorKeys {
  // Main container
  static const Key panel = Key('node_data_panel');

  // Editor-specific keys
  static const Key floatEditor = Key('property_float_editor');
  static const Key intEditor = Key('property_int_editor');
  static const Key boolEditor = Key('property_bool_editor');
  static const Key stringEditor = Key('property_string_editor');
  static const Key vec3Editor = Key('property_vec3_editor');
  static const Key cuboidEditor = Key('property_cuboid_editor');
  static const Key sphereEditor = Key('property_sphere_editor');

  // Input field keys
  static const Key floatValueInput = Key('property_float_value_input');
  static const Key intValueInput = Key('property_int_value_input');
  static const Key boolValueCheckbox = Key('property_bool_value_checkbox');
  static const Key stringValueInput = Key('property_string_value_input');

  // Vec3 input keys
  static const Key vec3XInput = Key('property_vec3_x_input');
  static const Key vec3YInput = Key('property_vec3_y_input');
  static const Key vec3ZInput = Key('property_vec3_z_input');

  // Cuboid-specific keys
  static const Key cuboidMinCornerXInput = Key('property_cuboid_min_x_input');
  static const Key cuboidMinCornerYInput = Key('property_cuboid_min_y_input');
  static const Key cuboidMinCornerZInput = Key('property_cuboid_min_z_input');
  static const Key cuboidExtentXInput = Key('property_cuboid_extent_x_input');
  static const Key cuboidExtentYInput = Key('property_cuboid_extent_y_input');
  static const Key cuboidExtentZInput = Key('property_cuboid_extent_z_input');

  // Sphere-specific keys
  static const Key sphereCenterXInput = Key('property_sphere_center_x_input');
  static const Key sphereCenterYInput = Key('property_sphere_center_y_input');
  static const Key sphereCenterZInput = Key('property_sphere_center_z_input');
  static const Key sphereRadiusInput = Key('property_sphere_radius_input');
}

/// A widget that displays and allows editing of node-specific data
/// based on the currently selected node in the graph.
class NodeDataWidget extends StatelessWidget {
  final StructureDesignerModel graphModel;
  final bool directEditingMode;

  const NodeDataWidget({
    super.key,
    required this.graphModel,
    this.directEditingMode = false,
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

          // Find the selected node — first in the active body's scope, then
          // anywhere in the scope tree. Body-scope selection drives the
          // property panel when the user clicked into a body. See
          // `doc/design_zones_ui.md` §"The active body".
          final selected = _findSelectedNode(model, nodeNetworkView);

          if (selected == null) {
            final description = getActiveNetworkDescription() ?? '';
            final summary = getActiveNetworkSummary() ?? '';
            return Padding(
              padding: const EdgeInsets.all(2.0),
              child: BlockingAwareSingleChildScrollView(
                child: NetworkDescriptionEditor(
                  key: ValueKey(nodeNetworkView.name),
                  description: description,
                  summary: summary,
                ),
              ),
            );
          }

          // Record the selected node's resolved scope so property getters and
          // setters address the right node even when a body node's id collides
          // with a top-level id. A plain field assignment (no notifyListeners)
          // during build is safe.
          model.propertyEditorScopeChain = selected.scopeChain;

          // Wrap the editor widget in a SingleChildScrollView to handle tall editors
          return Padding(
            padding: const EdgeInsets.all(2.0),
            child: BlockingAwareSingleChildScrollView(
              child: _buildNodeEditor(selected.node, model),
            ),
          );
        },
      ),
    );
  }

  /// Find the selected node anywhere in the scope tree, preferring the
  /// active body's selection. Returns null if nothing is selected.
  ({NodeView node, List<BigInt> scopeChain})? _findSelectedNode(
    StructureDesignerModel model,
    NodeNetworkView rootView,
  ) {
    // Active body first: if the user just clicked into a body, that body's
    // selected node drives the property panel.
    if (model.activeScopeChain.isNotEmpty) {
      Map<BigInt, NodeView> current = rootView.nodes;
      bool valid = true;
      for (final hofId in model.activeScopeChain) {
        final hof = current[hofId];
        final zone = hof?.zone;
        if (zone == null) {
          valid = false;
          break;
        }
        current = zone.nodes;
      }
      if (valid) {
        for (final entry in current.entries) {
          if (entry.value.selected) {
            return (node: entry.value, scopeChain: model.activeScopeChain);
          }
        }
      }
    }
    // Fall back: walk the whole tree, tracking the scope chain of the match.
    return _findSelectedRecursive(rootView.nodes, const <BigInt>[]);
  }

  ({NodeView node, List<BigInt> scopeChain})? _findSelectedRecursive(
    Map<BigInt, NodeView> nodes,
    List<BigInt> scopeChain,
  ) {
    for (final entry in nodes.entries) {
      if (entry.value.selected) {
        return (node: entry.value, scopeChain: scopeChain);
      }
    }
    for (final entry in nodes.entries) {
      final zone = entry.value.zone;
      if (zone == null) continue;
      final inner = _findSelectedRecursive(
        zone.nodes,
        [...scopeChain, entry.key],
      );
      if (inner != null) return inner;
    }
    return null;
  }

  // Helper method to build the appropriate editor based on node type
  Widget _buildNodeEditor(NodeView selectedNode, StructureDesignerModel model) {
    // Scope of the node being edited (set in `build` from the resolved
    // selection). Direct FRB getters below pass this so they resolve the right
    // node in its body; model-routed getters read it from the model.
    final scopePath = model.propertyEditorScopePath;
    // Based on the node type, show the appropriate editor
    switch (selectedNode.nodeTypeName) {
      case 'Comment':
        final commentData =
            getCommentData(scopePath: scopePath, nodeId: selectedNode.id);
        return CommentEditor(
          nodeId: selectedNode.id,
          data: commentData,
          model: model,
        );
      case 'cuboid':
        // Fetch the cuboid data here in the parent widget
        final cuboidData = getCuboidData(
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
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
      case 'structure_move':
        // Fetch the structure move data here in the parent widget
        final structureMoveData = model.getStructureMoveData(selectedNode.id);

        return StructureMoveEditor(
          nodeId: selectedNode.id,
          data: structureMoveData,
          model: model,
        );
      case 'structure_rot':
        // Fetch the structure rotation data here in the parent widget
        final structureRotData = model.getStructureRotData(selectedNode.id);

        return StructureRotEditor(
          nodeId: selectedNode.id,
          data: structureRotData,
          model: model,
        );
      case 'free_move':
        // Fetch the free move data here in the parent widget
        final freeMoveData = getFreeMoveData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return FreeMoveEditor(
          nodeId: selectedNode.id,
          data: freeMoveData,
          model: model,
        );
      case 'free_rot':
        // Fetch the free rotation data here in the parent widget
        final freeRotData = getFreeRotData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return FreeRotEditor(
          nodeId: selectedNode.id,
          data: freeRotData,
          model: model,
        );
      case 'edit_atom':
        // Fetch the edit atom data here in the parent widget
        final editAtomData = getEditAtomData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return EditAtomEditor(
          nodeId: selectedNode.id,
          data: editAtomData,
          model: model,
        );
      case 'atom_edit':
      case 'motif_edit':
        final atomEditData = getAtomEditData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return AtomEditEditor(
          nodeId: selectedNode.id,
          data: atomEditData,
          model: model,
          directEditingMode: directEditingMode,
        );
      case 'rect':
        // Fetch the rectangle data here in the parent widget
        final rectData = getRectData(
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return MapEditor(
          nodeId: selectedNode.id,
          data: mapData,
          model: model,
          node: selectedNode,
        );
      case 'filter':
        final filterData = getFilterData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return FilterEditor(
          nodeId: selectedNode.id,
          data: filterData,
          model: model,
        );
      case 'foreach':
        final foreachData = getForeachData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return ForeachEditor(
          nodeId: selectedNode.id,
          data: foreachData,
          model: model,
        );
      case 'collect':
        final collectData = getCollectData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return CollectEditor(
          nodeId: selectedNode.id,
          data: collectData,
          model: model,
        );
      case 'array_at':
        final arrayAtData = getArrayAtData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return ArrayAtEditor(
          nodeId: selectedNode.id,
          data: arrayAtData,
          model: model,
        );
      case 'fold':
        final foldData = getFoldData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return FoldEditor(
          nodeId: selectedNode.id,
          data: foldData,
          model: model,
        );
      case 'closure':
        final closureData =
            getClosureData(scopePath: scopePath, nodeId: selectedNode.id);
        return ClosureShapeEditor(
          title: 'Closure Properties',
          nodeTypeName: 'closure',
          kind: closureData?.kind ?? APIClosureKind.map,
          typeArgs: closureData?.typeArgs ?? const [],
          paramNames: closureData?.paramNames ?? const [],
          customLabel: closureData?.customLabel,
          labelEnabled: true,
          loading: closureData == null,
          onChanged: (kind, typeArgs, paramNames, customLabel) =>
              model.setClosureData(
            selectedNode.id,
            APIClosureData(
              kind: kind,
              typeArgs: typeArgs,
              paramNames: paramNames,
              customLabel: customLabel,
            ),
          ),
        );
      case 'apply':
        // Phase D: no kind picker — `apply` derives its arg pins from the
        // wired `f` source's flat function type. The panel is informational
        // only; `ApplyData.kind` / `type_args` stay on disk for `.cnnd`
        // back-compat but are structurally irrelevant. See
        // `doc/design_function_pin_unification.md` (Phase D).
        return ApplyEditor(node: selectedNode);
      case 'sequence':
        final sequenceData = model.getSequenceData(selectedNode.id);
        return SequenceEditor(
          nodeId: selectedNode.id,
          data: sequenceData,
          model: model,
        );
      case 'ivec3':
        // Fetch the ivec3 data here in the parent widget
        final ivec3Data = getIvec3Data(
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
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
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return RangeEditor(
          nodeId: selectedNode.id,
          data: rangeData,
          model: model,
        );
      case 'record_construct':
        final recordConstructData = getRecordConstructData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );
        return RecordConstructEditor(
          nodeId: selectedNode.id,
          data: recordConstructData,
          model: model,
        );
      case 'record_destructure':
        final recordDestructureData = getRecordDestructureData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );
        return RecordDestructureEditor(
          nodeId: selectedNode.id,
          data: recordDestructureData,
          model: model,
        );
      case 'product':
        final productData =
            getProductData(scopePath: scopePath, nodeId: selectedNode.id);
        return ProductEditor(
          nodeId: selectedNode.id,
          data: productData,
          model: model,
        );
      case 'string':
        // Fetch the string data here in the parent widget
        final stringData = getStringData(
          scopePath: scopePath,
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
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return BoolEditor(
          nodeId: selectedNode.id,
          data: boolData,
          model: model,
        );
      case 'print':
        final printData =
            getPrintData(scopePath: scopePath, nodeId: selectedNode.id);
        return PrintEditor(
          nodeId: selectedNode.id,
          data: printData,
          model: model,
        );
      case 'float':
        // Fetch the float data here in the parent widget
        final floatData = getFloatData(
          scopePath: scopePath,
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
          scopePath: scopePath,
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
      case 'motif_sub':
        final motifSubData = model.getMotifSubData(selectedNode.id);

        return MotifSubEditor(
          nodeId: selectedNode.id,
          data: motifSubData,
          model: model,
        );
      case 'materialize':
        // Fetch the materialize data here in the parent widget
        final materializeData = model.getMaterializeData(selectedNode.id);

        return MaterializeEditor(
          nodeId: selectedNode.id,
          data: materializeData,
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
      case 'import_cif':
        final importCifData = model.getImportCifData(selectedNode.id);

        return ImportCifEditor(
          nodeId: selectedNode.id,
          data: importCifData,
          model: model,
        );
      case 'infer_bonds':
        final inferBondsData = model.getInferBondsData(selectedNode.id);

        return InferBondsEditor(
          nodeId: selectedNode.id,
          data: inferBondsData,
          model: model,
        );
      case 'atom_replace':
        final atomReplaceData = model.getAtomReplaceData(selectedNode.id);

        return AtomReplaceEditor(
          nodeId: selectedNode.id,
          data: atomReplaceData,
          model: model,
        );
      case 'export_xyz':
        // Fetch the export_xyz data here in the parent widget
        final exportXyzData = getExportXyzData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return ExportXyzEditor(
          nodeId: selectedNode.id,
          data: exportXyzData,
          model: model,
        );
      case 'apply_diff':
        final applyDiffData = getApplyDiffData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return ApplyDiffEditor(
          nodeId: selectedNode.id,
          data: applyDiffData,
          model: model,
        );
      case 'atom_composediff':
        final composeDiffData = getAtomComposediffData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return AtomComposeDiffEditor(
          nodeId: selectedNode.id,
          data: composeDiffData,
          model: model,
        );
      case 'atom_cut':
        // Fetch the atom_cut data here in the parent widget
        final atomCutData = getAtomCutData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return AtomCutEditor(
          nodeId: selectedNode.id,
          data: atomCutData,
          model: model,
        );
      case 'lattice_vecs':
        final latticeVecsData = getLatticeVecsData(
          scopePath: scopePath,
          nodeId: selectedNode.id,
        );

        return LatticeVecsEditor(
          nodeId: selectedNode.id,
          data: latticeVecsData,
          model: model,
        );
      case 'supercell':
        final supercellData =
            getSupercellData(scopePath: scopePath, nodeId: selectedNode.id);

        return SupercellEditor(
          nodeId: selectedNode.id,
          data: supercellData,
          model: model,
        );
      case 'imat2_rows':
        return IMat2RowsEditor(
          nodeId: selectedNode.id,
          data: getImat2RowsData(scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      case 'imat2_cols':
        return IMat2ColsEditor(
          nodeId: selectedNode.id,
          data: getImat2ColsData(scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      case 'imat2_diag':
        return IMat2DiagEditor(
          nodeId: selectedNode.id,
          data: getImat2DiagData(scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      case 'plane_tiling_vectors':
        return PlaneTilingVectorsEditor(
          nodeId: selectedNode.id,
          data: getPlaneTilingVectorsData(
              scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      case 'imat3_rows':
        return IMat3RowsEditor(
          nodeId: selectedNode.id,
          data: getImat3RowsData(scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      case 'imat3_cols':
        return IMat3ColsEditor(
          nodeId: selectedNode.id,
          data: getImat3ColsData(scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      case 'imat3_diag':
        return IMat3DiagEditor(
          nodeId: selectedNode.id,
          data: getImat3DiagData(scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      case 'mat3_rows':
        return Mat3RowsEditor(
          nodeId: selectedNode.id,
          data: getMat3RowsData(scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      case 'mat3_cols':
        return Mat3ColsEditor(
          nodeId: selectedNode.id,
          data: getMat3ColsData(scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      case 'mat3_diag':
        return Mat3DiagEditor(
          nodeId: selectedNode.id,
          data: getMat3DiagData(scopePath: scopePath, nodeId: selectedNode.id),
          model: model,
        );
      default:
        // Custom node type names are dynamic, so they cannot be `case`
        // labels. `null` means it is genuinely not a custom node; a custom
        // node returns a (possibly empty) parameter list.
        final params = model.getCustomNodeParams(selectedNode.id);
        if (params == null) {
          return Center(
            child: Text('No editor available for ${selectedNode.nodeTypeName}'),
          );
        }
        return CustomNodeEditor(
          nodeId: selectedNode.id,
          nodeTypeName: selectedNode.nodeTypeName,
          params: params,
          model: model,
        );
    }
  }
}
