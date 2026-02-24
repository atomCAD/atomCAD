import 'dart:math';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/cad_viewport.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/edit_atom_api.dart'
    as edit_atom_api;
import 'package:flutter_cad/src/rust/api/structure_designer/atom_edit_api.dart'
    as atom_edit_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as structure_designer_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';

/// Delegate that handles primary mouse button events for the atom_edit Default
/// tool. Forwards pointer down/move/up to the Rust state machine.
class _AtomEditDefaultDelegate implements PrimaryPointerDelegate {
  final _StructureDesignerViewportState _viewport;
  SelectModifier? _storedModifier;

  _AtomEditDefaultDelegate(this._viewport);

  @override
  bool onPrimaryDown(Offset pos) {
    final ray = _viewport.getRayFromPointerPos(pos);
    _storedModifier = getSelectModifierFromKeyboard();

    final result = atom_edit_api.defaultToolPointerDown(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
      selectModifier: _storedModifier!,
    );

    if (result.kind == PointerDownResultKind.gadgetHit) {
      // Hand off to the EXISTING gadget system. Consume the down event
      // (preventing startPrimaryDrag from double-starting), but return false
      // on move/up so base class drives the gadget drag.
      _viewport.delegateStartGadgetDrag(result.gadgetHandleIndex, pos);
      return true;
    }

    // PendingAtom, PendingBond, or PendingMarquee — delegate owns the interaction
    return true;
  }

  @override
  bool onPrimaryMove(Offset pos) {
    if (_viewport.isGadgetDragging) return false;

    final ray = _viewport.getRayFromPointerPos(pos);
    final result = atom_edit_api.defaultToolPointerMove(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
      viewportWidth: _viewport.viewportWidth,
      viewportHeight: _viewport.viewportHeight,
    );

    if (result.kind == PointerMoveResultKind.marqueeUpdated) {
      _viewport._setMarqueeRect(Rect.fromLTWH(
        result.marqueeRectX,
        result.marqueeRectY,
        result.marqueeRectW,
        result.marqueeRectH,
      ));
      _viewport.renderingNeeded();
    } else if (result.kind == PointerMoveResultKind.dragging) {
      _viewport.renderingNeeded();
    }
    return true;
  }

  @override
  bool onPrimaryUp(Offset pos) {
    if (_viewport.isGadgetDragging) return false;

    final ray = _viewport.getRayFromPointerPos(pos);
    atom_edit_api.defaultToolPointerUp(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
      selectModifier: _storedModifier ?? SelectModifier.replace,
      viewportWidth: _viewport.viewportWidth,
      viewportHeight: _viewport.viewportHeight,
    );

    _viewport._setMarqueeRect(null);
    _viewport.refreshFromKernel();
    _viewport.renderingNeeded();
    return true;
  }

  @override
  void onPrimaryCancel() {
    atom_edit_api.defaultToolPointerCancel();
    _viewport._setMarqueeRect(null);
    _viewport.refreshFromKernel();
    _viewport.renderingNeeded();
  }
}

/// Delegate that handles primary mouse button events for the atom_edit AddBond
/// tool. Implements drag-to-bond interaction: pointer down on atom, drag to
/// target atom, release to create bond.
class _AtomEditAddBondDelegate implements PrimaryPointerDelegate {
  final _StructureDesignerViewportState _viewport;

  _AtomEditAddBondDelegate(this._viewport);

  @override
  bool onPrimaryDown(Offset pos) {
    final ray = _viewport.getRayFromPointerPos(pos);
    final hit = atom_edit_api.addBondPointerDown(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );
    if (hit) {
      _viewport.renderingNeeded();
    }
    return true;
  }

  @override
  bool onPrimaryMove(Offset pos) {
    final ray = _viewport.getRayFromPointerPos(pos);
    final result = atom_edit_api.addBondPointerMove(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );
    _viewport._setAddBondPreview(result);
    return true;
  }

  @override
  bool onPrimaryUp(Offset pos) {
    final ray = _viewport.getRayFromPointerPos(pos);
    atom_edit_api.addBondPointerUp(
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );
    _viewport._setAddBondPreview(null);
    _viewport.refreshFromKernel();
    _viewport.renderingNeeded();
    return true;
  }

