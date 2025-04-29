import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer_viewport.dart';
import 'package:flutter_cad/structure_designer/node_network.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';
import 'package:flutter_cad/structure_designer/node_networks_list_panel.dart';
import 'package:flutter_cad/common/section.dart';
import 'package:file_picker/file_picker.dart';
import 'package:provider/provider.dart';

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
  }

  @override
  Widget build(BuildContext context) {
    // Initialize the graph model here
    graphModel.init("sample");

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
                child: Section(
                  title: 'Node networks',
                  content: NodeNetworksListPanel(model: graphModel),
                  expand: true,
                ),
              ),
              // Main content area
              Expanded(
                child: Column(
                  children: [
                    Expanded(
                      flex: 2,
                      child: StructureDesignerViewport(graphModel: graphModel),
                    ),
                    Expanded(
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
    String? outputPath = await FilePicker.platform.saveFile(
      dialogTitle: 'Save Design File',
      fileName: 'design.cnnd',
      type: FileType.custom,
      allowedExtensions: ['cnnd'],
    );

    if (outputPath != null) {
      debugPrint('Saving design file to: $outputPath');
      graphModel.saveNodeNetworks(outputPath);
    } else {
      debugPrint('Design file save canceled');
    }
  }
}
