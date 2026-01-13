import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'helpers/test_utils.dart';

/// Screenshot tests for Node Network interactions
///
/// Note: Core node network tests are in node_network/network_list_test.dart
///
/// Run with: flutter test integration_test/node_network_test.dart -d windows
void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

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

  group('Screenshots', () {
    testWidgets('capture app screenshot', (tester) async {
      await pumpApp(tester, model);
      await binding.takeScreenshot('app_initial_state');
    });

    testWidgets('capture menu open state', (tester) async {
      await pumpApp(tester, model);
      await tester.tap(find.byKey(TestKeys.fileMenu));
      await tester.pumpAndSettle();
      await binding.takeScreenshot('file_menu_open');
    });

    testWidgets('capture with network added', (tester) async {
      await pumpApp(tester, model);

      // Add a network
      await tester.tap(find.byKey(TestKeys.addNetworkButton));
      await tester.pumpAndSettle();

      await binding.takeScreenshot('with_network_added');
    });
  });
}
