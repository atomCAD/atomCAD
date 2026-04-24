import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/matrix_cell.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor for the `imat3_rows` node. Lays out the stored 3x3 integer matrix
/// as three rows; each row is overridden by the corresponding wired input
/// pin (`a` → row 0, `b` → row 1, `c` → row 2).
class IMat3RowsEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIIMat3RowsData? data;
  final StructureDesignerModel model;

  const IMat3RowsEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  Widget build(BuildContext context) {
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final connected = _connectedRowPins();
    final rows = [
      [data!.a.x, data!.a.y, data!.a.z],
      [data!.b.x, data!.b.y, data!.b.z],
      [data!.c.x, data!.c.y, data!.c.z],
    ];

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'IMat3 Rows',
            nodeTypeName: 'imat3_rows',
          ),
          const SizedBox(height: 8),
          const Text(
            'Each row is one row of the output 3x3 integer matrix. A wired '
            'input pin overrides the stored row.',
            style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
          ),
          const SizedBox(height: 8),
          _row(
              rowIndex: 0, label: 'a', values: rows[0], enabled: !connected[0]),
          const SizedBox(height: 6),
          _row(
              rowIndex: 1, label: 'b', values: rows[1], enabled: !connected[1]),
          const SizedBox(height: 6),
          _row(
              rowIndex: 2, label: 'c', values: rows[2], enabled: !connected[2]),
        ],
      ),
    );
  }

  Widget _row({
    required int rowIndex,
    required String label,
    required List<int> values,
    required bool enabled,
  }) {
    final children = <Widget>[
      SizedBox(
        width: 24,
        child: Text('$label =', style: AppTextStyles.regular),
      ),
      const SizedBox(width: 4),
    ];
    for (var col = 0; col < 3; col++) {
      if (col > 0) children.add(const SizedBox(width: 4));
      children.add(IntMatrixCell(
        inputKey: Key('imat3_rows_cell_${rowIndex}_$col'),
        value: values[col],
        enabled: enabled,
        onChanged: (v) => _updateCell(rowIndex, col, v),
      ));
    }
    return Row(children: children);
  }

  void _updateCell(int row, int col, int value) {
    final rows = [
      [data!.a.x, data!.a.y, data!.a.z],
      [data!.b.x, data!.b.y, data!.b.z],
      [data!.c.x, data!.c.y, data!.c.z],
    ];
    rows[row][col] = value;
    model.setImat3RowsData(
      nodeId,
      APIIMat3RowsData(
        a: APIIVec3(x: rows[0][0], y: rows[0][1], z: rows[0][2]),
        b: APIIVec3(x: rows[1][0], y: rows[1][1], z: rows[1][2]),
        c: APIIVec3(x: rows[2][0], y: rows[2][1], z: rows[2][2]),
      ),
    );
  }

  List<bool> _connectedRowPins() {
    final connected = [false, false, false];
    final view = model.nodeNetworkView;
    if (view == null) return connected;
    for (final wire in view.wires) {
      if (wire.destNodeId != nodeId) continue;
      final idx = wire.destParamIndex.toInt();
      if (idx >= 0 && idx <= 2) connected[idx] = true;
    }
    return connected;
  }
}
