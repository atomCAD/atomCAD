import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:file_picker/file_picker.dart';
import '../common/draggable_dialog.dart';
import '../common/menu_widget.dart';
import '../common/section.dart';
import 'structure_designer_model.dart';
import 'node_network/node_network.dart';
import 'atomic_structure_visualization_widget.dart';
import 'geometry_visualization_widget.dart';
import 'import_cnnd_library_dialog.dart';
import 'node_networks_list/node_networks_panel.dart';
import 'node_display_widget.dart';
import 'node_data/node_data_widget.dart';
import 'direct_mode_display_widget.dart';
import 'camera_control_widget.dart';
import 'preferences_window.dart';
import 'main_content_area.dart';

/// The structure designer editor.
class StructureDesigner extends StatefulWidget {
  final StructureDesignerModel model;

  const StructureDesigner({super.key, required this.model});

  @override
  State<StructureDesigner> createState() => _StructureDesignerState();
}

class _StructureDesignerState extends State<StructureDesigner> {
  late StructureDesignerModel graphModel;

  // Whether the division between viewport and node network is vertical (true) or horizontal (false)
  bool verticalDivision = true;

  // Resizable sidebar width for direct editing mode
  double _directEditingSidebarWidth = 360;

  // GlobalKey to access the NodeNetwork widget state
  final GlobalKey<NodeNetworkState> nodeNetworkKey =
      GlobalKey<NodeNetworkState>();

