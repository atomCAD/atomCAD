import 'dart:convert';
import 'dart:io';
import 'package:args/args.dart';
import 'package:http/http.dart' as http;

const int defaultPort = 19847;

Future<void> main(List<String> args) async {
  final parser = ArgParser()
    ..addFlag('help', abbr: 'h', negatable: false, help: 'Show help')
    ..addOption('port',
        abbr: 'p', defaultsTo: '$defaultPort', help: 'Server port');

  final queryParser = ArgParser();

  final editParser = ArgParser()
    ..addOption('code', abbr: 'c', help: 'Edit commands (optional)')
    ..addFlag('replace',
        abbr: 'r', defaultsTo: false, help: 'Replace entire network');

  final nodesParser = ArgParser()
    ..addOption('category',
        abbr: 'c',
        help:
            'Filter by category (Annotation, MathAndProgramming, Geometry2D, Geometry3D, AtomicStructure, OtherBuiltin, Custom)')
    ..addFlag('verbose',
        abbr: 'v', defaultsTo: false, help: 'Include descriptions');

  final describeParser = ArgParser();

  final evaluateParser = ArgParser()
    ..addFlag('verbose',
        abbr: 'v', defaultsTo: false, help: 'Show detailed output');

  final cameraParser = ArgParser()
    ..addOption('eye', help: 'Camera position as x,y,z')
    ..addOption('target', help: 'Look-at point as x,y,z')
    ..addOption('up', help: 'Up vector as x,y,z')
    ..addFlag('orthographic',
        negatable: false, help: 'Use orthographic projection')
    ..addFlag('perspective',
        negatable: false, help: 'Use perspective projection')
    ..addOption('ortho-height', help: 'Orthographic half-height (zoom level)');

  final screenshotParser = ArgParser()
    ..addOption('output',
        abbr: 'o', help: 'Output PNG file path', mandatory: true)
    ..addOption('width', abbr: 'w', help: 'Image width in pixels')
    ..addOption('height', abbr: 'h', help: 'Image height in pixels')
    ..addOption('background', help: 'Background color as R,G,B (0-255)');

  parser.addCommand('query', queryParser);
  parser.addCommand('edit', editParser);
  parser.addCommand('nodes', nodesParser);
  parser.addCommand('describe', describeParser);
  parser.addCommand('evaluate', evaluateParser);
  parser.addCommand('camera', cameraParser);
  parser.addCommand('screenshot', screenshotParser);

  ArgResults results;
  try {
    results = parser.parse(args);
  } catch (e) {
    stderr.writeln('Error: $e');
    _printUsage();
    exit(1);
  }

  if (results['help']) {
    _printUsage();
    exit(0);
  }

  final port = int.tryParse(results['port']) ?? defaultPort;
  final serverUrl = 'http://localhost:$port';

  // No command = REPL mode
  if (results.command == null) {
    await _runRepl(serverUrl);
    return;
  }

  // Check if atomCAD is running for command mode
  final isRunning = await _checkHealth(serverUrl);
  if (!isRunning) {
    stderr.writeln('Error: atomCAD is not running on localhost:$port');
    stderr.writeln('Please start atomCAD and try again.');
    exit(1);
  }

  final command = results.command!;
  switch (command.name) {
    case 'query':
      await _runQuery(serverUrl);
      break;
    case 'edit':
      final code = command['code'] as String?;
      final replace = command['replace'] as bool;
      if (code != null) {
        // Inline code provided - process escape sequences
        final processedCode = _processEscapeSequences(code);
        await _runEdit(serverUrl, processedCode, replace);
      } else {
        // Multi-line mode: read from stdin
        await _runMultilineEdit(serverUrl, replace);
      }
      break;
    case 'nodes':
      final category = command['category'] as String?;
      final verbose = command['verbose'] as bool;
      await _runNodes(serverUrl, category, verbose);
      break;
    case 'describe':
      // Get the node name from positional arguments (rest)
      if (command.rest.isEmpty) {
        stderr.writeln('Error: Missing node name');
        stderr.writeln('Usage: atomcad-cli describe <node-name>');
        exit(1);
      }
      final nodeName = command.rest.first;
      await _runDescribe(serverUrl, nodeName);
      break;
    case 'evaluate':
      // Get the node identifier from positional arguments (rest)
      if (command.rest.isEmpty) {
        stderr.writeln('Error: Missing node identifier');
        stderr.writeln('Usage: atomcad-cli evaluate <node_id> [--verbose]');
        exit(1);
      }
      final nodeId = command.rest.first;
      final verbose = command['verbose'] as bool;
      await _runEvaluate(serverUrl, nodeId, verbose);
      break;
    case 'camera':
      await _runCamera(serverUrl, command);
      break;
    case 'screenshot':
      await _runScreenshot(serverUrl, command);
      break;
    default:
      stderr.writeln('Unknown command: ${command.name}');
      exit(1);
  }
}

