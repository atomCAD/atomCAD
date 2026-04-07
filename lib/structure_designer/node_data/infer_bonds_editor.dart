import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Editor widget for infer_bonds nodes
class InferBondsEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIInferBondsData? data;
  final StructureDesignerModel model;

  const InferBondsEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  @override
  State<InferBondsEditor> createState() => _InferBondsEditorState();
}

class _InferBondsEditorState extends State<InferBondsEditor> {
  APIInferBondsData _currentData() {
    return widget.data ??
        const APIInferBondsData(
          additive: false,
          bondTolerance: 1.15,
        );
  }

  void _updateData(APIInferBondsData data) {
    widget.model.setInferBondsData(widget.nodeId, data);
  }

  void _updateAdditive(bool value) {
    final data = _currentData();
    _updateData(APIInferBondsData(
      additive: value,
      bondTolerance: data.bondTolerance,
    ));
  }

  void _updateBondTolerance(String value) {
    final tolerance = double.tryParse(value);
    if (tolerance == null || tolerance <= 0) return;
    final data = _currentData();
    _updateData(APIInferBondsData(
      additive: data.additive,
      bondTolerance: tolerance,
    ));
  }

  @override
  Widget build(BuildContext context) {
    if (widget.data == null) {
      return const Center(child: CircularProgressIndicator());
    }

    final data = _currentData();

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Infer Bonds',
            nodeTypeName: 'infer_bonds',
          ),
          const SizedBox(height: 16),

          // Additive checkbox
          CheckboxListTile(
            title: const Text('Additive'),
            subtitle: const Text('Keep existing bonds and add new ones'),
            value: data.additive,
            onChanged: (value) {
              if (value != null) _updateAdditive(value);
            },
            dense: true,
            contentPadding: EdgeInsets.zero,
          ),
          const SizedBox(height: 8),

          // Bond tolerance
          SizedBox(
            width: double.infinity,
            child: StringInput(
              label: 'Bond Tolerance',
              value: data.bondTolerance.toString(),
              onChanged: _updateBondTolerance,
            ),
          ),
        ],
      ),
    );
  }
}
