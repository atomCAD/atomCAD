import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/inputs/ivec3_input.dart';
import 'package:flutter_cad/inputs/int_input.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for lattice_move nodes
class LatticeMoveEditor extends StatefulWidget {
  final BigInt nodeId;
  final APILatticeMoveData? data;
  final StructureDesignerModel model;

  const LatticeMoveEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<LatticeMoveEditor> createState() => _LatticeMoveEditorState();
}

class _LatticeMoveEditorState extends State<LatticeMoveEditor> {
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
          const NodeEditorHeader(
            title: 'Lattice Move Properties',
            nodeTypeName: 'lattice_move',
          ),
          const SizedBox(height: 16),

          // Translation input
          IVec3Input(
            label: 'Translation',
            value: widget.data!.translation,
            onChanged: (newValue) {
              widget.model.setLatticeMoveData(
                widget.nodeId,
                APILatticeMoveData(
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
              widget.model.setLatticeMoveData(
                widget.nodeId,
                APILatticeMoveData(
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
