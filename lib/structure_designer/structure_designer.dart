import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/node_network/node_network.dart';
import 'package:flutter_cad/structure_designer/main_content_area.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_networks_list_panel.dart';
import 'package:flutter_cad/structure_designer/node_display_widget.dart';
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

  // Whether the division between viewport and node network is vertical (true) or horizontal (false)
  bool verticalDivision = true;

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
                  MenuItemButton(
                    onPressed: _exportVisibleAsXyz,
                    child: const Text('Export visible as XYZ'),
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
                  MenuItemButton(
                    onPressed: _toggleDivisionOrientation,
                    child: Text(verticalDivision
                        ? 'Switch to Horizontal Layout'
                        : 'Switch to Vertical Layout'),
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
                    // Display settings section
                    Section(
                      title: 'Display',
                      content: Padding(
                        padding: const EdgeInsets.symmetric(
                            horizontal: 8.0, vertical: 4.0),
                        child: Row(
                          mainAxisAlignment: MainAxisAlignment.spaceBetween,
                          children: [
                            // Geometry visualization widget (left aligned)
                            GeometryVisualizationWidget(model: graphModel),
                            
                            // Node display widget (right aligned)
                            NodeDisplayWidget(model: graphModel),
                          ],
                        ),
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
              MainContentArea(
                graphModel: graphModel,
                nodeNetworkKey: nodeNetworkKey,
                verticalDivision: verticalDivision,
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
      final loadResult = graphModel.loadNodeNetworks(filePath);
      
      if (!loadResult.success) {
        // Show error dialog
        if (mounted) {
          showDialog(
            context: context,
            builder: (BuildContext context) {
              return AlertDialog(
                title: const Text('Load Error'),
                content: Text(loadResult.errorMessage),
                actions: [
                  TextButton(
                    onPressed: () => Navigator.of(context).pop(),
                    child: const Text('OK'),
                  ),
                ],
              );
            },
          );
        }
      }
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

  /// Toggle between vertical and horizontal division orientation
  void _toggleDivisionOrientation() {
    setState(() {
      verticalDivision = !verticalDivision;
    });
  }

  /// Export visible atomic structures as XYZ file
  Future<void> _exportVisibleAsXyz() async {
    try {
      // Open file picker for saving XYZ files
      String? outputFile = await FilePicker.platform.saveFile(
        dialogTitle: 'Export visible as XYZ',
        fileName: 'structure.xyz',
        type: FileType.custom,
        allowedExtensions: ['xyz'],
      );

      if (outputFile != null) {
        // Call the export method
        final result = graphModel.exportVisibleAtomicStructuresAsXyz(outputFile);
        
        // Check if there was an error
        if (!result.success) {
          // Show error dialog
          if (mounted) {
            showDialog(
              context: context,
              builder: (context) => AlertDialog(
                title: const Text('Export Error'),
                content: Text(result.errorMessage),
                actions: [
                  TextButton(
                    onPressed: () => Navigator.of(context).pop(),
                    child: const Text('OK'),
                  ),
                ],
              ),
            );
          }
        }
      }
    } catch (e) {
      // Handle any unexpected errors
      if (mounted) {
        showDialog(
          context: context,
          builder: (context) => AlertDialog(
            title: const Text('Export Error'),
            content: Text('An unexpected error occurred: $e'),
            actions: [
              TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: const Text('OK'),
              ),
            ],
          ),
        );
      }
    }
  }
}