  @override
  void onPrimaryCancel() {
    atom_edit_api.addBondPointerCancel();
    _viewport._setAddBondPreview(null);
    _viewport.refreshFromKernel();
    _viewport.renderingNeeded();
  }
}

/// Custom painter that draws the marquee selection rectangle.
class MarqueePainter extends CustomPainter {
  final Rect rect;
  MarqueePainter({required this.rect});

  @override
  void paint(Canvas canvas, Size size) {
    final fillPaint = Paint()
      ..color = const Color(0x264FC3F7)
      ..style = PaintingStyle.fill;
    canvas.drawRect(rect, fillPaint);

    final borderPaint = Paint()
      ..color = const Color(0xFF4FC3F7)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.0;
    canvas.drawRect(rect, borderPaint);
  }

  @override
  bool shouldRepaint(MarqueePainter oldDelegate) => rect != oldDelegate.rect;
}

/// Custom painter that draws the rubber-band preview line during AddBond drag.
class AddBondPreviewPainter extends CustomPainter {
  final Offset startPos;
  final Offset endPos;
  final bool snapped;
  final int bondOrder;

  AddBondPreviewPainter({
    required this.startPos,
    required this.endPos,
    required this.snapped,
    required this.bondOrder,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final Color lineColor;
    final bool useDashed;
    final double lineWidth;

    // Bond order visual styling — colors match 3D tessellation
    // (atomic_tessellator.rs get_bond_color_inline)
    switch (bondOrder) {
      case 5: // Aromatic — amber
        lineColor = Colors.amber;
        useDashed = !snapped;
        lineWidth = 1.5;
      case 6: // Dative — teal
        lineColor = Colors.teal;
        useDashed = !snapped;
        lineWidth = 1.5;
      case 7: // Metallic — steel blue
        lineColor = Colors.blueGrey;
        useDashed = !snapped;
        lineWidth = 2.5;
      default:
        lineColor = snapped
            ? const Color(0xFF4FC3F7)
            : const Color(0xFF4FC3F7).withValues(alpha: 0.7);
        useDashed = !snapped;
        lineWidth = 1.5;
    }

    final paint = Paint()
      ..color = lineColor
      ..strokeWidth = lineWidth
      ..style = PaintingStyle.stroke;

    if (useDashed) {
      // Draw dashed line
      final dx = endPos.dx - startPos.dx;
      final dy = endPos.dy - startPos.dy;
      final length = sqrt(dx * dx + dy * dy);
      if (length < 1.0) return;
      final nx = dx / length;
      final ny = dy / length;
      const dashLen = 6.0;
      const gapLen = 4.0;
      var d = 0.0;
      while (d < length) {
        final segEnd = min(d + dashLen, length);
        canvas.drawLine(
          Offset(startPos.dx + nx * d, startPos.dy + ny * d),
          Offset(startPos.dx + nx * segEnd, startPos.dy + ny * segEnd),
          paint,
        );
        d += dashLen + gapLen;
      }
    } else {
      // Draw solid line(s) based on bond order
      final dx = endPos.dx - startPos.dx;
      final dy = endPos.dy - startPos.dy;
      final length = sqrt(dx * dx + dy * dy);
      if (length < 1.0) return;

      // Perpendicular direction for parallel line offsets
      final px = -dy / length;
      final py = dx / length;

      final lineCount = bondOrder <= 4 ? bondOrder : 1;
      const spacing = 3.0;
      final totalWidth = (lineCount - 1) * spacing;

      for (int i = 0; i < lineCount; i++) {
        final offset = -totalWidth / 2 + i * spacing;
        canvas.drawLine(
          Offset(startPos.dx + px * offset, startPos.dy + py * offset),
          Offset(endPos.dx + px * offset, endPos.dy + py * offset),
          paint,
        );
      }

      // Dative arrow head
      if (bondOrder == 6 && length > 12) {
        final arrowSize = 8.0;
        final tipX = endPos.dx;
        final tipY = endPos.dy;
        final nx = dx / length;
        final ny = dy / length;
        final path = Path()
          ..moveTo(tipX, tipY)
          ..lineTo(tipX - nx * arrowSize + px * arrowSize * 0.5,
              tipY - ny * arrowSize + py * arrowSize * 0.5)
          ..lineTo(tipX - nx * arrowSize - px * arrowSize * 0.5,
              tipY - ny * arrowSize - py * arrowSize * 0.5)
          ..close();
        canvas.drawPath(
            path,
            Paint()
              ..color = lineColor
              ..style = PaintingStyle.fill);
      }
    }

    // Draw snap target highlight circle
    if (snapped) {
      final highlightPaint = Paint()
        ..color = const Color(0xFF4FC3F7).withValues(alpha: 0.6)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2.0;
      canvas.drawCircle(endPos, 8.0, highlightPaint);
    }

    // Draw source atom highlight circle
    final sourcePaint = Paint()
      ..color = const Color(0xFF4FC3F7).withValues(alpha: 0.4)
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5;
    canvas.drawCircle(startPos, 6.0, sourcePaint);
  }

  @override
  bool shouldRepaint(AddBondPreviewPainter oldDelegate) =>
      startPos != oldDelegate.startPos ||
      endPos != oldDelegate.endPos ||
      snapped != oldDelegate.snapped ||
      bondOrder != oldDelegate.bondOrder;
}

class StructureDesignerViewport extends CadViewport {
  final StructureDesignerModel graphModel;

