import 'dart:math' as math;
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/node_network/scope_resolver.dart';

const double COMMENT_MIN_WIDTH = 100.0;
const double COMMENT_MIN_HEIGHT = 60.0;
const double COMMENT_RESIZE_HANDLE_SIZE = 12.0;
const Color COMMENT_BACKGROUND_COLOR = Color(0xCCFFF9C4);
const Color COMMENT_BORDER_COLOR = Color(0xFF9E9E9E);
const Color COMMENT_HEADER_COLOR = Color(0xFFFFEB3B);
const Color COMMENT_SELECTED_BORDER_COLOR = Color(0xFFE08000);

class CommentNodeWidget extends StatefulWidget {
  final NodeView node;
  final Offset panOffset;
  final ZoomLevel zoomLevel;

  /// Root network view, used to build a per-frame [ScopeResolver] so the
  /// comment is positioned in its body-local frame when it lives inside an
  /// HOF/closure body (see [scopeChain]).
  final NodeNetworkView rootView;

  /// Scope chain of the body this comment lives in (`const []` at top level).
  /// Forwarded to every selection/drag/edit API call so the right node is
  /// addressed across scopes.
  final List<BigInt> scopeChain;

  const CommentNodeWidget({
    super.key,
    required this.node,
    required this.panOffset,
    required this.zoomLevel,
    required this.rootView,
    this.scopeChain = const [],
  });

  @override
  State<CommentNodeWidget> createState() => _CommentNodeWidgetState();
}

class _CommentNodeWidgetState extends State<CommentNodeWidget> {
  bool _isResizing = false;
  double _resizeStartWidth = 0;
  double _resizeStartHeight = 0;
  Offset _resizeStartPosition = Offset.zero;

  double get _width => widget.node.commentWidth ?? 200.0;
  double get _height => widget.node.commentHeight ?? 100.0;
  String get _label => widget.node.commentLabel ?? '';
  String get _text => widget.node.commentText ?? '';

  /// Byte-encoded scope path for FRB API calls that address this comment node.
  Uint64List get _scopePath => scopeChainToBytes(widget.scopeChain);

  @override
  Widget build(BuildContext context) {
    final scale = getZoomScale(widget.zoomLevel);
    // Position via the scope resolver: the comment's stored position lives in
    // its body-local frame, which the resolver maps to screen. For the
    // top-level scope (empty chain) this is identical to `logicalToScreen`.
    final resolver = ScopeResolver(
      root: widget.rootView,
      panOffset: widget.panOffset,
      scale: scale,
      zoomLevel: widget.zoomLevel,
    );
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
        onTapDown: (_) => _handleTap(context),
        onSecondaryTapDown: (details) => _handleContextMenu(context, details),
        onPanStart: (details) => _handlePanStart(context, details),
        onPanUpdate: (details) => _handlePanUpdate(context, details),
        onPanEnd: (details) => _handlePanEnd(context),
        child: Container(
          width: scaledWidth,
          height: scaledHeight,
          decoration: BoxDecoration(
            color: COMMENT_BACKGROUND_COLOR,
            border: Border.all(
              color: widget.node.selected
                  ? COMMENT_SELECTED_BORDER_COLOR
                  : COMMENT_BORDER_COLOR,
              width: widget.node.selected ? 2.0 : 1.0,
              style: BorderStyle.solid,
            ),
            borderRadius: BorderRadius.circular(4.0),
            boxShadow: widget.node.selected
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
              Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (_label.isNotEmpty)
                    Container(
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
                      child: Text(
                        _label,
                        style: TextStyle(
                          fontSize: headerFontSize,
                          fontWeight: FontWeight.bold,
                          color: Colors.black87,
                        ),
                        overflow: TextOverflow.ellipsis,
                        maxLines: 1,
                      ),
                    ),
                  Expanded(
                    child: Padding(
                      padding: EdgeInsets.all(6.0 * scale),
                      child: SingleChildScrollView(
                        child: Text(
                          _text,
                          style: TextStyle(
                            fontSize: fontSize,
                            color: Colors.black87,
                          ),
                        ),
                      ),
                    ),
                  ),
                ],
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

  void _handleTap(BuildContext context) {
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
      _handleTap(context);
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
    sd_api.beginEditCommentNode(
        scopePath: _scopePath, nodeId: widget.node.id);
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
    sd_api.endEditCommentNode();
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
          value: 'duplicate',
          child: Text('Duplicate node (Ctrl+D)'),
        ),
      ],
    ).then((value) {
      if (!context.mounted) return;
      if (value == 'duplicate') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.duplicateNode(widget.node.id, scopeChain: widget.scopeChain);
      }
    });
  }
}
