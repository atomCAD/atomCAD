import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/inputs/float_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for atom_cut nodes
class AtomCutEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIAtomCutData? data;
  final StructureDesignerModel model;

  const AtomCutEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<AtomCutEditor> createState() => AtomCutEditorState();
}

class AtomCutEditorState extends State<AtomCutEditor> {
  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const NodeEditorHeader(
            title: 'Atom Cut Properties',
            nodeTypeName: 'atom_cut',
          ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Cut SDF Value',
              value: widget.data!.cutSdfValue,
              onChanged: (newValue) {
                widget.model.setAtomCutData(
                  widget.nodeId,
                  APIAtomCutData(
                    cutSdfValue: newValue,
                    unitCellSize: widget.data!.unitCellSize,
                  ),
                );
              },
            ),
            const SizedBox(height: 8),
            FloatInput(
              label: 'Unit Cell Size',
              value: widget.data!.unitCellSize,
              onChanged: (newValue) {
                widget.model.setAtomCutData(
                  widget.nodeId,
                  APIAtomCutData(
                    cutSdfValue: widget.data!.cutSdfValue,
                    unitCellSize: newValue,
                  ),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}
