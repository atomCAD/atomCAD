/// The 3D viewport, including **click-to-activate**
/// (`doc/design_click_to_activate_node.md`).
///
/// When several nodes are visible, clicking a non-active node's rendered output
/// activates that node — a two-step interaction, where the first click activates
/// and the second performs the normal action. The interception happens in
/// `onPointerDown` **before** delegate dispatch, calling the Rust
/// `viewport_pick()`, which returns `ActivateNode`, `Disambiguation`,
/// `ActiveNodeHit`, or `NoHit`. A performance guard skips the pick entirely when
/// only 0–1 nodes are displayed.
///
/// Overlapping outputs (within 0.1 Å) raise a `_DisambiguationOverlay` popup near
/// the click, offering two actions per candidate: name click (activate + scroll)
/// and solo eye icon (activate + scroll + hide the other overlapping nodes). If
/// the active node is among the overlapping hits, the click passes through as
/// normal.
///
/// **Scroll-to-node callback pattern.** After activating, the viewport calls
/// `model.scrollToNode(nodeId)`. `StructureDesignerModel.onScrollToNode` is a
/// callback registered by `NodeNetworkState` in `initState` (cleared in
/// `dispose`), which bridges viewport → model → node-network-widget without the
/// viewport holding a `GlobalKey` to the node network. A SnackBar
/// (`"Activated: {nodeName}"`) confirms the activation.
///
/// The callback carries two optional extras used by Find Usages: `scopeChain`
/// (to address a node inside an HOF / closure body) and `screenAnchor` (the
/// point, in the node-network widget's local screen coordinates, that the node's
/// *center* should land on — omitted means viewport center, which is the
/// click-to-activate behavior).
library;