  @override
  void initState() {
    super.initState();
    graphModel = widget.model;
  }

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: graphModel,
      child: Focus(
        onKeyEvent: _handleGlobalKeyEvent,
        child: Column(
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
                  Consumer<StructureDesignerModel>(
                    builder: (context, model, child) {
                      return MenuWidget(
                        key: const Key('file_menu'),
                        label: 'File',
                        menuItems: [
                          MenuItemButton(
                            key: const Key('new_design_item'),
                            onPressed: _newDesign,
                            child: const Text('New'),
                          ),
                          MenuItemButton(
                            key: const Key('load_design_item'),
                            onPressed: _loadDesign,
                            child: const Text('Load Design'),
                          ),
                          MenuItemButton(
                            key: const Key('save_design_item'),
                            onPressed: model.canSave ? _saveDesign : null,
                            child: const Text('Save Design'),
                          ),
                          MenuItemButton(
                            key: const Key('save_design_as_item'),
                            onPressed: _saveDesignAs,
                            child: const Text('Save Design As'),
                          ),
                          MenuItemButton(
                            key: const Key('export_visible_item'),
                            onPressed: _exportVisible,
                            child: const Text('Export visible'),
                          ),
                          if (model.directEditingMode)
                            MenuItemButton(
                              key: const Key('import_xyz_item'),
                              onPressed: _importXyz,
                              child: const Text('Import XYZ'),
                            ),
                          if (!model.directEditingMode)
                            MenuItemButton(
                              key: const Key('import_from_library_item'),
                              onPressed: _importFromCnndLibrary,
                              child: const Text('Import from .cnnd library'),
                            ),
                        ],
                      );
                    },
                  ),

                  // View Menu
                  Consumer<StructureDesignerModel>(
                    builder: (context, model, child) {
                      return MenuWidget(
                        key: const Key('view_menu'),
                        label: 'View',
                        menuItems: [
                          if (model.directEditingMode)
                            MenuItemButton(
                              key: const Key('switch_to_node_network_item'),
                              onPressed: () =>
                                  graphModel.switchToNodeNetworkMode(),
                              child: const Text('Switch to Node Network Mode'),
                            ),
                          if (!model.directEditingMode)
                            MenuItemButton(
                              key: const Key('switch_to_direct_editing_item'),
                              onPressed: model.canSwitchToDirectEditingMode
                                  ? () => graphModel.switchToDirectEditingMode()
                                  : null,
                              child:
                                  const Text('Switch to Direct Editing Mode'),
                            ),
                          if (!model.directEditingMode)
                            MenuItemButton(
                              key: const Key('reset_view_item'),
                              onPressed: _resetNodeNetworkView,
                              child: const Text('Reset node network view'),
                            ),
                          if (!model.directEditingMode)
                            MenuItemButton(
                              key: const Key('toggle_layout_item'),
                              onPressed: _toggleDivisionOrientation,
                              child: Text(verticalDivision
                                  ? 'Switch to Horizontal Layout'
                                  : 'Switch to Vertical Layout'),
                            ),
                        ],
                      );
                    },
                  ),

                  // Edit Menu
                  Consumer<StructureDesignerModel>(
                    builder: (context, model, child) {
                      return MenuWidget(
                        key: const Key('edit_menu'),
                        label: 'Edit',
                        menuItems: [
                          MenuItemButton(
                            key: const Key('undo_item'),
                            onPressed: model.canUndo
                                ? () {
                                    final desc = graphModel.undo();
                                    if (desc != null) {
                                      _showUndoRedoSnackBar(
                                          context, 'Undo: $desc');
                                    }
                                  }
                                : null,
                            shortcut: const SingleActivator(
                                LogicalKeyboardKey.keyZ,
                                control: true),
                            child: Text(model.undoDescription != null
                                ? 'Undo ${model.undoDescription}'
                                : 'Undo'),
                          ),
                          MenuItemButton(
                            key: const Key('redo_item'),
                            onPressed: model.canRedo
                                ? () {
                                    final desc = graphModel.redo();
                                    if (desc != null) {
                                      _showUndoRedoSnackBar(
                                          context, 'Redo: $desc');
                                    }
                                  }
                                : null,
                            shortcut: const SingleActivator(
                                LogicalKeyboardKey.keyZ,
                                control: true,
                                shift: true),
                            child: Text(model.redoDescription != null
                                ? 'Redo ${model.redoDescription}'
                                : 'Redo'),
                          ),
                          if (!model.directEditingMode) ...[
                            const Divider(),
                            MenuItemButton(
                              key: const Key('validate_network_item'),
                              onPressed: () {
                                widget.model.validateActiveNetwork();
                              },
                              child: const Text('Validate active network'),
                            ),
                            MenuItemButton(
                              key: const Key('auto_layout_network_item'),
                              onPressed: () {
                                widget.model.autoLayoutNetwork();
                                // Reset view to show all nodes after layout
                                final state = nodeNetworkKey.currentState;
                                if (state != null) {
                                  state.updatePanOffsetForCurrentNetwork(
                                      forceUpdate: true);
                                }
                              },
                              child: const Text('Auto-Layout Network'),
                            ),
                          ],
                          MenuItemButton(
                            key: const Key('preferences_item'),
                            onPressed: _showPreferences,
                            child: const Text('Preferences'),
                          ),
                        ],
                      );
                    },
                  ),
                ],
              ),
            ),
            // Main content
            Expanded(
              child: Consumer<StructureDesignerModel>(
                builder: (context, model, child) {
                  return Stack(
                    children: [
                      Row(
                        children: [
                          // Left sidebar
                          model.directEditingMode
                              ? _buildDirectEditingSidebar()
                              : _buildNodeNetworkSidebar(),
                          // Main content area
                          MainContentArea(
                            graphModel: graphModel,
                            nodeNetworkKey: nodeNetworkKey,
                            verticalDivision: verticalDivision,
                            directEditingMode: model.directEditingMode,
                          ),
                        ],
                      ),
                      // Validation warning banner (direct editing mode only)
                      if (model.directEditingMode && model.hasValidationErrors)
                        _buildValidationWarningBanner(),
                    ],
                  );
                },
              ),
            ),
          ],
        ),
      ),
    );
  }

  /// Builds the left sidebar for Direct Editing Mode.
  /// Contains simplified Display, Camera Control, and the atom edit editor.
  Widget _buildDirectEditingSidebar() {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: _directEditingSidebarWidth,
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
              // Simplified Display section (atomic visualization + mode switch)
              Section(
                title: 'Display',
                content: Padding(
                  padding: const EdgeInsets.symmetric(
                      horizontal: 8.0, vertical: 4.0),
                  child: DirectModeDisplayWidget(model: graphModel),
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
              // Atom Edit Editor (via NodeDataWidget, which routes to AtomEditEditor)
              // Wrapped in Expanded so the sidebar Column gives it bounded height;
              // without this, the Column passes infinite height to non-flex children,
              // and Section(expand: true) uses Flexible/Expanded internally which
              // requires bounded constraints.
              Expanded(
                child: Section(
                  title: 'Editor',
                  content: NodeDataWidget(
                    graphModel: graphModel,
                    directEditingMode: true,
                  ),
                  expand: true,
                ),
              ),
            ],
          ),
        ),
        // Drag handle for resizing
        GestureDetector(
          onHorizontalDragUpdate: (details) {
            setState(() {
              _directEditingSidebarWidth =
                  (_directEditingSidebarWidth + details.delta.dx)
                      .clamp(250, 500);
            });
          },
          child: MouseRegion(
            cursor: SystemMouseCursors.resizeColumn,
            child: Container(
              width: 6,
              color: Colors.grey.shade300,
            ),
          ),
        ),
      ],
    );
  }

  /// Builds the left sidebar for Node Network Mode.
  /// Contains full Display, Camera Control, and Node Networks panel.
  Widget _buildNodeNetworkSidebar() {
    return Container(
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
              padding:
                  const EdgeInsets.symmetric(horizontal: 8.0, vertical: 4.0),
              child: Column(
                children: [
                  // First row: Geometry visualization and Node display
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      GeometryVisualizationWidget(model: graphModel),
                      NodeDisplayWidget(model: graphModel),
                    ],
                  ),
                  const SizedBox(height: 8),
                  // Second row: Atomic structure visualization + mode switch
                  Row(
                    children: [
                      AtomicStructureVisualizationWidget(model: graphModel),
                      const SizedBox(width: 8),
                      Container(
                        width: 1,
                        height: 20,
                        color: Colors.grey.shade400,
                      ),
                      const SizedBox(width: 8),
                      ModeToggleButtons(model: graphModel),
                    ],
                  ),
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
              padding:
                  const EdgeInsets.symmetric(horizontal: 8.0, vertical: 4.0),
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
              content: NodeNetworksPanel(model: graphModel),
              expand: true,
            ),
          ),
        ],
      ),
    );
  }

  /// Builds the validation warning banner shown at the top of the viewport
  /// in Direct Editing Mode when the network has validation errors.
  Widget _buildValidationWarningBanner() {
    return Positioned(
      top: 8,
      left: _directEditingSidebarWidth +
          14, // Sidebar width + drag handle + offset
      right: 8,
      child: Center(
        child: Material(
          elevation: 2,
          borderRadius: BorderRadius.circular(6),
          color: Colors.orange.shade100,
          child: InkWell(
            borderRadius: BorderRadius.circular(6),
            onTap: () => graphModel.switchToNodeNetworkMode(),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(Icons.warning_amber_rounded,
                      size: 18, color: Colors.orange.shade800),
                  const SizedBox(width: 8),
                  Text(
                    'Network has issues \u2014 click to inspect in Node Network Mode.',
                    style: TextStyle(
                      fontSize: 12,
                      color: Colors.orange.shade900,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  /// Global keyboard handler for undo/redo.
  /// Catches Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y regardless of which panel has focus.
  KeyEventResult _handleGlobalKeyEvent(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;

    if (HardwareKeyboard.instance.isControlPressed) {
      // Ctrl+Shift+Z or Ctrl+Y: Redo
      if ((HardwareKeyboard.instance.isShiftPressed &&
              event.logicalKey == LogicalKeyboardKey.keyZ) ||
          event.logicalKey == LogicalKeyboardKey.keyY) {
        final desc = graphModel.redo();
        if (desc != null) {
          _showUndoRedoSnackBar(context, 'Redo: $desc');
        }
        return KeyEventResult.handled;
      }
      // Ctrl+Z: Undo
      if (event.logicalKey == LogicalKeyboardKey.keyZ) {
        final desc = graphModel.undo();
        if (desc != null) {
          _showUndoRedoSnackBar(context, 'Undo: $desc');
        }
        return KeyEventResult.handled;
      }
    }
    return KeyEventResult.ignored;
  }

  void _showUndoRedoSnackBar(BuildContext context, String message) {
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          content: Text(message),
          duration: const Duration(seconds: 2),
          behavior: SnackBarBehavior.floating,
          width: 300,
        ),
      );
  }

  Future<bool> _confirmDiscardChanges() async {
    if (!graphModel.isDirty) return true;
    final shouldProceed = await showDraggableAlertDialog<bool>(
      context: context,
      title: const Text('Unsaved Changes'),
      content:
          const Text('You have unsaved changes. Do you want to discard them?'),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancel'),
        ),
        TextButton(
          onPressed: () => Navigator.of(context).pop(true),
          child: const Text('Discard'),
        ),
      ],
    );
    return shouldProceed ?? false;
  }

  Future<void> _newDesign() async {
    if (!await _confirmDiscardChanges()) return;
    graphModel.newProject();
  }

  Future<void> _importXyz() async {
    FilePickerResult? result = await FilePicker.platform.pickFiles(
      type: FileType.custom,
      allowedExtensions: ['xyz'],
      dialogTitle: 'Import XYZ File',
    );

    if (result != null && result.files.isNotEmpty) {
      String filePath = result.files.first.path!;
      final error = graphModel.importXyzIntoAtomEdit(filePath);
      if (error.isNotEmpty && mounted) {
        showDraggableAlertDialog(
          context: context,
          title: const Text('Import Error'),
          content: Text(error),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('OK'),
            ),
          ],
        );
      }
    }
  }

  Future<void> _loadDesign() async {
    if (!await _confirmDiscardChanges()) return;

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
          showDraggableAlertDialog(
            context: context,
            title: const Text('Load Error'),
            content: Text(loadResult.errorMessage),
            actions: [
              TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: const Text('OK'),
              ),
            ],
          );
        }
      }
    } else {
      debugPrint('No design file selected');
    }
  }

  Future<void> _saveDesignAs() async {
    FocusManager.instance.primaryFocus?.unfocus();

    // Open file picker for saving CNND files
    String? outputFile = await FilePicker.platform.saveFile(
      dialogTitle: 'Save Design As',
      fileName: 'design',
      type: FileType.custom,
      allowedExtensions: ['cnnd'],
    );

    if (outputFile != null) {
      // Add .cnnd extension only if user didn't specify any extension
      String finalPath = outputFile;
      if (!finalPath.contains('.')) {
        finalPath = '$outputFile.cnnd';
      }
      final result = graphModel.saveNodeNetworksAs(finalPath);
      if (!result.success) {
        _showSaveErrorDialog(result.errorMessage);
      }
    }
  }

  void _saveDesign() {
    FocusManager.instance.primaryFocus?.unfocus();

    final result = graphModel.saveNodeNetworks();
    if (!result.success) {
      _showSaveErrorDialog(result.errorMessage);
    }
  }

  void _showSaveErrorDialog(String errorMessage) {
    showDraggableAlertDialog(
      context: context,
      title: const Text('Save Error'),
      content: Text(errorMessage),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('OK'),
        ),
      ],
    );
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

  /// Import node networks from a .cnnd library file
  Future<void> _importFromCnndLibrary() async {
    try {
      // Open file picker for CNND library files
      FilePickerResult? result = await FilePicker.platform.pickFiles(
        type: FileType.custom,
        allowedExtensions: ['cnnd'],
        dialogTitle: 'Select .cnnd Library File',
      );

      if (result != null && result.files.isNotEmpty) {
        String filePath = result.files.first.path!;

        // Show the import dialog
        if (mounted) {
          showDialog(
            context: context,
            builder: (context) => ImportCnndLibraryDialog(
              libraryFilePath: filePath,
              model: graphModel,
            ),
          );
        }
      }
    } catch (e) {
      // Handle any unexpected errors
      if (mounted) {
        showDraggableAlertDialog(
          context: context,
          title: const Text('Import Error'),
          content: Text('An unexpected error occurred: $e'),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('OK'),
            ),
          ],
        );
      }
    }
  }

  /// Export visible atomic structures as XYZ or MOL file
  Future<void> _exportVisible() async {
    try {
      // First, let user select the format
      if (!mounted) return;
      String? selectedFormat = await showDraggableAlertDialog<String>(
        context: context,
        title: const Text('Select Export Format'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('Choose the file format for export:'),
            const SizedBox(height: 16),
            ListTile(
              leading: const Icon(Icons.description),
              title: const Text('MOL format (.mol)'),
              subtitle: const Text('Molecular structure with bond information'),
              onTap: () => Navigator.of(context).pop('mol'),
            ),
            ListTile(
              leading: const Icon(Icons.scatter_plot),
              title: const Text('XYZ format (.xyz)'),
              subtitle: const Text('Atomic coordinates only'),
              onTap: () => Navigator.of(context).pop('xyz'),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
        ],
      );

      if (selectedFormat == null) return;

      // Open file picker for saving structure files with the selected format
      String? outputFile = await FilePicker.platform.saveFile(
        dialogTitle: 'Export visible structures',
        fileName: 'structure',
        type: FileType.custom,
        allowedExtensions: [selectedFormat],
      );

      if (outputFile != null) {
        // Ensure the file has the correct extension
        if (!outputFile.toLowerCase().endsWith('.$selectedFormat')) {
          outputFile = '$outputFile.$selectedFormat';
        }

        // Call the export method
        final result = graphModel.exportVisibleAtomicStructures(outputFile);

        // Check if there was an error
        if (!result.success) {
          // Show error dialog
          if (mounted) {
            showDraggableAlertDialog(
              context: context,
              title: const Text('Export Error'),
              content: Text(result.errorMessage),
              actions: [
                TextButton(
                  onPressed: () => Navigator.of(context).pop(),
                  child: const Text('OK'),
                ),
              ],
            );
          }
        }
      }
    } catch (e) {
      // Handle any unexpected errors
      if (mounted) {
        showDraggableAlertDialog(
          context: context,
          title: const Text('Export Error'),
          content: Text('An unexpected error occurred: $e'),
          actions: [
            TextButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('OK'),
            ),
          ],
        );
      }
    }
  }
}
