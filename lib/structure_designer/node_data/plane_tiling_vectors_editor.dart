import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/matrix_cell.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

const int _SUPERLATTICE_PIN_INDEX = 1;

/// Editor widget for the `plane_tiling_vectors` node.
///
/// Renders the stored 2×2 integer superlattice as two rows, each expressing a
/// tiling vector as an integer combination of the in-plane basis vectors `u`
/// and `v` (supplied by the wired `plane` input). A live determinant readout
/// sits below. When the `superlattice` input pin is connected the stored matrix
/// is overridden at eval time by the wired IMat2, so the grid is grayed out and
/// the determinant is unknown at edit time.
class PlaneTilingVectorsEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIPlaneTilingVectorsData? data;
  final StructureDesignerModel model;

  const PlaneTilingVectorsEditor({
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

    final connected = _isSuperlatticeConnected();
    final matrix = [
      [data!.a.x, data!.a.y],
      [data!.b.x, data!.b.y],
    ];

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Plane Tiling Vectors',
            nodeTypeName: 'plane_tiling_vectors',
          ),
          const SizedBox(height: 8),
          _hintText(connected),
          const SizedBox(height: 8),
          _matrixRows(matrix: matrix, enabled: !connected),
          const SizedBox(height: 12),
          _determinantReadout(matrix: matrix, pinConnected: connected),
        ],
      ),
    );
  }

  Widget _hintText(bool connected) {
    if (connected) {
      return const Text(
        'Superlattice pin is connected — the stored matrix is overridden by '
        'the wired IMat2 at evaluation.',
        style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
      );
    }
    return const Text(
      'Each row is a tiling vector as an integer combination of the in-plane '
      'lattice vectors u, v (supplied by the plane input).',
      style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
    );
  }

  Widget _matrixRows({
    required List<List<int>> matrix,
    required bool enabled,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _row(rowIndex: 0, label: 'vec1', values: matrix[0], enabled: enabled),
        const SizedBox(height: 6),
        _row(rowIndex: 1, label: 'vec2', values: matrix[1], enabled: enabled),
      ],
    );
  }

  Widget _row({
    required int rowIndex,
    required String label,
    required List<int> values,
    required bool enabled,
  }) {
    final basisLabels = ['u', 'v'];
    final children = <Widget>[
      SizedBox(
        width: 48,
        child: Text('$label  =', style: AppTextStyles.regular),
      ),
    ];
    for (var col = 0; col < 2; col++) {
      if (col > 0) {
        children.add(Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4),
          child: Text('+', style: AppTextStyles.regular),
        ));
      }
      children.add(IntMatrixCell(
        inputKey: Key('plane_tiling_vectors_cell_${rowIndex}_$col'),
        value: values[col],
        enabled: enabled,
        onChanged: (newValue) => _updateCell(rowIndex, col, newValue),
      ));
      children.add(Padding(
        padding: const EdgeInsets.only(left: 4),
        child: Text('·${basisLabels[col]}', style: AppTextStyles.regular),
      ));
    }
    return Row(children: children);
  }

  void _updateCell(int row, int col, int value) {
    final matrix = [
      [data!.a.x, data!.a.y],
      [data!.b.x, data!.b.y],
    ];
    matrix[row][col] = value;
    model.setPlaneTilingVectorsData(
      nodeId,
      APIPlaneTilingVectorsData(
        a: APIIVec2(x: matrix[0][0], y: matrix[0][1]),
        b: APIIVec2(x: matrix[1][0], y: matrix[1][1]),
      ),
    );
  }

  Widget _determinantReadout({
    required List<List<int>> matrix,
    required bool pinConnected,
  }) {
    if (pinConnected) {
      return const Text(
        'det = ?  (superlattice pin connected — effective matrix depends on '
        'runtime input)',
        style: TextStyle(fontSize: 13),
      );
    }
    final det = matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0];
    if (det == 0) {
      return const Text(
        'det = 0  (singular — the two tiling vectors are linearly dependent)',
        style: TextStyle(fontSize: 13, color: Colors.red),
      );
    }
    return Text(
      'det = $det  (surface cell area = ${det.abs()} × the (1×1) cell)',
      style: const TextStyle(fontSize: 13),
    );
  }

  bool _isSuperlatticeConnected() {
    final view = model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == nodeId &&
          wire.destParamIndex == BigInt.from(_SUPERLATTICE_PIN_INDEX)) {
        return true;
      }
    }
    return false;
  }
}
