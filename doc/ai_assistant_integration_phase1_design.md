# AI Assistant Integration - Phase 1 Technical Design

## Objective

Implement the infrastructure for AI assistant integration with **fake/stub implementations**:
- `query` returns a hardcoded example text
- `edit` accepts input but performs no actual modifications

This establishes the full communication stack (CLI → HTTP → Flutter) before adding real node network logic.

## Components

### 1. Flutter HTTP Server

**Location:** `lib/ai_assistant/http_server.dart`

```
┌─────────────────────────────────────────┐
│          AiAssistantServer              │
├─────────────────────────────────────────┤
│ - _server: HttpServer?                  │
│ - port: int (default 19847)             │
├─────────────────────────────────────────┤
│ + start() → Future<void>                │
│ + stop() → Future<void>                 │
│ - _handleRequest(HttpRequest)           │
│ - _handleQuery() → String               │
│ - _handleEdit(String code, bool replace)│
└─────────────────────────────────────────┘
```

**Endpoints:**
| Method | Path | Description |
|--------|------|-------------|
| GET | `/query` | Returns stub text |
| POST | `/edit` | Accepts body, returns success JSON |
| POST | `/edit?replace=true` | Same, with replace flag |
| GET | `/health` | Returns `{"status": "ok"}` for CLI connection check |

**Stub Responses:**

```dart
// GET /query response
String _handleQuery() {
  return '''
# atomCAD Node Network (stub response)
sphere1 = sphere { center: (0, 0, 0), radius: 5.0 }
box1 = cuboid { min_corner: (-2, -2, -2), extent: (4, 4, 4) }
diff1 = diff { base: sphere1, sub: box1 }
output diff1
''';
}

// POST /edit response
Map<String, dynamic> _handleEdit(String code, bool replace) {
  // Phase 1: ignore code, return success
  return {
    'success': true,
    'message': 'Edit received (stub - no changes applied)',
    'replace': replace,
    'code_length': code.length,
  };
}
```

### 2. Server Lifecycle Integration

**Location:** Modify `lib/main.dart`

The HTTP server starts when atomCAD launches in GUI mode:

```dart
void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Start AI assistant HTTP server
  final aiServer = AiAssistantServer();
  await aiServer.start();

  runApp(const MyApp());
}
```

**Considerations:**
- Server runs on separate isolate or uses `dart:io` HttpServer (single-threaded is fine for local CLI)
- Graceful shutdown on app exit
- Log server start/stop for debugging

### 3. CLI Tool

**Location:** `bin/atomcad_cli.dart` (Dart CLI, bundled with Flutter project)

```
┌─────────────────────────────────────────┐
│              atomcad-cli                │
├─────────────────────────────────────────┤
│ Commands:                               │
│   query              GET /query         │
│   edit --code=...    POST /edit         │
│   edit --replace     POST /edit?replace │
└─────────────────────────────────────────┘
```

**Implementation:**

```dart
// bin/atomcad_cli.dart
import 'dart:io';
import 'package:args/args.dart';
import 'package:http/http.dart' as http;

const defaultPort = 19847;
const baseUrl = 'http://localhost:$defaultPort';

Future<void> main(List<String> args) async {
  final parser = ArgParser()
    ..addCommand('query')
    ..addCommand('edit');

  final editParser = ArgParser()
    ..addOption('code', mandatory: true)
    ..addFlag('replace', defaultsTo: false);

  // ... parse and dispatch
}
```

**CLI Behavior:**
1. Check if atomCAD is running (`GET /health`)
2. If not running, print error: "atomCAD is not running. Please start atomCAD first."
3. Execute command and print result to stdout
4. Exit with code 0 on success, non-zero on error

### 4. File Structure

```
lib/
├── ai_assistant/
│   ├── http_server.dart      # HTTP server implementation
│   └── constants.dart        # Port, stub responses
├── main.dart                 # Modified to start server

bin/
└── atomcad_cli.dart          # CLI entry point

pubspec.yaml                  # Add dependencies: args, http
```

### 5. Dependencies

Add to `pubspec.yaml`:

```yaml
dependencies:
  # ... existing

dev_dependencies:
  # ... existing
  args: ^2.4.0      # CLI argument parsing
  http: ^1.1.0      # HTTP client for CLI
```

Note: `dart:io` HttpServer is built-in, no additional server dependency needed.

## Implementation Steps

### Step 1: Create HTTP Server (lib/ai_assistant/)

1. Create `lib/ai_assistant/constants.dart` with port and stub text
2. Create `lib/ai_assistant/http_server.dart` with `AiAssistantServer` class
3. Implement endpoints: `/health`, `/query`, `/edit`
4. Add error handling and JSON responses

### Step 2: Integrate Server into App Startup

1. Modify `lib/main.dart` to instantiate and start server
2. Add shutdown handling
3. Test: run app, verify `curl http://localhost:19847/health` returns OK

### Step 3: Create CLI Tool

1. Add dependencies to `pubspec.yaml`
2. Create `bin/atomcad_cli.dart`
3. Implement `query` command (GET /query, print response)
4. Implement `edit` command (POST /edit with --code body)
5. Add connection error handling

### Step 4: End-to-End Testing

1. Start atomCAD
2. Run `dart run bin/atomcad_cli.dart query` → should print stub text
3. Run `dart run bin/atomcad_cli.dart edit --code="test"` → should print success JSON
4. Test with `--replace` flag
5. Test error case: run CLI without atomCAD running

## Testing Plan

| Test | Expected Result |
|------|-----------------|
| `GET /health` | `{"status": "ok"}` |
| `GET /query` | Stub text with example nodes |
| `POST /edit` body="anything" | `{"success": true, ...}` |
| `POST /edit?replace=true` | Same, with `"replace": true` |
| CLI without atomCAD running | Error message, exit code 1 |
| Invalid endpoint | 404 response |

## Future Work (Not in Phase 1)

- Real `query`: serialize node network to text format
- Real `edit`: parse text, apply to node network, notify UI
- Port configurability
- MCP server wrapper for Claude Code native integration

## Notes

- Single port binding: if port 19847 is in use, server fails to start (acceptable for phase 1)
- No authentication for localhost (security acceptable for local development tool)
- CLI is a Dart script; can be compiled to native binary later with `dart compile exe`
