import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart';
import '../helpers/test_utils.dart';

/// Integration tests for the Preferences dialog
///
/// Run with: flutter test integration_test/dialogs/preferences_test.dart -d windows
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

  group('Preferences Dialog Opening/Closing', () {
    testWidgets('Preferences opens from Edit menu', (tester) async {
      await pumpApp(tester, model);

      // Open Edit menu
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();

      // Tap Preferences menu item
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Verify preferences dialog is visible
      expect(TestFinders.preferencesDialog, findsOneWidget);
      expect(find.text('Preferences'), findsOneWidget);
    });

    testWidgets('Preferences closes on X button', (tester) async {
      await pumpApp(tester, model);

      // Open Edit menu and Preferences
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Verify dialog is open
      expect(TestFinders.preferencesDialog, findsOneWidget);

      // Click close button
      await tester.tap(TestFinders.preferencesCloseButton);
      await tester.pumpAndSettle();

      // Verify dialog is dismissed
      expect(TestFinders.preferencesDialog, findsNothing);
    });
  });

  group('Visualization Method Dropdown', () {
    testWidgets('Visualization method dropdown is visible', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Verify dropdown is visible
      expect(TestFinders.visualizationMethodDropdown, findsOneWidget);
    });

    testWidgets('Visualization method dropdown works', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Open the visualization method dropdown
      await tester.tap(TestFinders.visualizationMethodDropdown);
      await tester.pumpAndSettle();

      // Verify dropdown options are visible
      expect(find.text('Surface Splatting'), findsWidgets);
      expect(find.text('Solid'), findsWidgets);
      expect(find.text('Wireframe'), findsWidgets);
    });

    testWidgets('Selecting Solid changes visualization method', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Open dropdown and select Solid
      await tester.tap(TestFinders.visualizationMethodDropdown);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Solid').last);
      await tester.pumpAndSettle();

      // Verify the model was updated
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

    testWidgets('Selecting Wireframe changes visualization method',
        (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Open dropdown and select Wireframe
      await tester.tap(TestFinders.visualizationMethodDropdown);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Wireframe').last);
      await tester.pumpAndSettle();

      // Verify the model was updated
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

    testWidgets('Selecting Surface Splatting changes visualization method',
        (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // First switch to Wireframe
      await tester.tap(TestFinders.visualizationMethodDropdown);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Wireframe').last);
      await tester.pumpAndSettle();

      // Now switch back to Surface Splatting
      await tester.tap(TestFinders.visualizationMethodDropdown);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Surface Splatting').last);
      await tester.pumpAndSettle();

      // Verify the model was updated
      expect(
        model.preferences?.geometryVisualizationPreferences
            .geometryVisualization,
        equals(GeometryVisualization.surfaceSplatting),
      );
    });
  });

  group('Display Camera Pivot Checkbox', () {
    testWidgets('Display camera pivot checkbox is visible', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Scroll down to find the checkbox
      await tester.scrollUntilVisible(
        TestFinders.displayCameraPivotCheckbox,
        100.0,
        scrollable: find.byType(Scrollable).first,
      );

      // Verify checkbox is visible
      expect(TestFinders.displayCameraPivotCheckbox, findsOneWidget);
    });

    testWidgets('Display camera pivot checkbox works', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Scroll down to find the checkbox
      await tester.scrollUntilVisible(
        TestFinders.displayCameraPivotCheckbox,
        100.0,
        scrollable: find.byType(Scrollable).first,
      );

      // Get initial value
      final initialValue = model
          .preferences?.geometryVisualizationPreferences.displayCameraTarget;

      // Toggle the checkbox
      await tester.tap(TestFinders.displayCameraPivotCheckbox);
      await tester.pumpAndSettle();

      // Verify value changed
      expect(
        model.preferences?.geometryVisualizationPreferences.displayCameraTarget,
        isNot(equals(initialValue)),
      );

      // Toggle back
      await tester.tap(TestFinders.displayCameraPivotCheckbox);
      await tester.pumpAndSettle();

      // Verify back to original
      expect(
        model.preferences?.geometryVisualizationPreferences.displayCameraTarget,
        equals(initialValue),
      );
    });
  });

  group('Background Settings', () {
    testWidgets('Background color input is visible', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Scroll down to find the background color input
      await tester.scrollUntilVisible(
        TestFinders.backgroundColorInput,
        100.0,
        scrollable: find.byType(Scrollable).first,
      );

      // Verify input is visible
      expect(TestFinders.backgroundColorInput, findsOneWidget);
      expect(find.text('Background color (RGB)'), findsOneWidget);
    });

    testWidgets('Show axes checkbox is visible and works', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Scroll down to find the show axes checkbox
      await tester.scrollUntilVisible(
        TestFinders.showAxesCheckbox,
        100.0,
        scrollable: find.byType(Scrollable).first,
      );

      // Verify checkbox is visible
      expect(TestFinders.showAxesCheckbox, findsOneWidget);
      expect(find.text('Show axes'), findsOneWidget);

      // Get initial value
      final initialValue = model.preferences?.backgroundPreferences.showAxes;

      // Toggle the checkbox
      await tester.tap(TestFinders.showAxesCheckbox);
      await tester.pumpAndSettle();

      // Verify value changed
      expect(
        model.preferences?.backgroundPreferences.showAxes,
        isNot(equals(initialValue)),
      );

      // Toggle back
      await tester.tap(TestFinders.showAxesCheckbox);
      await tester.pumpAndSettle();

      // Verify back to original
      expect(
        model.preferences?.backgroundPreferences.showAxes,
        equals(initialValue),
      );
    });

    testWidgets('Show grid checkbox is visible', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Scroll down to find the show grid checkbox
      await tester.scrollUntilVisible(
        TestFinders.showGridCheckbox,
        100.0,
        scrollable: find.byType(Scrollable).first,
      );

      // Verify checkbox is visible
      expect(TestFinders.showGridCheckbox, findsOneWidget);
      expect(find.text('Show grid'), findsOneWidget);
    });

    testWidgets('Show grid checkbox works', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Scroll down to find the show grid checkbox
      await tester.scrollUntilVisible(
        TestFinders.showGridCheckbox,
        100.0,
        scrollable: find.byType(Scrollable).first,
      );

      // Get initial value
      final initialValue = model.preferences?.backgroundPreferences.showGrid;

      // Toggle the checkbox
      await tester.tap(TestFinders.showGridCheckbox);
      await tester.pumpAndSettle();

      // Verify value changed
      expect(
        model.preferences?.backgroundPreferences.showGrid,
        isNot(equals(initialValue)),
      );

      // Toggle back
      await tester.tap(TestFinders.showGridCheckbox);
      await tester.pumpAndSettle();

      // Verify back to original
      expect(
        model.preferences?.backgroundPreferences.showGrid,
        equals(initialValue),
      );
    });

    testWidgets('Grid size input is visible', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Scroll down to find the grid size input
      await tester.scrollUntilVisible(
        TestFinders.gridSizeInput,
        100.0,
        scrollable: find.byType(Scrollable).first,
      );

      // Verify input is visible
      expect(TestFinders.gridSizeInput, findsOneWidget);
      expect(find.text('Grid size'), findsOneWidget);
    });
  });

  group('Preferences Persistence', () {
    testWidgets('Changes are applied immediately to model', (tester) async {
      await pumpApp(tester, model);

      // Open preferences dialog
      await tester.tap(find.byKey(TestKeys.editMenu));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(TestKeys.preferencesMenuItem));
      await tester.pumpAndSettle();

      // Change visualization method to Wireframe
      await tester.tap(TestFinders.visualizationMethodDropdown);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Wireframe').last);
      await tester.pumpAndSettle();

      // Verify model was updated immediately
      expect(
        model.preferences?.geometryVisualizationPreferences.wireframeGeometry,
        isTrue,
      );

      // Close dialog
      await tester.tap(TestFinders.preferencesCloseButton);
      await tester.pumpAndSettle();

      // Verify changes persist after closing
      expect(
        model.preferences?.geometryVisualizationPreferences.wireframeGeometry,
        isTrue,
      );
    });
  });
}
