import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/literal_fields_editor.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Auto-generated property panel for custom nodes (node types implemented by
/// user-defined node networks). A thin adapter over [LiteralFieldsEditor];
/// rendering, state machine, and input widgets all live in the shared widget.
///
/// See `doc/design_record_construct_property_panel.md`.
class CustomNodeEditor extends StatelessWidget {
  final BigInt nodeId;
  final String nodeTypeName;
  final List<APILiteralField> params;
  final StructureDesignerModel model;

  const CustomNodeEditor({
    super.key,
    required this.nodeId,
    required this.nodeTypeName,
    required this.params,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: LiteralFieldsEditor(
        header: NodeEditorHeader(
          title: 'Custom Node Properties',
          nodeTypeName: nodeTypeName,
        ),
        fields: params,
        emptyMessage: 'This custom node has no editable parameters.',
        onSet: (name, value) =>
            model.setCustomNodeLiteral(nodeId, name, value),
        onClear: (name) => model.clearCustomNodeLiteral(nodeId, name),
        keyPrefix: 'custom_param',
      ),
    );
  }
}
