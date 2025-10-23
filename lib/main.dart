import 'package:flutter/material.dart';
import 'package:flutter_cad/structure_designer/structure_designer.dart';
import 'package:flutter_cad/common/ui_common.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/common_api.dart';
import 'package:window_manager/window_manager.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';

Future<void> main() async {
  await RustLib.init();

  WidgetsFlutterBinding.ensureInitialized();
  await windowManager.ensureInitialized();

  windowManager.setTitle('atomCAD');

  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  // Set this to true to force textScaleFactor to 1.0
  static const bool forceTextScaleFactor = true;

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider(
      create: (_) => MouseWheelBlockService(),
      child: MaterialApp(
        home: const Scaffold(
          body: StructureDesigner(),
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
