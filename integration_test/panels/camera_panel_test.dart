import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import '../helpers/test_utils.dart';

/// Integration tests for camera control panel
///
/// Run with: flutter test integration_test/panels/camera_panel_test.dart -d windows
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

  group('Camera Control Panel', () {
    testWidgets('Camera control panel is visible', (tester) async {
      await pumpApp(tester, model);

      // Verify the camera control elements exist
      expect(TestFinders.cameraViewDropdown, findsOneWidget);
      expect(TestFinders.cameraPerspectiveButton, findsOneWidget);
      expect(TestFinders.cameraOrthographicButton, findsOneWidget);
    });

    testWidgets('Camera view dropdown has all view options', (tester) async {
      await pumpApp(tester, model);

      // Open the dropdown
      await tester.tap(TestFinders.cameraViewDropdown);
      await tester.pumpAndSettle();

      // Verify all camera view options are present in the dropdown menu
      expect(find.text('Custom'), findsWidgets);
      expect(find.text('Top'), findsWidgets);
      expect(find.text('Bottom'), findsWidgets);
      expect(find.text('Front'), findsWidgets);
      expect(find.text('Back'), findsWidgets);
      expect(find.text('Left'), findsWidgets);
      expect(find.text('Right'), findsWidgets);

      // Close the dropdown by tapping elsewhere
      await tester.tapAt(Offset.zero);
      await tester.pumpAndSettle();
    });

    testWidgets('Camera view dropdown selection changes model',
        (tester) async {
      await pumpApp(tester, model);

      // Open the dropdown
      await tester.tap(TestFinders.cameraViewDropdown);
      await tester.pumpAndSettle();

      // Select 'Top' view (find the one in the dropdown menu, not the button)
      await tester.tap(find.text('Top').last);
      await tester.pumpAndSettle();

      // Verify the model was updated
      expect(model.cameraCanonicalView, equals(APICameraCanonicalView.top));
    });

    testWidgets('Can select different canonical views', (tester) async {
      await pumpApp(tester, model);

      // Test selecting Front view
      await tester.tap(TestFinders.cameraViewDropdown);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Front').last);
      await tester.pumpAndSettle();

      expect(model.cameraCanonicalView, equals(APICameraCanonicalView.front));

      // Test selecting Back view
      await tester.tap(TestFinders.cameraViewDropdown);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Back').last);
      await tester.pumpAndSettle();

      expect(model.cameraCanonicalView, equals(APICameraCanonicalView.back));

      // Test selecting Left view
      await tester.tap(TestFinders.cameraViewDropdown);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Left').last);
      await tester.pumpAndSettle();

      expect(model.cameraCanonicalView, equals(APICameraCanonicalView.left));

      // Test selecting Right view
      await tester.tap(TestFinders.cameraViewDropdown);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Right').last);
      await tester.pumpAndSettle();

      expect(model.cameraCanonicalView, equals(APICameraCanonicalView.right));
    });
  });

  group('Projection Mode', () {
    testWidgets('Perspective button sets perspective mode', (tester) async {
      await pumpApp(tester, model);

      // First ensure we're in orthographic mode
      await tester.tap(TestFinders.cameraOrthographicButton);
      await tester.pumpAndSettle();
      expect(model.isOrthographic, isTrue);

      // Switch to perspective
      await tester.tap(TestFinders.cameraPerspectiveButton);
      await tester.pumpAndSettle();

      // Verify perspective mode is set
      expect(model.isOrthographic, isFalse);
    });

    testWidgets('Orthographic button sets orthographic mode', (tester) async {
      await pumpApp(tester, model);

      // First ensure we're in perspective mode
      await tester.tap(TestFinders.cameraPerspectiveButton);
      await tester.pumpAndSettle();
      expect(model.isOrthographic, isFalse);

      // Switch to orthographic
      await tester.tap(TestFinders.cameraOrthographicButton);
      await tester.pumpAndSettle();

      // Verify orthographic mode is set
      expect(model.isOrthographic, isTrue);
    });

    testWidgets('Switching between projection modes works', (tester) async {
      await pumpApp(tester, model);

      // Get initial state
      final initialOrthographic = model.isOrthographic;

      // Toggle projection mode
      if (initialOrthographic) {
        await tester.tap(TestFinders.cameraPerspectiveButton);
      } else {
        await tester.tap(TestFinders.cameraOrthographicButton);
      }
      await tester.pumpAndSettle();

      // Verify it changed
      expect(model.isOrthographic, isNot(equals(initialOrthographic)));

      // Toggle back
      if (model.isOrthographic) {
        await tester.tap(TestFinders.cameraPerspectiveButton);
      } else {
        await tester.tap(TestFinders.cameraOrthographicButton);
      }
      await tester.pumpAndSettle();

      // Verify it's back to original
      expect(model.isOrthographic, equals(initialOrthographic));
    });
  });
}
