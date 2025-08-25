import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
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
            label: 'Rotation',
            value: widget.data!.rotation,
            onChanged: (newValue) {
              widget.model.setAtomTransData(
                widget.nodeId,
                APIAtomTransData(
                  translation: widget.data!.translation,
                  rotation: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
