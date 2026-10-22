import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/record_path_rows_editor.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Property editor for `record_get` nodes: a root — a schema, or the type
/// taken from the wire — and the paths to read, one output pin per path
/// (`doc/design_record_get_set.md`, `doc/design_anonymous_record_readers.md`).
/// The panel is the shared [RecordPathRowsEditor].
class RecordGetEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIRecordPathNodeData? data;
  final StructureDesignerModel model;

  const RecordGetEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    return RecordPathRowsEditor(
      nodeId: nodeId,
      title: 'Record Get',
      nodeTypeName: 'record_get',
      data: data,
      model: model,
      onSetData: (schema, paths) =>
          model.setRecordGetData(nodeId, schema, paths),
    );
  }
}
