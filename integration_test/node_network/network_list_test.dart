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

  group('Network Selection', () {
    testWidgets('Model setActiveNodeNetwork works correctly', (tester) async {
      await pumpApp(tester, model);

      // Add first network and capture its name
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();
      final firstName = model.nodeNetworkView!.name;

      // Add second network (becomes active)
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();
      final secondName = model.nodeNetworkView!.name;

      expect(model.nodeNetworkView?.name, equals(secondName));
      expect(firstName, isNot(equals(secondName)));

      // Use model directly to select first network
      model.setActiveNodeNetwork(firstName);
      await tester.pumpAndSettle();

      // First network should now be active
      expect(model.nodeNetworkView?.name, equals(firstName));
    });

    testWidgets('Network list items are displayed with correct Keys',
        (tester) async {
      await pumpApp(tester, model);

      // Add a network and capture its name
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();
      final networkName = model.nodeNetworkView!.name;

      // Verify the network item exists with the expected Key
      expect(TestFinders.networkListItem(networkName), findsOneWidget);
    });

    testWidgets('Network tree view displays networks', (tester) async {
      await pumpApp(tester, model);

      // Add a network so there's content to display
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      // Switch to tree tab
      await tester.tap(find.byKey(TestKeys.networkTreeTab));
      await tester.pumpAndSettle();

      // Wait for tree animation to complete (tree nodes animate in over ~1 second)
      await tester.pump(const Duration(seconds: 2));
      await tester.pumpAndSettle();

      // Verify the tree view shows networks (check for any UNTITLED text)
      // Note: The tree view uses AnimatedTreeView which virtualizes off-screen items
      expect(find.textContaining('UNTITLED'), findsWidgets);
    });
  });

  group('Navigation Buttons', () {
    testWidgets('Back and forward buttons exist', (tester) async {
      await pumpApp(tester, model);

      expect(find.byKey(TestKeys.backButton), findsOneWidget);
      expect(find.byKey(TestKeys.forwardButton), findsOneWidget);
    });

    testWidgets('Back button navigates to previous network', (tester) async {
      await pumpApp(tester, model);

      // Add first network
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      // Add second network (becomes active, creates history entry)
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();
      final secondName = model.nodeNetworkView!.name;

      expect(model.nodeNetworkView?.name, equals(secondName));

      // Should now be able to go back
      expect(model.canNavigateBack(), isTrue);

      // Click back button
      await tester.tap(find.byKey(TestKeys.backButton));
      await tester.pumpAndSettle();

      // Should now be at a different network than second (navigated back)
      expect(model.nodeNetworkView?.name, isNot(equals(secondName)));

      // Should now be able to go forward
      expect(model.canNavigateForward(), isTrue);
    });

    testWidgets('Forward button navigates to next network', (tester) async {
      await pumpApp(tester, model);

      // Add first network and capture its name
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      // Add second network (becomes active)
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();
      final secondName = model.nodeNetworkView!.name;

      // Go back
      await tester.tap(find.byKey(TestKeys.backButton));
      await tester.pumpAndSettle();

      expect(model.canNavigateForward(), isTrue);

      // Go forward
      await tester.tap(find.byKey(TestKeys.forwardButton));
      await tester.pumpAndSettle();

      // Should be back at second network
      expect(model.nodeNetworkView?.name, equals(secondName));
    });

    testWidgets('Selecting network clears forward history', (tester) async {
      await pumpApp(tester, model);

      // Add three networks and capture first name
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();
      final firstName = model.nodeNetworkView!.name;

      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();
      final thirdName = model.nodeNetworkView!.name;

      // Go back once (now at second network)
      await tester.tap(find.byKey(TestKeys.backButton));
      await tester.pumpAndSettle();

      // We should be able to go forward to third network
      expect(model.canNavigateForward(), isTrue);
      expect(model.nodeNetworkView?.name, isNot(equals(thirdName)));

      // Select first network using model (should clear forward history)
      model.setActiveNodeNetwork(firstName);
      await tester.pumpAndSettle();

      expect(model.nodeNetworkView?.name, equals(firstName));

      // Forward should now be disabled (history cleared by explicit selection)
      expect(model.canNavigateForward(), isFalse);
    });
  });
}
