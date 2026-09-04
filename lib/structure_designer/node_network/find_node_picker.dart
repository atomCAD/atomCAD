import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';

/// **Find Node** — the "go to symbol" picker
/// (`doc/design_node_names_in_ui.md` D7).
///
/// The AI, the text format, the AI History panel and every error path name a
/// node by its **name path** (`map4/e1`). This is the way back: type the name,
/// land on the node. Matching, ranking and the path spelling all live in Rust
/// (`find_nodes_by_name`, D8) so there is exactly one path rule; this file is
/// the overlay, the keyboard handling and the rows.
///
/// It is deliberately *not* a modal dialog: a compact box anchored at the top
/// of the canvas, Esc to close, Enter to jump, Up/Down to move the highlight.
/// The landing goes through `jumpToNode` with no screen anchor — the user typed
/// a name, so there is no "where I was looking" position worth preserving.

/// Looks up matches. Injected so the widget is a pure function of its inputs
/// and the widget test needs no kernel.
typedef NodeNameSearch = List<APINodeNameMatch> Function(
    String query, bool allNetworks);

/// The *all networks* switch is remembered for the session (D7), not persisted:
/// it is a property of the hunt the user is on, not of the document. Kept here
/// rather than on the model because nothing outside this file reads it and
/// nothing needs to be notified when it changes.
bool _allNetworksSession = false;

/// The one open picker, so a second Ctrl+F (or the icon while it is open) does
/// not stack overlays.
OverlayEntry? _openPicker;

/// Opens the picker over the canvas, or re-focuses the one already open.
///
/// [anchorKey] is the node-network editor's own key: the box is placed at the
/// top-centre of that widget, which is where the eye already is. If the anchor
/// has not been laid out (an editor that is not on screen), the picker falls
/// back to the top-centre of the overlay.
void showFindNodePicker({
  required BuildContext context,
  required StructureDesignerModel model,
  required GlobalKey anchorKey,
}) {
  if (_openPicker != null) return;

  final overlay = Overlay.maybeOf(context);
  if (overlay == null) return;

  // Anchor geometry, in the overlay's coordinate space.
  final overlayBox = overlay.context.findRenderObject() as RenderBox?;
  final anchorBox = anchorKey.currentContext?.findRenderObject() as RenderBox?;
  final Size overlaySize = overlayBox?.size ?? MediaQuery.of(context).size;
  Rect anchorRect = Offset.zero & overlaySize;
  if (anchorBox != null && overlayBox != null && anchorBox.hasSize) {
    final topLeft = anchorBox.localToGlobal(Offset.zero, ancestor: overlayBox);
    anchorRect = topLeft & anchorBox.size;
  }
  final double width = anchorRect.width.clamp(220.0, 460.0);
  final double left = anchorRect.left + (anchorRect.width - width) / 2;
  // A little breathing room below the tab strip.
  final double top = anchorRect.top + 8;

  void close() {
    _openPicker?.remove();
    _openPicker = null;
  }

  final entry = OverlayEntry(
    builder: (_) => Stack(
      children: [
        // Click anywhere else to dismiss, the way a "go to symbol" box does.
        //
        // The barrier is **opaque**, and that is load-bearing rather than
        // cosmetic: the node canvas takes keyboard focus on pointer *enter*
        // (`node_network.dart`'s `MouseRegion.onEnter`), so a translucent
        // barrier would let a pointer drifting over the canvas silently steal
        // focus from the query field mid-word. Opaque keeps the canvas out of
        // the hit-test path while the picker is up.
        Positioned.fill(
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: close,
          ),
        ),
        Positioned(
          left: left,
          top: top,
          width: width,
          child: FindNodePicker(
            search: (query, allNetworks) =>
                model.findNodesByName(query, allNetworks: allNetworks),
            activeNetwork: model.nodeNetworkView?.name,
            allNetworks: _allNetworksSession,
            onAllNetworksChanged: (value) => _allNetworksSession = value,
            onPick: (match) {
              close();
              model.jumpToNodeMatch(match);
            },
            onClose: close,
          ),
        ),
      ],
    ),
  );
  _openPicker = entry;
  overlay.insert(entry);
}

