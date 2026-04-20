import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/namespace_utils.dart';
import 'package:flutter_cad/structure_designer/factor_into_subnetwork_dialog.dart';

/// Key constants for node widget testing
class NodeWidgetKeys {
  /// Returns a Key for a node widget container by its ID
  static Key nodeWidget(BigInt id) => Key('node_widget_$id');

  /// Returns a Key for a node's visibility toggle button by its ID
  static Key visibilityButton(BigInt id) => Key('node_visibility_$id');

  /// Returns a Key for an input pin by node ID and pin index
  static Key inputPin(BigInt nodeId, int pinIndex) =>
      Key('node_${nodeId}_input_$pinIndex');

  /// Returns a Key for an output pin by node ID and pin index
  static Key outputPin(BigInt nodeId, [int pinIndex = 0]) =>
      Key('node_${nodeId}_output_$pinIndex');

  /// Returns a Key for an output pin visibility button by node ID and pin index
  static Key outputPinVisibility(BigInt nodeId, int pinIndex) =>
      Key('node_${nodeId}_output_vis_$pinIndex');

  /// Returns a Key for the function pin by node ID
  static Key functionPin(BigInt nodeId) => Key('node_${nodeId}_function');
}

// Pin appearance constants
const double PIN_SIZE = 14.0;
const double PIN_BORDER_WIDTH = 5.0;
const double PIN_HIT_AREA_WIDTH = 24.0; // Larger hit area for easier dragging
const double PIN_HIT_AREA_HEIGHT = 22.0;

// Node appearance constants
const Color NODE_BACKGROUND_COLOR = Color(0xFF212121); // Colors.grey[900]
const Color NODE_COLOR_ACTIVE = Color(0xFFD84315); // Active node border & title
const Color NODE_COLOR_SELECTED =
    Color(0xFFE08000); // Selected node border & title
const Color NODE_BORDER_COLOR_NORMAL = Colors.blueAccent;
const Color NODE_BORDER_COLOR_ERROR = Colors.red;
const double NODE_BORDER_WIDTH_ACTIVE = 3.0;
const double NODE_BORDER_WIDTH_SELECTED = 2.0;
const double NODE_BORDER_WIDTH_NORMAL = 2.0;
const double NODE_BORDER_RADIUS = 8.0;
const Color NODE_TITLE_COLOR_NORMAL = Color(0xFF37474F); // Colors.blueGrey[800]
const Color NODE_TITLE_COLOR_RETURN = Color(0xFF0D47A1); // Dark blue
const Color NODE_TITLE_COLOR_PARAMETER = Color(0xFF1B5E20); // Dark green

const double WIRE_GLOW_BLUR_RADIUS = 8.0;
const double WIRE_GLOW_SPREAD_RADIUS = 2.0;

// Alignment tooltip colors — see doc/design_blueprint_alignment.md §6.2
const Color ALIGNMENT_MOTIF_UNALIGNED_COLOR = Color(0xFF6D4C41); // brown
// LatticeUnaligned reuses WIRE_COLOR_SELECTED (0xFFD84315, orange) defined in
// node_network.dart.

class PinViewWidget extends StatelessWidget {
  final String dataType;
  final bool multi;

  /// Input pins may declare an abstract data type (`Atomic`, `StructureBound`,
  /// `Unanchored`); those render as an N-sliced pie of concrete-satisfier
  /// colors. Output pins are always concrete and render single-colored.
  final bool isInput;
  final String? outputString;
  final String? pinName;
  final APIAlignment? alignment;

  const PinViewWidget(
      {super.key,
      required this.dataType,
      required this.multi,
      required this.isInput,
      this.outputString,
      this.pinName,
      this.alignment});

