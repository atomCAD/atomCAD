import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer.dart';
import 'package:flutter_cad/scene_composer/scene_composer.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/common_api.dart';
import 'package:window_manager/window_manager.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';

Future<void> main() async {
  await RustLib.init();

  WidgetsFlutterBinding.ensureInitialized();
  await windowManager.ensureInitialized();

  windowManager.setTitle('atomCAD');

  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider(
      create: (_) => MouseWheelBlockService(),
      child: const MaterialApp(
        home: EditorSelector(),
      ),
    );
  }
}

class EditorSelector extends StatelessWidget {
  const EditorSelector({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('atomCAD Editor Selection'),
      ),
      body: Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            ElevatedButton(
              onPressed: () {
                setActiveEditor(editor: Editor.structureDesigner);
                Navigator.of(context).pushReplacement(
                  MaterialPageRoute(
                    builder: (context) => const Scaffold(
                      body: StructureDesigner(),
                    ),
                  ),
                );
              },
              style: AppButtonStyles.primary,
              child: const Text('Structure Designer'),
            ),
            const SizedBox(height: 20),
            ElevatedButton(
              onPressed: () {
                setActiveEditor(editor: Editor.sceneComposer);
                Navigator.of(context).pushReplacement(
                  MaterialPageRoute(
                    builder: (context) => const Scaffold(
                      body: SceneComposer(),
                    ),
                  ),
                );
              },
              style: AppButtonStyles.primary,
              child: const Text('Scene Composer'),
            ),
          ],
        ),
      ),
    );
  }
}