/// The picker box itself: a query field over a result list.
class FindNodePicker extends StatefulWidget {
  const FindNodePicker({
    super.key,
    required this.search,
    required this.onPick,
    required this.onClose,
    this.activeNetwork,
    this.allNetworks = false,
    this.onAllNetworksChanged,
  });

  /// Runs one query. Called on open, on every keystroke and when the
  /// *all networks* switch flips.
  final NodeNameSearch search;

  /// Invoked with the row the user chose (Enter on the highlight, or a click).
  final ValueChanged<APINodeNameMatch> onPick;

  /// Invoked on Esc.
  final VoidCallback onClose;

  /// The active network's name, used only to word the scope switch.
  final String? activeNetwork;

  /// Initial state of the *all networks* switch (remembered for the session).
  final bool allNetworks;

  /// Reports a flip of the switch so the caller can remember it.
  final ValueChanged<bool>? onAllNetworksChanged;

  @override
  State<FindNodePicker> createState() => _FindNodePickerState();
}

class _FindNodePickerState extends State<FindNodePicker> {
  final TextEditingController _controller = TextEditingController();
  final FocusNode _fieldFocus = FocusNode();
  final ScrollController _scrollController = ScrollController();

  late bool _allNetworks = widget.allNetworks;
  List<APINodeNameMatch> _matches = const [];
  int _highlight = 0;

  /// Row height, fixed so the highlight can be scrolled into view without
  /// measuring anything.
  static const double _ROW_HEIGHT = 32;
  static const double _MAX_LIST_HEIGHT = 8 * _ROW_HEIGHT;

  @override
  void initState() {
    super.initState();
    // An empty query lists every node of the active network, so the picker
    // doubles as a name directory (D8).
    _runQuery();
  }

