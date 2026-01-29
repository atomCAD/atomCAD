import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_cad/structure_designer/structure_designer.dart';
import 'package:flutter_cad/structure_designer/structure_designer_model.dart';
import 'package:flutter_cad/src/rust/frb_generated.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api_types.dart';
import 'package:window_manager/window_manager.dart';
import 'package:provider/provider.dart';
import 'package:flutter_cad/common/mouse_wheel_block_service.dart';
import 'package:flutter_window_close/flutter_window_close.dart';
import 'package:flutter_cad/ai_assistant/http_server.dart';

/// Global AI assistant server instance.
/// This is set in main() and accessed by _MyAppState to connect the UI refresh callback.
AiAssistantServer? _aiServer;

Future<void> main(List<String> args) async {
  await RustLib.init();

  // Check for CLI mode
  if (args.contains('--headless')) {
    await runHeadlessMode(args);
    exit(0);
  }

  // Normal GUI mode
  WidgetsFlutterBinding.ensureInitialized();
  await windowManager.ensureInitialized();

  // Start AI assistant HTTP server
  _aiServer = AiAssistantServer();
  try {
    await _aiServer!.start();
  } catch (e) {
    // Server failed to start (e.g., port in use) - continue without it
    print('[AI Assistant] Warning: Server failed to start: $e');
  }

  runApp(const MyApp());
}

Future<void> runHeadlessMode(List<String> args) async {
  try {
    if (args.contains('--batch')) {
      // Batch mode
      final config = _parseBatchArgs(args);
      print('Starting batch CLI mode...\n');
      final result = sd_api.runCliBatch(config: config);
      if (!result.success) {
        stderr.writeln('\n❌ ERROR: ${result.errorMessage}');
        exit(1);
      }
    } else {
      // Single run mode
      final config = _parseSingleArgs(args);
      print('Starting single CLI mode...\n');
      final result = sd_api.runCliSingle(config: config);
      if (!result.success) {
        stderr.writeln('\n❌ ERROR: ${result.errorMessage}');
        exit(1);
      }
    }
  } catch (e, stackTrace) {
    stderr.writeln('\n❌ FATAL ERROR: $e');
    stderr.writeln('Stack trace:');
    stderr.writeln(stackTrace);
    exit(1);
  }
}

CliConfig _parseSingleArgs(List<String> args) {
  String? cnndFile;
  String? networkName;
  String? outputFile;
  final params = <String, String>{};

  for (int i = 0; i < args.length; i++) {
    if (args[i] == '--file' || args[i] == '-f') {
      if (i + 1 < args.length) cnndFile = args[++i];
    } else if (args[i] == '--network' || args[i] == '-n') {
      if (i + 1 < args.length) networkName = args[++i];
    } else if (args[i] == '--output' || args[i] == '-o') {
      if (i + 1 < args.length) outputFile = args[++i];
    } else if (args[i] == '--param' || args[i] == '-p') {
      if (i + 1 < args.length) {
        final param = args[++i];
        final parts = param.split('=');
        if (parts.length == 2) {
          params[parts[0]] = parts[1];
        } else {
          stderr.writeln(
              'Invalid parameter format: $param (expected name=value)');
          exit(1);
        }
      }
    }
  }

  if (cnndFile == null || networkName == null || outputFile == null) {
    stderr.writeln(
        'Usage: atomcad --headless --file <path> --network <name> --output <path> [--param name=value]');
    exit(1);
  }

  return CliConfig(
    cnndFile: cnndFile,
    networkName: networkName,
    outputFile: outputFile,
    parameters: params,
  );
}

BatchCliConfig _parseBatchArgs(List<String> args) {
  String? cnndFile;
  String? batchFile;

  for (int i = 0; i < args.length; i++) {
    if (args[i] == '--file' || args[i] == '-f') {
      if (i + 1 < args.length) cnndFile = args[++i];
    } else if (args[i] == '--batch' || args[i] == '-b') {
      if (i + 1 < args.length) batchFile = args[++i];
    }
  }

  if (batchFile == null) {
    stderr.writeln(
        'Usage: atomcad --headless --batch <batch_file> [--file <cnnd_file>]');
    exit(1);
  }

  return BatchCliConfig(
    cnndFile: cnndFile ?? '',
    batchFile: batchFile,
  );
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

    // Connect AI assistant server to refresh UI after edits
    _aiServer?.onNetworkEdited = () {
      structureDesignerModel.refreshFromKernel();
    };

    // Connect AI assistant server to request re-render (for camera changes)
    _aiServer?.onRenderingNeeded = () {
      SchedulerBinding.instance.scheduleFrame();
    };

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
          if (forceTextScaleFactor) {
            return MediaQuery(
              data: MediaQuery.of(context)
                  .copyWith(textScaler: TextScaler.linear(1.0)),
              child: child!,
            );
          }

          return child!;
        },
      ),
    );
  }
}
