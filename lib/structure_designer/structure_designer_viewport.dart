import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/structure_designer/graph_model.dart';
import 'package:flutter_cad/common/cad_viewport.dart';

class StructureDesignerViewport extends CadViewport {
  final GraphModel graphModel;

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
  bool _isGadgetDragging = false;

  int draggedGadgetHandle =
      -1; // Relevant when _dragState == ViewportDragState.gadgetDrag

  @override
  void startPrimaryDrag(Offset pointerPos) {
    super.startPrimaryDrag(pointerPos);
    final ray = getRayFromPointerPos(pointerPos);

    final hitResult = gadgetHitTest(
        rayOrigin: Vector3ToAPIVec3(ray.start),
        rayDirection: Vector3ToAPIVec3(ray.direction));

    if (hitResult != null) {
      _isGadgetDragging = true;
      draggedGadgetHandle = hitResult;
      gadgetStartDrag(
          nodeNetworkName: "sample", // TODO: this should not be needed
          handleIndex: draggedGadgetHandle,
          rayOrigin: Vector3ToAPIVec3(ray.start),
          rayDirection: Vector3ToAPIVec3(ray.direction));
      renderingNeeded();
    }
  }

  @override
  void defaultDrag(Offset pointerPos) {
    super.defaultDrag(pointerPos);
    if (_isGadgetDragging) {
      _dragGadget(pointerPos);
    }
  }

  void _dragGadget(Offset pointerPos) {
    final ray = getRayFromPointerPos(pointerPos);
    gadgetDrag(
        nodeNetworkName: "sample", // TODO: this should not be needed
        handleIndex: draggedGadgetHandle,
        rayOrigin: Vector3ToAPIVec3(ray.start),
        rayDirection: Vector3ToAPIVec3(ray.direction));
    syncGadgetData(nodeNetworkName: "sample");
    renderingNeeded();
    widget.graphModel
        .refreshFromKernel(); // Refresh other widgets when dragging a gadget
  }

  @override
  void endDrag(Offset pointerPos) {
    final oldDragState = dragState;
    super.endDrag(pointerPos);

    if (oldDragState == ViewportDragState.defaultDrag && _isGadgetDragging) {
      gadgetEndDrag(
          nodeNetworkName: "sample"); // TODO: this should not be needed
      renderingNeeded();
      _isGadgetDragging = false;
    }
  }

  @override
  void initState() {
    super.initState();
    initTexture();
  }

  @override
  void dispose() {
    super.dispose();
  }
}