void _printUsage() {
  stdout.writeln('atomcad-cli - AI assistant interface for atomCAD');
  stdout.writeln('');
  stdout.writeln('Usage:');
  stdout.writeln('  atomcad-cli                           Enter REPL mode');
  stdout.writeln(
      '  atomcad-cli query                     Query the active node network');
  stdout
      .writeln('  atomcad-cli edit --code="..."         Edit the node network');
  stdout.writeln(
      '  atomcad-cli edit --code="..." --replace  Replace entire network');
  stdout.writeln(
      '  atomcad-cli edit                      Multi-line edit from stdin');
  stdout.writeln(
      '  atomcad-cli edit --replace            Multi-line replace from stdin');
  stdout.writeln(
      '  atomcad-cli nodes                     List all available node types');
  stdout.writeln(
      '  atomcad-cli nodes --category=<cat>    List nodes in specific category');
  stdout
      .writeln('  atomcad-cli nodes --verbose           Include descriptions');
  stdout.writeln(
      '  atomcad-cli describe <node-name>      Describe a specific node type');
  stdout.writeln(
      '  atomcad-cli evaluate <node_id>        Evaluate a node and show result');
  stdout.writeln(
      '  atomcad-cli evaluate <node_id> -v     Evaluate with detailed output');
  stdout.writeln(
      '  atomcad-cli camera                    Get current camera state');
  stdout.writeln('  atomcad-cli camera --eye x,y,z --target x,y,z --up x,y,z');
  stdout.writeln('                                        Set camera position');
  stdout.writeln(
      '  atomcad-cli camera --orthographic     Switch to orthographic projection');
  stdout.writeln(
      '  atomcad-cli camera --perspective      Switch to perspective projection');
  stdout.writeln(
      '  atomcad-cli camera --ortho-height N   Set orthographic zoom level');
  stdout.writeln(
      '  atomcad-cli screenshot -o <path.png>  Capture viewport to PNG file');
  stdout.writeln('  atomcad-cli screenshot -o <path.png> -w 800 -h 600');
  stdout.writeln(
      '                                        Capture with specific resolution');
  stdout.writeln('');
  stdout.writeln('Options:');
  stdout.writeln('  -h, --help     Show this help');
  stdout.writeln('  -p, --port     Server port (default: $defaultPort)');
  stdout.writeln('');
  stdout.writeln('Categories:');
  stdout.writeln('  Annotation, MathAndProgramming, Geometry2D, Geometry3D,');
  stdout.writeln('  AtomicStructure, OtherBuiltin, Custom');
}