  @override
  Widget build(BuildContext context) {
    final List<Color> sliceColors;
    final String typeLabel;
    if (isInput && isAbstractDataType(dataType)) {
      final concretes = ABSTRACT_TYPE_CONCRETES[dataType]!;
      sliceColors =
          concretes.map(getDataTypeColor).toList(growable: false);
      typeLabel = '$dataType (${concretes.join(' or ')})';
    } else {
      sliceColors = [getDataTypeColor(dataType)];
      typeLabel = dataType;
    }

    final List<InlineSpan> spans = [];
    if (pinName != null && pinName!.isNotEmpty) {
      spans.add(TextSpan(text: '── $pinName ──  $typeLabel'));
    } else {
      spans.add(TextSpan(text: typeLabel));
    }
    if (alignment == APIAlignment.motifUnaligned) {
      spans.add(const TextSpan(
        text: '\nAlignment: motif-unaligned',
        style: TextStyle(color: ALIGNMENT_MOTIF_UNALIGNED_COLOR),
      ));
    } else if (alignment == APIAlignment.latticeUnaligned) {
      spans.add(const TextSpan(
        text: '\nAlignment: lattice-unaligned',
        style: TextStyle(color: WIRE_COLOR_SELECTED),
      ));
    }
    if (outputString != null && outputString!.isNotEmpty) {
      const maxLines = 15;
      const maxChars = 500;
      var preview = outputString!;
      var truncated = false;
      if (preview.length > maxChars) {
        preview = preview.substring(0, maxChars);
        truncated = true;
      }
      final lines = preview.split('\n');
      if (lines.length > maxLines) {
        preview = lines.take(maxLines).join('\n');
        truncated = true;
      }
      if (truncated) {
        preview = '$preview\n...';
      }
      spans.add(TextSpan(text: '\n$preview'));
    }

    final bool unaligned = alignment == APIAlignment.motifUnaligned ||
        alignment == APIAlignment.latticeUnaligned;

    return Tooltip(
      richMessage: TextSpan(children: spans),
      preferBelow: false,
      child: Center(
        child: SizedBox(
          width: PIN_SIZE,
          height: PIN_SIZE,
          child: CustomPaint(
            painter: unaligned
                ? _WarningTrianglePinPainter(color: sliceColors.first)
                : _PinPainter(
                    sliceColors: sliceColors,
                    hollow: multi,
                    borderWidth: PIN_BORDER_WIDTH,
                  ),
          ),
        ),
      ),
    );
  }
}

/// Paints a pin as either a filled/ringed disk (single-color) or a pie-sliced
/// disk/ring (multi-color). Slices start at 12 o'clock and sweep clockwise.
class _PinPainter extends CustomPainter {
  final List<Color> sliceColors;
  final bool hollow;
  final double borderWidth;

  const _PinPainter({
    required this.sliceColors,
    required this.hollow,
    required this.borderWidth,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height / 2);
    final outerRadius = size.width / 2;

    if (hollow) {
      // Match Flutter's BoxDecoration border behavior: the border is drawn
      // inside the shape bounds, so the stroke centerline sits at
      // outerRadius - borderWidth/2.
      final strokeRadius = outerRadius - borderWidth / 2;
      final strokeRect = Rect.fromCircle(center: center, radius: strokeRadius);
      final paint = Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = borderWidth;

      if (sliceColors.length == 1) {
        paint.color = sliceColors.first;
        canvas.drawCircle(center, strokeRadius, paint);
      } else {
        final sweep = 2 * math.pi / sliceColors.length;
        double startAngle = -math.pi / 2;
        for (final color in sliceColors) {
          paint.color = color;
          canvas.drawArc(strokeRect, startAngle, sweep, false, paint);
          startAngle += sweep;
        }
      }
    } else {
      final paint = Paint()..style = PaintingStyle.fill;

      if (sliceColors.length == 1) {
        paint.color = sliceColors.first;
        canvas.drawCircle(center, outerRadius, paint);
      } else {
        final rect = Rect.fromCircle(center: center, radius: outerRadius);
        final sweep = 2 * math.pi / sliceColors.length;
        double startAngle = -math.pi / 2;
        for (final color in sliceColors) {
          paint.color = color;
          canvas.drawArc(rect, startAngle, sweep, true, paint);
          startAngle += sweep;
        }
      }
    }
  }

