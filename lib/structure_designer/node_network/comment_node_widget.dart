import 'dart:math' as math;
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network_painter.dart';
import 'package:flutter_cad/structure_designer/node_network/node_title.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';

const double COMMENT_MIN_WIDTH = 100.0;
const double COMMENT_MIN_HEIGHT = 60.0;
const double COMMENT_RESIZE_HANDLE_SIZE = 12.0;

/// Side of the square anchor handle in the note's bottom-left corner — the
/// grab point for "what does this note document?"
/// (`doc/design_wire_annotations.md` Interaction). Same size as the resize
/// handle it mirrors across the note's bottom edge.
const double COMMENT_ANCHOR_HANDLE_SIZE = 12.0;
const Color COMMENT_BACKGROUND_COLOR = Color(0xCCFFF9C4);
const Color COMMENT_BORDER_COLOR = Color(0xFF9E9E9E);
const Color COMMENT_HEADER_COLOR = Color(0xFFFFEB3B);
const Color COMMENT_SELECTED_BORDER_COLOR = Color(0xFFE08000);

/// Which of a note's two text fields the in-place editor is aimed at.
enum _CommentField { label, text }

class CommentNodeWidget extends StatefulWidget {
  final NodeView node;
  final Offset panOffset;
  final ZoomLevel zoomLevel;

  /// Scope chain of the body this comment lives in (`const []` at top level).
  /// Forwarded to every selection/drag/edit API call so the right node is
  /// addressed across scopes.
  final List<BigInt> scopeChain;

  /// Resolver shared by the whole canvas for the current frame (see
  /// `NodeWidget.resolver` — constructing one per widget is O(N) each and
  /// made canvas rebuilds O(N²)). Also the comment's positioning authority:
  /// it maps the body-local stored position to screen when the comment lives
  /// inside an HOF/closure body (see [scopeChain]).
  final ScopeResolver resolver;

  /// Draw the note as if it were not selected. Set by the image export — see
  /// `NodeWidget.hideSelection`.
  final bool hideSelection;

  /// Whether the title bar writes the note's user-supplied label or the node's
  /// *name* (`doc/design_node_names_in_ui.md` D4). A comment is a node like any
  /// other and has a name, so in Name mode its header appears even when the
  /// note is untitled — see [_headerLabel].
  final NodeTitleMode titleMode;

  const CommentNodeWidget({
    super.key,
    required this.node,
    required this.panOffset,
    required this.zoomLevel,
    required this.resolver,
    this.scopeChain = const [],
    this.hideSelection = false,
    this.titleMode = NodeTitleMode.type,
  });

  /// Selection state as it should be **drawn**; interaction handlers keep
  /// reading `node.selected`.
  bool get drawSelected => node.selected && !hideSelection;

  @override
  State<CommentNodeWidget> createState() => _CommentNodeWidgetState();
}

class _CommentNodeWidgetState extends State<CommentNodeWidget> {
  bool _isResizing = false;
  double _resizeStartWidth = 0;
  double _resizeStartHeight = 0;
  Offset _resizeStartPosition = Offset.zero;

  /// True while the note is open for editing directly on the canvas
  /// (issue #421). Entered by double-clicking the note, left when focus
  /// leaves both fields or Escape is pressed.
  ///
  /// While it is set, the note's own drag handlers are detached so that
  /// dragging inside a field selects text instead of moving the note, and the
  /// canvas is told (via `StructureDesignerModel.inPlaceEditRef`) not to steal
  /// keyboard focus back.
  bool _isEditing = false;

  late final TextEditingController _labelController;
  late final TextEditingController _textController;
  late final FocusNode _labelFocusNode;
  late final FocusNode _textFocusNode;

  /// Field values captured when edit mode was entered, so Escape can put them
  /// back before the undo group closes.
  String _labelAtEditStart = '';
  String _textAtEditStart = '';

  /// Model captured when edit mode was entered. [dispose] has to clear the
  /// model's `inPlaceEditRef` for a note deleted mid-edit, and reaching for an
  /// inherited widget from `dispose` is not allowed — so hold the reference.
  StructureDesignerModel? _editModel;

  /// Previous tap, for the hand-rolled double-click detection in
  /// [_handleTapDown]. See that method for why `onDoubleTap` is not used.
  DateTime? _lastTapTimestamp;
  Offset? _lastTapPosition;

