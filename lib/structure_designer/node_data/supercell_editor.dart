import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:provider/provider.dart';

const int _MATRIX_PIN_INDEX = 1;
const double _CELL_WIDTH = 44.0;
const double _CELL_HEIGHT = 28.0;

/// Editor widget for the `supercell` node.
///
/// Renders the stored 3x3 integer matrix as three inline equations:
///   new_a = [n]·a + [n]·b + [n]·c
/// plus a live determinant readout below. When the `matrix` input pin is
/// connected the stored matrix is overridden at eval time by the wired
/// IMat3, so the grid is grayed out and the determinant is unknown at
/// edit time.
class SupercellEditor extends StatelessWidget {
  final BigInt nodeId;
  final APISupercellData? data;
  final StructureDesignerModel model;

  const SupercellEditor({
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

    final networkView = model.nodeNetworkView;
    final matrixConnected = _isMatrixConnected(networkView);
    final effective = _matrixFromData(data!);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Supercell Properties',
            nodeTypeName: 'supercell',
          ),
          const SizedBox(height: 8),
          _hintText(matrixConnected),
          const SizedBox(height: 8),
          _matrixRows(
            context: context,
            matrix: effective,
            enabled: !matrixConnected,
          ),
          const SizedBox(height: 12),
          _determinantReadout(
            matrix: effective,
            pinConnected: matrixConnected,
          ),
        ],
      ),
    );
  }

  Widget _hintText(bool matrixConnected) {
    if (matrixConnected) {
      return const Text(
        'Matrix pin is connected — the stored matrix is overridden by the '
        'wired IMat3 at evaluation.',
        style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
      );
    }
    return const Text(
      'Each row defines a new basis vector as an integer combination of the '
      'old basis a, b, c.',
      style: TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
    );
  }

  Widget _matrixRows({
    required BuildContext context,
    required List<List<int>> matrix,
    required bool enabled,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _row(rowIndex: 0, label: 'new_a', values: matrix[0], enabled: enabled),
        const SizedBox(height: 6),
        _row(rowIndex: 1, label: 'new_b', values: matrix[1], enabled: enabled),
        const SizedBox(height: 6),
        _row(rowIndex: 2, label: 'new_c', values: matrix[2], enabled: enabled),
      ],
    );
  }

  Widget _row({
    required int rowIndex,
    required String label,
    required List<int> values,
    required bool enabled,
  }) {
    final basisLabels = ['a', 'b', 'c'];
    final children = <Widget>[
      SizedBox(
        width: 52,
        child: Text(
          '$label  =',
          style: AppTextStyles.regular,
        ),
      ),
    ];
    for (var col = 0; col < 3; col++) {
      if (col > 0) {
        children.add(Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4),
          child: Text('+', style: AppTextStyles.regular),
        ));
      }
      children.add(_IntCell(
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
    final current = _matrixFromData(data!);
    current[row][col] = value;
    final a = current[0];
    final b = current[1];
    final c = current[2];
    model.setSupercellData(
      nodeId,
      APISupercellData(
        a: APIIVec3(x: a[0], y: a[1], z: a[2]),
        b: APIIVec3(x: b[0], y: b[1], z: b[2]),
        c: APIIVec3(x: c[0], y: c[1], z: c[2]),
      ),
    );
  }

  Widget _determinantReadout({
    required List<List<int>> matrix,
    required bool pinConnected,
  }) {
    if (pinConnected) {
      return const Text(
        'det = ?  (matrix pin connected — effective matrix depends on '
        'runtime input)',
        style: TextStyle(fontSize: 13),
      );
    }
    final det = _det3(matrix);
    if (det == 0) {
      return const Text(
        'det = 0  (singular — rows are linearly dependent)',
        style: TextStyle(fontSize: 13, color: Colors.red),
      );
    }
    if (det < 0) {
      return Text(
        'det = $det  (left-handed basis — not supported)',
        style: const TextStyle(fontSize: 13, color: Colors.red),
      );
    }
    return Text(
      'det = $det  (new volume = $det × old)',
      style: const TextStyle(fontSize: 13),
    );
  }

  bool _isMatrixConnected(NodeNetworkView? networkView) {
    if (networkView == null) return false;
    for (final wire in networkView.wires) {
      if (wire.destNodeId == nodeId &&
          wire.destParamIndex == BigInt.from(_MATRIX_PIN_INDEX)) {
        return true;
      }
    }
    return false;
  }

  static List<List<int>> _matrixFromData(APISupercellData d) {
    return [
      [d.a.x, d.a.y, d.a.z],
      [d.b.x, d.b.y, d.b.z],
      [d.c.x, d.c.y, d.c.z],
    ];
  }

  static int _det3(List<List<int>> m) {
    // 64-bit arithmetic via Dart's arbitrary-precision int avoids overflow
    // for typical supercell matrices; a `1000x1000x1000` diagonal yields 1e9,
    // well within range.
    return m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) -
        m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) +
        m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
  }
}

