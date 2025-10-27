import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/common_api.dart';
import 'package:window_manager/window_manager.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:flutter_window_close/flutter_window_close.dart';

Future<void> main() async {
  await RustLib.init();

  WidgetsFlutterBinding.ensureInitialized();
  await windowManager.ensureInitialized();

  runApp(const MyApp());
}

class MyApp extends StatefulWidget {
  const MyApp({super.key});

  @override
  State<MyApp> createState() => _MyAppState();
}

class _MyAppState extends State<MyApp> {
  late StructureDesignerModel structureDesignerModel;
  final GlobalKey<NavigatorState> _navigatorKey = GlobalKey<NavigatorState>();

  // Set this to true to force textScaleFactor to 1.0
  static const bool forceTextScaleFactor = true;

  @override
  void initState() {
    super.initState();
    structureDesignerModel = StructureDesignerModel();
    structureDesignerModel.init();

    // Listen to model changes to update window title
    structureDesignerModel.addListener(_updateWindowTitle);
    _updateWindowTitle(); // Set initial title

    // Set up window close handler after the first frame
    WidgetsBinding.instance.addPostFrameCallback((_) {
      FlutterWindowClose.setWindowShouldCloseHandler(() async {
        if (structureDesignerModel.isDirty) {
          final context = _navigatorKey.currentContext;
          if (context == null) {
            return false;
          }
          final shouldClose = await showDialog<bool>(
            context: context,
            barrierDismissible: false,
            builder: (dialogContext) => AlertDialog(
              title: const Text('atomCAD'),
              content: Text(
                  'Do you want to quit without saving changes to ${structureDesignerModel.displayFileName}?'),
              actions: [
                TextButton(
                  onPressed: () => Navigator.of(dialogContext).pop(false),
                  child: const Text('Cancel'),
                ),
                TextButton(
                  onPressed: () => Navigator.of(dialogContext).pop(true),
                  child: const Text('Quit'),
                ),
              ],
            ),
          );
          return shouldClose ?? false;
        }
        return true; // No unsaved changes, allow close
      });
    });
  }

  @override
  void dispose() {
    structureDesignerModel.removeListener(_updateWindowTitle);
    structureDesignerModel.dispose();
    super.dispose();
  }

  void _updateWindowTitle() {
    final title = 'atomCAD - ${structureDesignerModel.windowTitle}';
    windowManager.setTitle(title);
  }

  @override
  Widget build(BuildContext context) {
    return MultiProvider(
      providers: [
        ChangeNotifierProvider(create: (_) => MouseWheelBlockService()),
        ChangeNotifierProvider.value(value: structureDesignerModel),
      ],
      child: MaterialApp(
        navigatorKey: _navigatorKey,
        home: Scaffold(
          body: StructureDesigner(
            model: structureDesignerModel,
          ),
        ),
        builder: (context, child) {
          // Debug: Print the current textScaleFactor
          final textScaleFactor = MediaQuery.of(context).textScaleFactor;
          print('Current textScaleFactor: $textScaleFactor');

          if (forceTextScaleFactor) {
            print('Forcing textScaleFactor to 1.0');
            return MediaQuery(
              data: MediaQuery.of(context).copyWith(textScaleFactor: 1.0),
              child: child!,
            );
          }

          return child!;
        },
      ),
    );
  }
}
