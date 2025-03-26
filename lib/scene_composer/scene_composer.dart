import 'package:flutter/material.dart';
import 'package:flutter_cad/scene_composer/scene_composer_viewport.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:flutter_cad/scene_composer/cluster_list_panel.dart';
import 'package:flutter_cad/scene_composer/scene_selection_data_widget.dart';

/// The scene composer editor.
class SceneComposer extends StatefulWidget {
  const SceneComposer({super.key});

  @override
  State<SceneComposer> createState() => _SceneComposerState();
}

class _SceneComposerState extends State<SceneComposer> {
  // GlobalKey to access the viewport state
  final _viewportKey = GlobalKey();

  late SceneComposerModel model;

  @override
  void initState() {
    super.initState();
    model = SceneComposerModel();
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
      model.importXyz(filePath);

      // Trigger rendering in the viewport by accessing its state
      if (_viewportKey.currentState != null) {
        (_viewportKey.currentState as dynamic).renderingNeeded();
      }
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
        Expanded(
          child: Row(
            children: [
              // Left panel - Cluster List
              SizedBox(
                width: 300,
                child: Column(
                  children: [
                    Expanded(
                      child: ClusterListPanel(model: model),
                    ),
                    const Divider(
                        height: 1, thickness: 1, color: Colors.black12),
                    SceneSelectionDataWidget(model: model),
                  ],
                ),
              ),
              // Vertical divider
              const VerticalDivider(
                width: 1,
                thickness: 1,
                color: Colors.black12,
              ),
              // Main viewport
              Expanded(
                child: SceneComposerViewport(
                  key: _viewportKey,
                  model: model,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
