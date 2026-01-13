import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'helpers/test_utils.dart';

/// Additional integration tests for atomCAD UI patterns
///
/// Note: Core smoke tests are in smoke_test.dart
/// Note: Menu tests are in menu_test.dart
/// Note: Node network tests are in node_network/network_list_test.dart
///
/// Run with: flutter test integration_test/app_test.dart -d windows
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

  group('Widget Finders', () {
    testWidgets('finds TabBar widget by type', (tester) async {
      await pumpApp(tester, model);
      expect(find.byType(TabBar), findsOneWidget);
    });

    testWidgets('finds layout by ValueKey', (tester) async {
      await pumpApp(tester, model);
      expect(find.byKey(const ValueKey('vertical_layout')), findsOneWidget);
    });

    testWidgets('finds all IconButtons in action bar', (tester) async {
      await pumpApp(tester, model);

      // Find all icon buttons - should have at least back, forward, add, delete
      final iconButtons = find.byType(IconButton);
      expect(iconButtons.evaluate().length, greaterThanOrEqualTo(4));
    });

    testWidgets('finds Tab widgets', (tester) async {
      await pumpApp(tester, model);

      // Find Tab widgets - List and Tree tabs
      final tabs = find.byType(Tab);
      expect(tabs.evaluate().length, equals(2));
    });
  });
}