void _printReplHelp() {
  stdout.writeln('atomCAD REPL Commands:');
  stdout.writeln('');
  stdout.writeln('  query, q            Show current node network');
  stdout.writeln('  edit                Enter edit mode (incremental)');
  stdout.writeln(
      '  edit --replace      Enter edit mode (replace entire network)');
  stdout.writeln("  replace, r          Same as 'edit --replace'");
  stdout.writeln('  nodes               List all available node types');
  stdout.writeln('  nodes <category>    List nodes in specific category');
  stdout.writeln('  nodes -v            List with descriptions (verbose)');
  stdout.writeln('  describe, d <node>  Describe a specific node type');
  stdout.writeln('  evaluate, e <node>  Evaluate a node and show result');
  stdout.writeln('  evaluate -v <node>  Evaluate with detailed output');
  stdout.writeln('  camera, c           Get current camera state');
  stdout.writeln('  camera --eye x,y,z --target x,y,z --up x,y,z');
  stdout.writeln('                      Set camera position');
  stdout.writeln('  camera --ortho      Switch to orthographic projection');
  stdout.writeln('  camera --persp      Switch to perspective projection');
  stdout.writeln('  screenshot, s <path.png>');
  stdout.writeln('                      Capture viewport to PNG');
  stdout.writeln('  screenshot <path> -w 800 -h 600');
  stdout.writeln('                      Capture with specific resolution');
  stdout.writeln('  help, ?             Show this help');
  stdout.writeln('  quit, exit          Exit REPL');
  stdout.writeln('');
  stdout.writeln('Edit mode:');
  stdout.writeln('  Type text format commands, then:');
  stdout.writeln('  - Empty line to send');
  stdout.writeln("  - '.' on its own line to send");
  stdout.writeln('  - Ctrl+C to cancel');
}

/// Process escape sequences in --code argument.
///
/// Converts `\n` to actual newlines and `\\` to literal backslashes.
/// This allows multi-line input via --code without shell quoting issues.
String _processEscapeSequences(String input) {
  final buffer = StringBuffer();
  var i = 0;
  while (i < input.length) {
    if (input[i] == '\\' && i + 1 < input.length) {
      final next = input[i + 1];
      switch (next) {
        case 'n':
          buffer.write('\n');
          i += 2;
          continue;
        case 't':
          buffer.write('\t');
          i += 2;
          continue;
        case '\\':
          buffer.write('\\');
          i += 2;
          continue;
      }
    }
    buffer.write(input[i]);
    i++;
  }
  return buffer.toString();
}

Future<bool> _checkHealth(String serverUrl) async {
  try {
    final response = await http
        .get(Uri.parse('$serverUrl/health'))
        .timeout(const Duration(seconds: 2));
    return response.statusCode == 200;
  } catch (e) {
    return false;
  }
}

/// Resolves a potentially relative path to an absolute path.
/// This ensures paths are resolved relative to the CLI's working directory,
/// not atomCAD's working directory.
String _resolveToAbsolutePath(String path) {
  final file = File(path);
  if (file.isAbsolute) {
    return path;
  }
  return File(path).absolute.path;
}

Future<void> _runQuery(String serverUrl) async {
  try {
    final response = await http
        .get(Uri.parse('$serverUrl/query'))
        .timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      stdout.write(response.body);
      // Ensure trailing newline
      if (response.body.isNotEmpty && !response.body.endsWith('\n')) {
        stdout.writeln();
      }
    } else {
      stderr.writeln('Error: Server returned ${response.statusCode}');
      stderr.writeln(response.body);
    }
  } catch (e) {
    stderr.writeln('Error: Failed to connect to atomCAD: $e');
  }
}

Future<void> _runNodes(String serverUrl, String? category, bool verbose) async {
  try {
    final params = <String, String>{};
    if (category != null) params['category'] = category;
    if (verbose) params['verbose'] = 'true';

    final uri = Uri.parse('$serverUrl/nodes')
        .replace(queryParameters: params.isEmpty ? null : params);

    final response = await http.get(uri).timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      stdout.write(response.body);
      // Ensure trailing newline
      if (response.body.isNotEmpty && !response.body.endsWith('\n')) {
        stdout.writeln();
      }
    } else {
      stderr.writeln('Error: Server returned ${response.statusCode}');
      stderr.writeln(response.body);
    }
  } catch (e) {
    stderr.writeln('Error: Failed to connect to atomCAD: $e');
  }
}

Future<void> _runDescribe(String serverUrl, String nodeName) async {
  try {
    final uri = Uri.parse('$serverUrl/describe')
        .replace(queryParameters: {'node': nodeName});

    final response = await http.get(uri).timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      stdout.write(response.body);
      // Ensure trailing newline
      if (response.body.isNotEmpty && !response.body.endsWith('\n')) {
        stdout.writeln();
      }
    } else {
      stderr.writeln('Error: Server returned ${response.statusCode}');
      stderr.writeln(response.body);
    }
  } catch (e) {
    stderr.writeln('Error: Failed to connect to atomCAD: $e');
  }
}

