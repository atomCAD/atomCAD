import 'package:flutter/material.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/matrix_cell.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor for the `mat3_diag` node. Renders the stored diagonal vector `v`
/// as three cells; the input pin overrides the whole vector when wired.
class Mat3DiagEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIMat3DiagData? data;
  final StructureDesignerModel model;

  const Mat3DiagEditor({
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
    final connected = _isVConnected();
    final v = [data!.v.x, data!.v.y, data!.v.z];

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Mat3 Diagonal',
            nodeTypeName: 'mat3_diag',
          ),
          const SizedBox(height: 8),
          Text(
            connected
                ? 'v pin is connected — the stored diagonal is overridden by '
                    'the wired Vec3 at evaluation.'
                : 'Output is diag(v.x, v.y, v.z).',
            style: const TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              SizedBox(
                width: 28,
                child: Text('v =', style: AppTextStyles.regular),
              ),
              const SizedBox(width: 4),
              for (var i = 0; i < 3; i++) ...[
                if (i > 0) const SizedBox(width: 4),
                FloatMatrixCell(
                  value: v[i],
                  enabled: !connected,
                  onChanged: (newValue) => _updateCell(i, newValue),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }

  void _updateCell(int idx, double value) {
    final v = [data!.v.x, data!.v.y, data!.v.z];
    v[idx] = value;
    model.setMat3DiagData(
      nodeId,
      APIMat3DiagData(v: APIVec3(x: v[0], y: v[1], z: v[2])),
    );
  }

  bool _isVConnected() {
    final view = model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == nodeId && wire.destParamIndex == BigInt.zero) {
        return true;
      }
    }
    return false;
  }
}
