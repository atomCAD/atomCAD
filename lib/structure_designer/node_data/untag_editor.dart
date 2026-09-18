import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/structure_designer/node_data/tag_name_suggestions.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Editor widget for the `untag` node — removes a named tag from atoms (all
/// atoms, or only those inside a wired `region`). An **empty** name removes
/// **all** tags from the affected atoms (the blanket-clear analog of `xray`
/// α = 1.0). The input structure's existing tag names are offered as one-click
/// suggestions. When the `name` input pin is wired the wired value wins at eval.
class UntagEditor extends StatelessWidget {
  final BigInt nodeId;
  final APIUntagData? data;
  final StructureDesignerModel model;

  const UntagEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  void _setName(String value) {
    model.setUntagData(
      nodeId,
      APIUntagData(name: value.trim(), availableTags: const []),
    );
  }

  @override
  Widget build(BuildContext context) {
    final data = this.data;
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Untag',
            nodeTypeName: 'untag',
          ),
          const SizedBox(height: 16),
          if (data != null) ...[
            StringInput(
              key: ValueKey('untag_name_${data.name}'),
              label: 'Tag name (empty = all tags)',
              value: data.name,
              onChanged: _setName,
            ),
            TagNameSuggestions(
              available: data.availableTags,
              currentName: data.name,
              onPick: _setName,
            ),
            const SizedBox(height: 8),
            Text(
              data.name.trim().isEmpty
                  ? 'Empty name removes every tag from the affected atoms.'
                  : 'Removes this tag from atoms. Removing a tag an atom does '
                      'not carry is a no-op.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 8),
            Text(
              'Wire a `region` to affect only atoms inside it; otherwise all '
              'atoms are affected. Wiring the `name` input pin overrides this '
              'value.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }
}
