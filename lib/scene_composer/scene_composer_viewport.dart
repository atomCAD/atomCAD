import 'package:flutter/services.dart';
import 'package:flutter_cad/common/cad_viewport.dart';
import 'package:flutter/material.dart';
import 'package:flutter_cad/src/rust/api/simple.dart';
import 'package:flutter_cad/common/api_utils.dart';
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

    final selectModifier = HardwareKeyboard.instance.isControlPressed
        ? SelectModifier.toggle
        : HardwareKeyboard.instance.isShiftPressed
            ? SelectModifier.expand
            : SelectModifier.replace;

    widget.model.selectClusterByRay(
      ray.start,
      ray.direction,
      selectModifier,
    );
    renderingNeeded();
  }
}