/// Compact inline integer input used inside a matrix row. Narrower than the
/// shared `IntInput` and without a column label.
class _IntCell extends StatefulWidget {
  final int value;
  final bool enabled;
  final ValueChanged<int> onChanged;

  const _IntCell({
    required this.value,
    required this.enabled,
    required this.onChanged,
  });

  @override
  State<_IntCell> createState() => _IntCellState();
}

class _IntCellState extends State<_IntCell> {
  late TextEditingController _controller;
  late FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value.toString());
    _focusNode = FocusNode();
    _focusNode.addListener(() {
      if (!_focusNode.hasFocus) {
        _commit(_controller.text);
      }
    });
  }

  @override
  void didUpdateWidget(_IntCell oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.value != widget.value) {
      final selection = _controller.selection;
      _controller.text = widget.value.toString();
      final newLen = _controller.text.length;
      if (selection.isValid && selection.end <= newLen) {
        _controller.selection = selection;
      } else {
        _controller.selection = TextSelection.collapsed(offset: newLen);
      }
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _commit(String text) {
    final parsed = int.tryParse(text);
    if (parsed == null) {
      _controller.text = widget.value.toString();
      _controller.selection =
          TextSelection.fromPosition(TextPosition(offset: _controller.text.length));
      return;
    }
    if (parsed != widget.value) {
      widget.onChanged(parsed);
    }
  }

  void _step(int delta) {
    if (!widget.enabled) return;
    final current = int.tryParse(_controller.text) ?? widget.value;
    final next = current + delta;
    _controller.text = next.toString();
    widget.onChanged(next);
  }

  @override
  Widget build(BuildContext context) {
    final field = SizedBox(
      width: _CELL_WIDTH,
      height: _CELL_HEIGHT,
      child: TextField(
        controller: _controller,
        focusNode: _focusNode,
        enabled: widget.enabled,
        textAlign: TextAlign.center,
        style: AppTextStyles.inputField,
        keyboardType: TextInputType.number,
        decoration: AppInputDecorations.standard,
        onSubmitted: _commit,
      ),
    );

    if (!widget.enabled) {
      return Opacity(opacity: 0.5, child: field);
    }

    return MouseRegion(
      onEnter: (PointerEnterEvent event) {
        try {
          context.read<MouseWheelBlockService>().block();
        } catch (_) {}
      },
      onExit: (PointerExitEvent event) {
        try {
          context.read<MouseWheelBlockService>().unblock();
        } catch (_) {}
      },
      child: Listener(
        onPointerSignal: (event) {
          if (event is PointerScrollEvent) {
            final step = HardwareKeyboard.instance.isShiftPressed ? 10 : 1;
            if (event.scrollDelta.dy > 0) {
              _step(-step);
            } else if (event.scrollDelta.dy < 0) {
              _step(step);
            }
          }
        },
        child: field,
      ),
    );
  }
}
