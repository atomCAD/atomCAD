import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_editor_header.dart';
import 'package:flutter_cad/inputs/string_input.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Editor widget for the `tag` node — adds a named, durable per-atom tag to
/// atoms (all atoms, or only those inside a wired `region`). The stored `name`
/// is edited here as free text; the input structure's existing tag names are
/// offered as one-click suggestions. When the `name` input pin is wired the
/// wired value wins at eval, so the panel shows the stored value only as an
/// inert fallback. Tags have no visual effect on their own — hover an atom to
/// see its tags.
class TagEditor extends StatelessWidget {
  final BigInt nodeId;
  final APITagData? data;
  final StructureDesignerModel model;

  const TagEditor({
    super.key,
    required this.nodeId,
    required this.data,
    required this.model,
  });

  void _setName(String value) {
    model.setTagData(
      nodeId,
      APITagData(name: value.trim(), availableTags: const []),
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
            title: 'Tag',
            nodeTypeName: 'tag',
          ),
          const SizedBox(height: 16),
          if (data != null) ...[
            StringInput(
              // Key on the stored value so the field rebuilds its controller
              // when a suggestion chip changes the name out from under it.
              key: ValueKey('tag_name_${data.name}'),
              label: 'Tag name',
              value: data.name,
              onChanged: _setName,
            ),
            _TagSuggestions(
              available: data.availableTags,
              currentName: data.name,
              onPick: _setName,
            ),
            const SizedBox(height: 8),
            Text(
              'Adds this tag to atoms. Wire a `region` to tag only atoms inside '
              'it; otherwise all atoms are tagged. Wiring the `name` input pin '
              'overrides this value.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
            const SizedBox(height: 8),
            Text(
              'Tags are inert metadata — they have no visual effect on their '
              'own. Hover an atom to see its tags. A structure supports at most '
              '32 distinct tag names.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ],
      ),
    );
  }
}

/// Shows the input structure's existing tag names as one-click suggestion
/// chips (§Existing-names suggestions). Empty (renders nothing) until the node
/// has evaluated with a wired input.
class _TagSuggestions extends StatelessWidget {
  final List<String> available;
  final String currentName;
  final ValueChanged<String> onPick;

  const _TagSuggestions({
    required this.available,
    required this.currentName,
    required this.onPick,
  });

  @override
  Widget build(BuildContext context) {
    final suggestions = available.where((name) => name != currentName).toList();
    if (suggestions.isEmpty) {
      return const SizedBox.shrink();
    }
    return Padding(
      padding: const EdgeInsets.only(top: 8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Existing tags in input',
            style: Theme.of(context).textTheme.bodySmall,
          ),
          const SizedBox(height: 4),
          Wrap(
            spacing: 6,
            runSpacing: 4,
            children: [
              for (final name in suggestions)
                ActionChip(
                  label: Text(name),
                  onPressed: () => onPick(name),
                ),
            ],
          ),
        ],
      ),
    );
  }
}
