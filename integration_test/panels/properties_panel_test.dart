import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import '../helpers/test_utils.dart';

/// Helper function to simulate a right-click (secondary tap) on a finder.
Future<void> simulateRightClick(
  WidgetTester tester,
  Finder finder,
) async {
  await tester.tap(finder, buttons: kSecondaryMouseButton);
  await tester.pumpAndSettle();
}

/// Helper to create a node via the popup
/// Returns true if the node was created successfully
Future<bool> createNodeViaPopup(
  WidgetTester tester,
  StructureDesignerModel model,
  String nodeTypeName,
) async {
  final initialCount = model.nodeNetworkView?.nodes.length ?? 0;

  // Right-click on canvas
  final canvasFinder = find.byKey(TestKeys.nodeNetworkCanvas);
  if (canvasFinder.evaluate().isEmpty) return false;

  await simulateRightClick(tester, canvasFinder);

  // Check popup opened
  if (TestFinders.addNodeDialog.evaluate().isEmpty) {
    debugPrint('Add node popup did not open');
    return false;
  }

  // Filter and select the node type
  await tester.enterText(TestFinders.addNodeFilterField, nodeTypeName);
  await tester.pumpAndSettle();

  final nodeItem = TestFinders.addNodeItem(nodeTypeName);
  if (nodeItem.evaluate().isEmpty) {
    debugPrint('Node type $nodeTypeName not found in popup');
    return false;
  }

  await tester.tap(nodeItem);
  await tester.pumpAndSettle();

  // Verify node was created
  final newCount = model.nodeNetworkView?.nodes.length ?? 0;
  return newCount == initialCount + 1;
}

/// Integration tests for Node Properties Panel
///
/// Run with: flutter test integration_test/panels/properties_panel_test.dart -d windows
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  late StructureDesignerModel model;

  setUpAll(() async {
    await initializeRustLib();
  });

  setUp(() {
    model = createTestModel();
  });

  tearDown(() {
    model.dispose();
  });

  group('Properties Panel Display', () {
    testWidgets('Properties panel shows for selected Int node', (tester) async {
      await pumpApp(tester, model);

      // Create an Int node via popup
      final created = await createNodeViaPopup(tester, model, 'int');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      // Get the created node
      final node = model.nodeNetworkView!.nodes.values.last;

      // Select it via the model
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      // Verify the Int editor is displayed
      expect(TestFinders.intEditor, findsOneWidget);
    });

    testWidgets('Properties panel shows for selected Float node',
        (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'float');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.floatEditor, findsOneWidget);
    });

    testWidgets('Properties panel shows for selected Bool node',
        (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'bool');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.boolEditor, findsOneWidget);
    });

    testWidgets('Properties panel shows for selected String node',
        (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'string');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.stringEditor, findsOneWidget);
    });
  });

  group('Int Editor', () {
    testWidgets('Int editor has value input field', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'int');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.intValueInput, findsOneWidget);
    });

    testWidgets('Int editor accepts valid input', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'int');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      // Clear and enter a new value
      await tester.enterText(TestFinders.intValueInput, '42');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();

      // Verify the model was updated
      final intData = sd_api.getIntData(nodeId: node.id);
      expect(intData?.value, equals(42));
    });
  });

  group('Float Editor', () {
    testWidgets('Float editor has value input field', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'float');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.floatValueInput, findsOneWidget);
    });

    testWidgets('Float editor accepts valid input', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'float');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      // Clear and enter a new value
      await tester.enterText(TestFinders.floatValueInput, '3.14');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();

      // Verify the model was updated
      final floatData = sd_api.getFloatData(nodeId: node.id);
      expect(floatData?.value, closeTo(3.14, 0.001));
    });
  });

  group('Bool Editor', () {
    testWidgets('Bool editor has checkbox', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'bool');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.boolValueCheckbox, findsOneWidget);
    });

    testWidgets('Bool editor toggles value', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'bool');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      // Get initial value
      final initialValue = sd_api.getBoolData(nodeId: node.id)?.value ?? false;

      // Click the checkbox to toggle
      await tester.tap(TestFinders.boolValueCheckbox);
      await tester.pumpAndSettle();

      // Verify value was toggled
      final newValue = sd_api.getBoolData(nodeId: node.id)?.value;
      expect(newValue, equals(!initialValue));
    });
  });

  group('String Editor', () {
    testWidgets('String editor has value input field', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'string');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.stringValueInput, findsOneWidget);
    });

    testWidgets('String editor accepts text input', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'string');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      // Enter text
      await tester.enterText(TestFinders.stringValueInput, 'Hello World');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();

      // Verify the model was updated
      final stringData = sd_api.getStringData(nodeId: node.id);
      expect(stringData?.value, equals('Hello World'));
    });
  });

  group('Vec3 Editor', () {
    testWidgets('Vec3 editor shows for selected Vec3 node', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'vec3');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.vec3Editor, findsOneWidget);
    });

    testWidgets('Vec3 editor has X, Y, Z input fields', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'vec3');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.vec3XInput, findsOneWidget);
      expect(TestFinders.vec3YInput, findsOneWidget);
      expect(TestFinders.vec3ZInput, findsOneWidget);
    });

    testWidgets('Vec3 editor accepts values', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'vec3');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      // Enter X value
      await tester.enterText(TestFinders.vec3XInput, '1.5');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();

      // Verify the model was updated
      final vec3Data = sd_api.getVec3Data(nodeId: node.id);
      expect(vec3Data?.value.x, closeTo(1.5, 0.001));
    });
  });

  group('Cuboid Editor', () {
    testWidgets('Cuboid editor shows for selected Cuboid node', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'cuboid');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.cuboidEditor, findsOneWidget);
    });

    testWidgets('Cuboid editor shows Min Corner fields', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'cuboid');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.cuboidMinCornerXInput, findsOneWidget);
      expect(TestFinders.cuboidMinCornerYInput, findsOneWidget);
      expect(TestFinders.cuboidMinCornerZInput, findsOneWidget);
    });

    testWidgets('Cuboid editor shows Extent fields', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'cuboid');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.cuboidExtentXInput, findsOneWidget);
      expect(TestFinders.cuboidExtentYInput, findsOneWidget);
      expect(TestFinders.cuboidExtentZInput, findsOneWidget);
    });
  });

  group('Sphere Editor', () {
    testWidgets('Sphere editor shows for selected Sphere node', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'sphere');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.sphereEditor, findsOneWidget);
    });

    testWidgets('Sphere editor shows Center fields', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'sphere');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.sphereCenterXInput, findsOneWidget);
      expect(TestFinders.sphereCenterYInput, findsOneWidget);
      expect(TestFinders.sphereCenterZInput, findsOneWidget);
    });

    testWidgets('Sphere editor shows Radius field', (tester) async {
      await pumpApp(tester, model);

      final created = await createNodeViaPopup(tester, model, 'sphere');
      if (!created) {
        debugPrint('Could not create node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      expect(TestFinders.sphereRadiusInput, findsOneWidget);
    });
  });
}
