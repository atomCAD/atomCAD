import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:flutter_cad/common/draggable_dialog.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';

/// A small info icon button that shows the node type description in a dialog.
/// Can be placed in any node editor header.
class NodeDescriptionButton extends StatelessWidget {
  final String nodeTypeName;
  final double iconSize;

  const NodeDescriptionButton({
    super.key,
    required this.nodeTypeName,
    this.iconSize = 18,
  });

  @override
  Widget build(BuildContext context) {
    return IconButton(
      icon: Icon(
        Icons.info_outline,
        size: iconSize,
        color: Colors.blue[300],
      ),
      tooltip: 'Show description',
      padding: EdgeInsets.zero,
      constraints: const BoxConstraints(),
      onPressed: () => showNodeDescriptionDialog(context, nodeTypeName),
    );
  }
}

/// Opens the node-type description dialog. Shared by [NodeDescriptionButton]
/// and by any editor that wants a second, more discoverable entry point (e.g.
/// the expr node's "Syntax reference" button). Descriptions are authored as
/// Markdown on the Rust side and rendered as such here.
void showNodeDescriptionDialog(BuildContext context, String nodeTypeName) {
  final description = getNetworkDescription(networkName: nodeTypeName);

  if (description == null) {
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(
        content: Text('No description available'),
        duration: Duration(seconds: 2),
      ),
    );
    return;
  }

  showDialog(
    context: context,
    barrierDismissible: true, // Click outside to dismiss
    builder: (context) => _DescriptionDialog(
      nodeTypeName: nodeTypeName,
      description: description,
    ),
  );
}

/// Dialog that displays the node type description, rendered as Markdown.
class _DescriptionDialog extends StatelessWidget {
  final String nodeTypeName;
  final String description;

  const _DescriptionDialog({
    required this.nodeTypeName,
    required this.description,
  });

  @override
  Widget build(BuildContext context) {
    // Size the content area relative to the window so long references (like the
    // expr language cheat sheet) get room to breathe, while short one-paragraph
    // descriptions still render in a compact box (the Column is min-sized).
    final media = MediaQuery.of(context);
    final maxContentHeight = media.size.height * 0.7;

    return DraggableDialog(
      backgroundColor: Colors.grey[900]!,
      width: 620,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Header with title and close button
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Expanded(
                  child: Text(
                    nodeTypeName,
                    style: const TextStyle(
                      color: Colors.white,
                      fontSize: 16,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.close, color: Colors.white70),
                  iconSize: 20,
                  padding: EdgeInsets.zero,
                  constraints: const BoxConstraints(),
                  onPressed: () => Navigator.of(context).pop(),
                  tooltip: 'Close',
                ),
              ],
            ),
            const SizedBox(height: 12),
            // Description body, rendered as Markdown.
            Flexible(
              child: ConstrainedBox(
                constraints: BoxConstraints(maxHeight: maxContentHeight),
                child: SingleChildScrollView(
                  child: description.isNotEmpty
                      ? MarkdownBody(
                          data: description,
                          selectable: true,
                          styleSheet: buildDescriptionMarkdownStyleSheet(context),
                        )
                      : const Text(
                          'No description available.',
                          style: TextStyle(
                            color: Colors.white70,
                            fontSize: 14,
                            height: 1.5,
                          ),
                        ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// A dark-themed Markdown style sheet tuned for node descriptions: readable body
/// text, clearly-set headings, and monospaced/boxed code so language cheat
/// sheets (operators, functions, examples) scan easily.
///
/// Shared by the description dialog and the Add Node popup's description panel
/// so both render node descriptions identically. [baseFontSize] scales the body
/// text (the popup panel is narrower and uses a slightly smaller base).
MarkdownStyleSheet buildDescriptionMarkdownStyleSheet(
  BuildContext context, {
  double baseFontSize = 14,
}) {
  const bodyColor = Colors.white70;
  const headingColor = Colors.white;
  const codeColor = Color(0xFFB5CEA8); // soft green, echoes code editors
  final codeBackground = Colors.black.withValues(alpha: 0.35);

  final body = TextStyle(color: bodyColor, fontSize: baseFontSize, height: 1.5);

  return MarkdownStyleSheet.fromTheme(Theme.of(context)).copyWith(
    p: body,
    a: const TextStyle(color: Color(0xFF81B5FF)),
    strong:
        const TextStyle(color: headingColor, fontWeight: FontWeight.bold),
    em: body.copyWith(fontStyle: FontStyle.italic),
    listBullet: body,
    h1: TextStyle(
        color: headingColor,
        fontSize: baseFontSize + 6,
        fontWeight: FontWeight.bold),
    h2: TextStyle(
        color: headingColor,
        fontSize: baseFontSize + 3,
        fontWeight: FontWeight.bold),
    h3: TextStyle(
        color: headingColor,
        fontSize: baseFontSize + 1,
        fontWeight: FontWeight.bold),
    code: TextStyle(
      fontFamily: 'monospace',
      color: codeColor,
      fontSize: baseFontSize - 1,
      backgroundColor: codeBackground,
    ),
    codeblockPadding: const EdgeInsets.all(10),
    codeblockDecoration: BoxDecoration(
      color: codeBackground,
      borderRadius: BorderRadius.circular(4),
    ),
    blockquoteDecoration: BoxDecoration(
      color: Colors.white.withValues(alpha: 0.05),
      borderRadius: BorderRadius.circular(4),
    ),
  );
}
