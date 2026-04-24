import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import '../helpers/test_utils.dart';

/// Helper to right-click and open the add-node popup.
Future<void> _simulateRightClick(WidgetTester tester, Finder finder) async {
  await tester.tap(finder, buttons: kSecondaryMouseButton);
  await tester.pumpAndSettle();
}

/// Creates a node by name via the popup. Returns true on success.
Future<bool> _createNode(
  WidgetTester tester,
  StructureDesignerModel model,
  String nodeTypeName,
) async {
  final initialCount = model.nodeNetworkView?.nodes.length ?? 0;

  final canvas = find.byKey(TestKeys.nodeNetworkCanvas);
  if (canvas.evaluate().isEmpty) return false;

  await _simulateRightClick(tester, canvas);

  if (TestFinders.addNodeDialog.evaluate().isEmpty) return false;

  await tester.enterText(TestFinders.addNodeFilterField, nodeTypeName);
  await tester.pumpAndSettle();

  final nodeItem = TestFinders.addNodeItem(nodeTypeName);
  if (nodeItem.evaluate().isEmpty) return false;

  await tester.tap(nodeItem);
  await tester.pumpAndSettle();

  return (model.nodeNetworkView?.nodes.length ?? 0) == initialCount + 1;
}

/// Run with: flutter test integration_test/panels/matrix_editors_test.dart -d windows
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

  group('IMat3 Rows Editor (integer)', () {
    testWidgets('default matrix is identity', (tester) async {
      await pumpApp(tester, model);

      final created = await _createNode(tester, model, 'imat3_rows');
      if (!created) {
        debugPrint('Could not create imat3_rows node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      final data = sd_api.getImat3RowsData(nodeId: node.id);
      expect(data, isNotNull);
      expect(data!.a.x, 1);
      expect(data.a.y, 0);
      expect(data.a.z, 0);
      expect(data.b.x, 0);
      expect(data.b.y, 1);
      expect(data.b.z, 0);
      expect(data.c.x, 0);
      expect(data.c.y, 0);
      expect(data.c.z, 1);
    });

    testWidgets('editing a cell updates the stored matrix', (tester) async {
      await pumpApp(tester, model);

      final created = await _createNode(tester, model, 'imat3_rows');
      if (!created) {
        debugPrint('Could not create imat3_rows node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      // Edit row 1, column 2 (value at b.z) to 7.
      final cell = find.byKey(const Key('imat3_rows_cell_1_2'));
      expect(cell, findsOneWidget);
      await tester.enterText(cell, '7');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();

      final data = sd_api.getImat3RowsData(nodeId: node.id);
      expect(data?.b.z, equals(7));
      // Other cells unchanged.
      expect(data?.a.x, equals(1));
      expect(data?.c.z, equals(1));
    });
  });

  group('Mat3 Rows Editor (float)', () {
    testWidgets('default matrix is identity', (tester) async {
      await pumpApp(tester, model);

      final created = await _createNode(tester, model, 'mat3_rows');
      if (!created) {
        debugPrint('Could not create mat3_rows node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      final data = sd_api.getMat3RowsData(nodeId: node.id);
      expect(data, isNotNull);
      expect(data!.a.x, 1.0);
      expect(data.a.y, 0.0);
      expect(data.b.y, 1.0);
      expect(data.c.z, 1.0);
    });

    testWidgets('editing a cell updates the stored matrix', (tester) async {
      await pumpApp(tester, model);

      final created = await _createNode(tester, model, 'mat3_rows');
      if (!created) {
        debugPrint('Could not create mat3_rows node - skipping test');
        return;
      }

      final node = model.nodeNetworkView!.nodes.values.last;
      model.setSelectedNode(node.id);
      await tester.pumpAndSettle();

      // Edit row 0, column 1 (value at a.y) to 2.5.
      final cell = find.byKey(const Key('mat3_rows_cell_0_1'));
      expect(cell, findsOneWidget);
      await tester.enterText(cell, '2.5');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();

      final data = sd_api.getMat3RowsData(nodeId: node.id);
      expect(data?.a.y, equals(2.5));
      // Other cells unchanged (identity defaults).
      expect(data?.a.x, equals(1.0));
      expect(data?.c.z, equals(1.0));
    });
  });
}
