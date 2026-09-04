import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/node_network/node_name_path.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// The property panel's **node name strip** — one shared, editable line above
/// whichever per-type editor the router dispatches to
/// (`doc/design_node_names_in_ui.md` D3).
///
/// It exists here rather than in the 90 `NodeEditorHeader` call sites for the
/// obvious reason (one change covers every node type), and it shows the *name*
/// because the name is the one identifier the canvas, the Text tab, the AI
/// History panel, the `.cnnd` file and every AI message agree on. Every node
/// has one: `add_node` mints it and the `.cnnd` loader backfills.
///
/// Layout: `map4 /` (muted scope prefix, body nodes only) · the editable name
/// in monospace · the type in muted text · a *Copy name* button. The field
/// edits only the last path segment; the prefix is there so the user can see
/// which body they are in.
///
/// Editing manners follow the network-rename field in the node-networks list:
/// **Enter or focus loss commits, Esc reverts, nothing is written while
/// typing.** Validation lives in Rust (`rename_node`) — sharing the text
/// editor's validator is what keeps a GUI-typed name printable — and a
/// rejection renders the reason under the field with the stored name
/// untouched. The text is never silently suffixed: the user typed it on
/// purpose.
class NodeNameStrip extends StatefulWidget {
  const NodeNameStrip({
    super.key,
    required this.model,
    required this.root,
    required this.node,
    required this.scopeChain,
  });

  final StructureDesignerModel model;

  /// The active network's view — the scope prefix is read from it.
  final NodeNetworkView root;

  /// The node the property panel is showing.
  final NodeView node;

  /// The node's resolved scope (empty for a top-level node).
  final List<BigInt> scopeChain;

  @override
  State<NodeNameStrip> createState() => _NodeNameStripState();
}

class _NodeNameStripState extends State<NodeNameStrip> {
  final TextEditingController _controller = TextEditingController();
  final FocusNode _focusNode = FocusNode();

  /// The name the field was seeded from — what Esc reverts to, and what a
  /// commit compares against to detect a no-op.
  String _storedName = '';

  /// The kernel's rejection reason, cleared on the next edit or reseed.
  String? _error;

  @override
  void initState() {
    super.initState();
    _seed();
    _focusNode.addListener(_onFocusChange);
  }

  @override
  void didUpdateWidget(NodeNameStrip oldWidget) {
    super.didUpdateWidget(oldWidget);
    // A different node (or the same node renamed from elsewhere — the Text
    // tab, an AI edit, an undo) reseeds the field. Never while it has focus:
    // that would clobber what the user is typing.
    if (!_focusNode.hasFocus && _currentName != _storedName) {
      setState(_seed);
    }
  }

  @override
  void dispose() {
    _focusNode.removeListener(_onFocusChange);
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  String get _currentName => widget.node.customName ?? '';

  void _seed() {
    _storedName = _currentName;
    _controller.text = _storedName;
    _error = null;
  }

  void _onFocusChange() {
    if (!_focusNode.hasFocus) _commit();
  }

  void _commit() {
    final typed = _controller.text.trim();
    if (typed == _storedName) {
      if (_error != null) setState(() => _error = null);
      return;
    }
    final result =
        widget.model.renameNode(widget.scopeChain, widget.node.id, typed);
    setState(() {
      if (result.success) {
        _storedName = typed;
        _controller.text = typed;
        _error = null;
      } else {
        // The stored name is untouched; leave the rejected text in the field
        // so the user can fix it rather than retype it.
        _error = result.errorMessage;
      }
    });
  }

  void _revert() {
    setState(() {
      _controller.text = _storedName;
      _error = null;
    });
    _focusNode.unfocus();
  }

  @override
  Widget build(BuildContext context) {
    final prefix = scopeOwnerNames(widget.root, widget.scopeChain);
    final muted = TextStyle(fontSize: 12, color: Colors.grey.shade600);
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              if (prefix.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(right: 4),
                  child: Text(
                    '${prefix.join('/')} /',
                    style: muted.copyWith(fontFamily: 'monospace'),
                  ),
                ),
              Expanded(
                child: CallbackShortcuts(
                  bindings: {
                    const SingleActivator(LogicalKeyboardKey.escape): _revert,
                  },
                  child: TextField(
                    key: const Key('node_name_field'),
                    controller: _controller,
                    focusNode: _focusNode,
                    style: const TextStyle(
                      fontSize: 14,
                      fontFamily: 'monospace',
                    ),
                    decoration: const InputDecoration(
                      isDense: true,
                      contentPadding:
                          EdgeInsets.symmetric(horizontal: 6, vertical: 6),
                      border: OutlineInputBorder(),
                    ),
                    onSubmitted: (_) => _commit(),
                  ),
                ),
              ),
              const SizedBox(width: 6),
              Text(widget.node.nodeTypeName, style: muted),
              const SizedBox(width: 2),
              // D9: the exact spelling to paste into a prompt for the AI. A
              // body node copies as its path (`map4/e1`), which is what the
              // AI is handed in error paths and the AI History panel.
              CopyTextButton(
                text: nodeNamePath(widget.root, widget.scopeChain, widget.node),
                tooltip: 'Copy node name',
                confirmation: 'Node name copied to clipboard',
              ),
            ],
          ),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: ErrorBanner(message: _error!),
            ),
        ],
      ),
    );
  }
}
