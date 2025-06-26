import 'package:flutter/material.dart';
import 'package:flutter_resizable_container/flutter_resizable_container.dart';
import 'package:flutter_cad/structure_designer/structure_designer_viewport.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';
import 'package:flutter_cad/structure_designer/node_networks_list_panel.dart';
import 'package:flutter_cad/structure_designer/camera_control_widget.dart';
import 'package:flutter_cad/common/section.dart';
import 'package:file_picker/file_picker.dart';
import 'package:flutter_cad/structure_designer/geometry_visualization_widget.dart';
import 'package:flutter_cad/structure_designer/preferences_window.dart';
import 'package:flutter_cad/common/menu_widget.dart';

/// The structure designer editor.
class StructureDesigner extends StatefulWidget {
  const StructureDesigner({super.key});

  @override
  State<StructureDesigner> createState() => _StructureDesignerState();
}

class _StructureDesignerState extends State<StructureDesigner> {
  late StructureDesignerModel graphModel;

  // GlobalKey to access the NodeNetwork widget state
  final GlobalKey<NodeNetworkState> nodeNetworkKey =
      GlobalKey<NodeNetworkState>();

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
              // File Menu
              MenuWidget(
                label: 'File',
                menuItems: [
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

              // View Menu
              MenuWidget(
                label: 'View',
                menuItems: [
                  MenuItemButton(
                    onPressed: _resetNodeNetworkView,
                    child: const Text('Reset node network view'),
                  ),
                ],
              ),

              // Edit Menu
              MenuWidget(
                label: 'Edit',
                menuItems: [
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
                child: ResizableContainer(
                  direction: Axis.vertical,
                  children: [
                    // 3D Viewport panel - initially 65% of height
                    ResizableChild(
                      size: ResizableSize.ratio(0.65),
                      // Custom divider that appears below this panel
                      divider: ResizableDivider(
                        thickness: 8, // Height of the divider
                        color: Colors.grey.shade300,
                        cursor: SystemMouseCursors.resizeRow,
                      ),
                      child: StructureDesignerViewport(graphModel: graphModel),
                    ),
                    // Node Network panel - initially 35% of height
                    ResizableChild(
                      size: ResizableSize.ratio(0.35),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Expanded(
                            flex: 4,
                            child: NodeNetwork(
                                key: nodeNetworkKey, graphModel: graphModel),
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

  /// Reset the node network view to show all nodes
  void _resetNodeNetworkView() {
    // Access the NodeNetworkState directly through the key
    final state = nodeNetworkKey.currentState;

    // Call the updatePanOffsetForCurrentNetwork method with forceUpdate=true if state exists
    if (state != null) {
      state.updatePanOffsetForCurrentNetwork(forceUpdate: true);
    }
  }
}
