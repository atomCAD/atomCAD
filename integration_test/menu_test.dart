import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'helpers/test_utils.dart';

/// Integration tests for menu interactions
///
/// Run with: flutter test integration_test/menu_test.dart -d windows
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

  group('File Menu', () {
    testWidgets('File menu opens and shows items', (tester) async {
      await pumpApp(tester, model);

      // Tap on File menu using key
      await tester.tap(find.byKey(TestKeys.fileMenu));
      await tester.pumpAndSettle();

      // Verify menu items are visible using keys
      expect(find.byKey(TestKeys.loadDesignMenuItem), findsOneWidget);
      expect(find.byKey(TestKeys.saveDesignMenuItem), findsOneWidget);
      expect(find.byKey(TestKeys.saveDesignAsMenuItem), findsOneWidget);
      expect(find.byKey(TestKeys.exportVisibleMenuItem), findsOneWidget);
      expect(find.byKey(TestKeys.importFromLibraryMenuItem), findsOneWidget);
    });

    testWidgets('Load Design menu item exists', (tester) async {
      await pumpApp(tester, model);

      // Open File menu
      await tester.tap(find.byKey(TestKeys.fileMenu));
      await tester.pumpAndSettle();

      // Verify Load Design item exists
      expect(find.text('Load Design'), findsOneWidget);
    });

    testWidgets('Save Design As menu item exists', (tester) async {
      await pumpApp(tester, model);

      // Open File menu
      await tester.tap(find.byKey(TestKeys.fileMenu));
      await tester.pumpAndSettle();

      // Verify Save Design As item exists
      expect(find.text('Save Design As'), findsOneWidget);
    });

    testWidgets('Export visible shows format selection dialog', (tester) async {
      await pumpApp(tester, model);

      // Open File menu
      await tester.tap(find.byKey(TestKeys.fileMenu));
      await tester.pumpAndSettle();

      // Tap Export visible
      await tester.tap(find.byKey(TestKeys.exportVisibleMenuItem));
      await tester.pumpAndSettle();

      // Verify export format dialog appears
      expect(find.text('Select Export Format'), findsOneWidget);
      expect(find.text('MOL format (.mol)'), findsOneWidget);
      expect(find.text('XYZ format (.xyz)'), findsOneWidget);

      // Close the dialog
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();

      // Dialog should be dismissed
      expect(find.text('Select Export Format'), findsNothing);
    });
  });

  group('View Menu', () {
    testWidgets('View menu opens and shows items', (tester) async {
      await pumpApp(tester, model);

      // Tap on View menu
      await tester.tap(find.byKey(TestKeys.viewMenu));
      await tester.pumpAndSettle();

      // Verify menu items are visible
      expect(find.byKey(TestKeys.resetViewMenuItem), findsOneWidget);
      expect(find.byKey(TestKeys.toggleLayoutMenuItem), findsOneWidget);
    });

    testWidgets('Reset node network view item exists', (tester) async {
      await pumpApp(tester, model);

      // Open View menu
      await tester.tap(find.byKey(TestKeys.viewMenu));
      await tester.pumpAndSettle();

      // Verify Reset node network view item exists
      expect(find.text('Reset node network view'), findsOneWidget);
    });

    testWidgets('Layout toggle item shows correct text', (tester) async {
      await pumpApp(tester, model);

      // Open View menu
      await tester.tap(find.byKey(TestKeys.viewMenu));
      await tester.pumpAndSettle();

      // Initially should show "Switch to Horizontal Layout"
      expect(find.text('Switch to Horizontal Layout'), findsOneWidget);
    });
  });

  group('Edit Menu', () {
    testWidgets('Edit menu opens and shows Preferences', (tester) async {
      await pumpApp(tester, model);

      // Tap on Edit menu
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();

      // Verify Preferences option exists
      expect(find.byKey(TestKeys.preferencesMenuItem), findsOneWidget);
      expect(find.text('Preferences'), findsOneWidget);
    });

    testWidgets('Validate active network item exists', (tester) async {
      await pumpApp(tester, model);

      // Open Edit menu
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();

      // Verify Validate active network item exists
      expect(find.byKey(TestKeys.validateNetworkMenuItem), findsOneWidget);
      expect(find.text('Validate active network'), findsOneWidget);
    });
  });
}