  const StructureDesignerViewport({
    super.key,
    required this.graphModel,
  });

  @override
  _StructureDesignerViewportState createState() =>
      _StructureDesignerViewportState();
}

class _StructureDesignerViewportState
    extends CadViewportState<StructureDesignerViewport> {
  _AtomEditDefaultDelegate? _atomEditDefaultDelegate;
  _AtomEditAddBondDelegate? _atomEditAddBondDelegate;
  Rect? _marqueeRect;
  APIAddBondMoveResult? _addBondPreview;
  final FocusNode _focusNode = FocusNode();

  // Spring-loaded B key state
  APIAtomEditTool? _springLoadedPreviousTool;
  bool _springLoadedActive = false;
  bool _springLoadedDeferRelease = false;

  @override
  void dispose() {
    _focusNode.dispose();
    super.dispose();
  }

  void _setMarqueeRect(Rect? rect) {
    setState(() => _marqueeRect = rect);
  }

  void _setAddBondPreview(APIAddBondMoveResult? result) {
    setState(() => _addBondPreview = result);
  }

  /// Project a 3D world position to 2D screen coordinates.
  Offset? _projectWorldToScreen(double wx, double wy, double wz) {
    final camera = common_api.getCamera();
    final ct = getCameraTransform(camera);
    if (ct == null || camera == null) return null;

    final dx = wx - ct.eye.x;
    final dy = wy - ct.eye.y;
    final dz = wz - ct.eye.z;

    final xView = dx * ct.right.x + dy * ct.right.y + dz * ct.right.z;
    final yView = dx * ct.up.x + dy * ct.up.y + dz * ct.up.z;
    final zView = dx * ct.forward.x + dy * ct.forward.y + dz * ct.forward.z;

    if (camera.orthographic) {
      final orthoHalfWidth =
          camera.orthoHalfHeight * (viewportWidth / viewportHeight);
      final sx = (xView / orthoHalfWidth) * (viewportWidth * 0.5) +
          viewportWidth * 0.5;
      final sy = -(yView / camera.orthoHalfHeight) * (viewportHeight * 0.5) +
          viewportHeight * 0.5;
      return Offset(sx, sy);
    } else {
      if (zView <= 0.001) return null; // Behind camera
      final d = viewportHeight * 0.5 / tan(camera.fovy * 0.5);
      final sx = (xView / zView) * d + viewportWidth * 0.5;
      final sy = -(yView / zView) * d + viewportHeight * 0.5;
      return Offset(sx, sy);
    }
  }

  KeyEventResult _onKeyEvent(FocusNode node, KeyEvent event) {
    if (!widget.graphModel.isNodeTypeActive("atom_edit")) {
      return KeyEventResult.ignored;
    }

    // Escape: cancel guided placement
    if (event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.escape &&
        atom_edit_api.atomEditIsInGuidedPlacement()) {
      widget.graphModel.atomEditCancelGuidedPlacement();
      renderingNeeded();
      return KeyEventResult.handled;
    }

    // B key: spring-loaded AddBond tool activation
    if (event.logicalKey == LogicalKeyboardKey.keyB) {
      if (event is KeyDownEvent && !_springLoadedActive) {
        final currentTool = atom_edit_api.getActiveAtomEditTool();
        if (currentTool != null && currentTool != APIAtomEditTool.addBond) {
          _springLoadedPreviousTool = currentTool;
          _springLoadedActive = true;
          _springLoadedDeferRelease = false;
          widget.graphModel.setActiveAtomEditTool(APIAtomEditTool.addBond);
          return KeyEventResult.handled;
        }
      } else if (event is KeyUpEvent && _springLoadedActive) {
        // Check if there's an active drag — if so, defer tool switch
        if (_addBondPreview != null && _addBondPreview!.isDragging) {
          _springLoadedDeferRelease = true;
        } else {
          _completeSpringLoadedRelease();
        }
        return KeyEventResult.handled;
      }
    }

    // Number keys 1-7: bond order shortcuts
    if (event is KeyDownEvent) {
      final int? bondOrder = _bondOrderFromKey(event.logicalKey);
      if (bondOrder != null) {
        final tool = atom_edit_api.getActiveAtomEditTool();
        if (tool == APIAtomEditTool.addBond) {
          atom_edit_api.setAddBondOrder(order: bondOrder);
          widget.graphModel.refreshFromKernel();
          return KeyEventResult.handled;
        } else if (tool == APIAtomEditTool.default_) {
          // Only act if bonds are selected
          final selectedNode = widget.graphModel.nodeNetworkView?.nodes.entries
              .where((entry) => entry.value.selected)
              .map((entry) => entry.value)
              .firstOrNull;
          if (selectedNode != null) {
            final data =
                structure_designer_api.getAtomEditData(nodeId: selectedNode.id);
            if (data != null && data.hasSelectedBonds) {
              atom_edit_api.changeSelectedBondsOrder(newOrder: bondOrder);
              widget.graphModel.refreshFromKernel();
              renderingNeeded();
              return KeyEventResult.handled;
            }
          }
        }
      }
    }

    return KeyEventResult.ignored;
  }

  int? _bondOrderFromKey(LogicalKeyboardKey key) {
    if (key == LogicalKeyboardKey.digit1) return 1;
    if (key == LogicalKeyboardKey.digit2) return 2;
    if (key == LogicalKeyboardKey.digit3) return 3;
    if (key == LogicalKeyboardKey.digit4) return 4;
    if (key == LogicalKeyboardKey.digit5) return 5;
    if (key == LogicalKeyboardKey.digit6) return 6;
    if (key == LogicalKeyboardKey.digit7) return 7;
    return null;
  }

  void _completeSpringLoadedRelease() {
    if (_springLoadedPreviousTool != null) {
      widget.graphModel.setActiveAtomEditTool(_springLoadedPreviousTool!);
    }
    _springLoadedActive = false;
    _springLoadedPreviousTool = null;
    _springLoadedDeferRelease = false;
  }

  void _onHover(PointerHoverEvent event) {
    // Track cursor for free sphere guided placement mode
    if (atom_edit_api.atomEditIsInGuidedPlacement()) {
      final ray = getRayFromPointerPos(event.localPosition);
      final changed = atom_edit_api.atomEditGuidedPlacementPointerMove(
        rayStart: vector3ToApiVec3(ray.start),
        rayDir: vector3ToApiVec3(ray.direction),
      );
      if (changed) {
        renderingNeeded();
      }
    }
  }

  /// Forward to the protected startGadgetDragFromHandle for the delegate.
  void delegateStartGadgetDrag(int handleIndex, Offset pos) {
    startGadgetDragFromHandle(handleIndex, pos);
  }

  @override
  PrimaryPointerDelegate? get primaryPointerDelegate {
    if (!widget.graphModel.isNodeTypeActive("atom_edit")) {
      _atomEditDefaultDelegate = null;
      _atomEditAddBondDelegate = null;
      return null;
    }

    final tool = atom_edit_api.getActiveAtomEditTool();
    if (tool == APIAtomEditTool.default_) {
      _atomEditAddBondDelegate = null;
      _atomEditDefaultDelegate ??= _AtomEditDefaultDelegate(this);
      return _atomEditDefaultDelegate;
    } else if (tool == APIAtomEditTool.addBond) {
      _atomEditDefaultDelegate = null;
      _atomEditAddBondDelegate ??= _AtomEditAddBondDelegate(this);
      return _atomEditAddBondDelegate;
    }

    _atomEditDefaultDelegate = null;
    _atomEditAddBondDelegate = null;
    return null;
  }

  @override
  void onDefaultClick(Offset pointerPos) {
    if (widget.graphModel.isNodeTypeActive("facet_shell")) {
      onFacetShellClick(pointerPos);
    } else if (widget.graphModel.isNodeTypeActive("atom_edit")) {
      onAtomEditClick(pointerPos);
    } else if (widget.graphModel.isNodeTypeActive("edit_atom")) {
      onEditAtomClick(pointerPos);
    }
  }

  void onFacetShellClick(Offset pointerPos) {
    final ray = getRayFromPointerPos(pointerPos);
    widget.graphModel.selectFacetShellFacetByRay(
      ray.start,
      ray.direction,
    );
  }

  void _showSaturationFeedback(
      bool hasAdditionalCapacity, bool dativeIncompatible) {
    final String message;
    final inDativeMode = widget.graphModel.bondMode == APIBondMode.dative;
    if (hasAdditionalCapacity && inDativeMode && dativeIncompatible) {
      message = 'No dative bond possible between these elements.';
    } else if (hasAdditionalCapacity && !inDativeMode && !dativeIncompatible) {
      message =
          'Atom is covalently saturated. Switch to Dative bond mode to access additional bonding positions.';
    } else {
      message = 'Atom is fully bonded';
    }
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(content: Text(message), duration: const Duration(seconds: 2)),
    );
  }