Future<void> _runEvaluate(
    String serverUrl, String nodeIdentifier, bool verbose) async {
  try {
    final params = <String, String>{'node': nodeIdentifier};
    if (verbose) params['verbose'] = 'true';

    final uri =
        Uri.parse('$serverUrl/evaluate').replace(queryParameters: params);

    final response = await http.get(uri).timeout(const Duration(seconds: 30));

    if (response.statusCode == 200) {
      stdout.write(response.body);
      // Ensure trailing newline
      if (response.body.isNotEmpty && !response.body.endsWith('\n')) {
        stdout.writeln();
      }
    } else {
      stderr.writeln('Error: Server returned ${response.statusCode}');
      stderr.writeln(response.body);
    }
  } catch (e) {
    stderr.writeln('Error: Failed to connect to atomCAD: $e');
  }
}

Future<void> _runCamera(String serverUrl, ArgResults args) async {
  try {
    final queryParams = <String, String>{};

    if (args['eye'] != null) queryParams['eye'] = args['eye'];
    if (args['target'] != null) queryParams['target'] = args['target'];
    if (args['up'] != null) queryParams['up'] = args['up'];
    if (args['orthographic'] as bool) queryParams['orthographic'] = 'true';
    if (args['perspective'] as bool) queryParams['perspective'] = 'true';
    if (args['ortho-height'] != null) {
      queryParams['ortho_height'] = args['ortho-height'];
    }

    final uri = Uri.parse('$serverUrl/camera')
        .replace(queryParameters: queryParams.isEmpty ? null : queryParams);

    final response = await http.get(uri).timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      stdout.writeln(response.body);
    } else {
      stderr.writeln('Error: Server returned ${response.statusCode}');
      stderr.writeln(response.body);
      exit(1);
    }
  } catch (e) {
    stderr.writeln('Error: Failed to connect to atomCAD: $e');
    exit(1);
  }
}

Future<void> _runScreenshot(String serverUrl, ArgResults args) async {
  try {
    // Resolve relative paths to absolute paths based on CLI's working directory
    final outputPath = _resolveToAbsolutePath(args['output']);
    final queryParams = <String, String>{
      'output': outputPath,
    };

    if (args['width'] != null) queryParams['width'] = args['width'];
    if (args['height'] != null) queryParams['height'] = args['height'];
    if (args['background'] != null) {
      queryParams['background'] = args['background'];
    }

    final uri = Uri.parse('$serverUrl/screenshot')
        .replace(queryParameters: queryParams);

    final response = await http.get(uri).timeout(const Duration(seconds: 30));

    if (response.statusCode == 200) {
      // Parse the JSON response to show a nice message
      try {
        final result = jsonDecode(response.body);
        if (result['success'] == true) {
          stdout.writeln(
              'Screenshot saved: ${result['output_path']} (${result['width']}x${result['height']})');
        } else {
          stderr.writeln('Error: ${result['error']}');
          exit(1);
        }
      } catch (_) {
        stdout.writeln(response.body);
      }
    } else {
      stderr.writeln('Error: Server returned ${response.statusCode}');
      stderr.writeln(response.body);
      exit(1);
    }
  } catch (e) {
    stderr.writeln('Error: Failed to connect to atomCAD: $e');
    exit(1);
  }
}

Future<String?> _runEdit(String serverUrl, String code, bool replace) async {
  try {
    final uri = replace
        ? Uri.parse('$serverUrl/edit?replace=true')
        : Uri.parse('$serverUrl/edit');

    final response =
        await http.post(uri, body: code).timeout(const Duration(seconds: 30));

    if (response.statusCode == 200) {
      stdout.writeln(response.body);
      return null;
    } else {
      final error = 'Error: Server returned ${response.statusCode}';
      stderr.writeln(error);
      stderr.writeln(response.body);
      return error;
    }
  } catch (e) {
    final error = 'Error: Failed to connect to atomCAD: $e';
    stderr.writeln(error);
    return error;
  }
}

