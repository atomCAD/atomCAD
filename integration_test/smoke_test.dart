import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'helpers/test_utils.dart';

/// Smoke tests for atomCAD
///
/// These tests verify that the app launches correctly and FFI works.
/// Run with: flutter test integration_test/smoke_test.dart -d windows
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

  group('Smoke Tests', () {
    testWidgets('FFI initialization works', (tester) async {
      // Verify Rust FFI is working by calling the API
      final nodeTypes = sd_api.getNodeTypeViews();
      expect(nodeTypes, isNotNull);
      expect(nodeTypes!.isNotEmpty, isTrue);
    });

    testWidgets('app launches and shows main UI elements', (tester) async {
      await pumpApp(tester, model);

      // Verify menu bar items are visible
      expect(find.byKey(TestKeys.fileMenu), findsOneWidget);
      expect(find.byKey(TestKeys.viewMenu), findsOneWidget);
      expect(find.byKey(TestKeys.editMenu), findsOneWidget);

      // Verify left sidebar sections (titles are uppercase)
      expect(find.text('DISPLAY'), findsOneWidget);
      expect(find.text('CAMERA CONTROL'), findsOneWidget);
      expect(find.text('NODE NETWORKS'), findsOneWidget);

      // Verify node networks panel tabs
      expect(find.byKey(TestKeys.networkListTab), findsOneWidget);
      expect(find.byKey(TestKeys.networkTreeTab), findsOneWidget);
    });

    testWidgets('navigation and action buttons exist', (tester) async {
      await pumpApp(tester, model);

      // Verify navigation buttons
      expect(find.byKey(TestKeys.backButton), findsOneWidget);
      expect(find.byKey(TestKeys.forwardButton), findsOneWidget);

      // Verify action buttons
      expect(find.byKey(TestKeys.addNetworkButton), findsOneWidget);
      expect(find.byKey(TestKeys.deleteNetworkButton), findsOneWidget);
    });

    testWidgets('layout key exists', (tester) async {
      await pumpApp(tester, model);

      // Verify the layout key exists (starts in vertical layout)
      expect(find.byKey(const ValueKey('vertical_layout')), findsOneWidget);
    });
  });
}