  void onAtomEditClick(Offset pointerPos) {
    final ray = getRayFromPointerPos(pointerPos);
    final activeAtomEditTool = atom_edit_api.getActiveAtomEditTool();

    final selectedNode = widget.graphModel.nodeNetworkView?.nodes.entries
        .where((entry) => entry.value.selected)
        .map((entry) => entry.value)
        .firstOrNull;

    if (activeAtomEditTool == APIAtomEditTool.addAtom) {
      final atomEditData = structure_designer_api.getAtomEditData(
        nodeId: selectedNode?.id ?? BigInt.zero,
      );

      if (atomEditData != null) {
        final atomicNumber = atomEditData.addAtomToolAtomicNumber!;

        if (atomEditData.isInGuidedPlacement) {
          // Already in guided placement — try placing at a guide dot
          final placed = widget.graphModel.atomEditPlaceGuidedAtom(
            ray.start,
            ray.direction,
          );
          if (!placed) {
            // Missed guide dot — try switching anchor to a different atom
            final result = widget.graphModel.atomEditStartGuidedPlacement(
              ray.start,
              ray.direction,
              atomicNumber,
              widget.graphModel.hybridizationOverride,
              widget.graphModel.bondMode,
              widget.graphModel.bondLengthMode,
            );
            switch (result) {
              case GuidedPlacementApiResult_NoAtomHit():
                // Clicked empty space — cancel guided placement
                widget.graphModel.atomEditCancelGuidedPlacement();
              case GuidedPlacementApiResult_AtomSaturated(
                  :final hasAdditionalCapacity,
                  :final dativeIncompatible
                ):
                _showSaturationFeedback(
                    hasAdditionalCapacity, dativeIncompatible);
              case GuidedPlacementApiResult_GuidedPlacementStarted():
                break; // Switched anchor — guides already shown
            }
          }
        } else {
          // Not in guided placement — try to start it
          final result = widget.graphModel.atomEditStartGuidedPlacement(
            ray.start,
            ray.direction,
            atomicNumber,
            widget.graphModel.hybridizationOverride,
            widget.graphModel.bondMode,
            widget.graphModel.bondLengthMode,
          );
          switch (result) {
            case GuidedPlacementApiResult_NoAtomHit():
              // No atom hit — fall back to free placement
              final camera = common_api.getCamera();
              final cameraTransform = getCameraTransform(camera);
              final planeNormal = cameraTransform!.forward;
              widget.graphModel.atomEditAddAtomByRay(
                atomicNumber,
                planeNormal,
                ray.start,
                ray.direction,
              );
            case GuidedPlacementApiResult_AtomSaturated(
                :final hasAdditionalCapacity,
                :final dativeIncompatible
              ):
              _showSaturationFeedback(
                  hasAdditionalCapacity, dativeIncompatible);
            case GuidedPlacementApiResult_GuidedPlacementStarted():
              break; // Guided placement started — guides shown
          }
        }
      }
    }
    // AddBond tool is handled by _AtomEditAddBondDelegate (not through onDefaultClick)
    // Default tool is handled by _AtomEditDefaultDelegate (not through onDefaultClick)
  }