  @override
  bool shouldRepaint(covariant _PinPainter oldDelegate) {
    return !listEquals(oldDelegate.sliceColors, sliceColors) ||
        oldDelegate.hollow != hollow ||
        oldDelegate.borderWidth != borderWidth;
  }
}

/// Paints an output pin as a filled warning triangle with an exclamation mark
/// for Blueprint/Crystal pins whose alignment is MotifUnaligned or
/// LatticeUnaligned. The triangle fits within the pin's circle bounding box so
/// wire-endpoint math and hit testing stay unchanged. See
/// `doc/design_blueprint_alignment.md` §6.3.
class _WarningTrianglePinPainter extends CustomPainter {
  final Color color;

  const _WarningTrianglePinPainter({required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    final double w = size.width;
    final double h = size.height;
    final path = Path()
      ..moveTo(w / 2, 0)
      ..lineTo(w, h)
      ..lineTo(0, h)
      ..close();

    final fillPaint = Paint()
      ..style = PaintingStyle.fill
      ..color = color;
    canvas.drawPath(path, fillPaint);

    // Exclamation mark: a short black bar above a small black dot, centered
    // in the lower half of the triangle where it has room.
    final barPaint = Paint()
      ..style = PaintingStyle.fill
      ..color = Colors.black;
    final double barWidth = w * 0.14;
    final double barTop = h * 0.32;
    final double barBottom = h * 0.68;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTRB(w / 2 - barWidth / 2, barTop, w / 2 + barWidth / 2,
            barBottom),
        Radius.circular(barWidth / 2),
      ),
      barPaint,
    );
    canvas.drawCircle(Offset(w / 2, h * 0.83), barWidth * 0.75, barPaint);
  }

  @override
  bool shouldRepaint(covariant _WarningTrianglePinPainter oldDelegate) {
    return oldDelegate.color != color;
  }
}

class PinWidget extends StatelessWidget {
  final PinReference pinReference;
  final bool multi;
  final String? outputString;
  final String? pinName;
  final APIAlignment? alignment;
  PinWidget(
      {required this.pinReference,
      required this.multi,
      this.outputString,
      this.pinName,
      this.alignment})
      : super(
            key: ValueKey(pinReference.pinIndex +
                ((pinReference.pinType == PinType.output) ? 1000 : 0)));

  RenderBox? _findNodeNetworkRenderBox(BuildContext context) {
    RenderBox? result;
    context.visitAncestorElements((element) {
      if (element.widget is NodeNetwork) {
        result = element.renderObject as RenderBox?;
        return false; // Stop visiting
      }
      return true; // Continue visiting
    });
    return result;
  }

  @override
  Widget build(BuildContext context) {
    final bool isInput = pinReference.pinType == PinType.input;
    return SizedBox(
      width: PIN_HIT_AREA_WIDTH,
      height: PIN_HIT_AREA_HEIGHT,
      child: DragTarget<PinReference>(
        builder: (context, candidateData, rejectedData) {
          return Draggable<PinReference>(
            data: pinReference,
            feedback: SizedBox.shrink(),
            childWhenDragging: SizedBox(
              width: PIN_HIT_AREA_WIDTH,
              height: PIN_HIT_AREA_HEIGHT,
              child: Center(
                child: PinViewWidget(
                    dataType: pinReference.dataType,
                    multi: multi,
                    isInput: isInput,
                    outputString: outputString,
                    pinName: pinName,
                    alignment: alignment),
              ),
            ),
            child: SizedBox(
              width: PIN_HIT_AREA_WIDTH,
              height: PIN_HIT_AREA_HEIGHT,
              child: Center(
                child: PinViewWidget(
                    dataType: pinReference.dataType,
                    multi: multi,
                    isInput: isInput,
                    outputString: outputString,
                    pinName: pinName,
                    alignment: alignment),
              ),
            ),
            onDragUpdate: (details) {
              final nodeNetworkBox = _findNodeNetworkRenderBox(context);
              if (nodeNetworkBox != null) {
                final position =
                    nodeNetworkBox.globalToLocal(details.globalPosition);
                Provider.of<StructureDesignerModel>(context, listen: false)
                    .dragWire(pinReference, position);
              }
            },
            onDragEnd: (details) {
              final model =
                  Provider.of<StructureDesignerModel>(context, listen: false);
              // Store the drag info before clearing
              final dragInfo = model.draggedWire;
              if (dragInfo != null) {
                // Notify parent to handle the drop (will show popup if in empty space)
                model.handleWireDropInEmptySpace(
                  dragInfo.startPin,
                  dragInfo.wireEndPosition,
                );
              }
              model.cancelDragWire();
            },
          );
        },
        onWillAcceptWithDetails: (details) {
          return Provider.of<StructureDesignerModel>(context, listen: false)
              .canConnectPins(details.data, pinReference);
        },
        onAcceptWithDetails: (details) {
          //print("Connected pin ${details.data} to pin $pinReference");
          Provider.of<StructureDesignerModel>(context, listen: false)
              .connectPins(details.data, pinReference);
        },
      ),
    );
  }
}

