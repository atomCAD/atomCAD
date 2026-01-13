import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:provider/provider.dart';

/// Integration tests focused on Node Network interactions
///
/// Run with: flutter test integration_test/node_network_test.dart -d windows
void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  late StructureDesignerModel model;

  setUpAll(() async {
    try {
      await RustLib.init();
    } catch (e) {
      // Already initialized, ignore
    }
  });

  setUp(() {
    model = StructureDesignerModel();
    model.init();
  });

  tearDown(() {
    model.dispose();
  });

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

  group('Node Network Interactions', () {
    testWidgets('add multiple networks and navigate', (tester) async {
      await pumpApp(tester);

      // Add first network
      await tester.tap(find.byTooltip('Add network'));
      await tester.pumpAndSettle();

      final countAfterFirst = model.nodeNetworkNames.length;

      // Add second network
      await tester.tap(find.byTooltip('Add network'));
      await tester.pumpAndSettle();

      expect(model.nodeNetworkNames.length, greaterThan(countAfterFirst));
    });

    testWidgets('delete network shows confirmation dialog', (tester) async {
      await pumpApp(tester);

      // First add a network
      await tester.tap(find.byTooltip('Add network'));
      await tester.pumpAndSettle();

      // Try to delete it
      await tester.tap(find.byTooltip('Delete network'));
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

    testWidgets('confirm delete removes network', (tester) async {
      await pumpApp(tester);

      // Add a network
      await tester.tap(find.byTooltip('Add network'));
      await tester.pumpAndSettle();

      final countBefore = model.nodeNetworkNames.length;

      // Delete it
      await tester.tap(find.byTooltip('Delete network'));
      await tester.pumpAndSettle();

      // Confirm delete
      await tester.tap(find.text('Delete'));
      await tester.pumpAndSettle();

      // Network should be removed
      expect(model.nodeNetworkNames.length, lessThan(countBefore));
    });
  });

  group('Screenshots', () {
    testWidgets('capture app screenshot', (tester) async {
      await pumpApp(tester);
      await binding.takeScreenshot('app_initial_state');
    });

    testWidgets('capture menu open state', (tester) async {
      await pumpApp(tester);
      await tester.tap(find.text('File'));
      await tester.pumpAndSettle();
      await binding.takeScreenshot('file_menu_open');
    });
  });

  group('Complex Finders', () {
    testWidgets('find all IconButtons in action bar', (tester) async {
      await pumpApp(tester);

      // Find all icon buttons
      final iconButtons = find.byType(IconButton);
      // There should be at least back, forward, add, delete
      expect(iconButtons.evaluate().length, greaterThanOrEqualTo(4));
    });

    testWidgets('find widget by ancestor', (tester) async {
      await pumpApp(tester);

      // Find Tab widgets
      final tabs = find.byType(Tab);
      expect(tabs.evaluate().length, equals(2)); // List and Tree tabs
    });
  });
}
