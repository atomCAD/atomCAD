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
            'Filter by category (Annotation, MathAndProgramming, Geometry2D, Geometry3D, AtomicStructure, OtherBuiltin, Custom)');

  parser.addCommand('query', queryParser);
  parser.addCommand('edit', editParser);
  parser.addCommand('nodes', nodesParser);

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
        // Inline code provided
        await _runEdit(serverUrl, code, replace);
      } else {
        // Multi-line mode: read from stdin
        await _runMultilineEdit(serverUrl, replace);
      }
      break;
    case 'nodes':
      final category = command['category'] as String?;
      await _runNodes(serverUrl, category);
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
  stdout.writeln('  query, q          Show current node network');
  stdout.writeln('  edit              Enter edit mode (incremental)');
  stdout
      .writeln('  edit --replace    Enter edit mode (replace entire network)');
  stdout.writeln("  replace, r        Same as 'edit --replace'");
  stdout.writeln('  nodes             List all available node types');
  stdout.writeln('  nodes <category>  List nodes in specific category');
  stdout.writeln('  help, ?           Show this help');
  stdout.writeln('  quit, exit        Exit REPL');
  stdout.writeln('');
  stdout.writeln('Edit mode:');
  stdout.writeln('  Type text format commands, then:');
  stdout.writeln('  - Empty line to send');
  stdout.writeln("  - '.' on its own line to send");
  stdout.writeln('  - Ctrl+C to cancel');
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

Future<void> _runNodes(String serverUrl, String? category) async {
  try {
    final uri = category != null
        ? Uri.parse('$serverUrl/nodes?category=$category')
        : Uri.parse('$serverUrl/nodes');

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
        final category = parts.length > 1 ? parts[1] : null;
        await _runNodes(serverUrl, category);
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
