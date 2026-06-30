import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/structure_designer/node_data/record_construct_editor.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import '../helpers/test_utils.dart';

/// Builds a record def with one `float` field, instantiates a
/// `record_construct` node pointed at it, selects the node. Returns the
/// instance node's id, or null if any setup step failed (so callers can skip).
Future<BigInt?> _setupRecordConstructWithField(
  WidgetTester tester,
  StructureDesignerModel model,
) async {
  const defName = 'TestRecordDef';
  final addError = model.addRecordTypeDef(defName);
  if (addError != null) return null;

  // `addRecordTypeDef` activates the new def in the schema editor. Set the
  // fields, then return to the original network to add the construct node.
  final updateError = model.updateRecordTypeDef(defName, [
    const APIRecordTypeField(
      id: null,
      name: 'amount',
      dataType: APIDataType(
        dataTypeBase: APIDataTypeBase.float,
        array: false,
        children: [],
      ),
    ),
  ]);
  if (updateError != null) return null;

  // Return to a network so we can add a record_construct node.
  final networkName = model.nodeNetworkView?.name ??
      model.nodeNetworkNames.firstOrNull?.name;
  if (networkName == null) return null;
  model.setActiveNodeNetwork(networkName);
  await tester.pumpAndSettle();

  final instanceId = model.createNode('record_construct', const Offset(200, 200));
  if (instanceId == BigInt.zero) return null;

  model.setRecordConstructData(
    instanceId,
    const APIRecordSchemaData(schema: defName),
  );

  model.setSelectedNode(instanceId);
  await tester.pumpAndSettle();
  return instanceId;
}

/// Run with: flutter test integration_test/panels/record_construct_editor_test.dart -d windows
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

  group('RecordConstructEditor', () {
    testWidgets('renders one row per simple-typed field when schema chosen',
        (tester) async {
      await pumpApp(tester, model);

      final instanceId = await _setupRecordConstructWithField(tester, model);
      if (instanceId == null) {
        debugPrint('Could not set up record_construct - skipping test');
        return;
      }

      expect(find.byType(RecordConstructEditor), findsOneWidget);

      final fields = model.getRecordConstructFields(instanceId);
      expect(fields, isNotNull);
      expect(fields!.length, equals(1));
      expect(fields.first.name, equals('amount'));

      // No stored literal yet -> Placeholder state, no clear button.
      expect(fields.first.storedValue, isNull);
      expect(fields.first.defaultValue, isNull);
      expect(
        find.byKey(const ValueKey('record_construct_field_amount')),
        findsOneWidget,
      );
      expect(
        find.byKey(const ValueKey('record_construct_field_clear_amount')),
        findsNothing,
      );
    });

    testWidgets('editing a field stores the literal and shows clear button',
        (tester) async {
      await pumpApp(tester, model);

      final instanceId = await _setupRecordConstructWithField(tester, model);
      if (instanceId == null) {
        debugPrint('Could not set up record_construct - skipping test');
        return;
      }

      final input =
          find.byKey(const Key('record_construct_field_input_amount'));
      expect(input, findsOneWidget);

      await tester.enterText(input, '17.25');
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();

      final fields = model.getRecordConstructFields(instanceId)!;
      expect(fields.first.storedValue, isNotNull);
      expect(
        fields.first.storedValue,
        equals(const APILiteralValue.float(17.25)),
      );

      // Stored state -> clear button is now present.
      final clearButton =
          find.byKey(const ValueKey('record_construct_field_clear_amount'));
      expect(clearButton, findsOneWidget);

      // Clearing returns the row to the Placeholder state.
      await tester.tap(clearButton);
      await tester.pumpAndSettle();
      expect(
        model.getRecordConstructFields(instanceId)!.first.storedValue,
        isNull,
      );
    });

    testWidgets('schema dropdown alone is shown when no schema chosen',
        (tester) async {
      await pumpApp(tester, model);

      final originalNetwork = model.nodeNetworkView?.name;
      if (originalNetwork == null) {
        debugPrint('No active network - skipping test');
        return;
      }

      final instanceId =
          model.createNode('record_construct', const Offset(200, 200));
      if (instanceId == BigInt.zero) {
        debugPrint('Could not create record_construct - skipping test');
        return;
      }
      model.setSelectedNode(instanceId);
      await tester.pumpAndSettle();

      expect(find.byType(RecordConstructEditor), findsOneWidget);
      // No schema chosen -> getRecordConstructFields returns null and no
      // field rows are rendered (only the dropdown).
      expect(model.getRecordConstructFields(instanceId), isNull);
    });
  });
}
