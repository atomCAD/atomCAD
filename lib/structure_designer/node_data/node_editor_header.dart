import 'package:flutter/material.dart';
import 'node_description_button.dart';

/// A reusable header widget for node editors that includes:
/// - Title text
/// - Info button to show description
///
/// Usage in editors:
/// ```dart
/// NodeEditorHeader(
///   title: 'Cuboid Properties',
///   nodeTypeName: 'cuboid',
/// )
/// ```
class NodeEditorHeader extends StatelessWidget {
  final String title;
  final String nodeTypeName;
  final TextStyle? textStyle;

  const NodeEditorHeader({
    super.key,
    required this.title,
    required this.nodeTypeName,
    this.textStyle,
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        Expanded(
          child: Text(
            title,
            style: textStyle ?? Theme.of(context).textTheme.titleMedium,
          ),
        ),
        NodeDescriptionButton(nodeTypeName: nodeTypeName),
      ],
    );
  }
}