  /// Render-object handles used to translate a double-click into "which field,
  /// and which character" — see [_beginEditAtPointer].
  final GlobalKey _headerKey = GlobalKey();
  final GlobalKey _labelTextKey = GlobalKey();
  final GlobalKey _bodyTextKey = GlobalKey();

  double get _width => widget.node.commentWidth ?? 200.0;
  double get _height => widget.node.commentHeight ?? 100.0;
  String get _label => widget.node.commentLabel ?? '';

  /// What the yellow title bar *shows*: the note's own label in Type mode, the
  /// node's name in Name mode (D4). Editing is unaffected — the field always
  /// binds to `_label`, because the label is the only one of the two this
  /// widget owns; renaming a node is the property panel's strip (D3).
  String get _headerLabel =>
      nodeTitleLabel(widget.node, widget.titleMode, _label);
  String get _text => widget.node.commentText ?? '';

  /// Byte-encoded scope path for FRB API calls that address this comment node.
  Uint64List get _scopePath => scopeChainToBytes(widget.scopeChain);

  @override
  void initState() {
    super.initState();
    _labelController = TextEditingController();
    _textController = TextEditingController();
    _labelFocusNode = FocusNode(debugLabel: 'comment_label');
    _textFocusNode = FocusNode(debugLabel: 'comment_text');
    _labelFocusNode.addListener(_handleFieldFocusChange);
    _textFocusNode.addListener(_handleFieldFocusChange);
  }

  @override
  void dispose() {
    // Detached first: disposing a focused `FocusNode` unfocuses it, which
    // would otherwise re-enter [_handleFieldFocusChange] → [_exitEditMode] →
    // `setState` on an unmounted State.
    _labelFocusNode.removeListener(_handleFieldFocusChange);
    _textFocusNode.removeListener(_handleFieldFocusChange);
    // A note deleted (or otherwise dropped from the tree) mid-edit must not
    // leave the kernel's undo group dangling, nor the canvas permanently
    // unable to take keyboard focus back.
    if (_isEditing) {
      _isEditing = false;
      sd_api.endEditCommentNode();
      _clearInPlaceEditRef();
    }
    _labelFocusNode.dispose();
    _textFocusNode.dispose();
    _labelController.dispose();
    _textController.dispose();
    super.dispose();
  }

  /// Release this note's claim on the canvas focus guard — but only if it is
  /// still ours. Another note may already have taken over.
  void _clearInPlaceEditRef() {
    final model = _editModel;
    if (model != null &&
        model.isInPlaceEditTarget(widget.node.id, widget.scopeChain)) {
      model.inPlaceEditRef = null;
    }
    _editModel = null;
  }

