import 'dart:io';
import 'package:args/args.dart';
import 'package:http/http.dart' as http;

const int defaultPort = 19847;
const String baseUrl = 'http://localhost:$defaultPort';

Future<void> main(List<String> args) async {
  final parser = ArgParser()
    ..addFlag('help', abbr: 'h', negatable: false, help: 'Show help')
    ..addOption('port',
        abbr: 'p', defaultsTo: '$defaultPort', help: 'Server port');

  final queryParser = ArgParser();

  final editParser = ArgParser()
    ..addOption('code', abbr: 'c', mandatory: true, help: 'Edit commands')
    ..addFlag('replace',
        abbr: 'r', defaultsTo: false, help: 'Replace entire network');

  parser.addCommand('query', queryParser);
  parser.addCommand('edit', editParser);

  ArgResults results;
  try {
    results = parser.parse(args);
  } catch (e) {
    stderr.writeln('Error: $e');
    _printUsage(parser);
    exit(1);
  }

  if (results['help'] || results.command == null) {
    _printUsage(parser);
    exit(results['help'] ? 0 : 1);
  }

  final port = int.tryParse(results['port']) ?? defaultPort;
  final serverUrl = 'http://localhost:$port';

  // Check if atomCAD is running
  final isRunning = await _checkHealth(serverUrl);
  if (!isRunning) {
    stderr.writeln('Error: atomCAD is not running.');
    stderr.writeln('Please start atomCAD first, then try again.');
    exit(1);
  }

  final command = results.command!;
  switch (command.name) {
    case 'query':
      await _runQuery(serverUrl);
      break;
    case 'edit':
      final code = command['code'] as String;
      final replace = command['replace'] as bool;
      await _runEdit(serverUrl, code, replace);
      break;
    default:
      stderr.writeln('Unknown command: ${command.name}');
      exit(1);
  }
}

void _printUsage(ArgParser parser) {
  stdout.writeln('atomcad-cli - AI assistant interface for atomCAD');
  stdout.writeln('');
  stdout.writeln('Usage:');
  stdout.writeln('  atomcad-cli query                     Query the active node network');
  stdout.writeln('  atomcad-cli edit --code="..."         Edit the node network');
  stdout.writeln('  atomcad-cli edit --code="..." --replace  Replace entire network');
  stdout.writeln('');
  stdout.writeln('Options:');
  stdout.writeln(parser.usage);
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

Future<void> _runEdit(String serverUrl, String code, bool replace) async {
  try {
    final uri = replace
        ? Uri.parse('$serverUrl/edit?replace=true')
        : Uri.parse('$serverUrl/edit');

    final response = await http
        .post(uri, body: code)
        .timeout(const Duration(seconds: 30));

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
