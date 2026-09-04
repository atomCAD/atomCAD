/// A node **name path** rendered as a link that jumps to the node
/// (`doc/design_node_names_in_ui.md` D12).
///
/// The AI History panel is the only user today: the *Layout* tab's moved-node
/// rows and the *Diff* tab's hunk titles hold exactly the paths the AI itself
/// was handed, and the D1 spelling — unique names per scope, `/` banned from a
/// name — is what makes them resolvable against the document as it stands now.
///
/// **Resolution is live, not recorded.** The widget never pre-checks whether a
/// path still resolves: it hands the string to [onJump] and lets the caller
/// report a miss. A node renamed or deleted since the edit is a legitimate
/// miss, and it is the *expected* outcome for a removed hunk, whose title names
/// a node that very edit deleted. Pre-checking would also mean resolving every
/// visible path on every rebuild.
///
/// **Not every title slot holds a path.** The *Text* diff's hunk title is a
/// `@@ … @@` range header and the by-node diff's root bucket (header,
/// `description`, `summary`, `output`) has no path at all. [isLinkable] is the
/// one rule; a non-path renders as plain text — no underline, no tooltip, no
/// tap — and an empty path draws [NodeNameLink.label] instead.
library;

import 'package:flutter/material.dart';

/// What a link reports when clicked: the network the path belongs to, and the
/// path itself. Resolution happens at the receiving end, against the document
/// as it stands now.
typedef NodeJumpCallback = void Function(String network, String path);

class NodeNameLink extends StatefulWidget {
  const NodeNameLink({
    super.key,
    required this.path,
    required this.network,
    required this.onJump,
    this.label,
    this.style,
  });

  /// The node name path (`e1`, `map4/e1`), or a non-path the caller wants
  /// rendered as plain text (see [isLinkable]).
  final String path;

  /// The network [path] is relative to — the history entry's `networkName`,
  /// which is not necessarily the active one.
  final String network;

  /// Called with (`network`, `path`) on tap. A `null` callback renders plain
  /// text, so a caller that cannot jump need not special-case anything.
  final NodeJumpCallback? onJump;

  /// Drawn in place of an empty [path] — the *Diff* tab's `(hunk)` placeholder.
  final String? label;

  final TextStyle? style;

  /// Whether [path] is a node name path at all.
  ///
  /// Two non-paths reach this widget, and both are titles the panel already
  /// showed before D12: the empty string (the by-node diff's root bucket) and
  /// the *Text* diff's `@@ -1,3 +1,4 @@` range header. Interior whitespace is
  /// **not** disqualifying — `is_valid_user_name` allows it inside a node name,
  /// so `my node/e1` is a real path.
  static bool isLinkable(String path) =>
      path.isNotEmpty && !path.startsWith('@@');

  @override
  State<NodeNameLink> createState() => _NodeNameLinkState();
}

class _NodeNameLinkState extends State<NodeNameLink> {
  bool _hovering = false;

  @override
  Widget build(BuildContext context) {
    final text = widget.path.isEmpty ? (widget.label ?? '') : widget.path;
    final onJump = widget.onJump;
    if (onJump == null || !NodeNameLink.isLinkable(widget.path)) {
      return Text(
        text,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: widget.style,
      );
    }
    return
        // Excluded from the enclosing `SelectionArea`: a click on the link must
        // jump rather than start a text selection, while the hunk's own lines
        // stay selectable so copying a hunk still works (D12).
        SelectionContainer.disabled(
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => setState(() => _hovering = true),
        onExit: (_) => setState(() => _hovering = false),
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: () => onJump(widget.network, widget.path),
          child: Tooltip(
            message: 'Go to $text',
            waitDuration: const Duration(milliseconds: 400),
            child: Text(
              text,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: (widget.style ?? const TextStyle()).copyWith(
                decoration: _hovering ? TextDecoration.underline : null,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