  @override
  void dispose() {
    _controller.dispose();
    _fieldFocus.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  void _runQuery() {
    final matches = widget.search(_controller.text, _allNetworks);
    setState(() {
      _matches = matches;
      _highlight =
          matches.isEmpty ? 0 : _highlight.clamp(0, matches.length - 1);
    });
  }

  /// Typing re-queries *and* resets the highlight to the best match — the
  /// ranking put it first, so that is the row Enter should take.
  void _onQueryChanged(String _) {
    _highlight = 0;
    _runQuery();
    _scrollHighlightIntoView();
  }

  void _moveHighlight(int delta) {
    if (_matches.isEmpty) return;
    setState(() {
      _highlight = (_highlight + delta + _matches.length) % _matches.length;
    });
    _scrollHighlightIntoView();
  }

  void _scrollHighlightIntoView() {
    if (!_scrollController.hasClients || _matches.isEmpty) return;
    final double target = _highlight * _ROW_HEIGHT;
    final double top = _scrollController.offset;
    final double viewport = _scrollController.position.viewportDimension;
    double? to;
    if (target < top) {
      to = target;
    } else if (target + _ROW_HEIGHT > top + viewport) {
      to = target + _ROW_HEIGHT - viewport;
    }
    if (to != null) {
      _scrollController
          .jumpTo(to.clamp(0.0, _scrollController.position.maxScrollExtent));
    }
  }

  void _pickHighlighted() {
    if (_matches.isEmpty) return;
    widget.onPick(_matches[_highlight]);
  }

  void _setAllNetworks(bool value) {
    _allNetworks = value;
    widget.onAllNetworksChanged?.call(value);
    _highlight = 0;
    _runQuery();
  }

  /// Up/Down/Enter/Esc are the picker's, not the text field's. This `Focus` is
  /// a closer ancestor of the field than the app's text-editing shortcuts, so
  /// it sees them first; everything else falls through to normal typing.
  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    switch (event.logicalKey) {
      case LogicalKeyboardKey.escape:
        widget.onClose();
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowDown:
        _moveHighlight(1);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowUp:
        _moveHighlight(-1);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.enter:
      case LogicalKeyboardKey.numpadEnter:
        _pickHighlighted();
        return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final muted = TextStyle(fontSize: 11, color: Colors.grey.shade600);
    return Focus(
      onKeyEvent: _onKey,
      child: Material(
        elevation: 8,
        borderRadius: BorderRadius.circular(4),
        child: Container(
          decoration: BoxDecoration(
            border: Border.all(color: Colors.black26),
            borderRadius: BorderRadius.circular(4),
          ),
          padding: const EdgeInsets.all(6),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TextField(
                key: const Key('find_node_field'),
                controller: _controller,
                focusNode: _fieldFocus,
                autofocus: true,
                style: const TextStyle(fontSize: 14, fontFamily: 'monospace'),
                decoration: const InputDecoration(
                  isDense: true,
                  hintText: 'Node name, e.g. map4/e1',
                  prefixIcon: Icon(Icons.search, size: 16),
                  prefixIconConstraints:
                      BoxConstraints(minWidth: 28, minHeight: 24),
                  contentPadding:
                      EdgeInsets.symmetric(horizontal: 6, vertical: 8),
                  border: OutlineInputBorder(),
                ),
                onChanged: _onQueryChanged,
              ),
              const SizedBox(height: 4),
              Row(
                children: [
                  Expanded(
                    child: Text(
                      _matches.isEmpty
                          ? 'No matching node'
                          : '${_matches.length} node${_matches.length == 1 ? '' : 's'}',
                      style: muted,
                    ),
                  ),
                  Tooltip(
                    message: widget.activeNetwork == null
                        ? 'Search every network'
                        : "Search every network, not just '${widget.activeNetwork}'",
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text('All networks', style: muted),
                        SizedBox(
                          height: 24,
                          child: Switch(
                            key: const Key('find_node_all_networks'),
                            value: _allNetworks,
                            materialTapTargetSize:
                                MaterialTapTargetSize.shrinkWrap,
                            onChanged: _setAllNetworks,
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
              if (_matches.isNotEmpty)
                ConstrainedBox(
                  constraints:
                      const BoxConstraints(maxHeight: _MAX_LIST_HEIGHT),
                  child: ListView.builder(
                    controller: _scrollController,
                    shrinkWrap: true,
                    itemExtent: _ROW_HEIGHT,
                    itemCount: _matches.length,
                    itemBuilder: (context, index) => _row(
                      _matches[index],
                      index == _highlight,
                      theme,
                      muted,
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _row(APINodeNameMatch match, bool highlighted, ThemeData theme,
      TextStyle muted) {
    return InkWell(
      key: Key('find_node_row_${match.network}_${match.namePath}'),
      onTap: () => widget.onPick(match),
      child: Container(
        color: highlighted ? theme.highlightColor : null,
        padding: const EdgeInsets.symmetric(horizontal: 4),
        alignment: Alignment.centerLeft,
        child: Row(
          children: [
            Flexible(
              child: Text(
                match.namePath,
                style: const TextStyle(fontSize: 13, fontFamily: 'monospace'),
                overflow: TextOverflow.ellipsis,
              ),
            ),
            const SizedBox(width: 6),
            Flexible(
              child: Text(match.nodeTypeName,
                  style: muted, overflow: TextOverflow.ellipsis),
            ),
            // The network chip only earns its place once the search spans more
            // than one network.
            if (_allNetworks) ...[
              const Spacer(),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
                decoration: BoxDecoration(
                  color: Colors.grey.shade200,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Text(match.network,
                    style: muted, overflow: TextOverflow.ellipsis),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