  void onEditAtomClick(Offset pointerPos) {
    final ray = getRayFromPointerPos(pointerPos);
    final activeEditAtomTool = edit_atom_api.getActiveEditAtomTool();

    // Find the selected node
    final selectedNode = widget.graphModel.nodeNetworkView?.nodes.entries
        .where((entry) => entry.value.selected)
        .map((entry) => entry.value)
        .firstOrNull;

    if (activeEditAtomTool == APIEditAtomTool.addAtom) {
      // Get the atomic number from the current edit atom data
      final editAtomData = structure_designer_api.getEditAtomData(
        nodeId: selectedNode?.id ?? BigInt.zero,
      );

      if (editAtomData != null) {
        final camera = common_api.getCamera();
        final cameraTransform = getCameraTransform(camera);
        final planeNormal = cameraTransform!.forward;

        widget.graphModel.addAtomByRay(
          editAtomData.addAtomToolAtomicNumber!,
          planeNormal,
          ray.start,
          ray.direction,
        );
      }
    } else if (activeEditAtomTool == APIEditAtomTool.addBond) {
      // Add bond tool - create bonds between atoms
      widget.graphModel.drawBondByRay(
        ray.start,
        ray.direction,
      );
    } else if (activeEditAtomTool == APIEditAtomTool.default_) {
      // Default tool behavior - select atoms/bonds
      final selectModifier = getSelectModifierFromKeyboard();
      widget.graphModel.selectAtomOrBondByRay(
        ray.start,
        ray.direction,
        selectModifier,
      );
    }
  }

