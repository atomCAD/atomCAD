import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/record_path_rows_editor.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Property editor for `record_set` nodes: a root — a schema, or the type
/// taken from the wire — and the paths to assign,
/// one input pin per path, each with an inline literal used while its pin is
/// unwired (`doc/design_record_get_set.md`). The panel is the shared
/// [RecordPathRowsEditor]; literals are keyed by row index.
class RecordSetEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIRecordPathNodeData? data;
  final StructureDesignerModel model;

  const RecordSetEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return RecordPathRowsEditor(
      nodeId: nodeId,
      title: 'Record Set',
      nodeTypeName: 'record_set',
      data: data,
      model: model,
      onSetData: (schema, paths) =>
          model.setRecordSetData(nodeId, schema, paths),
      onSetLiteral: (rowIndex, value) =>
          model.setRecordSetLiteral(nodeId, rowIndex, value),
      onClearLiteral: (rowIndex) =>
          model.clearRecordSetLiteral(nodeId, rowIndex),
    );
  }
}