/// Read lines from stdin until terminator (empty line or '.')
List<String>? _readUntilTerminator({required bool showPrompt}) {
  final lines = <String>[];

  if (showPrompt) {
    stdout.write('edit> ');
  }

  while (true) {
    final line = stdin.readLineSync();

    // EOF (Ctrl+D on Unix, Ctrl+Z on Windows)
    if (line == null) {
      if (lines.isEmpty) {
        return null; // Cancelled
      }
      break;
    }

    // Empty line or single '.' terminates
    if (line.isEmpty || line == '.') {
      break;
    }

    lines.add(line);

    if (showPrompt) {
      stdout.write('edit> ');
    }
  }

  return lines;
}

Future<void> _runMultilineEdit(String serverUrl, bool replace) async {
  final isTty = stdin.hasTerminal;

  if (isTty) {
    stdout.writeln("Enter text format (empty line or '.' to send):");
  }

  final lines = _readUntilTerminator(showPrompt: false);

  if (lines == null || lines.isEmpty) {
    if (isTty) {
      stdout.writeln('No input provided.');
    }
    return;
  }

  final code = lines.join('\n');
  await _runEdit(serverUrl, code, replace);
}

Future<void> _runRepl(String serverUrl) async {
  // Check connection first
  final isRunning = await _checkHealth(serverUrl);
  if (!isRunning) {
    stderr.writeln('Error: atomCAD is not running on localhost');
    stderr.writeln('Please start atomCAD and try again.');
    exit(1);
  }

  stdout.writeln('atomCAD REPL (localhost:${Uri.parse(serverUrl).port})');
  stdout.writeln("Type 'help' for commands.");
  stdout.writeln('');

  while (true) {
    stdout.write('> ');
    final input = stdin.readLineSync();

    // EOF
    if (input == null) {
      stdout.writeln('');
      break;
    }

    final trimmed = input.trim();

    // Empty input
    if (trimmed.isEmpty) {
      continue;
    }

    // Parse command
    final parts = trimmed.split(RegExp(r'\s+'));
    final cmd = parts[0].toLowerCase();

    switch (cmd) {
      case 'query':
      case 'q':
        await _runQuery(serverUrl);
        break;

      case 'edit':
        final hasReplace = parts.contains('--replace') || parts.contains('-r');
        await _replEditMode(serverUrl, hasReplace);
        break;

      case 'replace':
      case 'r':
        await _replEditMode(serverUrl, true);
        break;

      case 'nodes':
      case 'n':
        // Check if a category argument was provided
        // Support: nodes, nodes <category>, nodes -v, nodes <category> -v
        final hasVerbose = parts.contains('-v') || parts.contains('--verbose');
        final categoryParts = parts
            .where((p) =>
                p != 'nodes' && p != 'n' && p != '-v' && p != '--verbose')
            .toList();
        final category = categoryParts.isNotEmpty ? categoryParts.first : null;
        await _runNodes(serverUrl, category, hasVerbose);
        break;

      case 'describe':
      case 'd':
        // describe <node-name>
        if (parts.length < 2) {
          stdout.writeln('Usage: describe <node-name>');
        } else {
          await _runDescribe(serverUrl, parts[1]);
        }
        break;

      case 'evaluate':
      case 'e':
        // evaluate <node_id> [-v]
        // Support: evaluate <node>, evaluate -v <node>, evaluate <node> -v
        final hasVerbose = parts.contains('-v') || parts.contains('--verbose');
        final nodeParts = parts
            .where((p) =>
                p != 'evaluate' && p != 'e' && p != '-v' && p != '--verbose')
            .toList();
        if (nodeParts.isEmpty) {
          stdout.writeln('Usage: evaluate <node_id> [-v]');
        } else {
          await _runEvaluate(serverUrl, nodeParts.first, hasVerbose);
        }
        break;

      case 'camera':
      case 'c':
        await _runCameraRepl(serverUrl, parts);
        break;

      case 'screenshot':
      case 's':
        await _runScreenshotRepl(serverUrl, parts);
        break;

      case 'help':
      case '?':
        _printReplHelp();
        break;

      case 'quit':
      case 'exit':
        return;

      default:
        stdout.writeln('Unknown command: $cmd');
        stdout.writeln("Type 'help' for available commands.");
    }
  }
}

