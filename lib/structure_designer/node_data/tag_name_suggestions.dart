import 'package:flutter/material.dart';

/// Shows the input structure's existing tag names as one-click suggestion
/// chips (§Existing-names suggestions). Renders nothing until the node has
/// evaluated with a wired input, and hides the name already selected.
///
/// Shared by `tag_editor.dart`, `untag_editor.dart` and `proxy_editor.dart` —
/// every panel whose stored property is a tag *name* offers the same
/// affordance, and three copies of a Wrap of `ActionChip`s drift.
class TagNameSuggestions extends StatelessWidget {
  final List<String> available;
  final String currentName;
  final ValueChanged<String> onPick;

  /// Caption above the chips. The default suits a panel that *reads* tags;
  /// `untag` says something else because it also offers a blanket clear.
  final String caption;

  const TagNameSuggestions({
    super.key,
    required this.available,
    required this.currentName,
    required this.onPick,
    this.caption = 'Existing tags in input',
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
          Text(caption, style: Theme.of(context).textTheme.bodySmall),
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
