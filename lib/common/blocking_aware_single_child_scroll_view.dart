import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';

/// A [SingleChildScrollView] that is aware of [MouseWheelBlockService]
/// and will disable scrolling when the service indicates it should be blocked.
///
/// This widget is a drop-in replacement for [SingleChildScrollView].
class BlockingAwareSingleChildScrollView extends StatelessWidget {
  /// The axis along which the scroll view scrolls.
  final Axis scrollDirection;

  /// Whether the scroll view scrolls in the reading direction.
  final bool reverse;

  /// The amount of space by which to inset the child.
  final EdgeInsetsGeometry? padding;

  /// Whether the scroll view should adjust for system intrusions.
  final bool? primary;

  /// How the scroll view should respond to user input.
  ///
  /// This will be overridden with [NeverScrollableScrollPhysics] when 
  /// scroll blocking is active.
  final ScrollPhysics? physics;

  /// The widget that scrolls.
  final Widget child;

  /// Creates a scroll view that only scrolls when [MouseWheelBlockService] allows it.
  const BlockingAwareSingleChildScrollView({
    super.key,
    this.scrollDirection = Axis.vertical,
    this.reverse = false,
    this.padding,
    this.primary,
    this.physics,
    required this.child,
  });

  @override
  Widget build(BuildContext context) {
    // Watch the MouseWheelBlockService to rebuild when isBlocked changes
    final blockService = context.watch<MouseWheelBlockService>();
    
    // Use NeverScrollableScrollPhysics when blocked, otherwise use provided physics
    final ScrollPhysics effectivePhysics = blockService.isBlocked 
        ? const NeverScrollableScrollPhysics() 
        : physics ?? const ClampingScrollPhysics();

    return SingleChildScrollView(
      scrollDirection: scrollDirection,
      reverse: reverse,
      padding: padding,
      primary: primary,
      physics: effectivePhysics,
      child: child,
    );
  }
}