Future<void> _runCameraRepl(String serverUrl, List<String> parts) async {
  try {
    final queryParams = <String, String>{};

    // Parse REPL-style arguments
    for (var i = 0; i < parts.length; i++) {
      final part = parts[i];
      if (part == 'camera' || part == 'c') continue;

      if (part == '--eye' && i + 1 < parts.length) {
        queryParams['eye'] = parts[++i];
      } else if (part == '--target' && i + 1 < parts.length) {
        queryParams['target'] = parts[++i];
      } else if (part == '--up' && i + 1 < parts.length) {
        queryParams['up'] = parts[++i];
      } else if (part == '--ortho' || part == '--orthographic') {
        queryParams['orthographic'] = 'true';
      } else if (part == '--persp' || part == '--perspective') {
        queryParams['perspective'] = 'true';
      } else if (part == '--ortho-height' && i + 1 < parts.length) {
        queryParams['ortho_height'] = parts[++i];
      }
    }

    final uri = Uri.parse('$serverUrl/camera')
        .replace(queryParameters: queryParams.isEmpty ? null : queryParams);

    final response = await http.get(uri).timeout(const Duration(seconds: 10));

    if (response.statusCode == 200) {
      stdout.writeln(response.body);
    } else {
      stderr.writeln('Error: Server returned ${response.statusCode}');
      stderr.writeln(response.body);
    }
  } catch (e) {
    stderr.writeln('Error: Failed to connect to atomCAD: $e');
  }
}

Future<void> _runScreenshotRepl(String serverUrl, List<String> parts) async {
  // Parse REPL-style arguments: screenshot <path> [-w width] [-h height] [--background r,g,b]
  String? outputPath;
  String? width;
  String? height;
  String? background;

  for (var i = 0; i < parts.length; i++) {
    final part = parts[i];
    if (part == 'screenshot' || part == 's') continue;

    if ((part == '-w' || part == '--width') && i + 1 < parts.length) {
      width = parts[++i];
    } else if ((part == '-h' || part == '--height') && i + 1 < parts.length) {
      height = parts[++i];
    } else if ((part == '-b' || part == '--background') &&
        i + 1 < parts.length) {
      background = parts[++i];
    } else if ((part == '-o' || part == '--output') && i + 1 < parts.length) {
      outputPath = parts[++i];
    } else if (!part.startsWith('-') && outputPath == null) {
      // First non-flag argument is the output path
      outputPath = part;
    }
  }

  if (outputPath == null || outputPath.isEmpty) {
    stdout.writeln('Usage: screenshot <output.png> [-w width] [-h height]');
    return;
  }

  // Resolve relative paths to absolute paths
  final absolutePath = _resolveToAbsolutePath(outputPath);

  try {
    final queryParams = <String, String>{'output': absolutePath};
    if (width != null) queryParams['width'] = width;
    if (height != null) queryParams['height'] = height;
    if (background != null) queryParams['background'] = background;

    final uri = Uri.parse('$serverUrl/screenshot')
        .replace(queryParameters: queryParams);

    final response = await http.get(uri).timeout(const Duration(seconds: 30));

    if (response.statusCode == 200) {
      try {
        final result = jsonDecode(response.body);
        if (result['success'] == true) {
          stdout.writeln(
              'Screenshot saved: ${result['output_path']} (${result['width']}x${result['height']})');
        } else {
          stderr.writeln('Error: ${result['error']}');
        }
      } catch (_) {
        stdout.writeln(response.body);
      }
    } else {
      stderr.writeln('Error: Server returned ${response.statusCode}');
      stderr.writeln(response.body);
    }
  } catch (e) {
    stderr.writeln('Error: Failed to connect to atomCAD: $e');
  }
}

Future<void> _replEditMode(String serverUrl, bool replace) async {
  final lines = _readUntilTerminator(showPrompt: true);

  // Ctrl+C or EOF with no content = cancel
  if (lines == null) {
    stdout.writeln('Cancelled.');
    return;
  }

  if (lines.isEmpty) {
    stdout.writeln('No input provided.');
    return;
  }

  final code = lines.join('\n');
  await _runEdit(serverUrl, code, replace);
}
