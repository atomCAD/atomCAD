import 'package:flutter/services.dart';
import 'package:flutter_cad/common/cad_viewport.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:flutter_cad/common/api_utils.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';

class SceneComposerViewport extends CadViewport {
  const SceneComposerViewport({
    super.key,
  });

  @override
  _SceneComposerViewportState createState() => _SceneComposerViewportState();
}

class _SceneComposerViewportState
    extends CadViewportState<SceneComposerViewport> {
  @override
  void onDefaultClick(Offset pointerPos) {
    final ray = getRayFromPointerPos(pointerPos);

    final selectModifier = HardwareKeyboard.instance.isControlPressed
        ? SelectModifier.toggle
        : HardwareKeyboard.instance.isShiftPressed
            ? SelectModifier.expand
            : SelectModifier.replace;

    selectCluster(
        rayStart: Vector3ToAPIVec3(ray.start),
        rayDir: Vector3ToAPIVec3(ray.direction),
        selectModifier: selectModifier);
    renderingNeeded();
  }
}
