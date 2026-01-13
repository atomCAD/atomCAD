import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:provider/provider.dart';

/// Integration tests for atomCAD UI
///
/// Run with: flutter test integration_test/app_test.dart -d windows
///
/// IMPORTANT: With flutter_rust_bridge, we can't call app.main() multiple times
/// because RustLib.init() can only be called once. Instead, we build the widget
/// tree directly using pumpWidget.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  late StructureDesignerModel model;

  setUpAll(() async {
    // Initialize Rust FFI once for all tests
    // Use try-catch to handle case where RustLib is already initialized
    // (happens when running multiple test files)
    try {
      await RustLib.init();
    } catch (e) {
      // Already initialized, ignore
    }
  });

  setUp(() {
    // Create a fresh model for each test
    model = StructureDesignerModel();
    model.init();
  });

  tearDown(() {
    model.dispose();
  });

  /// Helper to pump the app widget tree
  Future<void> pumpApp(WidgetTester tester) async {
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

  group('App Launch', () {
    testWidgets('app starts and shows main UI', (tester) async {
      await pumpApp(tester);

      // Verify the main structure is visible
      // Menu bar items
      expect(find.text('File'), findsOneWidget);
      expect(find.text('View'), findsOneWidget);
      expect(find.text('Edit'), findsOneWidget);

      // Left sidebar sections (titles are uppercase)
      expect(find.text('DISPLAY'), findsOneWidget);
      expect(find.text('CAMERA CONTROL'), findsOneWidget);
      expect(find.text('NODE NETWORKS'), findsOneWidget);

      // Node networks panel tabs
      expect(find.text('List'), findsOneWidget);
      expect(find.text('Tree'), findsOneWidget);
    });
  });

  group('Menu Interactions', () {
    testWidgets('File menu opens and shows items', (tester) async {
      await pumpApp(tester);

      // Tap on File menu
      await tester.tap(find.text('File'));
      await tester.pumpAndSettle();

      // Verify menu items are visible
      expect(find.text('Load Design'), findsOneWidget);
      expect(find.text('Save Design'), findsOneWidget);
      expect(find.text('Save Design As'), findsOneWidget);
      expect(find.text('Export visible'), findsOneWidget);
    });

    testWidgets('View menu opens and shows items', (tester) async {
      await pumpApp(tester);

      // Tap on View menu
      await tester.tap(find.text('View'));
      await tester.pumpAndSettle();

      // Verify menu items are visible
      expect(find.text('Reset node network view'), findsOneWidget);
      expect(find.textContaining('Layout'), findsOneWidget);
    });

    testWidgets('Edit menu opens and shows preferences', (tester) async {
      await pumpApp(tester);

      // Tap on Edit menu
      await tester.tap(find.text('Edit'));
      await tester.pumpAndSettle();

      // Verify Preferences option exists
      expect(find.text('Preferences'), findsOneWidget);
    });
  });

  group('Node Networks Panel', () {
    testWidgets('can switch between List and Tree tabs', (tester) async {
      await pumpApp(tester);

      // Initially on List tab, switch to Tree
      await tester.tap(find.text('Tree'));
      await tester.pumpAndSettle();

      // Switch back to List
      await tester.tap(find.text('List'));
      await tester.pumpAndSettle();

      expect(find.text('List'), findsOneWidget);
    });

    testWidgets('Add network button exists and works', (tester) async {
      await pumpApp(tester);

      // Find the add button by its tooltip
      final addButton = find.byTooltip('Add network');
      expect(addButton, findsOneWidget);

      // Tap it to add a network
      await tester.tap(addButton);
      await tester.pumpAndSettle();

      // Verify a network was added (model should have networks)
      expect(model.nodeNetworkNames.isNotEmpty, isTrue);
    });
  });

  group('Navigation Buttons', () {
    testWidgets('back and forward buttons exist', (tester) async {
      await pumpApp(tester);

      expect(find.byIcon(Icons.arrow_back), findsOneWidget);
      expect(find.byIcon(Icons.arrow_forward), findsOneWidget);
      expect(find.byTooltip('Go Back'), findsOneWidget);
      expect(find.byTooltip('Go Forward'), findsOneWidget);
    });
  });

  group('Custom Widget Finders', () {
    testWidgets('finds TabBar widget', (tester) async {
      await pumpApp(tester);
      expect(find.byType(TabBar), findsOneWidget);
    });

    testWidgets('finds layout by ValueKey', (tester) async {
      await pumpApp(tester);
      expect(find.byKey(const ValueKey('vertical_layout')), findsOneWidget);
    });
  });
}
