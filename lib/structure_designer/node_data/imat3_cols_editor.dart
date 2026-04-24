import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/matrix_cell.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor for the `imat3_cols` node. Lays out the stored 3x3 integer matrix
/// as three columns; each column is overridden by the corresponding wired
/// input pin (`a` → col 0, `b` → col 1, `c` → col 2).
class IMat3ColsEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIIMat3ColsData? data;
  final StructureDesignerModel model;

  const IMat3ColsEditor({
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
    final connected = _connectedColPins();
    final cols = [
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
            title: 'IMat3 Cols',
            nodeTypeName: 'imat3_cols',
          ),
          const SizedBox(height: 8),
          const Text(
            'Each column is one column of the output 3x3 integer matrix. A '
            'wired input pin overrides the stored column.',
            style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
          ),
          const SizedBox(height: 8),
          _headerRow(connected),
          const SizedBox(height: 6),
          for (var rowIndex = 0; rowIndex < 3; rowIndex++) ...[
            _bodyRow(rowIndex: rowIndex, cols: cols, connected: connected),
            if (rowIndex < 2) const SizedBox(height: 6),
          ],
        ],
      ),
    );
  }

  Widget _headerRow(List<bool> connected) {
    return Row(
      children: [
        const SizedBox(width: 28),
        for (var c = 0; c < 3; c++) ...[
          if (c > 0) const SizedBox(width: 4),
          SizedBox(
            width: MATRIX_CELL_WIDTH,
            child: Center(
              child: Text(
                ['a', 'b', 'c'][c],
                style: AppTextStyles.regular.copyWith(
                  color: connected[c] ? Colors.grey : null,
                ),
              ),
            ),
          ),
        ],
      ],
    );
  }

  Widget _bodyRow({
    required int rowIndex,
    required List<List<int>> cols,
    required List<bool> connected,
  }) {
    return Row(
      children: [
        SizedBox(
          width: 28,
          child: Center(
            child:
                Text(['x', 'y', 'z'][rowIndex], style: AppTextStyles.regular),
          ),
        ),
        for (var c = 0; c < 3; c++) ...[
          if (c > 0) const SizedBox(width: 4),
          IntMatrixCell(
            value: cols[c][rowIndex],
            enabled: !connected[c],
            onChanged: (v) => _updateCell(c, rowIndex, v),
          ),
        ],
      ],
    );
  }

  void _updateCell(int colIndex, int rowIndex, int value) {
    final cols = [
      [data!.a.x, data!.a.y, data!.a.z],
      [data!.b.x, data!.b.y, data!.b.z],
      [data!.c.x, data!.c.y, data!.c.z],
    ];
    cols[colIndex][rowIndex] = value;
    model.setImat3ColsData(
      nodeId,
      APIIMat3ColsData(
        a: APIIVec3(x: cols[0][0], y: cols[0][1], z: cols[0][2]),
        b: APIIVec3(x: cols[1][0], y: cols[1][1], z: cols[1][2]),
        c: APIIVec3(x: cols[2][0], y: cols[2][1], z: cols[2][2]),
      ),
    );
  }

  List<bool> _connectedColPins() {
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
