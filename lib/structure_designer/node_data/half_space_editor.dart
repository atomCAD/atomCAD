import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/inputs/miller_index_map.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for half_space nodes
class HalfSpaceEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIHalfSpaceData? data;
  final StructureDesignerModel model;

  const HalfSpaceEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<HalfSpaceEditor> createState() => HalfSpaceEditorState();
}

class HalfSpaceEditorState extends State<HalfSpaceEditor> {
  // Direct API calls are made in onChanged handlers

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(4.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Half Space Properties',
            nodeTypeName: 'half_space',
          ),
          const SizedBox(height: 8),
          // Max Miller Index input
          IntInput(
            label: 'Max Miller Index',
            value: widget.data!.maxMillerIndex,
            minimumValue: 1, // Must be at least 1
            maximumValue: 10, // Set a reasonable upper limit
            onChanged: (newValue) {
              widget.model.setHalfSpaceData(
                widget.nodeId,
                APIHalfSpaceData(
                  maxMillerIndex: newValue,
                  millerIndex: widget.data!.millerIndex,
                  center: widget.data!.center,
                  shift: widget.data!.shift,
                  subdivision: widget.data!.subdivision,
                ),
              );
            },
          ),
          const SizedBox(height: 12),
          // Miller Index Map (visualization)
          MillerIndexMap(
            label: 'Miller Index Map',
            value: widget.data!.millerIndex,
            onChanged: (newValue) {
              widget.model.setHalfSpaceData(
                widget.nodeId,
                APIHalfSpaceData(
                  maxMillerIndex: widget.data!.maxMillerIndex,
                  millerIndex: newValue,
                  center: widget.data!.center,
                  shift: widget.data!.shift,
                  subdivision: widget.data!.subdivision,
                ),
              );
            },
            maxValue: widget.data!.maxMillerIndex,
            mapWidth: 360,
            mapHeight: 180,
            dotColor: Theme.of(context).brightness == Brightness.dark
                ? Colors.grey.shade600
                : Colors.grey.shade400,
            selectedDotColor: Colors.red,
          ),
          const SizedBox(height: 12),
          // Traditional numeric input for Miller Index
          IVec3Input(
            label: 'Miller Index (numeric)',
            value: widget.data!.millerIndex,
            minimumValue: APIIVec3(
                x: -widget.data!.maxMillerIndex,
                y: -widget.data!.maxMillerIndex,
                z: -widget.data!.maxMillerIndex),
            maximumValue: APIIVec3(
                x: widget.data!.maxMillerIndex,
                y: widget.data!.maxMillerIndex,
                z: widget.data!.maxMillerIndex),
            onChanged: (newValue) {
              widget.model.setHalfSpaceData(
                widget.nodeId,
                APIHalfSpaceData(
                  maxMillerIndex: widget.data!.maxMillerIndex,
                  millerIndex: newValue,
                  center: widget.data!.center,
                  shift: widget.data!.shift,
                  subdivision: widget.data!.subdivision,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IVec3Input(
            label: 'Center',
            value: widget.data!.center,
            onChanged: (newValue) {
              widget.model.setHalfSpaceData(
                widget.nodeId,
                APIHalfSpaceData(
                  maxMillerIndex: widget.data!.maxMillerIndex,
                  millerIndex: widget.data!.millerIndex,
                  center: newValue,
                  shift: widget.data!.shift,
                  subdivision: widget.data!.subdivision,
                ),
              );
            },
          ),
          const SizedBox(height: 8),
          IntInput(
            label: 'Shift',
            value: widget.data!.shift,
            onChanged: (newValue) {
              widget.model.setHalfSpaceData(
                widget.nodeId,
                APIHalfSpaceData(
                  maxMillerIndex: widget.data!.maxMillerIndex,
                  millerIndex: widget.data!.millerIndex,
                  center: widget.data!.center,
                  shift: newValue,
                  subdivision: widget.data!.subdivision,
                ),
              );
            },
          ),
          const SizedBox(height: 8),

          // Subdivision input
          IntInput(
            label: 'Subdivision',
            value: widget.data!.subdivision,
            minimumValue: 1,
            onChanged: (newValue) {
              widget.model.setHalfSpaceData(
                widget.nodeId,
                APIHalfSpaceData(
                  maxMillerIndex: widget.data!.maxMillerIndex,
                  millerIndex: widget.data!.millerIndex,
                  center: widget.data!.center,
                  shift: widget.data!.shift,
                  subdivision: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