/// Widget representing a single draggable node.
class NodeWidget extends StatelessWidget {
  final NodeView node;
  final Offset panOffset;
  final ZoomLevel zoomLevel;

  NodeWidget(
      {required this.node, required this.panOffset, required this.zoomLevel})
      : super(key: NodeWidgetKeys.nodeWidget(node.id));

  @override
  Widget build(BuildContext context) {
    // Choose rendering mode based on zoom level
    final Widget nodeContent = zoomLevel == ZoomLevel.normal
        ? _buildNormalNodeContent(context)
        : _buildZoomedOutNodeContent(context);

    // Get node size for current zoom level
    final nodeSize = getNodeSize(node, zoomLevel);

    // Create container with node appearance
    // For normal zoom, don't set explicit height - let content determine it (for subtitle)
    // For zoomed-out, set explicit height for fixed compact size
    Widget nodeWidget = Container(
      width: nodeSize.width,
      height: zoomLevel == ZoomLevel.normal ? null : nodeSize.height,
      decoration: _getNodeDecoration(),
      child: nodeContent,
    );

    // Add tooltip for nodes with errors
    if (node.error != null && node.error!.isNotEmpty) {
      nodeWidget = _wrapWithErrorTooltip(nodeWidget);
    }

    // Position the node using central coordinate transformation
    final scale = getZoomScale(zoomLevel);
    final screenPos = logicalToScreen(
        Offset(node.position.x, node.position.y), panOffset, scale);
    return Positioned(
      left: screenPos.dx,
      top: screenPos.dy,
      child: nodeWidget,
    );
  }

