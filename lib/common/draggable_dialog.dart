import 'package:flutter/material.dart';

/// A draggable dialog that can be moved around the screen.
/// This implements a custom dialog that can be positioned by the user.
class DraggableDialog extends StatefulWidget {
  final Widget child;
  final double width;
  final double? height;
  final Color backgroundColor;

  const DraggableDialog({
    super.key,
    required this.child,
    required this.width,
    this.height,
    this.backgroundColor = Colors.white,
  });

  @override
  State<DraggableDialog> createState() => _DraggableDialogState();
}

class _DraggableDialogState extends State<DraggableDialog> {
  Offset? _position;
  bool _isDragging = false;

  @override
  void initState() {
    super.initState();
    // _position will be initialized in build based on screen size
  }
  
  @override
  Widget build(BuildContext context) {
    // Calculate the initial position to center the dialog
    final screenSize = MediaQuery.of(context).size;
    final initialPosition = Offset(
      (screenSize.width - widget.width) / 2,
      (screenSize.height - (widget.height ?? 300)) / 2,
    );
    
    // Initialize position if not already set
    _position ??= initialPosition;

    return Stack(
      children: [
        // Invisible full-screen barrier for detecting clicks outside
        Positioned.fill(
          child: GestureDetector(
            onTap: () => Navigator.of(context).pop(),
            behavior: HitTestBehavior.opaque,
            child: Container(color: Colors.transparent),
          ),
        ),
        
        // The draggable dialog
        AnimatedPositioned(
          duration: _isDragging 
              ? Duration.zero 
              : const Duration(milliseconds: 100),
          left: _position!.dx,
          top: _position!.dy,
          child: GestureDetector(
            onPanStart: (details) {
              setState(() {
                _isDragging = true;
              });
            },
            onPanUpdate: (details) {
              setState(() {
                _position = Offset(
                  _position!.dx + details.delta.dx,
                  _position!.dy + details.delta.dy,
                );
              });
            },
            onPanEnd: (_) {
              setState(() {
                _isDragging = false;
              });
            },
            child: Material(
              elevation: 8.0,
              borderRadius: BorderRadius.circular(8.0),
              child: Container(
                width: widget.width,
                height: widget.height, // null = intrinsic height
                decoration: BoxDecoration(
                  color: widget.backgroundColor,
                  borderRadius: BorderRadius.circular(8.0),
                ),
                child: widget.child,
              ),
            ),
          ),
        ),
      ],
    );
  }
}
