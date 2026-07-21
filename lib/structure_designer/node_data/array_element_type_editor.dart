import 'package:flutter/material.dart';
import 'package:flutter_cad/inputs/data_type_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';

/// Shared editor for the array nodes whose *only* stored property is the
/// element type — `array_append`, `array_concat`, `array_len`.
///
/// The three nodes differ solely in which pins the element type expands onto
/// (see each node's `calculate_custom_node_type` in Rust), so the panel is one
/// `DataTypeInput` plus a per-node explanatory line. `array_at` keeps its own
/// editor because it has a second property (the stored index).
class ArrayElementTypeEditor extends StatelessWidget {
  /// Node type name, used for the header and the description button.
  final String nodeTypeName;

  /// Panel title, e.g. `'Array Append Properties'`.
  final String title;

  /// One-line explanation of what the element type drives on this node.
  final String elementTypeHint;

  /// Current element type, or `null` while the node data is still loading.
  final APIDataType? elementType;

  final ValueChanged<APIDataType> onElementTypeChanged;

  const ArrayElementTypeEditor({
    super.key,
    required this.nodeTypeName,
    required this.title,
    required this.elementTypeHint,
    required this.elementType,
    required this.onElementTypeChanged,
  });

  @override
  Widget build(BuildContext context) {
    if (elementType == null) {
      return const Center(child: CircularProgressIndicator());
    }

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          NodeEditorHeader(title: title, nodeTypeName: nodeTypeName),
          const SizedBox(height: 8),
          DataTypeInput(
            label: 'Element Type',
            value: elementType!,
            onChanged: onElementTypeChanged,
          ),
          const SizedBox(height: 8),
          Text(
            elementTypeHint,
            style: const TextStyle(fontSize: 12, fontStyle: FontStyle.italic),
          ),
        ],
      ),
    );
  }
}
