import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';

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

  const CommentNodeWidget({
    super.key,
    required this.node,
    required this.panOffset,
    required this.zoomLevel,
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

  @override
  Widget build(BuildContext context) {
    final scale = getZoomScale(widget.zoomLevel);
    final screenPos = logicalToScreen(
      Offset(widget.node.position.x, widget.node.position.y),
      widget.panOffset,
      scale,
    );

    final scaledWidth = _width * scale;
    final scaledHeight = _height * scale;

    final fontSize = 12.0 * math.sqrt(scale);
    final headerFontSize = 11.0 * math.sqrt(scale);

    return Positioned(
      left: screenPos.dx,
      top: screenPos.dy,
      child: GestureDetector(
        onTapDown: (_) => _handleTap(context),
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
                      color: COMMENT_SELECTED_BORDER_COLOR.withValues(alpha: 0.3),
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
                child: MouseRegion(
                  cursor: SystemMouseCursors.resizeDownRight,
                  child: GestureDetector(
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
            ],
          ),
        ),
      ),
    );
  }

  void _handleTap(BuildContext context) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    if (HardwareKeyboard.instance.isControlPressed) {
      model.toggleNodeSelection(widget.node.id);
    } else if (HardwareKeyboard.instance.isShiftPressed) {
      model.addNodeToSelection(widget.node.id);
    } else if (widget.node.selected && !widget.node.active) {
      model.addNodeToSelection(widget.node.id);
    } else {
      model.setSelectedNode(widget.node.id);
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
      model.dragSelectedNodes(logicalDelta);
    } else {
      model.dragNodePosition(widget.node.id, logicalDelta);
    }
  }

  void _handlePanEnd(BuildContext context) {
    if (_isResizing) return;

    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    if (widget.node.selected) {
      model.updateSelectedNodesPosition();
    } else {
      model.updateNodePosition(widget.node.id);
    }
  }

  void _startResize(DragStartDetails details) {
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
    final newHeight =
        (_resizeStartHeight + delta.dy / scale).clamp(COMMENT_MIN_HEIGHT, 1000.0);

    sd_api.resizeCommentNode(
      nodeId: widget.node.id,
      width: newWidth,
      height: newHeight,
    );

    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    model.refreshFromKernel();
  }

  void _endResize(BuildContext context) {
    setState(() {
      _isResizing = false;
    });
  }
}
