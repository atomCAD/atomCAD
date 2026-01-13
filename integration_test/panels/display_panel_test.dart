import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import '../helpers/test_utils.dart';

/// Integration tests for display panel controls
///
/// Run with: flutter test integration_test/panels/display_panel_test.dart -d windows
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

  group('Geometry Visualization', () {
    testWidgets('Geometry visualization buttons are visible', (tester) async {
      await pumpApp(tester, model);

      // Verify all three geometry visualization buttons exist
      expect(TestFinders.geometryVisSurfaceSplatting, findsOneWidget);
      expect(TestFinders.geometryVisWireframe, findsOneWidget);
      expect(TestFinders.geometryVisSolid, findsOneWidget);
    });

    testWidgets('Surface splatting button changes mode', (tester) async {
      await pumpApp(tester, model);

      // Tap surface splatting button
      await tester.tap(TestFinders.geometryVisSurfaceSplatting);
      await tester.pumpAndSettle();

      // Verify the mode changed
      expect(
        model.preferences?.geometryVisualizationPreferences
            .geometryVisualization,
        equals(GeometryVisualization.surfaceSplatting),
      );
    });

    testWidgets('Wireframe button changes mode', (tester) async {
      await pumpApp(tester, model);

      // Tap wireframe button
      await tester.tap(TestFinders.geometryVisWireframe);
      await tester.pumpAndSettle();

      // Verify the mode changed
      expect(
        model.preferences?.geometryVisualizationPreferences
            .geometryVisualization,
        equals(GeometryVisualization.explicitMesh),
      );
      expect(
        model.preferences?.geometryVisualizationPreferences.wireframeGeometry,
        isTrue,
      );
    });

    testWidgets('Solid button changes mode', (tester) async {
      await pumpApp(tester, model);

      // Tap solid button
      await tester.tap(TestFinders.geometryVisSolid);
      await tester.pumpAndSettle();

      // Verify the mode changed
      expect(
        model.preferences?.geometryVisualizationPreferences
            .geometryVisualization,
        equals(GeometryVisualization.explicitMesh),
      );
      expect(
        model.preferences?.geometryVisualizationPreferences.wireframeGeometry,
        isFalse,
      );
    });

    testWidgets('Switching between modes updates selection', (tester) async {
      await pumpApp(tester, model);

      // Start with surface splatting
      await tester.tap(TestFinders.geometryVisSurfaceSplatting);
      await tester.pumpAndSettle();

      expect(
        model.preferences?.geometryVisualizationPreferences
            .geometryVisualization,
        equals(GeometryVisualization.surfaceSplatting),
      );

      // Switch to wireframe
      await tester.tap(TestFinders.geometryVisWireframe);
      await tester.pumpAndSettle();

      expect(
        model.preferences?.geometryVisualizationPreferences
            .geometryVisualization,
        equals(GeometryVisualization.explicitMesh),
      );
      expect(
        model.preferences?.geometryVisualizationPreferences.wireframeGeometry,
        isTrue,
      );

      // Switch to solid
      await tester.tap(TestFinders.geometryVisSolid);
      await tester.pumpAndSettle();

      expect(
        model.preferences?.geometryVisualizationPreferences.wireframeGeometry,
        isFalse,
      );
    });
  });

  group('Node Display Policy', () {
    testWidgets('Node display policy buttons are visible', (tester) async {
      await pumpApp(tester, model);

      // Verify all three node display policy buttons exist
      expect(TestFinders.nodeDisplayManual, findsOneWidget);
      expect(TestFinders.nodeDisplayPreferSelected, findsOneWidget);
      expect(TestFinders.nodeDisplayPreferFrontier, findsOneWidget);
    });

    testWidgets('Manual policy button changes mode', (tester) async {
      await pumpApp(tester, model);

      // Tap manual button
      await tester.tap(TestFinders.nodeDisplayManual);
      await tester.pumpAndSettle();

      // Verify the mode changed
      expect(
        model.preferences?.nodeDisplayPreferences.displayPolicy,
        equals(NodeDisplayPolicy.manual),
      );
    });

    testWidgets('Prefer selected policy button changes mode', (tester) async {
      await pumpApp(tester, model);

      // Tap prefer selected button
      await tester.tap(TestFinders.nodeDisplayPreferSelected);
      await tester.pumpAndSettle();

      // Verify the mode changed
      expect(
        model.preferences?.nodeDisplayPreferences.displayPolicy,
        equals(NodeDisplayPolicy.preferSelected),
      );
    });

    testWidgets('Prefer frontier policy button changes mode', (tester) async {
      await pumpApp(tester, model);

      // Tap prefer frontier button
      await tester.tap(TestFinders.nodeDisplayPreferFrontier);
      await tester.pumpAndSettle();

      // Verify the mode changed
      expect(
        model.preferences?.nodeDisplayPreferences.displayPolicy,
        equals(NodeDisplayPolicy.preferFrontier),
      );
    });

    testWidgets('Switching between policies updates selection', (tester) async {
      await pumpApp(tester, model);

      // Start with manual
      await tester.tap(TestFinders.nodeDisplayManual);
      await tester.pumpAndSettle();

      expect(
        model.preferences?.nodeDisplayPreferences.displayPolicy,
        equals(NodeDisplayPolicy.manual),
      );

      // Switch to prefer selected
      await tester.tap(TestFinders.nodeDisplayPreferSelected);
      await tester.pumpAndSettle();

      expect(
        model.preferences?.nodeDisplayPreferences.displayPolicy,
        equals(NodeDisplayPolicy.preferSelected),
      );

      // Switch to prefer frontier
      await tester.tap(TestFinders.nodeDisplayPreferFrontier);
      await tester.pumpAndSettle();

      expect(
        model.preferences?.nodeDisplayPreferences.displayPolicy,
        equals(NodeDisplayPolicy.preferFrontier),
      );
    });
  });

  group('Atomic Visualization', () {
    testWidgets('Atomic visualization buttons are visible', (tester) async {
      await pumpApp(tester, model);

      // Verify both atomic visualization buttons exist
      expect(TestFinders.atomicVisBallAndStick, findsOneWidget);
      expect(TestFinders.atomicVisSpaceFilling, findsOneWidget);
    });

    testWidgets('Ball and stick button changes mode', (tester) async {
      await pumpApp(tester, model);

      // Tap ball and stick button
      await tester.tap(TestFinders.atomicVisBallAndStick);
      await tester.pumpAndSettle();

      // Verify the mode changed
      expect(
        model.preferences?.atomicStructureVisualizationPreferences
            .visualization,
        equals(AtomicStructureVisualization.ballAndStick),
      );
    });

    testWidgets('Space filling button changes mode', (tester) async {
      await pumpApp(tester, model);

      // Tap space filling button
      await tester.tap(TestFinders.atomicVisSpaceFilling);
      await tester.pumpAndSettle();

      // Verify the mode changed
      expect(
        model.preferences?.atomicStructureVisualizationPreferences
            .visualization,
        equals(AtomicStructureVisualization.spaceFilling),
      );
    });

    testWidgets('Switching between modes updates selection', (tester) async {
      await pumpApp(tester, model);

      // Start with ball and stick
      await tester.tap(TestFinders.atomicVisBallAndStick);
      await tester.pumpAndSettle();

      expect(
        model.preferences?.atomicStructureVisualizationPreferences
            .visualization,
        equals(AtomicStructureVisualization.ballAndStick),
      );

      // Switch to space filling
      await tester.tap(TestFinders.atomicVisSpaceFilling);
      await tester.pumpAndSettle();

      expect(
        model.preferences?.atomicStructureVisualizationPreferences
            .visualization,
        equals(AtomicStructureVisualization.spaceFilling),
      );

      // Switch back to ball and stick
      await tester.tap(TestFinders.atomicVisBallAndStick);
      await tester.pumpAndSettle();

      expect(
        model.preferences?.atomicStructureVisualizationPreferences
            .visualization,
        equals(AtomicStructureVisualization.ballAndStick),
      );
    });
  });
}
