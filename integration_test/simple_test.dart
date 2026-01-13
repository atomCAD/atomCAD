import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:integration_test/integration_test.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    try {
      await RustLib.init();
    } catch (e) {
      // Already initialized, ignore
    }
  });

  testWidgets('FFI initialization works', (WidgetTester tester) async {
    final nodeTypes = sd_api.getNodeTypeViews();
    expect(nodeTypes, isNotNull);
    expect(nodeTypes!.isNotEmpty, true);
  });
}
