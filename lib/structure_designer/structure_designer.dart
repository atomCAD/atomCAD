import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_viewport.dart';
import 'package:flutter_cad/structure_designer/node_network.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';
import 'package:flutter_cad/structure_designer/node_networks_list_panel.dart';
import 'package:flutter_cad/structure_designer/camera_control_widget.dart';
import 'package:flutter_cad/common/section.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter_cad/structure_designer/geometry_visualization_widget.dart';
import 'package:flutter_cad/structure_designer/preferences_window.dart';

/// The structure designer editor.
class StructureDesigner extends StatefulWidget {
  const StructureDesigner({super.key});

  @override
  State<StructureDesigner> createState() => _StructureDesignerState();
}

class _StructureDesignerState extends State<StructureDesigner> {
  late StructureDesignerModel graphModel;

  @override
  void initState() {
    super.initState();
    graphModel = StructureDesignerModel();
    graphModel.init();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        // Menu bar
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
                    onPressed: _loadDesign,
                    child: const Text('Load Design'),
                  ),
                  MenuItemButton(
                    onPressed: _saveDesignAs,
                    child: const Text('Save Design As'),
                  ),
                ],
              ),
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
                    child: const Text('Edit'),
                  );
                },
                menuChildren: [
                  MenuItemButton(
                    onPressed: _showPreferences,
                    child: const Text('Preferences'),
                  ),
                ],
              ),
            ],
          ),
        ),
        // Main content
        Expanded(
          child: Row(
            children: [
              // Node Networks List Panel (left sidebar)
              Container(
                width: 200,
                decoration: const BoxDecoration(
                  border: Border(
                    right: BorderSide(
                      color: Colors.grey,
                      width: 1,
                    ),
                  ),
                ),
                child: Column(
                  children: [
                    // Geometry Visualization section
                    Section(
                      title: 'Geometry',
                      content: Padding(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 8.0, vertical: 4.0),
                        child: GeometryVisualizationWidget(model: graphModel),
                      ),
                      expand: false,
                    ),
                    const SizedBox(height: 8),
                    // Camera Control section
                    Section(
                      title: 'Camera control',
                      content: Padding(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 8.0, vertical: 4.0),
                        child: CameraControlWidget(model: graphModel),
                      ),
                      expand: false,
                    ),
                    const SizedBox(height: 8),
                    // Node networks section
                    Expanded(
                      flex: 5,
                      child: Section(
                        title: 'Node networks',
                        content: NodeNetworksListPanel(model: graphModel),
                        expand: true,
                      ),
                    ),
                  ],
                ),
              ),
              // Main content area
              Expanded(
                child: Column(
                  children: [
                    Expanded(
                      flex: 20,
                      child: StructureDesignerViewport(graphModel: graphModel),
                    ),
                    Expanded(
                      flex: 11,
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Expanded(
                            flex: 4,
                            child: NodeNetwork(graphModel: graphModel),
                          ),
                          Container(
                            width: 300,
                            padding: const EdgeInsets.all(8.0),
                            decoration: const BoxDecoration(
                              border: Border(
                                left: BorderSide(
                                  color: Colors.grey,
                                  width: 1,
                                ),
                              ),
                            ),
                            child: NodeDataWidget(graphModel: graphModel),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Future<void> _loadDesign() async {
    // Open file picker for CNND files
    FilePickerResult? result = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['cnnd'],
      dialogTitle: 'Load Design File',
    );

    if (result != null && result.files.isNotEmpty) {
      String filePath = result.files.first.path!;
      debugPrint('Design file selected: $filePath');
      graphModel.loadNodeNetworks(filePath);
    } else {
      debugPrint('No design file selected');
    }
  }

  Future<void> _saveDesignAs() async {
    // Open file picker for saving CNND files
    String? outputFile = await FilePicker.platform.saveFile(
      dialogTitle: 'Save Design As',
      fileName: 'design.atomcad',
      allowedExtensions: ['atomcad'],
    );

    if (outputFile != null) {
      graphModel.saveNodeNetworks(outputFile);
    }
  }

  void _showPreferences() {
    showDialog(
      context: context,
      barrierDismissible: true, // Allow dismissing when clicking outside
      builder: (context) {
        return PreferencesWindow(model: graphModel);
      },
    );
  }
}
