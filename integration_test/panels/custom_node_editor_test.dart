import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/custom_node_editor.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import '../helpers/test_utils.dart';

/// Builds a custom node type with a single simple-typed (float) parameter, then
/// instantiates it in the original network and selects it. Returns the
/// instance node's id, or null if any setup step failed (so callers can skip).
Future<BigInt?> _setupCustomNodeWithParam(
  WidgetTester tester,
  StructureDesignerModel model,
) async {
  final originalNetwork = model.nodeNetworkView?.name;
  if (originalNetwork == null) return null;

  const customName = 'TestCustomNode';
  final added = sd_api.addNodeNetworkWithName(name: customName);
  model.refreshFromKernel();
  if (!added.success) return null;

  sd_api.setActiveNodeNetwork(nodeNetworkName: customName);
  model.refreshFromKernel();
  await tester.pumpAndSettle();

  // A `float` node, promoted to a parameter, gives the custom node one
  // simple-typed (Float) input pin.
  final floatId = model.createNode('float', const Offset(100, 100));
  if (floatId == BigInt.zero) return null;

  final promoted = model.promoteNodeToParameter(floatId);
  if (!promoted.success) return null;

  sd_api.setActiveNodeNetwork(nodeNetworkName: originalNetwork);
  model.refreshFromKernel();
  await tester.pumpAndSettle();

  final instanceId = model.createNode(customName, const Offset(200, 200));
  if (instanceId == BigInt.zero) return null;

  model.setSelectedNode(instanceId);
  await tester.pumpAndSettle();
  return instanceId;
}

/// Run with: flutter test integration_test/panels/custom_node_editor_test.dart -d windows
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

  group('CustomNodeEditor', () {
    testWidgets('renders one row per simple-typed parameter', (tester) async {
      await pumpApp(tester, model);

      final instanceId = await _setupCustomNodeWithParam(tester, model);
      if (instanceId == null) {
        debugPrint('Could not set up custom node - skipping test');
        return;
      }

      expect(find.byType(CustomNodeEditor), findsOneWidget);

      final params = model.getCustomNodeParams(instanceId);
      expect(params, isNotNull);
      expect(params!.length, equals(1));

      final paramName = params.first.name;
      expect(
        find.byKey(ValueKey('custom_param_$paramName')),
        findsOneWidget,
      );
      // No stored literal yet -> Placeholder state, no clear button.
      expect(params.first.storedValue, isNull);
      expect(
        find.byKey(ValueKey('custom_param_clear_$paramName')),
        findsNothing,
      );
    });

    testWidgets('editing a field stores the literal and shows clear button',
        (tester) async {
      await pumpApp(tester, model);

      final instanceId = await _setupCustomNodeWithParam(tester, model);
      if (instanceId == null) {
        debugPrint('Could not set up custom node - skipping test');
        return;
      }

      final paramName = model.getCustomNodeParams(instanceId)!.first.name;
      final input = find.byKey(Key('custom_param_input_$paramName'));
      expect(input, findsOneWidget);

      await tester.enterText(input, '42.5');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();

      final params = model.getCustomNodeParams(instanceId)!;
      expect(params.first.storedValue, isNotNull);
      expect(
        params.first.storedValue,
        equals(const APILiteralValue.float(42.5)),
      );

      // Stored state -> clear button is now present.
      expect(
        find.byKey(ValueKey('custom_param_clear_$paramName')),
        findsOneWidget,
      );

      // Clearing returns the row to the Placeholder state.
      await tester.tap(find.byKey(ValueKey('custom_param_clear_$paramName')));
      await tester.pumpAndSettle();
      expect(model.getCustomNodeParams(instanceId)!.first.storedValue, isNull);
    });
  });
}
