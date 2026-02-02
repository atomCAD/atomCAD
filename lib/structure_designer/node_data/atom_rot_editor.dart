import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for atom_rot nodes.
/// Allows editing the rotation angle, axis, and pivot point in world space.
class AtomRotEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIAtomRotData? data;
  final StructureDesignerModel model;

  const AtomRotEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<AtomRotEditor> createState() => AtomRotEditorState();
}

class AtomRotEditorState extends State<AtomRotEditor> {
  // Preset axis options
  static const Map<String, APIVec3?> presetAxes = {
    'X-axis': APIVec3(x: 1, y: 0, z: 0),
    'Y-axis': APIVec3(x: 0, y: 1, z: 0),
    'Z-axis': APIVec3(x: 0, y: 0, z: 1),
    '-X-axis': APIVec3(x: -1, y: 0, z: 0),
    '-Y-axis': APIVec3(x: 0, y: -1, z: 0),
    '-Z-axis': APIVec3(x: 0, y: 0, z: -1),
    'Custom': null,
  };

  String? _getPresetForAxis(APIVec3 axis) {
    for (final entry in presetAxes.entries) {
      if (entry.value != null &&
          (entry.value!.x - axis.x).abs() < 0.001 &&
          (entry.value!.y - axis.y).abs() < 0.001 &&
          (entry.value!.z - axis.z).abs() < 0.001) {
        return entry.key;
      }
    }
    return 'Custom';
  }

  double _radiansToDegrees(double radians) => radians * 180.0 / math.pi;
  double _degreesToRadians(double degrees) => degrees * math.pi / 180.0;

  void _updateRotationAxis(APIVec3 newAxis) {
    widget.model.setAtomRotData(
      widget.nodeId,
      APIAtomRotData(
        angle: widget.data!.angle,
        rotAxis: newAxis,
        pivotPoint: widget.data!.pivotPoint,
      ),
    );
  }

  void _updateAngle(double newAngleRadians) {
    widget.model.setAtomRotData(
      widget.nodeId,
      APIAtomRotData(
        angle: newAngleRadians,
        rotAxis: widget.data!.rotAxis,
        pivotPoint: widget.data!.pivotPoint,
      ),
    );
  }

  void _updatePivotPoint(APIVec3 newPivotPoint) {
    widget.model.setAtomRotData(
      widget.nodeId,
      APIAtomRotData(
        angle: widget.data!.angle,
        rotAxis: widget.data!.rotAxis,
        pivotPoint: newPivotPoint,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final currentPreset = _getPresetForAxis(widget.data!.rotAxis);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Atom Rotation Properties',
            nodeTypeName: 'atom_rot',
          ),
          const SizedBox(height: 16),

          // Preset axis dropdown
          Text('Rotation Axis', style: Theme.of(context).textTheme.titleSmall),
          const SizedBox(height: 8),
          DropdownButtonFormField<String>(
            value: currentPreset,
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
              contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            ),
            items: presetAxes.keys.map((name) =>
              DropdownMenuItem(value: name, child: Text(name))
            ).toList(),
            onChanged: (String? newValue) {
              if (newValue != null && presetAxes[newValue] != null) {
                _updateRotationAxis(presetAxes[newValue]!);
              }
            },
          ),
          const SizedBox(height: 16),

          // Custom axis input (always visible for fine-tuning)
          Vec3Input(
            label: 'Custom Axis',
            value: widget.data!.rotAxis,
            onChanged: _updateRotationAxis,
          ),
          const SizedBox(height: 16),

          // Angle input (in degrees)
          FloatInput(
            label: 'Angle (degrees)',
            value: _radiansToDegrees(widget.data!.angle),
            onChanged: (newValue) {
              _updateAngle(_degreesToRadians(newValue));
            },
          ),
          const SizedBox(height: 8),

          // Current rotation display
          Card(
            color: Theme.of(context).colorScheme.surfaceContainerHighest.withValues(alpha: 0.5),
            child: Padding(
              padding: const EdgeInsets.all(12.0),
              child: Row(
                children: [
                  Icon(
                    Icons.rotate_right,
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                    size: 20,
                  ),
                  const SizedBox(width: 8),
                  Text(
                    'Current rotation: ${_radiansToDegrees(widget.data!.angle).toStringAsFixed(1)}°',
                    style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                      color: Theme.of(context).colorScheme.onSurfaceVariant,
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),

          // Pivot point input
          Vec3Input(
            label: 'Pivot Point (Å)',
            value: widget.data!.pivotPoint,
            onChanged: _updatePivotPoint,
          ),
        ],
      ),
    );
  }
}
