import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'dart:typed_data';

import 'constants.dart';
import 'package:flutter_cad/src/rust/api/structure_designer/ai_assistant_api.dart'
    as ai_api;
import 'package:flutter_cad/src/rust/api/structure_designer/ai_history_api.dart'
    as ai_history_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_api.dart'
    as sd_api;
import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;
import 'package:flutter_cad/src/rust/api/common_api_types.dart';
import 'package:flutter_cad/src/rust/api/screenshot_api.dart' as screenshot_api;
import 'package:flutter_cad/src/rust/api/structure_designer/structure_designer_preferences.dart'
    as prefs_api;

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
/// - `GET /describe?node=<name>` - Get detailed information about a specific node type
/// - `GET /evaluate?node=<id>&verbose=true` - Evaluate a node and return its result
/// - `GET /camera?eye=x,y,z&target=x,y,z&up=x,y,z&orthographic=true` - Control camera
/// - `GET /screenshot?output=<path>&width=<w>&height=<h>` - Capture viewport to PNG
/// - `GET /display` - Get current display preferences as JSON
/// - `GET /display?atomic-viz=...&geometry-viz=...` - Set display preferences
/// - `GET /networks` - List all node networks with validation status
/// - `POST /networks/add` - Create a new node network (optional: name parameter)
/// - `POST /networks/delete` - Delete a node network (required: name parameter)
/// - `POST /networks/activate` - Switch to a different node network (required: name parameter)
/// - `POST /networks/rename` - Rename a node network (required: old, new parameters)
///
/// ## Every request is logged
///
/// One hook in [_handleRequest] records each request into the running
/// application's AI History log — Phase 5 of `doc/design_ai_edit_history.md`.
/// `/edit` is deliberately **not** recorded here: it records itself far below,
/// inside `ai_text_edit`, with the snapshots, the diff and the layout outcome a
/// timeline entry has no room for. `/health` is skipped as polling noise — the
/// CLI health-checks before every single command, so recording it would double
/// the length of every session log for nothing.
///
/// A caller may identify itself with an `X-Client-Label` header
/// ([clientLabelHeader]); `atomcad-cli --label` sends one. The label is
/// announced to the kernel before the handler runs, so an `/edit` in this
/// request carries it too.
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
///
/// # Describe a specific node type
/// curl "http://localhost:19847/describe?node=sphere"
///
/// # Evaluate a node (brief output)
/// curl "http://localhost:19847/evaluate?node=sphere1"
///
/// # Evaluate a node (verbose output with detailed info)
/// curl "http://localhost:19847/evaluate?node=sphere1&verbose=true"
///
/// # Capture screenshot
/// curl "http://localhost:19847/screenshot?output=viewport.png"
///
/// # Capture screenshot with custom resolution
/// curl "http://localhost:19847/screenshot?output=hires.png&width=1920&height=1080"
/// ```
/// The header a client may use to say what it is — "Opus 5 / skill v3".
///
/// Deferred through Phases 1-4 as open question 5 and landed in Phase 5: the
/// application cannot otherwise know which model is driving the CLI, and a
/// manually typed session label cannot distinguish two models editing in turn.
const String clientLabelHeader = 'X-Client-Label';

/// Paths that the request hook does **not** record.
///
/// `/edit` records itself with full fidelity inside `ai_text_edit`; recording
/// it again here would put two rows on the timeline for one edit. `/health` is
/// what the CLI polls before every command — noise, and a lot of it.
const Set<String> _unloggedPaths = {'/edit', '/health'};

class AiAssistantServer {
  HttpServer? _server;
  final int port;

  /// A short note the current handler wants on its timeline entry — the network
  /// a rename targeted, the name a delete removed. Only handlers whose
  /// arguments arrive in a **request body** need it; a query string is recorded
  /// verbatim and already says everything.
  ///
  /// One field rather than a parameter threaded through nineteen handlers,
  /// which is only sound because requests here are served one at a time in
  /// practice (the CLI is serial, and `ai_history_set_client_label` on the Rust
  /// side already makes the same assumption). It is cleared at the start of
  /// every request, so the worst a genuinely concurrent pair could do is
  /// mislabel a row.
  String? _activityDetail;

