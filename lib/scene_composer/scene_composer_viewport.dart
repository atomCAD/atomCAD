import 'package:flutter_cad/common/cad_viewport.dart';

class SceneComposerViewport extends CadViewport {
  const SceneComposerViewport({
    super.key,
  });

  @override
  _SceneComposerViewportState createState() => _SceneComposerViewportState();
}

class _SceneComposerViewportState
    extends CadViewportState<SceneComposerViewport> {}
