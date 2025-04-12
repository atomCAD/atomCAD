import 'package:flutter/material.dart';
import 'package:flutter_cad/scene_composer/scene_composer_viewport.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter_cad/scene_composer/scene_composer_model.dart';
import 'package:flutter_cad/scene_composer/cluster_list_panel.dart';
import 'package:flutter_cad/scene_composer/scene_selection_data_widget.dart';
import 'package:flutter_cad/common/section.dart';
import 'package:flutter_cad/scene_composer/scene_composer_tools_panel.dart';
import 'package:flutter_cad/scene_composer/transform_control_widget.dart';
import 'package:flutter_cad/scene_composer/atom_info_widget.dart';
import 'package:flutter_cad/src/rust/api/api_types.dart';
import 'package:provider/provider.dart';

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
  APITransform? _stagedCameraTransform;
  SceneComposerView? _sceneComposerView;

  @override
  void initState() {
    super.initState();
    model = SceneComposerModel();
    _updateStagedCameraTransform();
  }

  void _updateStagedCameraTransform() {
    setState(() {
      _stagedCameraTransform = model.getCameraTransform();
    });
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

  Future<void> _exportXYZ() async {
    // Open file picker for saving XYZ files
    String? outputPath = await FilePicker.platform.saveFile(
      dialogTitle: 'Save XYZ File',
      fileName: 'scene.xyz',
      type: FileType.custom,
      allowedExtensions: ['xyz'],
    );

    if (outputPath != null) {
      debugPrint('Exporting XYZ file to: $outputPath');
      model.exportXyz(outputPath);
    } else {
      debugPrint('XYZ file export canceled');
    }
  }

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: model,
      child: Column(
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
                      onPressed: () {
                        model.newModel();
                      },
                      child: const Text('New Scene'),
                    ),
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
                Consumer<SceneComposerModel>(
                  builder: (context, model, child) {
                    return MenuAnchor(
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
                          child: const Text('Edit'),
                        );
                      },
                      menuChildren: [
                        MenuItemButton(
                          onPressed: model.sceneComposerView?.isUndoAvailable == true
                              ? () => model.undo()
                              : null,
                          child: const Text('Undo'),
                        ),
                        MenuItemButton(
                          onPressed: model.sceneComposerView?.isRedoAvailable == true
                              ? () => model.redo()
                              : null,
                          child: const Text('Redo'),
                        ),
                      ],
                    );
                  },
                ),
              ],
            ),
          ),
          Expanded(
            child: Row(
              children: [
                // Left panel
                SizedBox(
                  width: 340,
                  child: Column(
                    children: [
                      Section(
                        title: 'Tools',
                        content: SceneComposerToolsPanel(model: model),
                        addBottomPadding: true,
                      ),
                      Expanded(
                        child: Section(
                          title: 'Clusters',
                          content: ClusterListPanel(model: model),
                          expand: true,
                        ),
                      ),
                      Consumer<SceneComposerModel>(
                        builder: (context, sceneModel, child) {
                          // Update staged camera transform when the model view changes
                          if (_sceneComposerView !=
                              sceneModel.sceneComposerView) {
                            _stagedCameraTransform =
                                sceneModel.getCameraTransform();
                          }
                          _sceneComposerView = sceneModel.sceneComposerView;

                          return Section(
                            title: 'Camera',
                            content: TransformControlWidget(
                              initialTransform: _stagedCameraTransform,
                              onApplyTransform: (transform) {
                                sceneModel.setCameraTransform(transform);
                              },
                            ),
                            addBottomPadding: true,
                          );
                        },
                      ),
                      Consumer<SceneComposerModel>(
                        builder: (context, sceneModel, child) {
                          final activeTool =
                              sceneModel.sceneComposerView?.activeTool;

                          if (activeTool == APISceneComposerTool.align) {
                            return Section(
                              title: 'Align tool',
                              content: Padding(
                                padding: const EdgeInsets.all(8.0),
                                child: Text(
                                  sceneModel.alignToolStateText,
                                  style: const TextStyle(
                                    fontSize: 14,
                                    fontFamily: 'monospace',
                                  ),
                                ),
                              ),
                              addBottomPadding: false,
                            );
                          } else if (activeTool ==
                              APISceneComposerTool.distance) {
                            return Section(
                              title: 'Distance tool',
                              content: Padding(
                                padding: const EdgeInsets.all(8.0),
                                child: Text(
                                  sceneModel.distanceToolStateText,
                                  style: const TextStyle(
                                    fontSize: 14,
                                    fontFamily: 'monospace',
                                  ),
                                ),
                              ),
                              addBottomPadding: false,
                            );
                          } else if (activeTool ==
                              APISceneComposerTool.atomInfo) {
                            return Section(
                              title: 'Atom Information',
                              content: AtomInfoWidget(model: sceneModel),
                              addBottomPadding: false,
                            );
                          } else {
                            // Default tool or null
                            return Section(
                              title: 'Default tool',
                              content:
                                  SceneSelectionDataWidget(model: sceneModel),
                              addBottomPadding: false,
                            );
                          }
                        },
                      ),
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
                Consumer<SceneComposerModel>(
                  builder: (context, model, child) {
                    return MenuAnchor(
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
                          child: const Text('Edit'),
                        );
                      },
                      menuChildren: [
                        MenuItemButton(
                          onPressed: model.sceneComposerView?.isUndoAvailable == true
                              ? () => model.undo()
                              : null,
                          child: const Text('Undo'),
                        ),
                        MenuItemButton(
                          onPressed: model.sceneComposerView?.isRedoAvailable == true
                              ? () => model.redo()
                              : null,
                          child: const Text('Redo'),
                        ),
                      ],
                    );
                  },
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