  /// Callback to notify the UI when edits have been made.
  /// Set this to trigger a UI refresh after successful edits.
  void Function()? onNetworkEdited;

  /// Callback to request a viewport re-render (without re-evaluating nodes).
  /// Used for camera changes that only need visual refresh.
  void Function()? onRenderingNeeded;

  /// Callback fired after a request was recorded on the AI History timeline.
  ///
  /// Deliberately **not** [onNetworkEdited]: a `query` or a `screenshot`
  /// changes nothing about the network, and running a full kernel refresh after
  /// each one would make reading the network more expensive than editing it.
  /// This one only refreshes the history panel.
  void Function()? onCliActivity;

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
    request.response.headers.add(
        'Access-Control-Allow-Headers', 'Content-Type, $clientLabelHeader');

    // Handle preflight requests
    if (request.method == 'OPTIONS') {
      request.response.statusCode = HttpStatus.ok;
      await request.response.close();
      return;
    }

    final path = request.uri.path;

    // Phase 5 recording, half one: announce who is calling *before* dispatch,
    // so that an `/edit` in this request — whose record is pushed deep inside
    // the domain crate, which knows nothing about HTTP — carries the label too.
    final stopwatch = Stopwatch()..start();
    _activityDetail = null;
    ai_history_api.aiHistorySetClientLabel(
      label: request.headers.value(clientLabelHeader)?.trim() ?? '',
    );

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
        case '/describe':
          await _handleDescribe(request);
          break;
        case '/evaluate':
          await _handleEvaluate(request);
          break;
        case '/camera':
          await _handleCamera(request);
          break;
        case '/screenshot':
          await _handleScreenshot(request);
          break;
        case '/display':
          await _handleDisplay(request);
          break;
        case '/networks':
          await _handleNetworks(request);
          break;
        case '/networks/add':
          await _handleNetworksAdd(request);
          break;
        case '/networks/delete':
          await _handleNetworksDelete(request);
          break;
        case '/networks/activate':
          await _handleNetworksActivate(request);
          break;
        case '/networks/rename':
          await _handleNetworksRename(request);
          break;
        case '/file':
          await _handleFile(request);
          break;
        case '/load':
          await _handleLoad(request);
          break;
        case '/save':
          await _handleSave(request);
          break;
        case '/new':
          await _handleNew(request);
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

    // Read before the close: the status is settled by now, and reaching into a
    // closed response for it is asking for trouble as dart:io evolves.
    final status = request.response.statusCode;
    await request.response.close();