  @override
  void refreshFromKernel() {
    widget.graphModel.refreshFromKernel();
    // Complete deferred spring-loaded release after drag finishes
    if (_springLoadedDeferRelease) {
      _completeSpringLoadedRelease();
    }
  }

  @override
  Widget build(BuildContext context) {
    // Build rubber-band overlay if dragging in AddBond tool
    Widget? addBondOverlay;
    if (_addBondPreview != null &&
        _addBondPreview!.isDragging &&
        _addBondPreview!.hasSourcePos) {
      final startScreen = _projectWorldToScreen(
        _addBondPreview!.sourceAtomX,
        _addBondPreview!.sourceAtomY,
        _addBondPreview!.sourceAtomZ,
      );
      Offset? endScreen;
      if (_addBondPreview!.hasPreviewEnd) {
        endScreen = _projectWorldToScreen(
          _addBondPreview!.previewEndX,
          _addBondPreview!.previewEndY,
          _addBondPreview!.previewEndZ,
        );
      }
      if (startScreen != null && endScreen != null) {
        addBondOverlay = Positioned.fill(
          child: IgnorePointer(
            child: CustomPaint(
              painter: AddBondPreviewPainter(
                startPos: startScreen,
                endPos: endScreen,
                snapped: _addBondPreview!.snappedToAtom,
                bondOrder: _addBondPreview!.bondOrder,
              ),
            ),
          ),
        );
      }
    }

    return Focus(
      focusNode: _focusNode,
      onKeyEvent: _onKeyEvent,
      child: MouseRegion(
        onEnter: (_) => _focusNode.requestFocus(),
        onHover: _onHover,
        child: Stack(
          children: [
            super.build(context),
            if (_marqueeRect != null)
              Positioned.fill(
                child: IgnorePointer(
                  child: CustomPaint(
                    painter: MarqueePainter(rect: _marqueeRect!),
                  ),
                ),
              ),
            if (addBondOverlay != null) addBondOverlay,
          ],
        ),
      ),
    );
  }
}
