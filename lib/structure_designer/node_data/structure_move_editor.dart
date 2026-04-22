import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for structure_move nodes
class StructureMoveEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIStructureMoveData? data;
  final StructureDesignerModel model;
  final String title;
  final String nodeTypeName;

  const StructureMoveEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
    this.title = 'Structure Move Properties',
    this.nodeTypeName = 'structure_move',
  });

  @override
  State<StructureMoveEditor> createState() => _StructureMoveEditorState();
}

class _StructureMoveEditorState extends State<StructureMoveEditor> {
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
          NodeEditorHeader(
            title: widget.title,
            nodeTypeName: widget.nodeTypeName,
          ),
          const SizedBox(height: 16),

          // Translation input
          IVec3Input(
            label: 'Translation',
            value: widget.data!.translation,
            onChanged: (newValue) {
              widget.model.setStructureMoveData(
                widget.nodeId,
                APIStructureMoveData(
                  translation: newValue,
                  latticeSubdivision: widget.data!.latticeSubdivision,
                ),
              );
            },
          ),
          const SizedBox(height: 16),

          // Subdivision input
          IntInput(
            label: 'Subdivision',
            value: widget.data!.latticeSubdivision,
            minimumValue: 1,
            onChanged: (newValue) {
              widget.model.setStructureMoveData(
                widget.nodeId,
                APIStructureMoveData(
                  translation: widget.data!.translation,
                  latticeSubdivision: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }
}
