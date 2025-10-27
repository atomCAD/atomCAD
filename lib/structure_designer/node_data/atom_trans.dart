import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/inputs/vec3_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for atom_trans nodes
class AtomTransEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIAtomTransData? data;
  final StructureDesignerModel model;

  const AtomTransEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<AtomTransEditor> createState() => AtomTransEditorState();
}

class AtomTransEditorState extends State<AtomTransEditor> {
  // Direct API calls are made in onChanged handlers
  
  /// Convert radians to degrees for display
  APIVec3 _radiansToDegrees(APIVec3 radians) {
    return APIVec3(
      x: radians.x * 180.0 / math.pi,
      y: radians.y * 180.0 / math.pi,
      z: radians.z * 180.0 / math.pi,
    );
  }
  
  /// Convert degrees to radians for API
  APIVec3 _degreesToRadians(APIVec3 degrees) {
    return APIVec3(
      x: degrees.x * math.pi / 180.0,
      y: degrees.y * math.pi / 180.0,
      z: degrees.z * math.pi / 180.0,
    );
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Atom Transformation Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          Vec3Input(
            label: 'Translation',
            value: widget.data!.translation,
            onChanged: (newValue) {
              widget.model.setAtomTransData(
                widget.nodeId,
                APIAtomTransData(
                  translation: newValue,
                  rotation: widget.data!.rotation,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          Vec3Input(
            label: 'Rotation (degrees)',
            value: _radiansToDegrees(widget.data!.rotation),
            onChanged: (newValue) {
              widget.model.setAtomTransData(
                widget.nodeId,
                APIAtomTransData(
                  translation: widget.data!.translation,
                  rotation: _degreesToRadians(newValue),
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