import 'dart:async';
import 'dart:math';
import 'package:flutter/gestures.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Uint64List;
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter_cad/common/error_display.dart';
import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/cad_viewport.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/common/atom_tooltip.dart';
import 'package:flutter_cad/common/element_symbol_input.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/structure_designer/mechanosynth_ghost_painter.dart';
import 'package:flutter_cad/structure_designer/mechanosynth_offer_popup.dart';
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
  bool _frozenSnackbarShown = false;

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
      // Show snackbar once per drag when frozen atoms are in the selection
      if (!_frozenSnackbarShown &&
          result.frozenDragStatus != DragFrozenStatus.noneFrozen) {
        _frozenSnackbarShown = true;
        final message = result.frozenDragStatus == DragFrozenStatus.allFrozen
            ? 'All selected atoms are frozen \u2014 nothing moved'
            : 'Some frozen atoms in selection were not moved';
        ScaffoldMessenger.of(_viewport.context)
          ..hideCurrentSnackBar()
          ..showSnackBar(
            SnackBar(
              content: Text(message),
              duration: const Duration(seconds: 3),
              behavior: SnackBarBehavior.floating,
            ),
          );
      }
      _viewport.renderingNeeded();
    }
    return true;
  }

  @override
  bool onPrimaryUp(Offset pos) {
    if (_viewport.isGadgetDragging) return false;

    _frozenSnackbarShown = false;

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

/// Delegate that handles primary mouse button events for the atom_edit Guideline
/// tool (issue #368). Forwards pointer down/move/up to the Rust guideline tool
/// state machine: defining toggle (Define), pick + constrained drag (Move),
/// ghost drag (Place), unpick (empty click in Move).
class _AtomEditGuidelineDelegate implements PrimaryPointerDelegate {
  final _StructureDesignerViewportState _viewport;

  _AtomEditGuidelineDelegate(this._viewport);

  @override
  bool onPrimaryDown(Offset pos) {
    final ray = _viewport.getRayFromPointerPos(pos);
    atom_edit_api.guidelinePointerDown(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );
    return true;
  }

  @override
  bool onPrimaryMove(Offset pos) {
    final ray = _viewport.getRayFromPointerPos(pos);
    final changed = atom_edit_api.guidelinePointerMove(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );
    if (changed) {
      _viewport.renderingNeeded();
      // Rebuild the panel so the `t` field tracks the marker / atom live (a plain
      // notify — the Guideline card re-reads its view via FFI on rebuild).
      _viewport.widget.graphModel.notifyGuidelineToolSync();
    }
    return true;
  }

  @override
  bool onPrimaryUp(Offset pos) {
    final ray = _viewport.getRayFromPointerPos(pos);
    atom_edit_api.guidelinePointerUp(
      screenPos: offsetToApiVec2(pos),
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );
    _viewport.refreshFromKernel();
    _viewport.renderingNeeded();
    return true;
  }

  @override
  void onPrimaryCancel() {
    atom_edit_api.guidelineResetInteraction();
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
      case 7: // Metallic — copper/bronze
        lineColor = Colors.deepOrange.shade300;
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
  _AtomEditGuidelineDelegate? _atomEditGuidelineDelegate;
  Rect? _marqueeRect;
  APIAddBondMoveResult? _addBondPreview;
  Offset? _cursorPosition;
  final FocusNode _focusNode = FocusNode();

  // Spring-loaded B key state
  APIAtomEditTool? _springLoadedPreviousTool;
  bool _springLoadedActive = false;
  bool _springLoadedDeferRelease = false;

  // Element symbol typing accumulator
  late final ElementSymbolAccumulator _elementAccumulator =
      ElementSymbolAccumulator(onMatch: _onElementSymbolMatch);

  // ------------------------------------------------------------------
  // `mechanosynth_edit` placement tool (doc/design_mechanosynth_editor.md).
  //
  // Only the *presentation* lives here: the tool's real state machine is the
  // node's transient `PlacementState` in the kernel, and every one of these
  // fields is the answer to the last call rather than a second copy of it. The
  // popup is mounted inside this widget's own Stack rather than in a global
  // `Overlay` so it is clamped by the viewport's constraints and re-anchored
  // from the atom's projected position on every build.
  // ------------------------------------------------------------------

  /// The atom the popup hangs off: its id, world position and element. Kept
  /// separately from the sweep because the popup outlives it — choosing a row
  /// swaps the offer list for a candidate list without a second click, and the
  /// list must stay on the same atom.
  APIMechanosynthAnchor? _msAnchor;

  /// The last applicability sweep. Non-null means the popup is open in offer
  /// mode.
  APIMechanosynthOffers? _msOffers;

  /// The highlighted row's preview, ghosted on the workpiece. Nothing about it
  /// is committed; a near-miss preview is drawn in a warning colour.
  MechanosynthPreview? _msPreview;

  /// Which of the open sweep's operations are muted, as this popup understands
  /// it (`doc/design_mechanosynth_op_muting.md`).
  ///
  /// Seeded from the sweep — an ordinary one brings back none, a *show all
  /// here* one brings back exactly the muted ones — and updated in place when a
  /// row's eye-off is clicked, because muting from the popup deliberately does
  /// **not** re-sweep: the remaining rows were fitted against a workpiece the
  /// mute did not touch, so they are still correct, and the kernel leaves its
  /// stored offer list alone for the same reason.
  Set<String> _msMuted = {};

  /// Whether *show all here* has been taken for the open anchor. The answering
  /// sweep reports nothing skipped, so without this the footer would vanish at
  /// the moment it has something to say.
  bool _msShowingAll = false;

  /// Set while a popup is open so a camera move re-lays-out the anchor.
  /// `renderingNeeded()` only schedules a *frame*; the widget tree is rebuilt
  /// only by `setState`, and a popup pinned to a projected 3D point has to move
  /// with it.
  bool _msReanchorScheduled = false;

  /// How far the user has dragged the popup from where the automatic placement
  /// put it. A *delta*, not an absolute position, so the list still tracks its
  /// atom as the camera orbits — it just tracks it from wherever the user
  /// parked it. Cleared with the query, so each new click starts from the
  /// automatic placement again.
  Offset _msDragOffset = Offset.zero;

  /// The last layout `build` computed, so the header drag can clamp against the
  /// position the popup is actually at. Written during build, read only by the
  /// drag handler.
  MechanosynthLayout? _msLayout;

  // Hover tooltip state
  Timer? _hoverDebounceTimer;
  APIHoveredAtomInfo? _hoveredAtomInfo;
  Offset? _lastHoverPos;

  void _clearHoverTooltip() {
    _hoverDebounceTimer?.cancel();
    if (_hoveredAtomInfo != null) {
      setState(() => _hoveredAtomInfo = null);
    }
  }

  @override
  void dispose() {
    _hoverDebounceTimer?.cancel();
    _elementAccumulator.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _setMarqueeRect(Rect? rect) {
    setState(() => _marqueeRect = rect);
  }

  void _setAddBondPreview(APIAddBondMoveResult? result) {
    setState(() => _addBondPreview = result);
  }

  /// `renderingNeeded()` schedules a *frame*, which repaints the 3D texture but
  /// does not rebuild the widget tree — and the placement popup and its ghosts
  /// are laid out from world positions projected during `build`. So while
  /// either is on screen, a camera move has to drag a rebuild along with it or
  /// the popup stays pinned to where the atom used to be.
  ///
  /// The rebuild is deferred to a post-frame callback (a camera drag calls this
  /// from a pointer handler, sometimes several times per frame) and guarded, so
  /// one frame costs at most one extra rebuild.
  @override
  void renderingNeeded() {
    super.renderingNeeded();
    final anchored = _msOffers != null || _msPreview != null;
    if (!anchored || _msReanchorScheduled) return;
    _msReanchorScheduled = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _msReanchorScheduled = false;
      if (mounted) setState(() {});
    });
  }

  /// Project a 3D world position to 2D screen coordinates.
  Offset? _projectWorldToScreen(double wx, double wy, double wz) {
    final camera = common_api.getCamera();
    return _projectWithCamera(camera, getCameraTransform(camera), wx, wy, wz);
  }

  /// The same projection with the camera already in hand. Every fetch is an FFI
  /// call, and the placement tool projects a few hundred points per build (the
  /// preview's ghosts, plus every row's, to know where not to put the list) —
  /// one fetch for the lot, not one per atom.
  Offset? _projectWithCamera(
      APICamera? camera, CameraTransform? ct, double wx, double wy, double wz) {
    if (ct == null || camera == null) return null;

    final dx = wx - ct.eye.x;
    final dy = wy - ct.eye.y;
    final dz = wz - ct.eye.z;

    final xView = dx * ct.right.x + dy * ct.right.y + dz * ct.right.z;
    final yView = dx * ct.up.x + dy * ct.up.y + dz * ct.up.z;
    final zView = dx * ct.forward.x + dy * ct.forward.y + dz * ct.forward.z;

    if (camera.orthographic) {
      // `camera.aspect`, not `viewportWidth / viewportHeight`: the renderer
      // builds its orthographic projection from the former, and the two differ
      // by the integer truncation in `setViewportSize`. `getRayFromPointerPos`
      // already uses `camera.aspect`, and this is its inverse.
      final orthoHalfWidth = camera.orthoHalfHeight * camera.aspect;
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
    // Escape stops the placement tool. The popup handles its own Escape while
    // it holds focus; this is the case where it does not — focus back on the
    // viewport — and it comes before the atom_edit gate because
    // `mechanosynth_edit` is not an atom_edit-like node.
    if (event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.escape &&
        _mechanosynthEditNodeId != null) {
      _mechanosynthCancel();
      return KeyEventResult.handled;
    }

    if (!widget.graphModel.isAtomEditLikeActive) {
      return KeyEventResult.ignored;
    }

    // Escape: cancel guided placement first, then clear the guideline (#368).
    // Precedence: an active guided placement is cancelled before the guideline.
    if (event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.escape) {
      if (atom_edit_api.atomEditIsInGuidedPlacement()) {
        widget.graphModel.atomEditCancelGuidedPlacement();
        renderingNeeded();
        return KeyEventResult.handled;
      }
      // Guideline tool (#368): Escape clears the active line (→ Define) or, in
      // Define, clears the defining set — `guidelineClear` does both.
      if (atom_edit_api.getGuidelineToolView() != null) {
        widget.graphModel.guidelineClear();
        renderingNeeded();
        return KeyEventResult.handled;
      }
    }

    // Delete / Backspace: delete selected atoms/bonds
    if (event is KeyDownEvent &&
        (event.logicalKey == LogicalKeyboardKey.delete ||
            event.logicalKey == LogicalKeyboardKey.backspace)) {
      final tool = atom_edit_api.getActiveAtomEditTool();
      if (tool == APIAtomEditTool.default_) {
        widget.graphModel.atomEditDeleteSelected();
        renderingNeeded();
        return KeyEventResult.handled;
      }
    }

    // F2: switch to Default tool
    if (event is KeyDownEvent && event.logicalKey == LogicalKeyboardKey.f2) {
      final currentTool = atom_edit_api.getActiveAtomEditTool();
      if (currentTool != null && currentTool != APIAtomEditTool.default_) {
        _elementAccumulator.reset();
        widget.graphModel.setActiveAtomEditTool(APIAtomEditTool.default_);
        _clearHoverTooltip();
        return KeyEventResult.handled;
      }
    }

    // F3: switch to AddAtom tool
    if (event is KeyDownEvent && event.logicalKey == LogicalKeyboardKey.f3) {
      final currentTool = atom_edit_api.getActiveAtomEditTool();
      if (currentTool != null && currentTool != APIAtomEditTool.addAtom) {
        _elementAccumulator.reset();
        widget.graphModel.setActiveAtomEditTool(APIAtomEditTool.addAtom);
        _clearHoverTooltip();
        return KeyEventResult.handled;
      }
    }

    // F4: switch to AddBond tool
    if (event is KeyDownEvent && event.logicalKey == LogicalKeyboardKey.f4) {
      final currentTool = atom_edit_api.getActiveAtomEditTool();
      if (currentTool != null && currentTool != APIAtomEditTool.addBond) {
        _elementAccumulator.reset();
        widget.graphModel.setActiveAtomEditTool(APIAtomEditTool.addBond);
        _clearHoverTooltip();
        return KeyEventResult.handled;
      }
    }

    // F5: switch to Guideline tool (#368)
    if (event is KeyDownEvent && event.logicalKey == LogicalKeyboardKey.f5) {
      final currentTool = atom_edit_api.getActiveAtomEditTool();
      if (currentTool != null && currentTool != APIAtomEditTool.guideline) {
        _elementAccumulator.reset();
        widget.graphModel.setActiveAtomEditTool(APIAtomEditTool.guideline);
        _clearHoverTooltip();
        return KeyEventResult.handled;
      }
    }

    // J key: spring-loaded AddBond tool activation
    if (event.logicalKey == LogicalKeyboardKey.keyJ) {
      if (event is KeyDownEvent && !_springLoadedActive) {
        final currentTool = atom_edit_api.getActiveAtomEditTool();
        if (currentTool != null && currentTool != APIAtomEditTool.addBond) {
          _elementAccumulator.reset();
          _springLoadedPreviousTool = currentTool;
          _springLoadedActive = true;
          _springLoadedDeferRelease = false;
          widget.graphModel.setActiveAtomEditTool(APIAtomEditTool.addBond);
          _clearHoverTooltip();
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
        _elementAccumulator.reset();
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
            final data = structure_designer_api.getAtomEditData(
                scopePath: Uint64List(0), nodeId: selectedNode.id);
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

    // Ctrl+Shift+H: Remove hydrogen from selected atoms (Default tool only)
    if (event is KeyDownEvent &&
        HardwareKeyboard.instance.isControlPressed &&
        HardwareKeyboard.instance.isShiftPressed &&
        event.logicalKey == LogicalKeyboardKey.keyH) {
      final tool = atom_edit_api.getActiveAtomEditTool();
      if (tool == APIAtomEditTool.default_) {
        widget.graphModel.atomEditRemoveHydrogen(selectedOnly: true);
        renderingNeeded();
        return KeyEventResult.handled;
      }
    }

    // Ctrl+H: Add hydrogen to selected atoms (Default tool only)
    if (event is KeyDownEvent &&
        HardwareKeyboard.instance.isControlPressed &&
        event.logicalKey == LogicalKeyboardKey.keyH) {
      final tool = atom_edit_api.getActiveAtomEditTool();
      if (tool == APIAtomEditTool.default_) {
        widget.graphModel.atomEditAddHydrogen(selectedOnly: true);
        renderingNeeded();
        return KeyEventResult.handled;
      }
    }

    // Ctrl+Shift+M: Minimize selected atoms (Default tool only)
    if (event is KeyDownEvent &&
        HardwareKeyboard.instance.isControlPressed &&
        HardwareKeyboard.instance.isShiftPressed &&
        event.logicalKey == LogicalKeyboardKey.keyM) {
      final tool = atom_edit_api.getActiveAtomEditTool();
      if (tool == APIAtomEditTool.default_) {
        widget.graphModel.atomEditMinimize(
          APIMinimizeFreezeMode.freeSelected,
        );
        renderingNeeded();
        return KeyEventResult.handled;
      }
    }

    // Ctrl+M: Minimize unfrozen atoms (Default tool only)
    if (event is KeyDownEvent &&
        HardwareKeyboard.instance.isControlPressed &&
        !HardwareKeyboard.instance.isShiftPressed &&
        event.logicalKey == LogicalKeyboardKey.keyM) {
      final tool = atom_edit_api.getActiveAtomEditTool();
      if (tool == APIAtomEditTool.default_) {
        widget.graphModel.atomEditMinimize(
          APIMinimizeFreezeMode.freeAll,
        );
        renderingNeeded();
        return KeyEventResult.handled;
      }
    }

    // Element symbol typing: letter keys -> element selection/replacement
    if (event is KeyDownEvent &&
        !HardwareKeyboard.instance.isControlPressed &&
        !HardwareKeyboard.instance.isAltPressed &&
        !HardwareKeyboard.instance.isMetaPressed) {
      final tool = atom_edit_api.getActiveAtomEditTool();
      if (tool == APIAtomEditTool.default_ || tool == APIAtomEditTool.addAtom) {
        final letter = _letterFromKey(event.logicalKey);
        if (letter != null) {
          _elementAccumulator.handleLetter(letter);
          return KeyEventResult.handled;
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

  /// Extract a lowercase letter from a logical key, or null for non-letter keys.
  String? _letterFromKey(LogicalKeyboardKey key) {
    final label = key.keyLabel;
    if (label.length == 1) {
      final c = label.codeUnitAt(0);
      if (c >= 0x41 && c <= 0x5A) return label.toLowerCase(); // A-Z
      if (c >= 0x61 && c <= 0x7A) return label; // a-z
    }
    return null;
  }

  /// Called when the element symbol accumulator matches an element.
  /// Applies the element based on the current tool and selection state.
  void _onElementSymbolMatch(int atomicNumber, String symbol) {
    if (!mounted) return;
    final tool = atom_edit_api.getActiveAtomEditTool();
    if (tool == APIAtomEditTool.default_) {
      // If atoms are selected, replace them immediately
      final selectedNode = widget.graphModel.nodeNetworkView?.nodes.entries
          .where((entry) => entry.value.selected)
          .map((entry) => entry.value)
          .firstOrNull;
      if (selectedNode != null) {
        final data = structure_designer_api.getAtomEditData(
            scopePath: Uint64List(0), nodeId: selectedNode.id);
        if (data != null && data.hasSelectedAtoms) {
          widget.graphModel.atomEditReplaceSelected(atomicNumber);
          renderingNeeded();
          // Also update the shared element selection
          widget.graphModel.setAtomEditSelectedElement(atomicNumber);
          return;
        }
      }
    }
    // Set the shared element selection (works for any tool)
    if (tool == APIAtomEditTool.default_ || tool == APIAtomEditTool.addAtom) {
      widget.graphModel.setAtomEditSelectedElement(atomicNumber);
      setState(() {}); // Refresh cursor overlay immediately
    }
  }

  /// Whether the pointer is over an overlay this widget stacks *on* the
  /// viewport rather than over the viewport itself.
  ///
  /// The hover machinery hangs off one `MouseRegion` around the whole stack, so
  /// a pointer resting on the placement popup is still "hovering the viewport"
  /// as far as Flutter is concerned — and the atom tooltip would pop up behind
  /// the list the user is actually reading, over whatever atom happens to lie
  /// under it. Anything else stacked over the viewport belongs in this test.
  bool _pointerIsOverOverlay(Offset pos) {
    final popup = _msLayout?.rect;
    return popup != null && popup.contains(pos);
  }

  void _scheduleHoverHitTest(Offset pos) {
    _hoverDebounceTimer?.cancel();

    // Suppress while AddAtom tool is active (it has its own cursor label)
    if (widget.graphModel.isAtomEditLikeActive &&
        widget.graphModel.activeAtomEditTool == APIAtomEditTool.addAtom) {
      return;
    }

    if (_pointerIsOverOverlay(pos)) return;

    _lastHoverPos = pos;
    _hoverDebounceTimer = Timer(const Duration(milliseconds: 100), () {
      _performHoverHitTest(pos);
    });
  }

  void _performHoverHitTest(Offset pos) {
    final ray = getRayFromPointerPos(pos);
    final info = structure_designer_api.queryHoveredAtomInfo(
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );
    if (mounted && _lastHoverPos == pos) {
      setState(() => _hoveredAtomInfo = info);
    }
  }

  void _onHover(PointerHoverEvent event) {
    final pos = event.localPosition;
    setState(() => _cursorPosition = pos);

    if (_pointerIsOverOverlay(pos)) {
      _clearHoverTooltip();
      return;
    }

    // Movement threshold: suppress flicker from micro-movements (< 4 px).
    final moved = _lastHoverPos != null
        ? (pos - _lastHoverPos!).distance
        : double.infinity;
    if (moved >= 4.0) {
      _clearHoverTooltip();
    } else {
      _hoverDebounceTimer?.cancel();
    }

    _scheduleHoverHitTest(pos);

    // Existing guided placement tracking (unchanged)
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

  @override
  void onPointerDown(PointerDownEvent event) {
    _clearHoverTooltip();

    // Click-to-activate: intercept primary clicks when multiple nodes are
    // visible to check if the click hits a non-active node's output.
    if (event.kind == PointerDeviceKind.mouse &&
        event.buttons == kPrimaryMouseButton) {
      final pickResult = _tryViewportPick(event.localPosition);
      if (pickResult != null) {
        // Click was consumed by click-to-activate — do not pass to delegate.
        return;
      }
    }

    super.onPointerDown(event);
  }

  /// Attempts a viewport pick for click-to-activate. Returns true if the
  /// click was consumed (node activated or disambiguation shown), null if
  /// the click should pass through to normal handling.
  bool? _tryViewportPick(Offset pointerPos) {
    final nodes = widget.graphModel.nodeNetworkView?.nodes;
    if (nodes == null) return null;

    // Performance guard: only needed when there is at least one displayed
    // non-active node. If every displayed node is the active node (or none
    // are displayed), there is nothing to activate via viewport click.
    final hasDisplayedNonActive =
        nodes.values.any((n) => n.displayed && !n.active);
    if (!hasDisplayedNonActive) return null;

    final ray = getRayFromPointerPos(pointerPos);
    final result = structure_designer_api.viewportPick(
      rayOrigin: vector3ToApiVec3(ray.start),
      rayDirection: vector3ToApiVec3(ray.direction),
    );

    if (result is APIViewportPickResult_ActivateNode) {
      widget.graphModel.setSelectedNode(result.nodeId);
      widget.graphModel.scrollToNode(result.nodeId);
      renderingNeeded();
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('Activated: ${result.nodeName}'),
          duration: const Duration(seconds: 2),
        ),
      );
      return true;
    }

    if (result is APIViewportPickResult_Disambiguation) {
      _showDisambiguationMenu(pointerPos, result.candidates);
      return true;
    }

    // ActiveNodeHit or NoHit — pass through to normal handling.
    return null;
  }

  void _showDisambiguationMenu(
      Offset pointerPos, List<APICandidateNode> candidates) {
    final overlay = Overlay.of(context);
    final renderBox = context.findRenderObject() as RenderBox;
    final globalPos = renderBox.localToGlobal(pointerPos);

    late OverlayEntry entry;
    entry = OverlayEntry(
      builder: (context) => _DisambiguationOverlay(
        position: globalPos,
        candidates: candidates,
        onActivate: (candidate) {
          entry.remove();
          widget.graphModel.setSelectedNode(candidate.nodeId);
          widget.graphModel.scrollToNode(candidate.nodeId);
          renderingNeeded();
          ScaffoldMessenger.of(this.context).showSnackBar(
            SnackBar(
              content: Text('Activated: ${candidate.nodeName}'),
              duration: const Duration(seconds: 2),
            ),
          );
        },
        onSolo: (candidate) {
          entry.remove();
          // Hide other overlapping nodes, then activate the chosen one.
          for (final other in candidates) {
            if (other.nodeId != candidate.nodeId) {
              structure_designer_api.setNodeDisplay(
                scopePath: Uint64List(0),
                nodeId: other.nodeId,
                isDisplayed: false,
              );
            }
          }
          widget.graphModel.setSelectedNode(candidate.nodeId);
          widget.graphModel.scrollToNode(candidate.nodeId);
          renderingNeeded();
          ScaffoldMessenger.of(this.context).showSnackBar(
            SnackBar(
              content: Text('Activated: ${candidate.nodeName}'),
              duration: const Duration(seconds: 2),
            ),
          );
        },
        onDismiss: () {
          entry.remove();
        },
      ),
    );
    overlay.insert(entry);
  }

  /// Forward to the protected startGadgetDragFromHandle for the delegate.
  void delegateStartGadgetDrag(int handleIndex, Offset pos) {
    startGadgetDragFromHandle(handleIndex, pos);
  }

  @override
  PrimaryPointerDelegate? get primaryPointerDelegate {
    if (!widget.graphModel.isAtomEditLikeActive) {
      _atomEditDefaultDelegate = null;
      _atomEditAddBondDelegate = null;
      _atomEditGuidelineDelegate = null;
      return null;
    }

    final tool = atom_edit_api.getActiveAtomEditTool();
    if (tool == APIAtomEditTool.default_) {
      _atomEditAddBondDelegate = null;
      _atomEditGuidelineDelegate = null;
      _atomEditDefaultDelegate ??= _AtomEditDefaultDelegate(this);
      return _atomEditDefaultDelegate;
    } else if (tool == APIAtomEditTool.addBond) {
      _atomEditDefaultDelegate = null;
      _atomEditGuidelineDelegate = null;
      _atomEditAddBondDelegate ??= _AtomEditAddBondDelegate(this);
      return _atomEditAddBondDelegate;
    } else if (tool == APIAtomEditTool.guideline) {
      _atomEditDefaultDelegate = null;
      _atomEditAddBondDelegate = null;
      _atomEditGuidelineDelegate ??= _AtomEditGuidelineDelegate(this);
      return _atomEditGuidelineDelegate;
    }

    _atomEditDefaultDelegate = null;
    _atomEditAddBondDelegate = null;
    _atomEditGuidelineDelegate = null;
    return null;
  }

  @override
  void onDefaultClick(Offset pointerPos) {
    if (widget.graphModel.isNodeTypeActive("facet_shell")) {
      onFacetShellClick(pointerPos);
    } else if (widget.graphModel.isAtomEditLikeActive) {
      onAtomEditClick(pointerPos);
    } else if (widget.graphModel.isNodeTypeActive("edit_atom")) {
      onEditAtomClick(pointerPos);
    } else if (widget.graphModel.isNodeTypeActive("mechanosynth_edit")) {
      onMechanosynthEditClick(pointerPos);
    }
  }

  // ======================================================================
  // `mechanosynth_edit` placement tool
  // ======================================================================

  /// The selected `mechanosynth_edit` node, when the tool owns viewport picks:
  /// the node is the active, displayed one (`isNodeTypeActive`, the rule
  /// `atom_edit` uses) and is the selected one, so the property panel is
  /// showing it and `propertyEditorScopeChain` addresses it.
  BigInt? get _mechanosynthEditNodeId {
    if (!widget.graphModel.isNodeTypeActive('mechanosynth_edit')) return null;
    final selected = widget.graphModel.nodeNetworkView?.nodes.values
        .where(
            (node) => node.selected && node.nodeTypeName == 'mechanosynth_edit')
        .firstOrNull;
    return selected?.id;
  }

  /// Closes the popup and returns the kernel's tool to Idle.
  void _mechanosynthCancel({bool tellKernel = true}) {
    final nodeId = _mechanosynthEditNodeId;
    if (tellKernel && nodeId != null) {
      widget.graphModel.mechanosynthEditCancel(nodeId);
    }
    _mechanosynthClearPopup();
  }

  /// Clears the popup state this widget holds, without telling the kernel —
  /// for the transitions where the kernel has already moved on (a commit
  /// leaves the tool Armed, a choose clears the query itself).
  void _mechanosynthClearPopup() {
    setState(() {
      _msAnchor = null;
      _msOffers = null;
      _msPreview = null;
      _msDragOffset = Offset.zero;
      _msMuted = {};
      _msShowingAll = false;
    });
    renderingNeeded();
  }

  /// A click on the workpiece while a `mechanosynth_edit` node is active.
  ///
  /// The click is always a **question**: what can be done at this atom. There
  /// is no armed mode in which the same click means "place the last operation
  /// again" — the library splits a reaction into one operation per host
  /// environment, so the previous operation is usually the wrong one at the
  /// next site, and a click whose meaning depends on invisible state is worse
  /// than a click that always asks. Fast repetition is a real need and wants
  /// its own design (a family applied over a selection, a family armed as a
  /// tool with the atoms it fits highlighted); it is not this.
  void onMechanosynthEditClick(Offset pointerPos) {
    final nodeId = _mechanosynthEditNodeId;
    if (nodeId == null) return;
    final model = widget.graphModel;

    final ray = getRayFromPointerPos(pointerPos);
    final anchor =
        model.mechanosynthEditAnchorAtRay(nodeId, ray.start, ray.direction);
    if (anchor == null) {
      // Empty space closes the popup. The click does not change the app's atom
      // selection either way: the anchor is the tool's own transient state.
      if (_msOffers != null) _mechanosynthCancel();
      return;
    }

    final outcome = model.mechanosynthEditOffers(nodeId, anchor.atomId);
    if (outcome.value == null) {
      // No library wired, or a stale atom id — not "nothing applies here",
      // which is an empty row list rather than a failure.
      showErrorSnackBar(context, outcome.error ?? 'no offers');
      return;
    }
    setState(() {
      _msAnchor = anchor;
      _msOffers = outcome.value;
      _msPreview = null;
      _msMuted = _mutedOpsOf(outcome.value!);
      _msShowingAll = false;
    });
    renderingNeeded();
  }

  /// The muted operations a sweep brought back. Empty for an ordinary sweep —
  /// it did not look at them — and exactly the muted rows for a *show all
  /// here* one.
  static Set<String> _mutedOpsOf(APIMechanosynthOffers offers) => {
        for (final row in offers.rows)
          if (row.muted) row.op,
      };

  /// Sweeps the open anchor again over the **whole** library, mute set
  /// ignored.
  ///
  /// The escape hatch that keeps an empty offer list honest: the list's promise
  /// is that it reports what the library can do at this atom, and this is how
  /// the user cashes that promise in. It costs one extra sweep, only when
  /// asked, and the rows it brings back are fully placeable — mute filters the
  /// sweep and nothing else.
  void _mechanosynthShowAll() {
    final nodeId = _mechanosynthEditNodeId;
    final anchor = _msAnchor;
    if (nodeId == null || anchor == null) return;

    final outcome = widget.graphModel
        .mechanosynthEditOffers(nodeId, anchor.atomId, includeMuted: true);
    if (outcome.value == null) {
      showErrorSnackBar(context, outcome.error ?? 'no offers');
      return;
    }
    setState(() {
      _msOffers = outcome.value;
      _msPreview = null;
      _msMuted = _mutedOpsOf(outcome.value!);
      _msShowingAll = true;
    });
    renderingNeeded();
  }

  /// Leaves an operation out of this node's offer sweep, or puts it back.
  ///
  /// The popup is where the clutter is noticed, so it is the cheapest place to
  /// act on it. **No re-sweep**: the rows in hand were fitted against a
  /// workpiece muting does not touch, so hiding one in place is both correct
  /// and free. The kernel leaves its stored offer list alone for the same
  /// reason, which is what keeps the remaining rows placeable.
  void _mechanosynthMute(String op, bool muted) {
    final nodeId = _mechanosynthEditNodeId;
    if (nodeId == null) return;

    final error =
        widget.graphModel.setMechanosynthEditMuted(nodeId, [op], muted);
    if (error != null) {
      showErrorSnackBar(context, error);
      return;
    }
    // The row is about to vanish, so its ghosts must go with it — both the
    // projected copy this widget holds and the kernel's, which is what the
    // tessellator draws.
    final previewed = _msPreview?.op == op && muted;
    if (previewed) widget.graphModel.mechanosynthEditClearPreview(nodeId);
    setState(() {
      // A **new** set, not a mutation: the popup resets its highlight when the
      // muted set changes, and a set mutated in place is the same object on
      // both sides of `didUpdateWidget`.
      _msMuted = muted ? {..._msMuted, op} : (_msMuted..remove(op)).toSet();
      if (previewed) _msPreview = null;
    });
    renderingNeeded();
  }

  /// Places one row of the popup.
  ///
  /// Every row already names a single placement — an operation that fits more
  /// than one way is listed as one row per orientation — so this commits and
  /// never opens a second list.
  void _mechanosynthChoose(String op, int candidateIndex) {
    final nodeId = _mechanosynthEditNodeId;
    if (nodeId == null) return;
    final model = widget.graphModel;

    final error = model.mechanosynthEditChoose(nodeId, op, candidateIndex);
    if (error != null) {
      showErrorSnackBar(context, error);
      return;
    }
    _mechanosynthClearPopup();
  }

  /// Where the popup and its anchor sit this frame, computed once so the
  /// popup, the anchor ring and the leader line between them agree.
  /// Returns null when nothing is open.
  MechanosynthLayout? _mechanosynthLayout(BoxConstraints constraints) {
    final offers = _msOffers;
    final anchor = _msAnchor;
    if (anchor == null || offers == null) return null;

    // The kernel is the authority on whether a query is still live. Anything
    // that drops it without going through this widget — a cursor move from the
    // panel, an undo, a re-arm — leaves these fields holding candidates that
    // were fitted against a workpiece the node no longer shows, so the popup
    // goes with the query rather than waiting to be refused.
    final nodeId = _mechanosynthEditNodeId;
    final state = nodeId == null
        ? null
        : widget.graphModel.mechanosynthEditToolStatus(nodeId)?.toolState;
    if (state != 'offers' && state != 'candidates') return null;

    // Re-projected on every build, which is why `renderingNeeded` forces one
    // while the popup is open: the list has to stay on its atom as the camera
    // orbits. One camera fetch for every projection below.
    final camera = common_api.getCamera();
    final ct = getCameraTransform(camera);
    final anchorScreen = _projectWithCamera(
        camera, ct, anchor.position.x, anchor.position.y, anchor.position.z);

    // What gets *drawn*: the highlighted row only.
    final ghosts = _projectGhosts(_msPreview?.ghost ?? const [], camera, ct);

    // What the popup is placed clear of: **every** row's preview, not the
    // highlighted one's. Keying the placement on the highlight made the list
    // jump across the viewport each time the highlight moved — which is on
    // every hover and every arrow key, i.e. exactly while the user is reading
    // it. The union is fixed for a query, so the list holds still, and it is
    // the right region anyway: it is where a preview can appear, so it is
    // where the list must not be.
    final reach = <APIGhostAtom>[
      for (final row in offers.rows)
        for (final candidate in row.candidates) ...candidate.ghost,
    ];

    // One row per placement, plus a header line for each operation that has
    // more than one — the same shape the popup builds.
    final rowCount = offers.rows.fold<int>(
        0,
        (total, row) =>
            total +
            (row.fits && row.candidates.length > 1
                ? row.candidates.length + 1
                : 1));
    final size =
        Size(MECHANOSYNTH_POPUP_WIDTH, _mechanosynthPopupHeight(rowCount));
    final viewportSize = Size(constraints.maxWidth, constraints.maxHeight);

    final position = mechanosynthPopupPosition(
      box: mechanosynthActionBox(
        anchor: anchorScreen,
        ghosts: _projectGhosts(reach, camera, ct),
        fallbackCentre:
            Offset(constraints.maxWidth / 2, constraints.maxHeight / 2),
      ),
      size: size,
      viewportSize: viewportSize,
      drag: _msDragOffset,
    );

    return MechanosynthLayout(
      anchor: anchorScreen,
      ghosts: ghosts,
      rect: position & size,
      viewportSize: viewportSize,
    );
  }

  /// Moves the popup by a header drag.
  ///
  /// The stored offset is the movement that **actually happened**, not the
  /// pointer's travel: applying the raw delta and clamping afterwards lets the
  /// offset run away past the viewport edge, and the user then has to drag all
  /// of that invisible slack back before the list moves at all — which reads as
  /// the popup refusing to move on one axis.
  void _mechanosynthDragPopup(Offset delta) {
    final layout = _msLayout;
    if (layout == null) return;
    final moved = mechanosynthClampInto(
          layout.rect.topLeft + delta,
          layout.rect.size,
          layout.viewportSize,
        ) -
        layout.rect.topLeft;
    if (moved == Offset.zero) return;
    setState(() => _msDragOffset += moved);
  }

  /// Close enough to place with. The header is one or two lines and the list is
  /// capped, so this only has to bracket the truth — the flat 320 px it
  /// replaced over-estimated a short list by 200 px, which pinned the popup to
  /// the bottom of its clamp range with no visible relation to the atom at all.
  double _mechanosynthPopupHeight(int rowCount) {
    const headerHeight = 30.0;
    const rowHeight = 36.0;
    final list = (rowCount * rowHeight)
        .clamp(0.0, MECHANOSYNTH_POPUP_MAX_LIST_HEIGHT)
        .toDouble();
    return headerHeight + list + 8.0;
  }

  /// How long the pointer must rest on a row before it previews.
  ///
  /// A preview is a synchronous evaluation on the UI thread, so the honest
  /// delay is "about as long as the last one took": on a small molecule the
  /// list feels immediate, and on a slab where a refresh costs a third of a
  /// second it backs off instead of stuttering under the pointer. The app
  /// already measures every refresh for the profile strip, so this costs a
  /// field read rather than an instrument.
  ///
  /// The floor keeps a fast structure from previewing rows the pointer is only
  /// crossing; the ceiling keeps a slow one from feeling broken, on the grounds
  /// that a user who has waited half a second has stopped moving on purpose.
  Duration get _mechanosynthPreviewDelay {
    const floorMs = 90.0;
    const ceilingMs = 500.0;
    final last = widget.graphModel.refreshProfile.value?.kernel?.totalMs ?? 0.0;
    return Duration(milliseconds: last.clamp(floorMs, ceilingMs).round());
  }

  /// Which way a placement goes, in screen space, as an angle in radians.
  ///
  /// The direction from the clicked atom to the centroid of what the placement
  /// would put there, projected with the **live** camera — so the arrow in the
  /// list points the way the reaction goes on screen, and re-aims as the user
  /// orbits. `null` when there is nothing to take a centroid of (a bond-only
  /// operation) or when either end is behind the camera.
  double? _mechanosynthArrowAngle(List<APIGhostAtom> ghost) {
    if (ghost.isEmpty) return null;
    final anchor = _msAnchor;
    if (anchor == null) return null;

    var cx = 0.0, cy = 0.0, cz = 0.0;
    for (final atom in ghost) {
      cx += atom.position.x;
      cy += atom.position.y;
      cz += atom.position.z;
    }
    final n = ghost.length;

    final from = _projectWorldToScreen(
        anchor.position.x, anchor.position.y, anchor.position.z);
    final to = _projectWorldToScreen(cx / n, cy / n, cz / n);
    if (from == null || to == null) return null;
    final delta = to - from;
    // Below this the two points are on top of each other on screen and the
    // angle is noise, not a direction.
    if (delta.distance < 2.0) return null;
    return atan2(delta.dy, delta.dx);
  }

  /// The popup, placed clear of the reaction and clamped inside the viewport.
  Widget? _buildMechanosynthPopup(MechanosynthLayout? layout) {
    if (layout == null) return null;
    final anchor = _msAnchor;
    final offers = _msOffers;
    if (anchor == null || offers == null) return null;

    return Positioned(
      left: layout.rect.left,
      top: layout.rect.top,
      child: MechanosynthOfferPopup(
        moved: _msDragOffset != Offset.zero,
        arrowAngleFor: _mechanosynthArrowAngle,
        previewDelay: _mechanosynthPreviewDelay,
        onDrag: _mechanosynthDragPopup,
        onResetPosition: () => setState(() => _msDragOffset = Offset.zero),
        // A new sweep is a new popup: keying on the anchor atom resets the
        // selection and the filter without the widget having to notice.
        key: ValueKey('ms_popup_${anchor.atomId}'),
        anchorAtomicNumber: anchor.atomicNumber,
        offers: offers.rows,
        onChoose: _mechanosynthChoose,
        onMute: _mechanosynthMute,
        onShowAll: _mechanosynthShowAll,
        mutedOps: _msMuted,
        // The sweep's own count plus whatever has been muted from the list
        // since. While the list is not showing all, a row can only be muted
        // here (a muted one is not drawn), so the delta is exactly the set.
        mutedCount: offers.mutedCount + (_msShowingAll ? 0 : _msMuted.length),
        libraryCount: offers.libraryCount,
        showingAll: _msShowingAll,
        // Selecting a row is a round trip to the kernel: the ghosts go into the
        // node's transient state and the next evaluation tessellates them with
        // the workpiece. That is why the popup selects on a click and not on
        // hover.
        onPreview: (preview) {
          setState(() => _msPreview = preview);
          final id = _mechanosynthEditNodeId;
          if (id == null) return;
          if (preview == null) {
            widget.graphModel.mechanosynthEditClearPreview(id);
            return;
          }
          final error = widget.graphModel.mechanosynthEditSelectPreview(
              id, preview.op, preview.candidateIndex);
          if (error != null) showErrorSnackBar(context, error);
        },
        onCancel: _mechanosynthCancel,
      ),
    );
  }

  /// The highlighted row's ghost atoms, projected. A fixed world radius is
  /// projected at each atom's own depth, so a ghost shrinks with distance like
  /// the atoms around it.
  ///
  /// Separate from the widget because the popup placement needs these *before*
  /// the overlay is built: the list is placed clear of the reaction, and the
  /// reaction is these atoms.
  List<ProjectedGhost> _projectGhosts(
      List<APIGhostAtom> atoms, APICamera? camera, CameraTransform? ct) {
    if (atoms.isEmpty || ct == null || camera == null) return const [];

    const worldRadius = 0.35;
    final projected = <ProjectedGhost>[];
    for (final atom in atoms) {
      final centre = _projectWithCamera(
          camera, ct, atom.position.x, atom.position.y, atom.position.z);
      if (centre == null) continue;
      final edge = _projectWithCamera(
        camera,
        ct,
        atom.position.x + ct.right.x * worldRadius,
        atom.position.y + ct.right.y * worldRadius,
        atom.position.z + ct.right.z * worldRadius,
      );
      final tail = _projectWithCamera(
              camera, ct, atom.from.x, atom.from.y, atom.from.z) ??
          centre;
      projected.add(ProjectedGhost(
        kind: atom.kind,
        position: centre,
        from: tail,
        radius: edge == null ? 6.0 : (edge - centre).distance.clamp(3.0, 60.0),
      ));
    }
    return projected;
  }

  /// The anchor ring and the leader line joining it to the popup.
  ///
  /// The **ghost atoms are not here** — they are decorator geometry now,
  /// tessellated with the workpiece and depth-tested against it. What is left
  /// is annotation about the *question*: which atom was asked about, and which
  /// list is answering. Those are deliberately 2D — constant size, never
  /// occluded, drawn even when the atom is behind something.
  ///
  /// Wrapped in a `ClipRect` because a `CustomPainter` is not bounded by its
  /// widget: it paints wherever it is told, and an anchor that projects outside
  /// the viewport used to put the annotation over the node editor and the
  /// property panel.
  Widget? _buildMechanosynthAnnotations(MechanosynthLayout? layout) {
    if (layout == null || layout.anchor == null) return null;

    return Positioned.fill(
      child: IgnorePointer(
        child: ClipRect(
          child: CustomPaint(
            painter: MechanosynthGhostPainter(
              anchor: layout.anchor,
              leaderEnd: layout.leaderEnd,
            ),
          ),
        ),
      ),
    );
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
        scopePath: Uint64List(0),
        nodeId: selectedNode?.id ?? BigInt.zero,
      );

      if (atomEditData != null) {
        final atomicNumber = atomEditData.selectedAtomicNumber;

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
        scopePath: Uint64List(0),
        nodeId: selectedNode?.id ?? BigInt.zero,
      );

      if (editAtomData != null) {
        final camera = common_api.getCamera();
        final cameraTransform = getCameraTransform(camera);
        final planeNormal = cameraTransform!.forward;

        widget.graphModel.addAtomByRay(
          editAtomData.selectedAtomicNumber,
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
    _clearHoverTooltip();
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

    // Build element symbol cursor overlay for AddAtom tool
    Widget? elementSymbolOverlay;
    if (_cursorPosition != null &&
        widget.graphModel.isAtomEditLikeActive &&
        widget.graphModel.activeAtomEditTool == APIAtomEditTool.addAtom) {
      final elementNumber = widget.graphModel.atomEditSelectedElement;
      final symbol =
          elementNumber != null ? elementNumberToSymbol[elementNumber] : null;
      if (symbol != null) {
        elementSymbolOverlay = Positioned(
          left: _cursorPosition!.dx + 16,
          top: _cursorPosition!.dy - 10,
          child: IgnorePointer(
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
              decoration: BoxDecoration(
                color: const Color(0xDD303030),
                borderRadius: BorderRadius.circular(4),
                border: Border.all(
                  color: const Color(0xFF4FC3F7),
                  width: 0.5,
                ),
              ),
              child: Text(
                '+$symbol',
                style: const TextStyle(
                  color: Color(0xFF4FC3F7),
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  decoration: TextDecoration.none,
                ),
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
        // The placement popup holds keyboard focus while it is open, so the
        // viewport must not take it back when the pointer re-enters.
        onEnter: (_) {
          if (_msOffers == null) {
            _focusNode.requestFocus();
          }
        },
        onHover: _onHover,
        onExit: (_) {
          _clearHoverTooltip();
          setState(() => _cursorPosition = null);
        },
        child: LayoutBuilder(
          builder: (context, constraints) {
            // Build atom hover tooltip overlay using layout constraints
            // (context.size is not available during build).
            Widget? atomTooltipOverlay;
            if (_hoveredAtomInfo != null) {
              final info = _hoveredAtomInfo!;
              final screenPos = _projectWorldToScreen(
                info.x,
                info.y,
                info.z,
              );
              if (screenPos != null) {
                const offsetX = 20.0;
                const offsetY = -10.0;
                const estW = 180.0;
                const estH = 70.0;

                final vw = constraints.maxWidth;
                final vh = constraints.maxHeight;

                final left =
                    (screenPos.dx + offsetX).clamp(4.0, vw - estW - 4.0);
                final top =
                    (screenPos.dy + offsetY).clamp(4.0, vh - estH - 4.0);

                atomTooltipOverlay = Positioned(
                  left: left,
                  top: top,
                  child: IgnorePointer(child: AtomTooltip(info: info)),
                );
              }
            }

            // The placement tool's three overlays, from one layout pass so the
            // ring, the leader line and the list agree. The ghosts go under the
            // popup so a preview never hides the list that produced it.
            final msLayout = _mechanosynthLayout(constraints);
            _msLayout = msLayout;
            final ghostOverlay = _buildMechanosynthAnnotations(msLayout);
            final offerPopup = _buildMechanosynthPopup(msLayout);

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
                if (addBondOverlay != null) addBondOverlay,
                if (elementSymbolOverlay != null) elementSymbolOverlay,
                if (atomTooltipOverlay != null) atomTooltipOverlay,
                if (ghostOverlay != null) ghostOverlay,
                if (offerPopup != null) offerPopup,
              ],
            );
          },
        ),
      ),
    );
  }
}

/// Overlay widget for click-to-activate disambiguation.
///
/// Shows a popup near the click position with candidate nodes. Each row has
/// a clickable name (activate) and a solo eye icon (activate + hide others).
class _DisambiguationOverlay extends StatelessWidget {
  final Offset position;
  final List<APICandidateNode> candidates;
  final void Function(APICandidateNode) onActivate;
  final void Function(APICandidateNode) onSolo;
  final VoidCallback onDismiss;

  const _DisambiguationOverlay({
    required this.position,
    required this.candidates,
    required this.onActivate,
    required this.onSolo,
    required this.onDismiss,
  });

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        // Barrier: clicking away dismisses the popup.
        Positioned.fill(
          child: GestureDetector(
            onTap: onDismiss,
            behavior: HitTestBehavior.opaque,
            child: const SizedBox.expand(),
          ),
        ),
        Positioned(
          left: position.dx,
          top: position.dy,
          child: Material(
            elevation: 8,
            borderRadius: BorderRadius.circular(6),
            color: const Color(0xFF2D2D30),
            child: IntrinsicWidth(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  for (final candidate in candidates)
                    _DisambiguationRow(
                      candidate: candidate,
                      onActivate: () => onActivate(candidate),
                      onSolo: () => onSolo(candidate),
                    ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _DisambiguationRow extends StatelessWidget {
  final APICandidateNode candidate;
  final VoidCallback onActivate;
  final VoidCallback onSolo;

  const _DisambiguationRow({
    required this.candidate,
    required this.onActivate,
    required this.onSolo,
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Expanded(
          child: Tooltip(
            message: 'Activate',
            child: InkWell(
              onTap: onActivate,
              child: Padding(
                padding:
                    const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                child: Text(
                  candidate.nodeName,
                  style: const TextStyle(color: Colors.white, fontSize: 13),
                ),
              ),
            ),
          ),
        ),
        IconButton(
          icon: const Icon(Icons.center_focus_strong, size: 18),
          color: Colors.white70,
          tooltip: 'Activate and hide other overlapping nodes',
          onPressed: onSolo,
          constraints: const BoxConstraints(minWidth: 36, minHeight: 36),
          padding: const EdgeInsets.all(6),
        ),
      ],
    );
  }
}
