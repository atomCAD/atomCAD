import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'constants.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/ai_assistant_api.dart'
    as ai_api;

/// HTTP server for AI assistant integration.
///
/// Provides endpoints for querying and editing node networks via the
/// text format used by AI assistants.
///
/// ## Endpoints
///
/// - `GET /health` - Health check, returns `{"status": "ok"}`
/// - `GET /query` - Returns the active node network in text format
/// - `POST /edit?replace=true|false` - Applies text format edits to the network
/// - `GET /nodes?category=<cat>&verbose=true` - List all available node types by category
///
/// ## Example Usage
///
/// ```bash
/// # Query the network
/// curl http://localhost:19847/query
///
/// # Edit the network (incremental)
/// curl -X POST http://localhost:19847/edit \
///   -H "Content-Type: text/plain" \
///   -d 'sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
/// output sphere1'
///
/// # Edit the network (replace mode)
/// curl -X POST "http://localhost:19847/edit?replace=true" \
///   -H "Content-Type: text/plain" \
///   -d 'sphere1 = sphere { center: (0, 0, 0), radius: 5, visible: true }
/// output sphere1'
///
/// # List all node types (compact)
/// curl http://localhost:19847/nodes
///
/// # List node types in a specific category
/// curl "http://localhost:19847/nodes?category=Geometry3D"
///
/// # List all node types with descriptions (verbose)
/// curl "http://localhost:19847/nodes?verbose=true"
/// ```
class AiAssistantServer {
  HttpServer? _server;
  final int port;

  /// Callback to notify the UI when edits have been made.
  /// Set this to trigger a UI refresh after successful edits.
  void Function()? onNetworkEdited;

  AiAssistantServer({this.port = aiAssistantPort});

  /// Whether the server is currently running.
  bool get isRunning => _server != null;

  /// Start the HTTP server.
  Future<void> start() async {
    if (_server != null) {
      print('[AI Assistant] Server already running on port $port');
      return;
    }

    try {
      _server = await HttpServer.bind(InternetAddress.loopbackIPv4, port);
      print('[AI Assistant] Server started on http://localhost:$port');

      _server!.listen(_handleRequest);
    } catch (e) {
      print('[AI Assistant] Failed to start server on port $port: $e');
      rethrow;
    }
  }

  /// Stop the HTTP server.
  Future<void> stop() async {
    if (_server != null) {
      await _server!.close();
      _server = null;
      print('[AI Assistant] Server stopped');
    }
  }

  Future<void> _handleRequest(HttpRequest request) async {
    // Add CORS headers for local development
    request.response.headers.add('Access-Control-Allow-Origin', '*');
    request.response.headers
        .add('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
    request.response.headers
        .add('Access-Control-Allow-Headers', 'Content-Type');

    // Handle preflight requests
    if (request.method == 'OPTIONS') {
      request.response.statusCode = HttpStatus.ok;
      await request.response.close();
      return;
    }

    final path = request.uri.path;

    try {
      switch (path) {
        case '/health':
          await _handleHealth(request);
          break;
        case '/query':
          await _handleQuery(request);
          break;
        case '/edit':
          await _handleEdit(request);
          break;
        case '/nodes':
          await _handleNodes(request);
          break;
        default:
          request.response.statusCode = HttpStatus.notFound;
          request.response.headers.contentType = ContentType.json;
          request.response.write(jsonEncode({
            'error': 'Not found',
            'path': path,
          }));
      }
    } catch (e, stackTrace) {
      print('[AI Assistant] Error handling request: $e\n$stackTrace');
      request.response.statusCode = HttpStatus.internalServerError;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Internal server error',
        'message': e.toString(),
      }));
    }

    await request.response.close();
  }

  Future<void> _handleHealth(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    request.response.headers.contentType = ContentType.json;
    request.response.write(jsonEncode({'status': 'ok'}));
  }

  Future<void> _handleQuery(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Call Rust API to get network text representation
    final result = ai_api.aiQueryNetwork();

    request.response.headers.contentType = ContentType.text;
    request.response.write(result);
  }

  Future<void> _handleEdit(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Read request body
    final body = await utf8.decoder.bind(request).join();
    final replace = request.uri.queryParameters['replace'] == 'true';

    // Call Rust API to apply edits
    final resultJson = ai_api.aiEditNetwork(code: body, replace: replace);

    // Notify UI if callback is set
    onNetworkEdited?.call();

    request.response.headers.contentType = ContentType.json;
    request.response.write(resultJson);
  }

  Future<void> _handleNodes(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Get optional query parameters
    final category = request.uri.queryParameters['category'];
    final verbose = request.uri.queryParameters['verbose'] == 'true';

    // Call Rust API to get node types list
    final result = ai_api.aiListNodeTypes(category: category, verbose: verbose);

    request.response.headers.contentType = ContentType.text;
    request.response.write(result);
  }
}
