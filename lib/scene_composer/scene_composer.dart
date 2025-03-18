import 'package:flutter/material.dart';
import 'package:flutter_cad/scene_composer/scene_composer_viewport.dart';
import 'package:file_picker/file_picker.dart';

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

  Future<void> _importXYZ() async {
    // Open file picker for XYZ files
    FilePickerResult? result = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['xyz'],
      dialogTitle: 'Select XYZ File',
    );
    
    if (result != null && result.files.isNotEmpty) {
      String filePath = result.files.first.path!;
      debugPrint('XYZ file selected: $filePath');
      // TODO: Implement actual XYZ file parsing and importing
    } else {
      debugPrint('No XYZ file selected');
    }
  }

  void _exportXYZ() {
    // TODO: Implement XYZ file export functionality
    debugPrint('Export XYZ file selected');
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Container(
          height: 30,
          decoration: const BoxDecoration(
            color: Colors.grey,
            border: Border(
              bottom: BorderSide(
                color: Colors.black26,
                width: 1,
              ),
            ),
          ),
          child: Row(
            children: [
              MenuAnchor(
                builder: (context, controller, child) {
                  return TextButton(
                    onPressed: () {
                      if (controller.isOpen) {
                        controller.close();
                      } else {
                        controller.open();
                      }
                    },
                    style: TextButton.styleFrom(
                      foregroundColor: Colors.black87,
                      padding: const EdgeInsets.symmetric(horizontal: 16),
                    ),
                    child: const Text('File'),
                  );
                },
                menuChildren: [
                  MenuItemButton(
                    onPressed: _importXYZ,
                    child: const Text('Import XYZ'),
                  ),
                  MenuItemButton(
                    onPressed: _exportXYZ,
                    child: const Text('Export XYZ'),
                  ),
                ],
              ),
            ],
          ),
        ),
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