    // …and half two: one entry per request, after the response is written so
    // the duration is the whole round trip.
    _recordActivity(request, path, status, stopwatch);
  }

  /// Push one timeline entry for a finished request (Phase 5).
  ///
  /// Never throws into the request path: a log that can break the server it
  /// observes is worse than no log.
  void _recordActivity(
    HttpRequest request,
    String path,
    int status,
    Stopwatch stopwatch,
  ) {
    if (_unloggedPaths.contains(path)) return;
    try {
      ai_history_api.aiHistoryRecordActivity(
        method: request.method,
        path: path,
        query: request.uri.query,
        detail: _activityDetail ?? '',
        status: status,
        durationMs: stopwatch.elapsedMilliseconds,
      );
      onCliActivity?.call();
    } catch (e) {
      print('[AI Assistant] Failed to record activity for $path: $e');
    } finally {
      _activityDetail = null;
    }
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

  Future<void> _handleDescribe(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Get required node name parameter
    final nodeName = request.uri.queryParameters['node'];
    if (nodeName == null || nodeName.isEmpty) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Missing required parameter: node',
      }));
      return;
    }

    // Call Rust API to get node type description
    final result = ai_api.aiDescribeNodeType(nodeTypeName: nodeName);

    request.response.headers.contentType = ContentType.text;
    request.response.write(result);
  }

  Future<void> _handleEvaluate(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Get required node parameter
    final nodeIdentifier = request.uri.queryParameters['node'];
    if (nodeIdentifier == null || nodeIdentifier.isEmpty) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Missing required parameter: node',
      }));
      return;
    }

    // Get optional verbose flag
    final verbose = request.uri.queryParameters['verbose'] == 'true';

    try {
      // Call Rust API to evaluate the node
      final result = sd_api.evaluateNode(
        nodeIdentifier: nodeIdentifier,
        verbose: verbose,
      );

      // Format output based on verbosity
      if (verbose && result.detailedString != null) {
        request.response.headers.contentType = ContentType.text;
        request.response.write(result.detailedString!);
      } else {
        request.response.headers.contentType = ContentType.text;
        request.response.write(result.displayString);
      }
    } catch (e) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': e.toString(),
      }));
    }
  }

  Future<void> _handleCamera(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    final params = request.uri.queryParameters;

    var cameraChanged = false;

    // Parse and apply eye, target, up vectors
    final hasEye = params.containsKey('eye');
    final hasTarget = params.containsKey('target');
    final hasUp = params.containsKey('up');

    // Validate: if any position param is provided, all three must be provided
    if (hasEye || hasTarget || hasUp) {
      if (!(hasEye && hasTarget && hasUp)) {
        final missing = <String>[];
        if (!hasEye) missing.add('eye');
        if (!hasTarget) missing.add('target');
        if (!hasUp) missing.add('up');
        request.response.statusCode = HttpStatus.badRequest;
        request.response.headers.contentType = ContentType.json;
        request.response.write(jsonEncode({
          'success': false,
          'error':
              'eye, target, and up must all be provided together. Missing: ${missing.join(', ')}',
        }));
        return;
      }
      final eye = _parseVec3(params['eye']!);
      final target = _parseVec3(params['target']!);
      final up = _parseVec3(params['up']!);
      common_api.moveCamera(eye: eye, target: target, up: up);
      cameraChanged = true;
    }

    // Handle projection mode
    if (params.containsKey('orthographic')) {
      common_api.setOrthographicMode(orthographic: true);
      cameraChanged = true;
    } else if (params.containsKey('perspective')) {
      common_api.setOrthographicMode(orthographic: false);
      cameraChanged = true;
    }

    // Handle ortho height
    if (params.containsKey('ortho_height')) {
      final height = double.parse(params['ortho_height']!);
      common_api.setOrthoHalfHeight(halfHeight: height);
      cameraChanged = true;
    }

    // Request re-render if camera was changed (no need to re-evaluate nodes)
    if (cameraChanged) {
      onRenderingNeeded?.call();
    }

    // Return current camera state
    final camera = common_api.getCamera();
    request.response.headers.contentType = ContentType.json;
    if (camera != null) {
      request.response.write(jsonEncode({
        'success': true,
        'camera': {
          'eye': [camera.eye.x, camera.eye.y, camera.eye.z],
          'target': [camera.target.x, camera.target.y, camera.target.z],
          'up': [camera.up.x, camera.up.y, camera.up.z],
          'orthographic': camera.orthographic,
          'ortho_half_height': camera.orthoHalfHeight,
        }
      }));
    } else {
      request.response.statusCode = HttpStatus.internalServerError;
      request.response.write(jsonEncode({
        'success': false,
        'error': 'Camera not available',
      }));
    }
  }

  Future<void> _handleScreenshot(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    final params = request.uri.queryParameters;

    // Required: output path
    final outputPath = params['output'];
    if (outputPath == null || outputPath.isEmpty) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Missing required parameter: output',
      }));
      return;
    }

    // Optional: width and height
    final width =
        params['width'] != null ? int.tryParse(params['width']!) : null;
    final height =
        params['height'] != null ? int.tryParse(params['height']!) : null;

    // Validate resolution limits (max 4096x4096)
    const maxResolution = 4096;
    if (width != null && (width < 1 || width > maxResolution)) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Width must be between 1 and $maxResolution, got $width',
      }));
      return;
    }
    if (height != null && (height < 1 || height > maxResolution)) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Height must be between 1 and $maxResolution, got $height',
      }));
      return;
    }

    // Optional: background color as R,G,B
    Uint8List? bgColor;
    if (params['background'] != null) {
      try {
        final parts = params['background']!
            .split(',')
            .map((s) => int.parse(s.trim()))
            .toList();
        if (parts.length == 3) {
          bgColor = Uint8List.fromList(parts);
        }
      } catch (_) {
        // Ignore parse errors, use default background
      }
    }

    // Call Rust API
    final result = screenshot_api.captureScreenshot(
      outputPath: outputPath,
      width: width,
      height: height,
      backgroundRgb: bgColor,
    );

    request.response.headers.contentType = ContentType.json;
    if (result.success) {
      request.response.write(jsonEncode({
        'success': true,
        'output_path': result.outputPath,
        'width': result.width,
        'height': result.height,
      }));
    } else {
      request.response.statusCode = HttpStatus.internalServerError;
      request.response.write(jsonEncode({
        'success': false,
        'error': result.errorMessage,
      }));
    }
  }

  Future<void> _handleDisplay(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    final params = request.uri.queryParameters;

    // Get current preferences
    var prefs = sd_api.getStructureDesignerPreferences();
    if (prefs == null) {
      request.response.statusCode = HttpStatus.internalServerError;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'success': false,
        'error': 'Display preferences not available',
      }));
      return;
    }

    var changed = false;

    // Handle atomic-viz parameter
    if (params.containsKey('atomic-viz')) {
      final value = params['atomic-viz'];
      prefs_api.AtomicStructureVisualization? viz;
      switch (value) {
        case 'ball-and-stick':
          viz = prefs_api.AtomicStructureVisualization.ballAndStick;
          break;
        case 'space-filling':
          viz = prefs_api.AtomicStructureVisualization.spaceFilling;
          break;
        default:
          request.response.statusCode = HttpStatus.badRequest;
          request.response.headers.contentType = ContentType.json;
          request.response.write(jsonEncode({
            'error':
                'Invalid atomic-viz value: $value. Valid: ball-and-stick, space-filling',
          }));
          return;
      }
      prefs.atomicStructureVisualizationPreferences.visualization = viz;
      changed = true;
    }

    // Handle geometry-viz parameter
    if (params.containsKey('geometry-viz')) {
      final value = params['geometry-viz'];
      switch (value) {
        case 'surface-splatting':
          prefs.geometryVisualizationPreferences.geometryVisualization =
              prefs_api.GeometryVisualization.surfaceSplatting;
          prefs.geometryVisualizationPreferences.wireframeGeometry = false;
          break;
        case 'solid':
          prefs.geometryVisualizationPreferences.geometryVisualization =
              prefs_api.GeometryVisualization.explicitMesh;
          prefs.geometryVisualizationPreferences.wireframeGeometry = false;
          break;
        case 'wireframe':
          prefs.geometryVisualizationPreferences.geometryVisualization =
              prefs_api.GeometryVisualization.explicitMesh;
          prefs.geometryVisualizationPreferences.wireframeGeometry = true;
          break;
        default:
          request.response.statusCode = HttpStatus.badRequest;
          request.response.headers.contentType = ContentType.json;
          request.response.write(jsonEncode({
            'error':
                'Invalid geometry-viz value: $value. Valid: surface-splatting, solid, wireframe',
          }));
          return;
      }
      changed = true;
    }

    // Handle node-policy parameter
    if (params.containsKey('node-policy')) {
      final value = params['node-policy'];
      prefs_api.NodeDisplayPolicy? policy;
      switch (value) {
        case 'manual':
          policy = prefs_api.NodeDisplayPolicy.manual;
          break;
        case 'prefer-selected':
          policy = prefs_api.NodeDisplayPolicy.preferSelected;
          break;
        case 'prefer-frontier':
          policy = prefs_api.NodeDisplayPolicy.preferFrontier;
          break;
        default:
          request.response.statusCode = HttpStatus.badRequest;
          request.response.headers.contentType = ContentType.json;
          request.response.write(jsonEncode({
            'error':
                'Invalid node-policy value: $value. Valid: manual, prefer-selected, prefer-frontier',
          }));
          return;
      }
      prefs.nodeDisplayPreferences.displayPolicy = policy;
      changed = true;
    }

    // Handle background parameter (R,G,B)
    if (params.containsKey('background')) {
      try {
        final parts = params['background']!
            .split(',')
            .map((s) => int.parse(s.trim()))
            .toList();
        if (parts.length != 3) {
          throw FormatException('Expected 3 values');
        }
        for (final v in parts) {
          if (v < 0 || v > 255) {
            throw FormatException('Values must be 0-255');
          }
        }
        prefs.backgroundPreferences.backgroundColor =
            APIIVec3(x: parts[0], y: parts[1], z: parts[2]);
        changed = true;
      } catch (e) {
        request.response.statusCode = HttpStatus.badRequest;
        request.response.headers.contentType = ContentType.json;
        request.response.write(jsonEncode({
          'error':
              'Invalid background value: ${params['background']}. Expected R,G,B (0-255 each)',
        }));
        return;
      }
    }

    // Apply changes if any
    if (changed) {
      sd_api.setStructureDesignerPreferences(preferences: prefs);
      onNetworkEdited?.call();
    }

    // Build response with current state
    final atomicViz =
        prefs.atomicStructureVisualizationPreferences.visualization ==
                prefs_api.AtomicStructureVisualization.ballAndStick
            ? 'ball-and-stick'
            : 'space-filling';

    String geometryViz;
    if (prefs.geometryVisualizationPreferences.geometryVisualization ==
        prefs_api.GeometryVisualization.surfaceSplatting) {
      geometryViz = 'surface-splatting';
    } else if (prefs.geometryVisualizationPreferences.wireframeGeometry) {
      geometryViz = 'wireframe';
    } else {
      geometryViz = 'solid';
    }

    String nodePolicy;
    switch (prefs.nodeDisplayPreferences.displayPolicy) {
      case prefs_api.NodeDisplayPolicy.manual:
        nodePolicy = 'manual';
        break;
      case prefs_api.NodeDisplayPolicy.preferSelected:
        nodePolicy = 'prefer-selected';
        break;
      case prefs_api.NodeDisplayPolicy.preferFrontier:
        nodePolicy = 'prefer-frontier';
        break;
    }

    final bgColor = prefs.backgroundPreferences.backgroundColor;

    request.response.headers.contentType = ContentType.json;
    request.response.write(jsonEncode({
      'success': true,
      'display': {
        'atomic_visualization': atomicViz,
        'geometry_visualization': geometryViz,
        'node_display_policy': nodePolicy,
        'background_color': [bgColor.x, bgColor.y, bgColor.z],
      }
    }));
  }

  APIVec3 _parseVec3(String s) {
    // Handle both comma-separated and space-separated values
    // (URL encoding can convert commas to spaces in some cases)
    final parts = s
        .split(RegExp(r'[,\s]+'))
        .where((p) => p.isNotEmpty)
        .map((p) => double.parse(p.trim()))
        .toList();
    if (parts.length != 3) {
      throw FormatException(
          'Expected 3 values for vector, got ${parts.length}: "$s"');
    }
    return APIVec3(x: parts[0], y: parts[1], z: parts[2]);
  }

  Future<void> _handleNetworks(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Get all networks with validation status
    final networks = sd_api.getNodeNetworksWithValidation();
    if (networks == null) {
      request.response.statusCode = HttpStatus.internalServerError;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Networks not available',
      }));
      return;
    }

    // Get active network info
    final activeInfo = ai_api.aiGetActiveNetworkInfo();
    final activeName = activeInfo?.$1;

    // Build text output matching the plan format
    final buffer = StringBuffer();
    buffer.writeln('Node Networks:');

    var errorCount = 0;
    for (final network in networks) {
      final isActive = network.name == activeName;
      final activeMarker = isActive ? '* ' : '  ';
      final activeSuffix = isActive ? '  (active)' : '';

      if (network.validationErrors.isNotEmpty) {
        final errorText =
            network.validationErrors.map((e) => e.errorText).join('; ');
        buffer.writeln(
            '$activeMarker${network.name}$activeSuffix  [ERROR: $errorText]');
        errorCount++;
      } else {
        buffer.writeln('$activeMarker${network.name}$activeSuffix');
      }
    }

    buffer.writeln();
    if (errorCount > 0) {
      buffer.writeln('${networks.length} networks ($errorCount with errors)');
    } else {
      buffer.writeln('${networks.length} networks');
    }

    request.response.headers.contentType = ContentType.text;
    request.response.write(buffer.toString());
  }

  Future<void> _handleNetworksAdd(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Read request body as JSON (optional)
    final body = await utf8.decoder.bind(request).join();
    String? name;
    if (body.isNotEmpty) {
      try {
        final json = jsonDecode(body);
        name = json['name'] as String?;
      } catch (_) {
        // Ignore parse errors, proceed with auto-naming
      }
    }

    request.response.headers.contentType = ContentType.json;

    if (name != null && name.isNotEmpty) {
      _activityDetail = name;
      // Create with specific name
      final result = sd_api.addNodeNetworkWithName(name: name);
      if (result.success) {
        onNetworkEdited?.call();
        request.response.write(jsonEncode({
          'success': true,
          'message': "Created network '$name' (now active)",
          'name': name,
        }));
      } else {
        request.response.statusCode = HttpStatus.badRequest;
        request.response.write(jsonEncode({
          'success': false,
          'error': result.errorMessage,
        }));
      }
    } else {
      // Auto-name: generate unique name and use addNodeNetworkWithName
      // (which auto-activates the new network)
      final networks = sd_api.getNodeNetworksWithValidation();
      final existingNames = networks?.map((n) => n.name).toSet() ?? <String>{};

      var autoName = 'UNTITLED';
      var i = 1;
      while (existingNames.contains(autoName)) {
        autoName = 'UNTITLED$i';
        i++;
      }

      final result = sd_api.addNodeNetworkWithName(name: autoName);
      if (result.success) {
        onNetworkEdited?.call();
        request.response.write(jsonEncode({
          'success': true,
          'message': "Created network '$autoName' (now active)",
          'name': autoName,
        }));
      } else {
        request.response.statusCode = HttpStatus.badRequest;
        request.response.write(jsonEncode({
          'success': false,
          'error': result.errorMessage,
        }));
      }
    }
  }

  Future<void> _handleNetworksDelete(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Read request body as JSON
    final body = await utf8.decoder.bind(request).join();
    String? name;
    if (body.isNotEmpty) {
      try {
        final json = jsonDecode(body);
        name = json['name'] as String?;
      } catch (_) {
        // Ignore
      }
    }

    if (name == null || name.isEmpty) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Missing required parameter: name',
      }));
      return;
    }

    _activityDetail = name;
    final result = sd_api.deleteNodeNetwork(networkName: name);
    request.response.headers.contentType = ContentType.json;

    if (result.success) {
      onNetworkEdited?.call();
      request.response.write(jsonEncode({
        'success': true,
        'message': "Deleted network '$name'",
      }));
    } else {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.write(jsonEncode({
        'success': false,
        'error': result.errorMessage,
      }));
    }
  }

  Future<void> _handleNetworksActivate(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Read request body as JSON
    final body = await utf8.decoder.bind(request).join();
    String? name;
    if (body.isNotEmpty) {
      try {
        final json = jsonDecode(body);
        name = json['name'] as String?;
      } catch (_) {
        // Ignore
      }
    }

    if (name == null || name.isEmpty) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Missing required parameter: name',
      }));
      return;
    }

    // Check if network exists
    final networks = sd_api.getNodeNetworksWithValidation();
    final exists = networks?.any((n) => n.name == name) ?? false;

    if (!exists) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'success': false,
        'error': "Network '$name' not found",
      }));
      return;
    }

    _activityDetail = name;
    sd_api.setActiveNodeNetwork(nodeNetworkName: name);
    onNetworkEdited?.call();

    request.response.headers.contentType = ContentType.json;
    request.response.write(jsonEncode({
      'success': true,
      'message': "Switched to network '$name'",
    }));
  }

  Future<void> _handleNetworksRename(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    // Read request body as JSON
    final body = await utf8.decoder.bind(request).join();
    String? oldName;
    String? newName;
    if (body.isNotEmpty) {
      try {
        final json = jsonDecode(body);
        oldName = json['old'] as String?;
        newName = json['new'] as String?;
      } catch (_) {
        // Ignore
      }
    }

    if (oldName == null || oldName.isEmpty) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Missing required parameter: old',
      }));
      return;
    }

    if (newName == null || newName.isEmpty) {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.headers.contentType = ContentType.json;
      request.response.write(jsonEncode({
        'error': 'Missing required parameter: new',
      }));
      return;
    }

    _activityDetail = '$oldName → $newName';
    final success =
        sd_api.renameNodeNetwork(oldName: oldName, newName: newName);
    request.response.headers.contentType = ContentType.json;

    if (success) {
      onNetworkEdited?.call();
      request.response.write(jsonEncode({
        'success': true,
        'message': "Renamed '$oldName' to '$newName'",
      }));
    } else {
      request.response.statusCode = HttpStatus.badRequest;
      request.response.write(jsonEncode({
        'success': false,
        'error':
            "Failed to rename '$oldName' to '$newName' (name may already exist or network not found)",
      }));
    }
  }

  Future<void> _handleFile(HttpRequest request) async {
    if (request.method != 'GET') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    final filePath = sd_api.getDesignFilePath();
    final isDirty = sd_api.isDesignDirty();
    final networkCount = sd_api.getNetworkCount();

    request.response.headers.contentType = ContentType.json;
    request.response.write(jsonEncode({
      'success': true,
      'file_path': filePath,
      'modified': isDirty,
      'network_count': networkCount,
    }));
  }

  Future<void> _handleLoad(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    final params = request.uri.queryParameters;
    final filePath = params['path'];
    final force = params['force'] == 'true';

    request.response.headers.contentType = ContentType.json;

    if (filePath == null || filePath.isEmpty) {
      request.response.write(jsonEncode({
        'success': false,
        'error': 'Missing required parameter: path',
      }));
      return;
    }

    // Check for unsaved changes
    if (!force && sd_api.isDesignDirty()) {
      request.response.write(jsonEncode({
        'success': false,
        'error':
            "Unsaved changes exist. Use 'save <path>' to save first, or --force to discard.",
      }));
      return;
    }

    // Check if file exists
    final file = File(filePath);
    if (!await file.exists()) {
      request.response.write(jsonEncode({
        'success': false,
        'error': 'File not found: $filePath',
      }));
      return;
    }

    // Load the file
    final result = sd_api.loadNodeNetworks(filePath: filePath);

    if (result.success) {
      final networkCount = sd_api.getNetworkCount();
      onNetworkEdited?.call();
      request.response.write(jsonEncode({
        'success': true,
        'file_path': filePath,
        'network_count': networkCount,
      }));
    } else {
      request.response.write(jsonEncode({
        'success': false,
        'error': 'Failed to load file: ${result.errorMessage}',
      }));
    }
  }

  Future<void> _handleSave(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    final params = request.uri.queryParameters;
    final filePath = params['path'];

    request.response.headers.contentType = ContentType.json;

    if (filePath == null || filePath.isEmpty) {
      // Save to current file
      final currentPath = sd_api.getDesignFilePath();
      if (currentPath == null) {
        request.response.write(jsonEncode({
          'success': false,
          'error': 'No file loaded. Specify a path: atomcad-cli save <path>',
        }));
        return;
      }

      final result = sd_api.saveNodeNetworks();
      if (result.success) {
        request.response.write(jsonEncode({
          'success': true,
          'file_path': currentPath,
        }));
      } else {
        request.response.write(jsonEncode({
          'success': false,
          'error': result.errorMessage,
        }));
      }
    } else {
      // Save to specified path
      final result = sd_api.saveNodeNetworksAs(filePath: filePath);
      if (result.success) {
        request.response.write(jsonEncode({
          'success': true,
          'file_path': filePath,
        }));
      } else {
        request.response.write(jsonEncode({
          'success': false,
          'error': result.errorMessage,
        }));
      }
    }
  }

  Future<void> _handleNew(HttpRequest request) async {
    if (request.method != 'POST') {
      request.response.statusCode = HttpStatus.methodNotAllowed;
      return;
    }

    final params = request.uri.queryParameters;
    final force = params['force'] == 'true';

    request.response.headers.contentType = ContentType.json;

    // Check for unsaved changes
    if (!force && sd_api.isDesignDirty()) {
      request.response.write(jsonEncode({
        'success': false,
        'error':
            "Unsaved changes exist. Use 'save <path>' to save first, or --force to discard.",
      }));
      return;
    }

    sd_api.newProject();
    onNetworkEdited?.call();

    request.response.write(jsonEncode({
      'success': true,
    }));
  }
}
