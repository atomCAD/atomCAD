import 'package:flutter/material.dart';
import 'package:flutter_cad/scene_composer/scene_composer_viewport.dart';

/// The scene composer editor.
class SceneComposer extends StatefulWidget {
  const SceneComposer({super.key});

  @override
  State<SceneComposer> createState() => _SceneComposerState();
}

class _SceneComposerState extends State<SceneComposer> {
  @override
  void initState() {
    super.initState();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Center(
          child: SizedBox(
            width: 1280,
            height: 544,
            child: SceneComposerViewport(),
          ),
        ),
      ],
    );
  }
}
