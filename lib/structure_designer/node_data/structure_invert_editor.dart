import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Indexes of the input pins on `structure_invert`.
/// 0 = input, 1 = pivot_point, 2 = subdivision.
const int _PIVOT_POINT_PIN_INDEX = 1;
const int _SUBDIVISION_PIN_INDEX = 2;

/// Editor widget for structure_invert nodes.
///
/// The node inverts its input through the point `pivot_point / subdivision`
/// (lattice coordinates) — e.g. pivot (1,1,1) with subdivision 8 is diamond's
/// bond-center inversion center. Both fields follow the standard "disable on
/// wired input" pattern.
class StructureInvertEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIStructureInvertData? data;
  final StructureDesignerModel model;

  const StructureInvertEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  /// True when the input pin at [pinIndex] is wired. Detected by walking the
  /// current network view's wires (see node_data/AGENTS.md "Disable on wired
  /// input" pattern).
  bool _isPinConnected(int pinIndex) {
    final view = model.nodeNetworkView;
    if (view == null) return false;
    for (final wire in view.wires) {
      if (wire.destNodeId == nodeId &&
          wire.destParamIndex == BigInt.from(pinIndex)) {
        return true;
      }
    }
    return false;
  }

  Widget _disableWhenConnected({
    required BuildContext context,
    required bool connected,
    required String pinName,
    required Widget child,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (connected)
          Padding(
            padding: const EdgeInsets.only(bottom: 8.0),
            child: Text(
              'Value supplied by `$pinName` input. Disconnect to edit inline.',
              style: TextStyle(
                fontStyle: FontStyle.italic,
                fontSize: 12,
                color: Theme.of(context).colorScheme.primary,
              ),
            ),
          ),
        Opacity(
          opacity: connected ? 0.5 : 1.0,
          child: IgnorePointer(
            ignoring: connected,
            child: child,
          ),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    if (data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final pivotConnected = _isPinConnected(_PIVOT_POINT_PIN_INDEX);
    final subdivisionConnected = _isPinConnected(_SUBDIVISION_PIN_INDEX);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Structure Invert Properties',
            nodeTypeName: 'structure_invert',
          ),
          const SizedBox(height: 16),

          // Pivot point input
          _disableWhenConnected(
            context: context,
            connected: pivotConnected,
            pinName: 'pivot_point',
            child: IVec3Input(
              label: 'Pivot Point',
              value: data!.pivotPoint,
              onChanged: (newValue) {
                model.setStructureInvertData(
                  nodeId,
                  APIStructureInvertData(
                    pivotPoint: newValue,
                    subdivision: data!.subdivision,
                  ),
                );
              },
            ),
          ),
          const SizedBox(height: 16),

          // Subdivision input
          _disableWhenConnected(
            context: context,
            connected: subdivisionConnected,
            pinName: 'subdivision',
            child: IntInput(
              label: 'Subdivision',
              value: data!.subdivision,
              minimumValue: 1,
              onChanged: (newValue) {
                model.setStructureInvertData(
                  nodeId,
                  APIStructureInvertData(
                    pivotPoint: data!.pivotPoint,
                    subdivision: newValue,
                  ),
                );
              },
            ),
          ),
          const SizedBox(height: 8),
          Text(
            'The inversion pivot is pivot_point / subdivision in lattice '
            'coordinates. Example: (1,1,1) with subdivision 8 is the diamond '
            'bond-center inversion center.',
            style: TextStyle(
              fontSize: 12,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }
}
