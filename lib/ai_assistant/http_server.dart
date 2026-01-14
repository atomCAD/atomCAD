import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'constants.dart';

/// HTTP server for AI assistant integration.
///
/// Provides endpoints for querying and editing node networks.
/// In Phase 1, returns stub responses without actual node network interaction.
class AiAssistantServer {
  HttpServer? _server;
  final int port;

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
    request.response.headers.add('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
    request.response.headers.add('Access-Control-Allow-Headers', 'Content-Type');

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

    // Phase 1: Return stub response
    request.response.headers.contentType = ContentType.text;
    request.response.write(stubQueryResponse);
  }

  Future<void> _handleEdit(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Read request body
    final body = await utf8.decoder.bind(request).join();
    final replace = request.uri.queryParameters['replace'] == 'true';

    // Phase 1: Ignore the code, return success
    request.response.headers.contentType = ContentType.json;
    request.response.write(jsonEncode({
      'success': true,
      'message': 'Edit received (stub - no changes applied)',
      'replace': replace,
      'code_length': body.length,
    }));
  }
}
