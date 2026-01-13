import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import '../helpers/test_utils.dart';

/// Integration tests for node network list operations
///
/// Run with: flutter test integration_test/node_network/network_list_test.dart -d windows
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

  group('Add Network', () {
    testWidgets('Add network button creates network', (tester) async {
      await pumpApp(tester, model);

      final initialCount = model.nodeNetworkNames.length;

      // Tap add network button
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      // Verify a network was added
      expect(model.nodeNetworkNames.length, equals(initialCount + 1));
    });

    testWidgets('add multiple networks and verify count', (tester) async {
      await pumpApp(tester, model);

      // Add first network
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      final countAfterFirst = model.nodeNetworkNames.length;

      // Add second network
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      expect(model.nodeNetworkNames.length, equals(countAfterFirst + 1));
    });
  });

  group('Delete Network', () {
    testWidgets('Delete network shows confirmation dialog', (tester) async {
      await pumpApp(tester, model);

      // First add a network
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      // Try to delete it
      await tester.tap(find.byKey(TestKeys.deleteNetworkButton));
      await tester.pumpAndSettle();

      // Check confirmation dialog appeared
      expect(find.text('Delete Network'), findsOneWidget);
      expect(find.text('Cancel'), findsOneWidget);
      expect(find.text('Delete'), findsOneWidget);

      // Cancel the delete
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();

      // Dialog should be dismissed
      expect(find.text('Delete Network'), findsNothing);
    });

    testWidgets('Confirm delete removes network', (tester) async {
      await pumpApp(tester, model);

      // Add a network
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      final countBefore = model.nodeNetworkNames.length;

      // Delete it
      await tester.tap(find.byKey(TestKeys.deleteNetworkButton));
      await tester.pumpAndSettle();

      // Confirm delete
      await tester.tap(find.text('Delete'));
      await tester.pumpAndSettle();

      // Network should be removed
      expect(model.nodeNetworkNames.length, equals(countBefore - 1));
    });

    testWidgets('Cancel delete keeps network', (tester) async {
      await pumpApp(tester, model);

      // Add a network
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      final countBefore = model.nodeNetworkNames.length;

      // Try to delete it
      await tester.tap(find.byKey(TestKeys.deleteNetworkButton));
      await tester.pumpAndSettle();

      // Cancel the delete
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();

      // Network count should remain the same
      expect(model.nodeNetworkNames.length, equals(countBefore));
    });
  });

  group('Tab Navigation', () {
    testWidgets('Switch between List and Tree tabs', (tester) async {
      await pumpApp(tester, model);

      // Initially on List tab, switch to Tree
      await tester.tap(find.byKey(TestKeys.networkTreeTab));
      await tester.pumpAndSettle();

      // Switch back to List
      await tester.tap(find.byKey(TestKeys.networkListTab));
      await tester.pumpAndSettle();

      // Both tabs should still exist
      expect(find.byKey(TestKeys.networkListTab), findsOneWidget);
      expect(find.byKey(TestKeys.networkTreeTab), findsOneWidget);
    });
  });

  group('Navigation Buttons', () {
    testWidgets('Back and forward buttons exist', (tester) async {
      await pumpApp(tester, model);

      expect(find.byKey(TestKeys.backButton), findsOneWidget);
      expect(find.byKey(TestKeys.forwardButton), findsOneWidget);
    });

    testWidgets('Back button is disabled initially', (tester) async {
      await pumpApp(tester, model);

      // Back button should be disabled (no navigation history)
      expect(model.canNavigateBack(), isFalse);
    });

    testWidgets('Forward button is disabled initially', (tester) async {
      await pumpApp(tester, model);

      // Forward button should be disabled (no navigation history)
      expect(model.canNavigateForward(), isFalse);
    });
  });
}
