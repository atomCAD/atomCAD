import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/preferences_window.dart';
import 'package:flutter_cad/structure_designer/node_network/add_node_popup.dart';
import 'package:flutter_cad/structure_designer/node_network/node_widget.dart';
import 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:provider/provider.dart';

// Re-export PreferencesKeys for test usage
export 'package:flutter_cad/structure_designer/preferences_window.dart'
    show PreferencesKeys;

// Re-export AddNodePopupKeys for test usage
export 'package:flutter_cad/structure_designer/node_network/add_node_popup.dart'
    show AddNodePopupKeys;

// Re-export NodeWidgetKeys for test usage
export 'package:flutter_cad/structure_designer/node_network/node_widget.dart'
    show NodeWidgetKeys;

// Re-export PropertyEditorKeys for test usage
export 'package:flutter_cad/structure_designer/node_data/node_data_widget.dart'
    show PropertyEditorKeys;

/// Key constants for integration testing.
///
/// These keys are used in both source code and tests to reliably find widgets.
class TestKeys {
  // Menu keys
  static const Key fileMenu = Key('file_menu');
  static const Key viewMenu = Key('view_menu');
  static const Key editMenu = Key('edit_menu');
  static const Key loadDesignMenuItem = Key('load_design_item');
  static const Key saveDesignMenuItem = Key('save_design_item');
  static const Key saveDesignAsMenuItem = Key('save_design_as_item');
  static const Key exportVisibleMenuItem = Key('export_visible_item');
  static const Key importFromLibraryMenuItem = Key('import_from_library_item');
  static const Key preferencesMenuItem = Key('preferences_item');
  static const Key resetViewMenuItem = Key('reset_view_item');
  static const Key toggleLayoutMenuItem = Key('toggle_layout_item');
  static const Key validateNetworkMenuItem = Key('validate_network_item');

  // Node networks panel keys
  static const Key nodeNetworksPanel = Key('node_networks_panel');
  static const Key networkListTab = Key('network_list_tab');
  static const Key networkTreeTab = Key('network_tree_tab');
  static const Key addNetworkButton = Key('add_network_button');
  static const Key deleteNetworkButton = Key('delete_network_button');
  static const Key backButton = Key('back_button');
  static const Key forwardButton = Key('forward_button');

  // Dialog keys
  static const Key deleteConfirmDialog = Key('delete_confirm_dialog');
  static const Key exportFormatDialog = Key('export_format_dialog');

  // Node network canvas
  static const Key nodeNetworkCanvas = Key('node_network_canvas');

  // Rename elements
  static const Key renameTextField = Key('rename_text_field');

  // Dynamic keys for network list items
  /// Returns a Key for a network item in the list view
  static Key networkListItem(String networkName) =>
      Key('network_item_$networkName');

  /// Returns a Key for a network item in the tree view
  static Key networkTreeItem(String networkName) =>
      Key('network_tree_item_$networkName');

  /// Returns a Key for a namespace folder in the tree view
  static Key namespaceTreeItem(String namespacePath) =>
      Key('namespace_tree_item_$namespacePath');

  // Geometry visualization keys
  static const Key geometryVisSurfaceSplatting =
      Key('geometry_vis_surface_splatting');
  static const Key geometryVisWireframe = Key('geometry_vis_wireframe');
  static const Key geometryVisSolid = Key('geometry_vis_solid');

  // Node display policy keys
  static const Key nodeDisplayManual = Key('node_display_manual');
  static const Key nodeDisplayPreferSelected =
      Key('node_display_prefer_selected');
  static const Key nodeDisplayPreferFrontier =
      Key('node_display_prefer_frontier');

  // Atomic visualization keys
  static const Key atomicVisBallAndStick = Key('atomic_vis_ball_and_stick');
  static const Key atomicVisSpaceFilling = Key('atomic_vis_space_filling');

  // Camera control keys
  static const Key cameraViewDropdown = Key('camera_view_dropdown');
  static const Key cameraPerspectiveButton = Key('camera_perspective_button');
  static const Key cameraOrthographicButton = Key('camera_orthographic_button');
}

/// Initializes the Rust FFI library.
/// Call this in setUpAll() for integration tests.
Future<void> initializeRustLib() async {
  try {
    await RustLib.init();
  } catch (e) {
    // Already initialized, ignore
  }
}