  @override
  Widget build(BuildContext context) {
    final scale = getZoomScale(widget.zoomLevel);
    // Position via the scope resolver: the comment's stored position lives in
    // its body-local frame, which the resolver maps to screen. For the
    // top-level scope (empty chain) this is identical to `logicalToScreen`.
    final resolver = widget.resolver;
    final screenPos = resolver.scopedToScreen(
      widget.scopeChain,
      Offset(widget.node.position.x, widget.node.position.y),
    );

    final scaledWidth = _width * scale;
    final scaledHeight = _height * scale;

    final fontSize = 14.0 * math.sqrt(scale);
    final headerFontSize = 14.0 * math.sqrt(scale);

    return Positioned(
      left: screenPos.dx,
      top: screenPos.dy,
      child: GestureDetector(
        onTapDown: (details) => _handleTapDown(context, details),
        onSecondaryTapDown: (details) => _handleContextMenu(context, details),
        // Drag handlers are detached while editing so the text field's own
        // selection drag is the only drag recognizer in the arena — otherwise
        // swiping across a word would move the note instead of selecting it.
        onPanStart:
            _isEditing ? null : (details) => _handlePanStart(context, details),
        onPanUpdate:
            _isEditing ? null : (details) => _handlePanUpdate(context, details),
        onPanEnd: _isEditing ? null : (_) => _handlePanEnd(context),
        child: Container(
          width: scaledWidth,
          height: scaledHeight,
          decoration: BoxDecoration(
            color: COMMENT_BACKGROUND_COLOR,
            border: Border.all(
              color: widget.drawSelected
                  ? COMMENT_SELECTED_BORDER_COLOR
                  : COMMENT_BORDER_COLOR,
              width: widget.drawSelected ? 2.0 : 1.0,
              style: BorderStyle.solid,
            ),
            borderRadius: BorderRadius.circular(4.0),
            boxShadow: widget.drawSelected
                ? [
                    BoxShadow(
                      color:
                          COMMENT_SELECTED_BORDER_COLOR.withValues(alpha: 0.3),
                      blurRadius: 8.0,
                      spreadRadius: 2.0,
                    )
                  ]
                : null,
          ),
          child: Stack(
            children: [
              // Escape cancels the edit. This `Focus` never takes focus
              // itself; it sits above both fields in the focus tree purely so
              // their key events bubble through it.
              Focus(
                canRequestFocus: false,
                skipTraversal: true,
                onKeyEvent: (node, event) {
                  if (_isEditing &&
                      event is KeyDownEvent &&
                      event.logicalKey == LogicalKeyboardKey.escape) {
                    _exitEditMode(commit: false);
                    return KeyEventResult.handled;
                  }
                  return KeyEventResult.ignored;
                },
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    // The header is normally hidden when there is no label,
                    // but edit mode always shows it — otherwise an unlabelled
                    // note would have no way to acquire a label in place. In
                    // Name mode it is always shown: the name it carries is
                    // never empty. The note's box is a fixed user-set size, so
                    // showing the header eats into the body rather than
                    // changing the footprint (D5).
                    if (_isEditing || _headerLabel.isNotEmpty)
                      _buildHeader(scale, headerFontSize),
                    Expanded(child: _buildBody(scale, fontSize)),
                  ],
                ),
              ),
              // Bottom-LEFT: the anchor handle, mirroring the resize handle
              // across the note's bottom edge. Hidden while the note is open
              // for in-place editing, where every pointer gesture inside the
              // note belongs to the text fields.
              if (!_isEditing)
                Positioned(
                  left: 0,
                  bottom: 0,
                  child: _buildAnchorHandle(context, scale),
                ),
              Positioned(
                right: 0,
                bottom: 0,
                child: Listener(
                  onPointerDown: (event) {
                    // Stop propagation to prevent rectangle selection
                  },
                  behavior: HitTestBehavior.opaque,
                  child: MouseRegion(
                    cursor: SystemMouseCursors.resizeDownRight,
                    child: GestureDetector(
                      behavior: HitTestBehavior.opaque,
                      onPanStart: (details) => _startResize(details),
                      onPanUpdate: (details) => _updateResize(context, details),
                      onPanEnd: (details) => _endResize(context),
                      child: Container(
                        width: COMMENT_RESIZE_HANDLE_SIZE * scale,
                        height: COMMENT_RESIZE_HANDLE_SIZE * scale,
                        decoration: BoxDecoration(
                          color: Colors.grey.withValues(alpha: 0.5),
                          borderRadius: const BorderRadius.only(
                            bottomRight: Radius.circular(3.0),
                          ),
                        ),
                        child: Icon(
                          Icons.open_in_full,
                          size: 8.0 * scale,
                          color: Colors.white,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  /// The yellow title bar. Renders the label as static text, or as a
  /// single-line field while editing.
  Widget _buildHeader(double scale, double headerFontSize) {
    final style = TextStyle(
      fontSize: headerFontSize,
      fontWeight: FontWeight.bold,
      color: Colors.black87,
    );
    return Container(
      key: _headerKey,
      padding: EdgeInsets.symmetric(
        horizontal: 6.0 * scale,
        vertical: 3.0 * scale,
      ),
      decoration: const BoxDecoration(
        color: COMMENT_HEADER_COLOR,
        borderRadius: BorderRadius.only(
          topLeft: Radius.circular(3.0),
          topRight: Radius.circular(3.0),
        ),
      ),
      child: _isEditing
          ? TextField(
              controller: _labelController,
              focusNode: _labelFocusNode,
              style: style,
              cursorColor: Colors.black87,
              maxLines: 1,
              decoration: InputDecoration(
                isCollapsed: true,
                border: InputBorder.none,
                hintText: 'Optional title...',
                hintStyle: style.copyWith(
                  color: Colors.black38,
                  fontWeight: FontWeight.normal,
                ),
              ),
              onChanged: (_) => _pushToKernel(),
              // Enter finishes a one-line title rather than inserting a
              // newline the header could not show anyway.
              onSubmitted: (_) => _exitEditMode(commit: true),
            )
          : Text(
              _headerLabel,
              key: _labelTextKey,
              style: style,
              overflow: TextOverflow.ellipsis,
              maxLines: 1,
            ),
    );
  }

  /// The note body. Renders the text as static wrapped text, or as a
  /// multi-line field while editing.
  ///
  /// The two branches deliberately differ in how they scroll: the static
  /// branch keeps its `SingleChildScrollView`, while the editing branch lets
  /// the field scroll itself (`expands: true`). Wrapping an editable in an
  /// outer scroll view breaks drag-selection anchoring — see `lib/AGENTS.md`
  /// → "A Text Field Must Own Its Own Vertical Scroll" (issue #422).
  Widget _buildBody(double scale, double fontSize) {
    final style = TextStyle(fontSize: fontSize, color: Colors.black87);
    final padding = EdgeInsets.all(6.0 * scale);

    if (_isEditing) {
      return Padding(
        padding: padding,
        child: TextField(
          controller: _textController,
          focusNode: _textFocusNode,
          style: style,
          cursorColor: Colors.black87,
          expands: true,
          maxLines: null,
          minLines: null,
          textAlignVertical: TextAlignVertical.top,
          keyboardType: TextInputType.multiline,
          decoration: InputDecoration(
            isCollapsed: true,
            border: InputBorder.none,
            hintText: 'Enter comment text...',
            hintStyle: style.copyWith(color: Colors.black38),
          ),
          onChanged: (_) => _pushToKernel(),
        ),
      );
    }

    return Padding(
      padding: padding,
      child: SingleChildScrollView(
        child: Text(_text, key: _bodyTextKey, style: style),
      ),
    );
  }

  // ===== ANCHORING (doc/design_wire_annotations.md) =====

  /// The anchor handle: drag it onto a wire or a node to say what this note
  /// documents, or — once it is anchored — click it to detach. It carries its
  /// own [GestureDetector] so its pan never reaches the note's move-drag — the
  /// same arrangement the resize handle uses, and the reason the note is not
  /// simply given a second set of pan callbacks.
  ///
  /// **Detaching is a click, deliberately not a drop on empty space.** A wire
  /// is a ~10 px hit band, so missing one is the *common* outcome of an
  /// anchoring drag; making the miss destroy the anchor would make an accident
  /// and an intention indistinguishable. Empty space is also already spoken for
  /// on this canvas in the opposite direction — dropping a *wire* there opens
  /// the Add Node popup — so it cannot read as "detach" here.
  Widget _buildAnchorHandle(BuildContext context, double scale) {
    final anchored = widget.node.commentAnchors.isNotEmpty;
    return Listener(
      onPointerDown: (event) {
        // Swallowed so the canvas doesn't start a rectangle selection.
      },
      behavior: HitTestBehavior.opaque,
      child: MouseRegion(
        cursor: SystemMouseCursors.precise,
        child: Tooltip(
          message: anchored
              ? "Click to remove this note's anchor,\nor drag onto a new target"
              : 'Drag onto a wire or node to say what this note is about',
          waitDuration: const Duration(milliseconds: 600),
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            // Null when there is nothing to detach, so the tap falls through
            // to the note's own selection handling.
            onTap: anchored ? () => _removeAnchor(context) : null,
            onPanStart: (details) => _startAnchorDrag(context, details),
            onPanUpdate: (details) => _updateAnchorDrag(context, details),
            onPanEnd: (details) => _endAnchorDrag(context, details),
            onPanCancel: () => _cancelAnchorDrag(context),
            child: Container(
              width: COMMENT_ANCHOR_HANDLE_SIZE * scale,
              height: COMMENT_ANCHOR_HANDLE_SIZE * scale,
              decoration: BoxDecoration(
                color: (anchored ? ANCHOR_LEADER_COLOR_SELECTED : Colors.grey)
                    .withValues(alpha: 0.5),
                borderRadius: const BorderRadius.only(
                  bottomLeft: Radius.circular(3.0),
                ),
              ),
              // The icon shows what a click would *do*, not what the state is
              // — the orange tint already says "anchored".
              child: Icon(
                anchored ? Icons.link_off : Icons.link,
                size: 8.0 * scale,
                color: Colors.white,
              ),
            ),
          ),
        ),
      ),
    );
  }

  /// Detach the note from what it documents. Undoable like any other anchor
  /// change; also reachable from the note's context menu.
  void _removeAnchor(BuildContext context) {
    // Same reason as in [_startAnchorDrag]: the note's `onTapDown` already
    // fired for this press, and two detach clicks in a row must not add up to
    // a double-click that opens the in-place editor.
    _lastTapTimestamp = null;
    Provider.of<StructureDesignerModel>(context, listen: false)
        .setCommentAnchors(widget.node.id, const [],
            scopeChain: widget.scopeChain);
  }

  /// The canvas widget's render box, used to turn a drag's global position
  /// into the canvas-local (screen) coordinates every hit test and the
  /// painter's rubber band speak. Null in the detached tree the image export
  /// builds — which has no interaction anyway.
  RenderBox? _findCanvasRenderBox(BuildContext context) {
    RenderBox? result;
    context.visitAncestorElements((element) {
      if (element.widget is NodeNetwork) {
        result = element.renderObject as RenderBox?;
        return false;
      }
      return true;
    });
    return result;
  }

  void _startAnchorDrag(BuildContext context, DragStartDetails details) {
    // The note's `onTapDown` has already fired for this press (it runs on the
    // 100 ms deadline, before the arena hands the gesture to this handle), so
    // it recorded a tap. Forget it, or re-aiming the anchor twice in quick
    // succession reads as a double-click and opens the in-place editor.
    _lastTapTimestamp = null;

    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    // Anchoring a note is working on it; the drag never taps the note, so the
    // ordinary selection path doesn't run.
    model.setActiveScopeChain(widget.scopeChain);
    if (!widget.node.selected || !widget.node.active) {
      model.setSelectedNode(widget.node.id, scopeChain: widget.scopeChain);
    }
    _updateAnchorDrag(context, null, globalPosition: details.globalPosition);
  }

  void _updateAnchorDrag(BuildContext context, DragUpdateDetails? details,
      {Offset? globalPosition}) {
    final box = _findCanvasRenderBox(context);
    if (box == null) return;
    final global = globalPosition ?? details!.globalPosition;
    Provider.of<StructureDesignerModel>(context, listen: false)
        .dragCommentAnchor(
            widget.node.id, widget.scopeChain, box.globalToLocal(global));
  }

  void _cancelAnchorDrag(BuildContext context) {
    Provider.of<StructureDesignerModel>(context, listen: false)
        .cancelDragCommentAnchor();
  }

  /// Resolve the drop. A node under the pointer wins over a wire — nodes are
  /// the larger, more deliberate target and they render on top of the
  /// top-level wires. Dropping on empty space, on the note itself, or on
  /// anything in another scope (D5) cancels and leaves the note as it was.
  void _endAnchorDrag(BuildContext context, DragEndDetails details) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    final drag = model.draggedCommentAnchor;
    model.cancelDragCommentAnchor();
    if (drag == null) return;

    final anchor = _anchorAtDropPosition(model, drag.endPosition);
    if (anchor == null) return;
    // The first implementation keeps one anchor per note: the handle sets
    // `anchors[0]`, replacing whatever was there. The model is a list so that
    // multi-anchor notes (authorable in the text format today) need no
    // migration later.
    model.setCommentAnchors(widget.node.id, [anchor],
        scopeChain: widget.scopeChain);
  }

  /// The anchor a drop at [position] (canvas-local screen coordinates)
  /// designates, or null if it designates nothing anchorable.
  APICommentAnchor? _anchorAtDropPosition(
      StructureDesignerModel model, Offset position) {
    // Positions have not moved during the drag, so the frame's shared resolver
    // is still the right coordinate authority.
    final hitNode = widget.resolver.findNodeAtScreenPosition(position);
    if (hitNode != null) {
      if (!_sameScope(hitNode.scopeChain, widget.scopeChain)) return null;
      if (hitNode.node.id == widget.node.id) return null;
      return APICommentAnchor.node(nodeId: hitNode.node.id);
    }

    // One-shot resolver, the sanctioned pattern for event handlers.
    final hitWire = NodeNetworkPainter(model,
            panOffset: widget.panOffset, zoomLevel: widget.zoomLevel)
        .findWireAtPosition(position);
    if (hitWire == null) return null;
    if (!_sameScope(hitWire.scopeChain, widget.scopeChain)) return null;
    return APICommentAnchor.wire(
      anchor: APIWireAnchor(
        destinationNodeId: hitWire.destNodeId,
        // `findWireAtPosition` only ever reports selectable wires, which are
        // exactly the `External`-destination ones.
        destinationArgumentKind: APIArgumentKind.external_,
        destinationArgumentIndex: hitWire.destParamIndex,
        // The canvas doesn't know about persistent parameter ids; the kernel
        // fills one in when the destination carries one (D4).
        destinationParamId: null,
        sourceNodeId: hitWire.sourceNodeId,
      ),
    );
  }

  static bool _sameScope(List<BigInt> a, List<BigInt> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  // ===== IN-PLACE EDITING (issue #421) =====

  /// Open the editor from a double-click at [globalPosition], on the field
  /// that was clicked and with the caret on the character that was clicked.
  ///
  /// Both answers are read off the *rendered* widgets rather than recomputed
  /// from the layout constants: `RenderParagraph.getPositionForOffset` maps a
  /// point to a text offset using the very line breaks on screen, and
  /// `globalToLocal` folds in the note's padding, the canvas zoom, and the
  /// body's scroll position for free. Must run before the rebuild swaps those
  /// paragraphs out for text fields — hence the offset is resolved here and
  /// handed to [_enterEditMode], not looked up afterwards.
  void _beginEditAtPointer(Offset globalPosition) {
    final headerBox = _headerKey.currentContext?.findRenderObject();
    // `_headerLabel`, not `_label`: whatever the header *shows* is the header,
    // and clicking it always aims the editor at the label field — in Name mode
    // that is how an untitled note still acquires a title in place.
    final onHeader = _headerLabel.isNotEmpty &&
        headerBox is RenderBox &&
        headerBox.hasSize &&
        headerBox.globalToLocal(globalPosition).dy <= headerBox.size.height;

    final field = onHeader ? _CommentField.label : _CommentField.text;
    final caret = _caretOffsetAt(
      onHeader ? _labelTextKey : _bodyTextKey,
      globalPosition,
    );
    _enterEditMode(field, caretOffset: caret);
  }

  /// Character offset under [globalPosition] in the static `Text` behind
  /// [key], or null if it is not laid out (an empty note has no paragraph).
  int? _caretOffsetAt(GlobalKey key, Offset globalPosition) {
    final object = key.currentContext?.findRenderObject();
    if (object is! RenderParagraph || !object.hasSize) return null;
    return object
        .getPositionForOffset(object.globalToLocal(globalPosition))
        .offset;
  }

  /// Open the note for editing on the canvas and focus [field], putting the
  /// caret at [caretOffset]. A null offset means "no particular character":
  /// the title is selected whole (the in-place-rename convention) and the body
  /// gets the caret at its end.
  void _enterEditMode(_CommentField field, {int? caretOffset}) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    // Editing a note implies working on it. The click that opened the editor
    // may not have selected it — a double-click's second tap goes straight
    // here, and the context-menu route never taps the note at all — and an
    // unselected note leaves the properties panel showing something else.
    model.setActiveScopeChain(widget.scopeChain);
    if (!widget.node.selected || !widget.node.active) {
      model.setSelectedNode(widget.node.id, scopeChain: widget.scopeChain);
    }

    if (!_isEditing) {
      // The controllers are the source of truth for the whole edit: seed them
      // from the view once here and never resync from a later
      // `refreshFromKernel` (that is what makes the caret jump mid-typing).
      _labelController.text = _label;
      _textController.text = _text;
      _labelAtEditStart = _label;
      _textAtEditStart = _text;
      sd_api.beginEditCommentNode(
          scopePath: _scopePath, nodeId: widget.node.id);
      _editModel = model;
      model.inPlaceEditRef =
          (nodeId: widget.node.id, scopeChain: widget.scopeChain);
      setState(() => _isEditing = true);
    }

    // The target field only exists after the rebuild above.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_isEditing) return;
      final (focusNode, controller) = switch (field) {
        _CommentField.label => (_labelFocusNode, _labelController),
        _CommentField.text => (_textFocusNode, _textController),
      };
      focusNode.requestFocus();
      // Assigning `.text` above left the selection invalid, which
      // `EditableText` would resolve to end-of-text on focus. Set it
      // explicitly instead — after `requestFocus` but synchronously, so it is
      // already valid by the time the focus change is applied.
      controller.selection = switch ((caretOffset, field)) {
        (final int offset, _) => TextSelection.collapsed(
            offset: offset.clamp(0, controller.text.length)),
        (null, _CommentField.label) =>
          TextSelection(baseOffset: 0, extentOffset: controller.text.length),
        (null, _CommentField.text) =>
          TextSelection.collapsed(offset: controller.text.length),
      };
    });
  }

  /// Close the in-place editor. [commit] false (Escape) restores the values
  /// captured on entry first, so `endEditCommentNode` sees no net change and
  /// pushes no undo command.
  void _exitEditMode({required bool commit}) {
    if (!_isEditing) return;
    final model = _editModel;

    if (!commit) {
      // Assigning to a controller does not fire `onChanged`, so the restored
      // values need an explicit push.
      _labelController.text = _labelAtEditStart;
      _textController.text = _textAtEditStart;
      _pushToKernel();
    }
    sd_api.endEditCommentNode();
    _clearInPlaceEditRef();
    // Set before the unfocus calls below, so the focus notifications they
    // raise short-circuit in [_handleFieldFocusChange] instead of re-entering
    // here.
    _isEditing = false;
    if (mounted) setState(() {});
    _labelFocusNode.unfocus();
    _textFocusNode.unfocus();
    // The single full refresh for the whole edit — see [_pushToKernel].
    model?.refreshFromKernel();
  }

  /// Focus left one of the fields. Tabbing between the two is not an exit, so
  /// only close when neither holds focus any more. Focus transfers are applied
  /// as a unit before listeners run, so the new focus is already visible here.
  void _handleFieldFocusChange() {
    if (!_isEditing) return;
    if (_labelFocusNode.hasFocus || _textFocusNode.hasFocus) return;
    _exitEditMode(commit: true);
  }

  /// Mirror the fields into the kernel **without** a `refreshFromKernel()`.
  /// A full refresh rebuilds the whole canvas and hands this widget a fresh
  /// `NodeView`; doing that per keystroke is both wasteful and a caret-jump
  /// hazard, and while editing the text on screen is the `TextField`'s own
  /// content, not `node.commentText`. The one full refresh happens when edit
  /// mode ends ([_exitEditMode]).
  void _pushToKernel() {
    sd_api.updateCommentNode(
      scopePath: _scopePath,
      nodeId: widget.node.id,
      label: _labelController.text,
      text: _textController.text,
    );
  }

  /// Tap-down on the note: ordinary selection, or — on the second tap of a
  /// double-click — open the in-place editor at the clicked character.
  ///
  /// The double-click is detected by hand rather than with `onDoubleTap`,
  /// because a `DoubleTapGestureRecognizer` *holds the gesture arena* on the
  /// first tap (`multitap.dart` `_registerFirstTap`). That has two costs here:
  /// a real double-click makes it win the arena and **reject** this tap
  /// recognizer, so `onTapDown` never fires and the note is never selected;
  /// and a plain single click has its selection delayed by the whole
  /// `kDoubleTapTimeout` while the recognizer waits for a partner. Comparing
  /// timestamps costs neither.
  void _handleTapDown(BuildContext context, TapDownDetails details) {
    // While editing, this also fires for clicks that belong to the text field:
    // a tap-down handler runs on the 100 ms press deadline, before the gesture
    // arena resolves in the field's favour. Re-running the selection logic
    // (and its refresh) on every caret placement is pure noise.
    if (_isEditing) return;

    // Ctrl/Shift-clicking twice is selection fiddling, not a request to edit.
    final modified = HardwareKeyboard.instance.isControlPressed ||
        HardwareKeyboard.instance.isShiftPressed;
    final now = DateTime.now();
    final previous = _lastTapTimestamp;
    final previousPosition = _lastTapPosition;
    _lastTapTimestamp = now;
    _lastTapPosition = details.globalPosition;

    if (!modified &&
        previous != null &&
        previousPosition != null &&
        now.difference(previous) <= kDoubleTapTimeout &&
        (details.globalPosition - previousPosition).distance <=
            kDoubleTapSlop) {
      // Don't let a third click re-trigger against the second.
      _lastTapTimestamp = null;
      _beginEditAtPointer(details.globalPosition);
      return;
    }

    _handleSelectTap(context);
  }

  void _handleSelectTap(BuildContext context) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    final scopeChain = widget.scopeChain;

    // Keyboard ops (delete / copy / paste) act on the active scope, so a click
    // on a body-scoped comment must make that body active.
    model.setActiveScopeChain(scopeChain);

    if (HardwareKeyboard.instance.isControlPressed) {
      model.toggleNodeSelection(widget.node.id, scopeChain: scopeChain);
    } else if (HardwareKeyboard.instance.isShiftPressed) {
      model.addNodeToSelection(widget.node.id, scopeChain: scopeChain);
    } else if (widget.node.selected && !widget.node.active) {
      model.addNodeToSelection(widget.node.id, scopeChain: scopeChain);
    } else {
      model.setSelectedNode(widget.node.id, scopeChain: scopeChain);
    }
  }

  void _handlePanStart(BuildContext context, DragStartDetails details) {
    if (!_isResizing) {
      _handleSelectTap(context);
    }
  }

  void _handlePanUpdate(BuildContext context, DragUpdateDetails details) {
    if (_isResizing) return;

    final scale = getZoomScale(widget.zoomLevel);
    final logicalDelta = details.delta / scale;
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    if (widget.node.selected) {
      model.dragSelectedNodes(logicalDelta, scopeChain: widget.scopeChain);
    } else {
      model.dragNodePosition(widget.node.id, logicalDelta,
          scopeChain: widget.scopeChain);
    }
  }

  void _handlePanEnd(BuildContext context) {
    if (_isResizing) return;

    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    if (widget.node.selected) {
      model.updateSelectedNodesPosition(scopeChain: widget.scopeChain);
    } else {
      model.updateNodePosition(widget.node.id, scopeChain: widget.scopeChain);
    }
  }

  void _startResize(DragStartDetails details) {
    // Not while the note is open for in-place editing: that edit already owns
    // the kernel's undo group, and `begin_comment_edit` overwrites the pending
    // snapshot rather than nesting — the resize would swallow the text edit's
    // undo entry. Resizing mid-edit just folds into the one edit step.
    if (!_isEditing) {
      sd_api.beginEditCommentNode(
          scopePath: _scopePath, nodeId: widget.node.id);
    }
    setState(() {
      _isResizing = true;
      _resizeStartWidth = _width;
      _resizeStartHeight = _height;
      _resizeStartPosition = details.globalPosition;
    });
  }

  void _updateResize(BuildContext context, DragUpdateDetails details) {
    if (!_isResizing) return;

    final scale = getZoomScale(widget.zoomLevel);
    final delta = details.globalPosition - _resizeStartPosition;

    final newWidth =
        (_resizeStartWidth + delta.dx / scale).clamp(COMMENT_MIN_WIDTH, 1000.0);
    final newHeight = (_resizeStartHeight + delta.dy / scale)
        .clamp(COMMENT_MIN_HEIGHT, 1000.0);

    sd_api.resizeCommentNode(
      scopePath: _scopePath,
      nodeId: widget.node.id,
      width: newWidth,
      height: newHeight,
    );

    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    model.refreshFromKernel();
  }

  void _endResize(BuildContext context) {
    // Paired with the conditional begin in [_startResize].
    if (!_isEditing) {
      sd_api.endEditCommentNode();
    }
    setState(() {
      _isResizing = false;
    });
  }

  void _handleContextMenu(BuildContext context, TapDownDetails details) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    model.setActiveScopeChain(widget.scopeChain);
    model.setSelectedNode(widget.node.id, scopeChain: widget.scopeChain);

    final RenderBox overlay =
        Overlay.of(context).context.findRenderObject() as RenderBox;
    final RelativeRect position = RelativeRect.fromRect(
      Rect.fromPoints(
        details.globalPosition,
        details.globalPosition,
      ),
      Offset.zero & overlay.size,
    );

    showMenu(
      context: context,
      position: position,
      items: [
        PopupMenuItem(
          value: 'edit',
          child: Text('Edit note (double-click)'),
        ),
        if (widget.node.commentAnchors.isNotEmpty)
          PopupMenuItem(
            value: 'remove_anchor',
            child: Text('Remove anchor'),
          ),
        PopupMenuItem(
          value: 'duplicate',
          child: Text('Duplicate node (Ctrl+D)'),
        ),
      ],
    ).then((value) {
      if (!context.mounted) return;
      if (value == 'edit') {
        _enterEditMode(
            _label.isEmpty ? _CommentField.text : _CommentField.label);
      } else if (value == 'remove_anchor') {
        _removeAnchor(context);
      } else if (value == 'duplicate') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.duplicateNode(widget.node.id, scopeChain: widget.scopeChain);
      }
    });
  }
}
