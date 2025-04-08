import 'package:flutter/services.dart';
import 'package:flutter_cad/common/cad_viewport.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';

class SceneComposerViewport extends CadViewport {
  final SceneComposerModel model;
  const SceneComposerViewport({
    super.key,
    required this.model,
  });

  @override
  _SceneComposerViewportState createState() => _SceneComposerViewportState();
}

class _SceneComposerViewportState
    extends CadViewportState<SceneComposerViewport> {
  @override
  void onDefaultClick(Offset pointerPos) {
    final ray = getRayFromPointerPos(pointerPos);

    final activeTool = widget.model.sceneComposerView?.activeTool;

    if (activeTool == APISceneComposerTool.align) {
      if (widget.model.selectAlignAtomByRay(ray.start, ray.direction) != null) {
        renderingNeeded();
      }
    } else if (activeTool == APISceneComposerTool.distance) {
      if (widget.model.selectDistanceAtomByRay(ray.start, ray.direction) !=
          null) {
        renderingNeeded();
      }
    } else if (activeTool == APISceneComposerTool.atomInfo) {
      if (widget.model.selectAtomInfoAtomByRay(ray.start, ray.direction) !=
          null) {
        renderingNeeded();
      }
    } else if (activeTool == APISceneComposerTool.default_) {
      final selectModifier = HardwareKeyboard.instance.isControlPressed
          ? SelectModifier.toggle
          : HardwareKeyboard.instance.isShiftPressed
              ? SelectModifier.expand
              : SelectModifier.replace;

      if (widget.model.selectClusterByRay(
            ray.start,
            ray.direction,
            selectModifier,
          ) !=
          null) {
        renderingNeeded();
      }
    }
  }

  @override
  int transformDraggedGadgetHandle(int handleIndex) {
    // If the shift key ispressed when we start the drag of the axes,
    // we rotate instead of translating.
    if (handleIndex >= 0 &&
        handleIndex <= 2 &&
        HardwareKeyboard.instance.isShiftPressed) {
      return handleIndex + 3;
    }
    return handleIndex;
  }

  @override
  void refreshFromKernel() {
    widget.model.refreshFromKernel();
  }
}
