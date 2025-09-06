import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/ivec2_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// Editor widget for ivec2 nodes
class IVec2Editor extends StatefulWidget {
  final BigInt nodeId;
  final APIIVec2Data? data;
  final StructureDesignerModel model;

  const IVec2Editor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<IVec2Editor> createState() => IVec2EditorState();
}

class IVec2EditorState extends State<IVec2Editor> {
  // Direct API calls are made in onChanged handlers

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
          Text('IVec2 Properties',
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          IVec2Input(
            label: 'Value',
            value: widget.data!.value,
            onChanged: (newValue) {
              widget.model.setIvec2Data(
                widget.nodeId,
                APIIVec2Data(
                  value: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