  /// Builds the zoomed-out compact node showing only title
  Widget _buildZoomedOutNodeContent(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque, // Make entire node area interactive
      onTapDown: (details) => _handleNodeTap(context),
      onPanStart: (details) {
        _handleNodeTap(context);
        _handleNodeDragStart(context);
      },
      onPanUpdate: (details) => _handleNodeDrag(context, details),
      onPanEnd: (details) => _handleNodeDragEnd(context),
      onSecondaryTapDown: (details) => _handleContextMenu(context, details),
      child: Center(
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 4),
          child: Text(
            getSimpleName(node.nodeTypeName),
            style: TextStyle(
              color: Colors.white,
              fontWeight: FontWeight.bold,
              fontSize: getNodeTitleFontSize(zoomLevel),
            ),
            overflow: TextOverflow.ellipsis,
            maxLines: 3,
            textAlign: TextAlign.center,
          ),
        ),
      ),
    );
  }

  /// Builds the normal detailed node with all pins and controls
  Widget _buildNormalNodeContent(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // Title Bar
        GestureDetector(
          onTapDown: (details) => _handleNodeTap(context),
          onPanStart: (details) {
            _handleNodeTap(context);
            _handleNodeDragStart(context);
          },
          onPanUpdate: (details) => _handleNodeDrag(context, details),
          onPanEnd: (details) => _handleNodeDragEnd(context),
          onSecondaryTapDown: (details) => _handleContextMenu(context, details),
          child: Container(
            padding:
                const EdgeInsets.only(top: 4, bottom: 4, left: 8, right: 2),
            decoration: BoxDecoration(
              color: _getSpecialNodeColor() ?? _getTitleColor(),
              borderRadius: BorderRadius.vertical(
                  top: Radius.circular(NODE_BORDER_RADIUS - 2)),
            ),
            child: Row(
              children: [
                Expanded(
                  child: Tooltip(
                    message: node.nodeTypeName,
                    waitDuration: const Duration(milliseconds: 500),
                    preferBelow: false,
                    child: Text(
                      getSimpleName(node.nodeTypeName),
                      style: const TextStyle(
                        color: Colors.white,
                        fontWeight: FontWeight.bold,
                        fontSize: 14,
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                ),
                const SizedBox(width: 4),
                // Function pin
                PinWidget(
                  pinReference: PinReference(
                      node.id, PinType.output, -1, node.functionType),
                  multi: false,
                ),
              ],
            ),
          ),
        ),
        // Main Body
        Padding(
          padding: const EdgeInsets.all(2),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Left Side (Inputs)
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: node.inputPins
                      .asMap()
                      .entries
                      .map((entry) => _buildInputPin(
                          entry.value.name,
                          PinReference(node.id, PinType.input, entry.key,
                              entry.value.dataType),
                          entry.value.multi))
                      .toList(),
                ),
              ),
              // Right Side (Outputs)
              Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: node.outputPins
                    .map((pin) => _buildOutputPin(context, pin))
                    .toList(),
              ),
            ],
          ),
        ),
        // Subtitle (if present)
        if (node.subtitle != null && node.subtitle!.isNotEmpty)
          Container(
            width: double.infinity,
            padding: const EdgeInsets.only(left: 8, right: 8, bottom: 4),
            child: Tooltip(
              message: node.subtitle!,
              preferBelow: true,
              child: Text(
                node.subtitle!,
                style: const TextStyle(
                  color: Colors.white70,
                  fontSize: 12,
                  fontStyle: FontStyle.italic,
                ),
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
              ),
            ),
          ),
      ],
    );
  }

  /// Creates a labeled input pin.
  Widget _buildInputPin(String label, PinReference pinReference, bool multi) {
    return Row(
      children: [
        PinWidget(pinReference: pinReference, multi: multi),
        SizedBox(width: 2),
        Expanded(
          child: Text(
            label,
            style: TextStyle(color: Colors.white, fontSize: 14),
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ],
    );
  }

  /// Creates an output pin row with eye icon and pin dot.
  /// Pin name is shown in the hover tooltip, not inline.
  Widget _buildOutputPin(BuildContext context, OutputPinView pin) {
    final bool isPinDisplayed = node.displayedPins.contains(pin.index);

    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        // Eye icon for pin display toggle
        GestureDetector(
          key: NodeWidgetKeys.outputPinVisibility(node.id, pin.index),
          onTap: () {
            final model =
                Provider.of<StructureDesignerModel>(context, listen: false);
            model.toggleOutputPinDisplay(node.id, pin.index);
          },
          child: Icon(
            isPinDisplayed ? Icons.visibility : Icons.visibility_off,
            color: Colors.white70,
            size: 16,
          ),
        ),
        const SizedBox(width: 2),
        PinWidget(
          pinReference: PinReference(
              node.id, PinType.output, pin.index, pin.effectiveDataType),
          multi: false,
          outputString: pin.index < node.outputPinStrings.length
              ? node.outputPinStrings[pin.index]
              : null,
          pinName: node.outputPins.length > 1 ? pin.name : null,
          alignment: pin.alignment,
        ),
      ],
    );
  }

  /// Returns the special color for return/parameter nodes, or null for regular nodes
  Color? _getSpecialNodeColor() {
    if (node.returnNode) {
      return NODE_TITLE_COLOR_RETURN;
    } else if (node.nodeTypeName == "parameter") {
      return NODE_TITLE_COLOR_PARAMETER;
    }
    return null;
  }

  /// Returns the title bar color based on selection state
  Color _getTitleColor() {
    if (node.active) {
      return NODE_COLOR_ACTIVE;
    } else if (node.selected) {
      return NODE_COLOR_SELECTED;
    }
    return NODE_TITLE_COLOR_NORMAL;
  }

  /// Returns the decoration for the node container
  BoxDecoration _getNodeDecoration() {
    // Use colored background for special nodes in zoomed-out modes
    final backgroundColor = (zoomLevel != ZoomLevel.normal)
        ? (_getSpecialNodeColor() ?? NODE_BACKGROUND_COLOR)
        : NODE_BACKGROUND_COLOR;

    // Determine border color and width based on state:
    // Priority: error > active > selected > normal
    Color borderColor;
    double borderWidth;
    List<BoxShadow>? boxShadow;

    if (node.error != null) {
      borderColor = NODE_BORDER_COLOR_ERROR;
      borderWidth = NODE_BORDER_WIDTH_NORMAL;
      boxShadow = [
        BoxShadow(
            color: NODE_BORDER_COLOR_ERROR.withValues(alpha: WIRE_GLOW_OPACITY),
            blurRadius: WIRE_GLOW_BLUR_RADIUS,
            spreadRadius: WIRE_GLOW_SPREAD_RADIUS)
      ];
    } else if (node.active) {
      // Active node: thicker border, full glow
      borderColor = NODE_COLOR_ACTIVE;
      borderWidth = NODE_BORDER_WIDTH_ACTIVE;
      boxShadow = [
        BoxShadow(
            color: NODE_COLOR_ACTIVE.withValues(alpha: WIRE_GLOW_OPACITY),
            blurRadius: WIRE_GLOW_BLUR_RADIUS,
            spreadRadius: WIRE_GLOW_SPREAD_RADIUS)
      ];
    } else if (node.selected) {
      // Selected but not active
      borderColor = NODE_COLOR_SELECTED;
      borderWidth = NODE_BORDER_WIDTH_SELECTED;
      boxShadow = [
        BoxShadow(
            color:
                NODE_COLOR_SELECTED.withValues(alpha: WIRE_GLOW_OPACITY * 0.5),
            blurRadius: WIRE_GLOW_BLUR_RADIUS * 0.7,
            spreadRadius: WIRE_GLOW_SPREAD_RADIUS * 0.5)
      ];
    } else {
      // Normal (not selected)
      borderColor = NODE_BORDER_COLOR_NORMAL;
      borderWidth = NODE_BORDER_WIDTH_NORMAL;
      boxShadow = null;
    }

    return BoxDecoration(
      color: backgroundColor,
      borderRadius: BorderRadius.circular(NODE_BORDER_RADIUS),
      border: Border.all(color: borderColor, width: borderWidth),
      boxShadow: boxShadow,
    );
  }

  /// Wraps a widget with error tooltip
  Widget _wrapWithErrorTooltip(Widget child) {
    return Tooltip(
      message: node.error!,
      textStyle: const TextStyle(fontSize: 14, color: Colors.white),
      decoration: BoxDecoration(
        color: Colors.red.shade700,
        borderRadius: BorderRadius.circular(4),
      ),
      waitDuration: const Duration(milliseconds: 500),
      showDuration: const Duration(seconds: 5),
      padding: const EdgeInsets.symmetric(vertical: 8, horizontal: 12),
      preferBelow: true,
      verticalOffset: 35.0,
      child: child,
    );
  }

  /// Handles node tap for selection with modifier key support
  void _handleNodeTap(BuildContext context) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    if (HardwareKeyboard.instance.isControlPressed) {
      // Ctrl+click: toggle selection
      model.toggleNodeSelection(node.id);
    } else if (HardwareKeyboard.instance.isShiftPressed) {
      // Shift+click: add to selection
      model.addNodeToSelection(node.id);
    } else if (node.selected && !node.active) {
      // Simple click on selected (but not active) node: make it active
      model.addNodeToSelection(node.id);
    } else {
      // Normal click: select only this node
      model.setSelectedNode(node.id);
    }
  }

  /// Handles the start of a node drag - captures positions for undo coalescing
  void _handleNodeDragStart(BuildContext context) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);
    model.beginMoveNodes();
  }

  /// Handles node drag for positioning - moves all selected nodes if this node is selected
  void _handleNodeDrag(BuildContext context, DragUpdateDetails details) {
    // Convert screen-space delta to logical-space delta
    final scale = getZoomScale(zoomLevel);
    final logicalDelta = details.delta / scale;
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    if (node.selected) {
      // This node is part of selection - drag all selected nodes
      model.dragSelectedNodes(logicalDelta);
    } else {
      // Dragging an unselected node - just drag this one
      model.dragNodePosition(node.id, logicalDelta);
    }
  }

  /// Handles end of node drag - commits position of all moved nodes
  void _handleNodeDragEnd(BuildContext context) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    if (node.selected) {
      // Commit positions of all selected nodes
      model.updateSelectedNodesPosition();
    } else {
      // Only commit position of this single node
      model.updateNodePosition(node.id);
    }
    // End the move group for undo coalescing
    model.endMoveNodes();
  }

  /// Handles context menu for node
  void _handleContextMenu(BuildContext context, TapDownDetails details) {
    final model = Provider.of<StructureDesignerModel>(context, listen: false);

    // Only change selection if this node isn't already selected
    // This preserves multi-selection for operations like "Factor out to Subnetwork"
    if (!node.selected) {
      model.setSelectedNode(node.id);
    }

    final RenderBox overlay =
        Overlay.of(context).context.findRenderObject() as RenderBox;
    final RelativeRect position = RelativeRect.fromRect(
      Rect.fromPoints(
        details.globalPosition,
        details.globalPosition,
      ),
      Offset.zero & overlay.size,
    );

    final bool isCustomNode = isCustomNodeType(nodeTypeName: node.nodeTypeName);

    // Check if the selection can be factored into a subnetwork
    final factorInfo = getFactorSelectionInfo();
    final bool canFactor = factorInfo.canFactor;

    showMenu(
      context: context,
      position: position,
      items: [
        if (isCustomNode)
          PopupMenuItem(
            value: 'go_to_definition',
            child: Text('Go to Definition'),
          ),
        PopupMenuItem(
          value: 'return',
          child: Text(
              node.returnNode ? 'Unset as return node' : 'Set as return node'),
        ),
        PopupMenuItem(
          value: 'duplicate',
          child: Text('Duplicate node (Ctrl+D)'),
        ),
        PopupMenuItem(
          value: 'copy',
          child: Text('Copy (Ctrl+C)'),
        ),
        PopupMenuItem(
          value: 'cut',
          child: Text('Cut (Ctrl+X)'),
        ),
        if (canFactor)
          PopupMenuItem(
            value: 'factor_into_subnetwork',
            child: Text('Factor out to Subnetwork...'),
          ),
      ],
    ).then((value) {
      if (!context.mounted) return;
      if (value == 'go_to_definition') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.setActiveNodeNetwork(node.nodeTypeName);
      } else if (value == 'return') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        if (node.returnNode) {
          model.setReturnNodeId(null);
        } else {
          model.setReturnNodeId(node.id);
        }
      } else if (value == 'duplicate') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.duplicateNode(node.id);
      } else if (value == 'copy') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.copySelection();
      } else if (value == 'cut') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        model.cutSelection();
      } else if (value == 'factor_into_subnetwork') {
        final model =
            Provider.of<StructureDesignerModel>(context, listen: false);
        showFactorIntoSubnetworkDialog(context, model);
      }
    });
  }
}
