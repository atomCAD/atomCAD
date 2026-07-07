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

/// Dialog that displays the node type description, rendered as Markdown, with a
/// simple find bar that scrolls to matches.
class _DescriptionDialog extends StatefulWidget {
  final String nodeTypeName;
  final String description;

  const _DescriptionDialog({
    required this.nodeTypeName,
    required this.description,
  });

  @override
  State<_DescriptionDialog> createState() => _DescriptionDialogState();
}

class _DescriptionDialogState extends State<_DescriptionDialog> {
  final _searchController = TextEditingController();
  final _scrollController = ScrollController();

  /// The description split into renderable blocks (paragraphs / code fences /
  /// headings), each with its own [GlobalKey] so the find bar can scroll the
  /// block containing a match into view.
  late final List<String> _blocks;
  late final List<GlobalKey> _blockKeys;

  String _query = '';

  /// For each match occurrence (in document order), the index of the block it
  /// lives in. Length is the total match count; index into it with
  /// [_currentMatch].
  List<int> _matchBlocks = const [];
  int _currentMatch = 0;

  @override
  void initState() {
    super.initState();
    _blocks = _splitIntoBlocks(widget.description);
    _blockKeys = List.generate(_blocks.length, (_) => GlobalKey());
  }

  @override
  void dispose() {
    _searchController.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  /// Splits Markdown into blocks on blank lines, but never inside a fenced code
  /// block (so a ``` example stays intact). Matches the paragraph boundaries a
  /// reader perceives, which is the granularity the find bar scrolls to.
  static List<String> _splitIntoBlocks(String src) {
    final blocks = <String>[];
    final current = <String>[];
    var inFence = false;
    for (final line in src.split('\n')) {
      if (line.trimLeft().startsWith('```')) {
        inFence = !inFence;
      }
      if (line.trim().isEmpty && !inFence) {
        if (current.isNotEmpty) {
          blocks.add(current.join('\n'));
          current.clear();
        }
      } else {
        current.add(line);
      }
    }
    if (current.isNotEmpty) {
      blocks.add(current.join('\n'));
    }
    return blocks;
  }

  void _onQueryChanged(String query) {
    setState(() {
      _query = query;
      final matches = <int>[];
      if (query.isNotEmpty) {
        final needle = query.toLowerCase();
        for (var i = 0; i < _blocks.length; i++) {
          final hay = _blocks[i].toLowerCase();
          var from = 0;
          while (true) {
            final idx = hay.indexOf(needle, from);
            if (idx < 0) break;
            matches.add(i);
            from = idx + needle.length;
          }
        }
      }
      _matchBlocks = matches;
      _currentMatch = 0;
    });
    _scrollToCurrentMatch();
  }

  void _step(int delta) {
    if (_matchBlocks.isEmpty) return;
    setState(() {
      _currentMatch =
          (_currentMatch + delta) % _matchBlocks.length;
      if (_currentMatch < 0) _currentMatch += _matchBlocks.length;
    });
    _scrollToCurrentMatch();
  }

  void _scrollToCurrentMatch() {
    if (_matchBlocks.isEmpty) return;
    final key = _blockKeys[_matchBlocks[_currentMatch]];
    // Defer to after the frame so the keyed block has a live context.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final ctx = key.currentContext;
      if (ctx != null) {
        Scrollable.ensureVisible(
          ctx,
          duration: const Duration(milliseconds: 200),
          curve: Curves.easeInOut,
          alignment: 0.1, // near the top of the viewport
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    // Size the content area relative to the window so long references (like the
    // expr language cheat sheet) get room to breathe, while short one-paragraph
    // descriptions still render in a compact box (the Column is min-sized).
    final media = MediaQuery.of(context);
    final maxContentHeight = media.size.height * 0.7;
    final styleSheet = buildDescriptionMarkdownStyleSheet(context);
    final hasDescription = widget.description.isNotEmpty;

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
                    widget.nodeTypeName,
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
            if (hasDescription) ...[
              const SizedBox(height: 8),
              _buildFindBar(),
            ],
            const SizedBox(height: 12),
            // Description body, rendered as Markdown. Wrapped in a
            // SelectionArea so text can be selected contiguously across
            // paragraphs and code blocks (MarkdownBody's own per-widget
            // `selectable` only allows selecting within a single block, and
            // conflicts with SelectionArea, so it is left off here).
            Flexible(
              child: ConstrainedBox(
                constraints: BoxConstraints(maxHeight: maxContentHeight),
                child: SelectionArea(
                  child: SingleChildScrollView(
                    controller: _scrollController,
                    child: hasDescription
                        ? Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              for (var i = 0; i < _blocks.length; i++)
                                Padding(
                                  padding: const EdgeInsets.only(bottom: 8),
                                  child: KeyedSubtree(
                                    key: _blockKeys[i],
                                    child: MarkdownBody(
                                      data: _blocks[i],
                                      styleSheet: styleSheet,
                                    ),
                                  ),
                                ),
                            ],
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
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildFindBar() {
    final hasQuery = _query.isNotEmpty;
    final hasMatches = _matchBlocks.isNotEmpty;
    final String countLabel;
    if (!hasQuery) {
      countLabel = '';
    } else if (!hasMatches) {
      countLabel = 'No matches';
    } else {
      countLabel = '${_currentMatch + 1} / ${_matchBlocks.length}';
    }

    return Row(
      children: [
        Expanded(
          child: SizedBox(
            height: 34,
            child: TextField(
              controller: _searchController,
              onChanged: _onQueryChanged,
              onSubmitted: (_) => _step(1),
              style: const TextStyle(color: Colors.white, fontSize: 13),
              decoration: InputDecoration(
                isDense: true,
                filled: true,
                fillColor: Colors.black.withValues(alpha: 0.3),
                hintText: 'Find in description…',
                hintStyle: const TextStyle(color: Colors.white38),
                prefixIcon: const Icon(Icons.search,
                    size: 16, color: Colors.white54),
                prefixIconConstraints:
                    const BoxConstraints(minWidth: 32, minHeight: 32),
                contentPadding:
                    const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(4),
                  borderSide: BorderSide(color: Colors.grey[700]!),
                ),
                enabledBorder: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(4),
                  borderSide: BorderSide(color: Colors.grey[700]!),
                ),
              ),
            ),
          ),
        ),
        const SizedBox(width: 8),
        SizedBox(
          width: 68,
          child: Text(
            countLabel,
            textAlign: TextAlign.right,
            style: TextStyle(
              color: (hasQuery && !hasMatches)
                  ? Colors.orange[300]
                  : Colors.white54,
              fontSize: 12,
            ),
          ),
        ),
        IconButton(
          icon: const Icon(Icons.keyboard_arrow_up),
          iconSize: 20,
          color: Colors.white70,
          tooltip: 'Previous match',
          onPressed: hasMatches ? () => _step(-1) : null,
        ),
        IconButton(
          icon: const Icon(Icons.keyboard_arrow_down),
          iconSize: 20,
          color: Colors.white70,
          tooltip: 'Next match',
          onPressed: hasMatches ? () => _step(1) : null,
        ),
      ],
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