/// Pumps the StructureDesigner widget with the necessary providers.
///
/// [tester] The WidgetTester instance
/// [model] The StructureDesignerModel to use
Future<void> pumpApp(WidgetTester tester, StructureDesignerModel model) async {
  await tester.pumpWidget(
    MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => MouseWheelBlockService()),
        ChangeNotifierProvider.value(value: model),
      ],
      child: MaterialApp(
        home: Scaffold(
          body: StructureDesigner(model: model),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

/// Creates a fresh StructureDesignerModel for testing.
StructureDesignerModel createTestModel() {
  final model = StructureDesignerModel();
  model.init();
  return model;
}

/// Common finder helpers for frequently used widgets.
class TestFinders {
  /// Find the File menu button
  static Finder get fileMenu => find.byKey(TestKeys.fileMenu);

  /// Find the View menu button
  static Finder get viewMenu => find.byKey(TestKeys.viewMenu);

  /// Find the Edit menu button
  static Finder get editMenu => find.byKey(TestKeys.editMenu);

  /// Find the Add Network button
  static Finder get addNetworkButton => find.byKey(TestKeys.addNetworkButton);

  /// Find the Delete Network button
  static Finder get deleteNetworkButton =>
      find.byKey(TestKeys.deleteNetworkButton);

  /// Find the Back navigation button
  static Finder get backButton => find.byKey(TestKeys.backButton);

  /// Find the Forward navigation button
  static Finder get forwardButton => find.byKey(TestKeys.forwardButton);

  /// Find the List tab
  static Finder get listTab => find.byKey(TestKeys.networkListTab);

  /// Find the Tree tab
  static Finder get treeTab => find.byKey(TestKeys.networkTreeTab);

  /// Find the delete confirmation dialog
  static Finder get deleteConfirmDialog =>
      find.byKey(TestKeys.deleteConfirmDialog);

  /// Find the rename text field
  static Finder get renameTextField => find.byKey(TestKeys.renameTextField);

  /// Find a network item in the list view by name
  static Finder networkListItem(String networkName) =>
      find.byKey(TestKeys.networkListItem(networkName));

  /// Find a network item in the tree view by name
  static Finder networkTreeItem(String networkName) =>
      find.byKey(TestKeys.networkTreeItem(networkName));

  // Geometry visualization finders
  static Finder get geometryVisSurfaceSplatting =>
      find.byKey(TestKeys.geometryVisSurfaceSplatting);
  static Finder get geometryVisWireframe =>
      find.byKey(TestKeys.geometryVisWireframe);
  static Finder get geometryVisSolid => find.byKey(TestKeys.geometryVisSolid);

  // Node display policy finders
  static Finder get nodeDisplayManual => find.byKey(TestKeys.nodeDisplayManual);
  static Finder get nodeDisplayPreferSelected =>
      find.byKey(TestKeys.nodeDisplayPreferSelected);
  static Finder get nodeDisplayPreferFrontier =>
      find.byKey(TestKeys.nodeDisplayPreferFrontier);

  // Atomic visualization finders
  static Finder get atomicVisBallAndStick =>
      find.byKey(TestKeys.atomicVisBallAndStick);
  static Finder get atomicVisSpaceFilling =>
      find.byKey(TestKeys.atomicVisSpaceFilling);

  // Camera control finders
  static Finder get cameraViewDropdown =>
      find.byKey(TestKeys.cameraViewDropdown);
  static Finder get cameraPerspectiveButton =>
      find.byKey(TestKeys.cameraPerspectiveButton);
  static Finder get cameraOrthographicButton =>
      find.byKey(TestKeys.cameraOrthographicButton);

  // Preferences dialog finders
  static Finder get preferencesDialog =>
      find.byKey(PreferencesKeys.preferencesDialog);
  static Finder get preferencesCloseButton =>
      find.byKey(PreferencesKeys.closeButton);
  static Finder get visualizationMethodDropdown =>
      find.byKey(PreferencesKeys.visualizationMethodDropdown);
  static Finder get displayCameraPivotCheckbox =>
      find.byKey(PreferencesKeys.displayCameraPivotCheckbox);
  static Finder get showGridCheckbox =>
      find.byKey(PreferencesKeys.showGridCheckbox);
  static Finder get backgroundColorInput =>
      find.byKey(PreferencesKeys.backgroundColorInput);
  static Finder get gridSizeInput => find.byKey(PreferencesKeys.gridSizeInput);

  // Add node popup finders
  static Finder get addNodeDialog => find.byKey(AddNodePopupKeys.dialog);
  static Finder get addNodeFilterField =>
      find.byKey(AddNodePopupKeys.filterField);
  static Finder get addNodeListView =>
      find.byKey(AddNodePopupKeys.nodeListView);
  static Finder get addNodeDescriptionPanel =>
      find.byKey(AddNodePopupKeys.descriptionPanel);
  static Finder get addNodeDescriptionTitle =>
      find.byKey(AddNodePopupKeys.descriptionTitle);
  static Finder get addNodeDescriptionText =>
      find.byKey(AddNodePopupKeys.descriptionText);

  /// Find a category header in the add node popup
  static Finder addNodeCategoryHeader(NodeTypeCategory category) =>
      find.byKey(AddNodePopupKeys.categoryHeader(category));

  /// Find a node item in the add node popup by name
  static Finder addNodeItem(String nodeName) =>
      find.byKey(AddNodePopupKeys.nodeItem(nodeName));

  // Node widget finders
  /// Find a node widget by its ID
  static Finder nodeWidget(BigInt id) =>
      find.byKey(NodeWidgetKeys.nodeWidget(id));

  /// Find a node's visibility button by its ID
  static Finder nodeVisibilityButton(BigInt id) =>
      find.byKey(NodeWidgetKeys.visibilityButton(id));

  /// Find an input pin by node ID and pin index
  static Finder nodeInputPin(BigInt nodeId, int pinIndex) =>
      find.byKey(NodeWidgetKeys.inputPin(nodeId, pinIndex));

  /// Find an output pin by node ID
  static Finder nodeOutputPin(BigInt nodeId) =>
      find.byKey(NodeWidgetKeys.outputPin(nodeId));

  /// Find the node network canvas
  static Finder get nodeNetworkCanvas => find.byKey(TestKeys.nodeNetworkCanvas);

  // Property editor finders
  /// Find the float editor
  static Finder get floatEditor => find.byKey(PropertyEditorKeys.floatEditor);

  /// Find the int editor
  static Finder get intEditor => find.byKey(PropertyEditorKeys.intEditor);

  /// Find the bool editor
  static Finder get boolEditor => find.byKey(PropertyEditorKeys.boolEditor);

  /// Find the string editor
  static Finder get stringEditor => find.byKey(PropertyEditorKeys.stringEditor);

  /// Find the vec3 editor
  static Finder get vec3Editor => find.byKey(PropertyEditorKeys.vec3Editor);

  /// Find the cuboid editor
  static Finder get cuboidEditor => find.byKey(PropertyEditorKeys.cuboidEditor);

  /// Find the sphere editor
  static Finder get sphereEditor => find.byKey(PropertyEditorKeys.sphereEditor);

  // Property input field finders
  static Finder get floatValueInput =>
      find.byKey(PropertyEditorKeys.floatValueInput);
  static Finder get intValueInput =>
      find.byKey(PropertyEditorKeys.intValueInput);
  static Finder get boolValueCheckbox =>
      find.byKey(PropertyEditorKeys.boolValueCheckbox);
  static Finder get stringValueInput =>
      find.byKey(PropertyEditorKeys.stringValueInput);

  // Vec3 input finders
  static Finder get vec3XInput => find.byKey(PropertyEditorKeys.vec3XInput);
  static Finder get vec3YInput => find.byKey(PropertyEditorKeys.vec3YInput);
  static Finder get vec3ZInput => find.byKey(PropertyEditorKeys.vec3ZInput);

  // Cuboid input finders
  static Finder get cuboidMinCornerXInput =>
      find.byKey(PropertyEditorKeys.cuboidMinCornerXInput);
  static Finder get cuboidMinCornerYInput =>
      find.byKey(PropertyEditorKeys.cuboidMinCornerYInput);
  static Finder get cuboidMinCornerZInput =>
      find.byKey(PropertyEditorKeys.cuboidMinCornerZInput);
  static Finder get cuboidExtentXInput =>
      find.byKey(PropertyEditorKeys.cuboidExtentXInput);
  static Finder get cuboidExtentYInput =>
      find.byKey(PropertyEditorKeys.cuboidExtentYInput);
  static Finder get cuboidExtentZInput =>
      find.byKey(PropertyEditorKeys.cuboidExtentZInput);

  // Sphere input finders
  static Finder get sphereCenterXInput =>
      find.byKey(PropertyEditorKeys.sphereCenterXInput);
  static Finder get sphereCenterYInput =>
      find.byKey(PropertyEditorKeys.sphereCenterYInput);
  static Finder get sphereCenterZInput =>
      find.byKey(PropertyEditorKeys.sphereCenterZInput);
  static Finder get sphereRadiusInput =>
      find.byKey(PropertyEditorKeys.sphereRadiusInput);
}
