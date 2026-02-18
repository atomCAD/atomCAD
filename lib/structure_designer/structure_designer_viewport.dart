import 'package:flutter/material.dart';
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
  Rect? _marqueeRect;

  void _setMarqueeRect(Rect? rect) {
    setState(() => _marqueeRect = rect);
  }

  /// Forward to the protected startGadgetDragFromHandle for the delegate.
  void delegateStartGadgetDrag(int handleIndex, Offset pos) {
    startGadgetDragFromHandle(handleIndex, pos);
  }

  @override
  PrimaryPointerDelegate? get primaryPointerDelegate {
    if (!widget.graphModel.isNodeTypeActive("atom_edit")) return null;
    final tool = atom_edit_api.getActiveAtomEditTool();
    if (tool != APIAtomEditTool.default_) return null;
    _atomEditDefaultDelegate ??= _AtomEditDefaultDelegate(this);
    return _atomEditDefaultDelegate;
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
        final camera = common_api.getCamera();
        final cameraTransform = getCameraTransform(camera);
        final planeNormal = cameraTransform!.forward;

        widget.graphModel.atomEditAddAtomByRay(
          atomEditData.addAtomToolAtomicNumber!,
          planeNormal,
          ray.start,
          ray.direction,
        );
      }
    } else if (activeAtomEditTool == APIAtomEditTool.addBond) {
      widget.graphModel.atomEditDrawBondByRay(
        ray.start,
        ray.direction,
      );
    }
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
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
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
      ],
    );
  }
}
