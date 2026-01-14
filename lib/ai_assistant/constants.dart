/// Constants for AI assistant integration.

/// Default port for the AI assistant HTTP server.
const int aiAssistantPort = 19847;

/// Stub response for the query endpoint (Phase 1).
const String stubQueryResponse = '''
# atomCAD Node Network (stub response)
sphere1 = sphere { center: (0, 0, 0), radius: 5.0 }
box1 = cuboid { min_corner: (-2, -2, -2), extent: (4, 4, 4) }
diff1 = diff { base: sphere1, sub: box1 }
output diff1
''';
