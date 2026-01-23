# CLI Screenshot and Camera Control Plan

## Overview

Add `screenshot` and `camera` commands to atomcad-cli that enable AI agents to capture viewport images and control the camera position. This provides visual feedback that is often more informative than textual descriptions.

## Why This Is Useful

Visual feedback has high information density:
- A screenshot instantly shows geometry shape, scale, and orientation
- Errors like misaligned parts or missing atoms are immediately visible
- One image (~1500 tokens) conveys more than verbose text output for complex structures

Without visual feedback, an AI agent building atomic structures is essentially working blind - it can only verify atom counts and primitive values, not the actual 3D result.

## CLI Commands

### Camera Control

```bash
# Set camera position and orientation
atomcad-cli camera --eye <x,y,z> --target <x,y,z> --up <x,y,z>

# Set projection mode
atomcad-cli camera --orthographic
atomcad-cli camera --perspective

# Set orthographic zoom (only applies in orthographic mode)
atomcad-cli camera --ortho-height <value>

# Combined example
atomcad-cli camera --eye 20,20,20 --target 0,0,0 --up 0,0,1 --orthographic
```

Parameters:
- `--eye`: Camera position in world coordinates
- `--target`: Point the camera looks at
- `--up`: Up vector for camera orientation
- `--orthographic` / `--perspective`: Projection mode toggle
- `--ortho-height`: Half-height of orthographic viewport (controls zoom)

### Screenshot

```bash
# Capture current viewport to file
atomcad-cli screenshot --output <path.png>

# Capture with specific resolution
atomcad-cli screenshot --output <path.png> --width 800 --height 600
```

Parameters:
- `--output`: Output file path (PNG format)
- `--width`, `--height`: Optional resolution override (defaults to current viewport size)

### Typical Agent Workflow

```bash
# 1. Build geometry
atomcad-cli edit --code="sphere1 = sphere { radius: 10, visible: true }"

# 2. Position camera
atomcad-cli camera --eye 30,30,30 --target 0,0,0 --up 0,0,1 --orthographic

# 3. Capture screenshot for verification
atomcad-cli screenshot --output sphere_check.png
```

## Implementation Notes

### Existing Infrastructure

Camera control APIs already exist in [common_api.rs](rust/src/api/common_api.rs):
- `move_camera(eye, target, up)` - line 201
- `set_orthographic_mode(bool)` - line 348
- `set_ortho_half_height(half_height)` - line 370
- `get_camera()` - line 179

Rendering infrastructure exists in [renderer.rs](rust/src/renderer/renderer.rs):
- `renderer.render()` returns `Vec<u8>` **BGRA** pixel data - line 680 (note: requires BGRA→RGBA conversion for PNG)
- Viewport size control via `set_viewport_size()` - line 609
- 256-byte row alignment handled internally - lines 790-835

### What Needs to Be Built

1. **Camera command**: Wire CLI arguments to existing camera APIs via server commands

2. **Screenshot command**:
   - Add PNG encoding capability (use `image` crate)
   - Create server command that calls `renderer.render()`, encodes to PNG, saves to file
   - Handle viewport size override if specified

3. **Server-side handling**: Both commands require atomCAD GUI to be running since rendering uses the GPU context owned by the Flutter application. The CLI sends commands via TCP to the running instance.

### Dependencies

- `image` crate for PNG encoding (add to Cargo.toml)

### Architectural Note

This approach requires atomCAD to be running. True headless rendering (without GUI) would require creating a standalone wgpu context, which is significantly more complex. The GUI-based approach provides immediate value with lower implementation cost.

---

## Detailed Architecture

### Data Flow Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           CLI (Dart)                                     │
│  atomcad-cli camera --eye 10,10,10 ...                                  │
│  atomcad-cli screenshot --output foo.png --width 800 --height 600       │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │ HTTP GET/POST
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                     HTTP Server (Dart)                                   │
│  lib/ai_assistant/http_server.dart                                      │
│  Endpoints: /camera, /screenshot                                        │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │ FFI (flutter_rust_bridge)
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        Rust API Layer                                    │
│  rust/src/api/common_api.rs       - Camera APIs (existing)              │
│  rust/src/api/screenshot_api.rs   - Screenshot API (new)                │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          Renderer                                        │
│  rust/src/renderer/renderer.rs                                          │
│  - set_viewport_size(width, height)                                     │
│  - render() → Vec<u8> BGRA pixels                                       │
│  - Camera state management                                              │
└─────────────────────────────────────────────────────────────────────────┘
```

### Screenshot Command Architecture

**Key Design Decision**: The screenshot is saved directly in Rust to avoid transferring large pixel data back through FFI to Flutter. The CLI only sends the output path and optional resolution.

```
CLI: atomcad-cli screenshot --output /path/to/image.png --width 800 --height 600
  │
  ▼ HTTP GET /screenshot?output=/path/to/image.png&width=800&height=600
  │
Server: _handleScreenshot()
  │
  ▼ FFI call: screenshot_api.captureScreenshot(path, width, height)
  │
Rust: capture_screenshot(output_path, width, height)
  │
  ├─ 1. Save current viewport size
  ├─ 2. If custom resolution: call set_viewport_size(width, height)
  ├─ 3. Call renderer.render() → Vec<u8> BGRA
  ├─ 4. Convert BGRA → RGBA (wgpu outputs BGRA8Unorm)
  ├─ 5. Encode as PNG using `image` crate
  ├─ 6. Write PNG bytes to output_path
  ├─ 7. Restore original viewport size if changed
  └─ 8. Return success/error result
```

### Camera Command Architecture

Camera APIs already exist in `common_api.rs`. The implementation requires only wiring through the HTTP server and CLI.

```
CLI: atomcad-cli camera --eye 10,10,10 --target 0,0,0 --up 0,0,1 --orthographic
  │
  ▼ HTTP GET /camera?eye=10,10,10&target=0,0,0&up=0,0,1&orthographic=true
  │
Server: _handleCamera()
  │
  ▼ FFI calls (as needed):
  │   common_api.move_camera(eye, target, up)
  │   common_api.set_orthographic_mode(true)
  │   common_api.set_ortho_half_height(value)
  │
Rust: Updates camera state, GPU buffer refresh automatic
```

### Critical Technical Details

#### 1. Pixel Format: BGRA vs RGBA

The renderer outputs **BGRA8Unorm** format (line 963 in renderer.rs), not RGBA:
```rust
TextureFormat::Bgra8Unorm
```

When encoding to PNG (which expects RGBA), bytes must be swapped:
```rust
// BGRA → RGBA conversion
for chunk in pixels.chunks_exact_mut(4) {
    chunk.swap(0, 2); // Swap B and R
}
```

#### 2. Viewport Size Management

The `set_viewport_size()` method recreates GPU textures/buffers when size changes. For screenshot with custom resolution:

1. Save original: `let (orig_w, orig_h) = renderer.get_viewport_size()`
2. Set custom: `renderer.set_viewport_size(width, height)`
3. Render: `let pixels = renderer.render(bg_color)`
4. Restore: `renderer.set_viewport_size(orig_w, orig_h)`

This ensures the GUI viewport isn't permanently affected by screenshot resolution overrides.

#### 3. Thread Safety

The renderer has a `render_mutex: Mutex<()>` that serializes render calls. The screenshot API will safely serialize with the normal frame rendering.

#### 4. 256-byte Row Alignment

The renderer already handles WebGPU's 256-byte row alignment requirement internally (lines 790-835 in renderer.rs). The `Vec<u8>` returned from `render()` has clean, unpadded pixel data.

### File Locations Summary

| Component | File | Changes |
|-----------|------|---------|
| Rust screenshot API | `rust/src/api/screenshot_api.rs` | **New file** |
| Rust API module | `rust/src/api/mod.rs` | Add `pub mod screenshot_api;` |
| Rust dependencies | `rust/Cargo.toml` | Add `image = "0.25"` |
| HTTP server | `lib/ai_assistant/http_server.dart` | Add `/camera`, `/screenshot` handlers |
| CLI | `bin/atomcad_cli.dart` | Add `camera`, `screenshot` commands |
| FFI regeneration | N/A | Run `flutter_rust_bridge_codegen generate` |

---

## Phased Implementation Plan

### Phase 1: Camera Command (Simplest - existing APIs)

**Goal**: Wire existing camera APIs through HTTP server to CLI.

**Files to modify**:
- `lib/ai_assistant/http_server.dart`
- `bin/atomcad_cli.dart`

**Steps**:

1. **Add `/camera` endpoint to HTTP server** (`http_server.dart`):
   ```dart
   import 'package:flutter_cad/src/rust/api/common_api.dart' as common_api;

   // In _handleRequest switch:
   case '/camera':
     await _handleCamera(request);
     break;

   Future<void> _handleCamera(HttpRequest request) async {
     if (request.method != 'GET') {
       request.response.statusCode = HttpStatus.methodNotAllowed;
       return;
     }

     final params = request.uri.queryParameters;

     // Parse eye, target, up as "x,y,z" strings
     if (params.containsKey('eye') &&
         params.containsKey('target') &&
         params.containsKey('up')) {
       final eye = _parseVec3(params['eye']!);
       final target = _parseVec3(params['target']!);
       final up = _parseVec3(params['up']!);
       common_api.moveCamera(eye: eye, target: target, up: up);
     }

     // Handle projection mode
     if (params.containsKey('orthographic')) {
       common_api.setOrthographicMode(orthographic: true);
     } else if (params.containsKey('perspective')) {
       common_api.setOrthographicMode(orthographic: false);
     }

     // Handle ortho height
     if (params.containsKey('ortho_height')) {
       final height = double.parse(params['ortho_height']!);
       common_api.setOrthoHalfHeight(halfHeight: height);
     }

     // Return current camera state
     final camera = common_api.getCamera();
     request.response.headers.contentType = ContentType.json;
     request.response.write(jsonEncode({
       'success': true,
       'camera': {
         'eye': [camera?.eye.x, camera?.eye.y, camera?.eye.z],
         'target': [camera?.target.x, camera?.target.y, camera?.target.z],
         'up': [camera?.up.x, camera?.up.y, camera?.up.z],
         'orthographic': camera?.orthographic,
         'ortho_half_height': camera?.orthoHalfHeight,
       }
     }));
   }

   APIVec3 _parseVec3(String s) {
     final parts = s.split(',').map((p) => double.parse(p.trim())).toList();
     return APIVec3(x: parts[0], y: parts[1], z: parts[2]);
   }
   ```

2. **Add `camera` command to CLI** (`atomcad_cli.dart`):
   ```dart
   // In argument parser setup:
   final cameraParser = ArgParser()
     ..addOption('eye', help: 'Camera position as x,y,z')
     ..addOption('target', help: 'Look-at point as x,y,z')
     ..addOption('up', help: 'Up vector as x,y,z')
     ..addFlag('orthographic', negatable: false)
     ..addFlag('perspective', negatable: false)
     ..addOption('ortho-height', help: 'Orthographic half-height (zoom)');

   // Command handler:
   Future<void> _runCamera(String serverUrl, ArgResults args) async {
     final queryParams = <String, String>{};

     if (args['eye'] != null) queryParams['eye'] = args['eye'];
     if (args['target'] != null) queryParams['target'] = args['target'];
     if (args['up'] != null) queryParams['up'] = args['up'];
     if (args['orthographic']) queryParams['orthographic'] = 'true';
     if (args['perspective']) queryParams['perspective'] = 'true';
     if (args['ortho-height'] != null) queryParams['ortho_height'] = args['ortho-height'];

     final uri = Uri.parse('$serverUrl/camera').replace(queryParameters: queryParams);
     final response = await http.get(uri);

     if (response.statusCode == 200) {
       print(response.body);
     } else {
       stderr.writeln('Error: ${response.body}');
       exit(1);
     }
   }
   ```

3. **Test**:
   ```bash
   atomcad-cli camera --eye 30,30,30 --target 0,0,0 --up 0,0,1 --orthographic
   ```

**Estimated complexity**: Low - only wiring existing APIs.

---

### Phase 2: Screenshot Infrastructure (Rust side)

**Goal**: Add PNG encoding and file saving capability in Rust.

**Files to modify/create**:
- `rust/Cargo.toml`
- `rust/src/api/screenshot_api.rs` (new)
- `rust/src/api/mod.rs`

**Steps**:

1. **Add `image` crate to Cargo.toml**:
   ```toml
   [dependencies]
   # ... existing deps ...
   image = { version = "0.25", default-features = false, features = ["png"] }
   ```

   Note: Only enable `png` feature to minimize compile time and binary size.

2. **Create `rust/src/api/screenshot_api.rs`**:
   ```rust
   //! Screenshot capture API for CLI/AI agent use.
   //!
   //! Provides functionality to capture the current viewport to a PNG file.

   use crate::api::api_common::{with_mut_cad_instance_or, CADInstance};
   use image::{ImageBuffer, Rgba};
   use std::path::Path;

   /// Result of a screenshot capture operation
   #[derive(Debug, Clone)]
   pub struct ScreenshotResult {
       pub success: bool,
       pub output_path: String,
       pub width: u32,
       pub height: u32,
       pub error_message: Option<String>,
   }

   /// Capture the current viewport to a PNG file.
   ///
   /// # Arguments
   /// * `output_path` - Path where the PNG file will be written
   /// * `width` - Optional width override (uses current viewport if None)
   /// * `height` - Optional height override (uses current viewport if None)
   /// * `background_rgb` - Background color as [R, G, B] (0-255)
   ///
   /// # Returns
   /// `ScreenshotResult` indicating success/failure and metadata
   #[flutter_rust_bridge::frb(sync)]
   pub fn capture_screenshot(
       output_path: String,
       width: Option<u32>,
       height: Option<u32>,
       background_rgb: Option<Vec<u8>>,
   ) -> ScreenshotResult {
       unsafe {
           with_mut_cad_instance_or(
               |cad_instance| {
                   capture_screenshot_impl(cad_instance, &output_path, width, height, background_rgb)
               },
               ScreenshotResult {
                   success: false,
                   output_path: output_path.clone(),
                   width: 0,
                   height: 0,
                   error_message: Some("CAD instance not initialized".to_string()),
               },
           )
       }
   }

   fn capture_screenshot_impl(
       cad_instance: &mut CADInstance,
       output_path: &str,
       width: Option<u32>,
       height: Option<u32>,
       background_rgb: Option<Vec<u8>>,
   ) -> ScreenshotResult {
       let renderer = &mut cad_instance.renderer;

       // Save original viewport size
       let orig_size = renderer.get_viewport_size();
       let (orig_width, orig_height) = (orig_size.0, orig_size.1);

       // Determine target size
       let target_width = width.unwrap_or(orig_width);
       let target_height = height.unwrap_or(orig_height);

       // Set viewport size if different
       let size_changed = target_width != orig_width || target_height != orig_height;
       if size_changed {
           renderer.set_viewport_size(target_width, target_height);
       }

       // Render
       let bg_color = background_rgb
           .map(|v| [v[0], v[1], v[2]])
           .unwrap_or([30, 30, 30]); // Default dark gray
       let mut pixels = renderer.render(bg_color);

       // Restore viewport if changed
       if size_changed {
           renderer.set_viewport_size(orig_width, orig_height);
       }

       // Convert BGRA → RGBA (wgpu uses BGRA8Unorm)
       for chunk in pixels.chunks_exact_mut(4) {
           chunk.swap(0, 2); // Swap B and R
       }

       // Create image buffer and save as PNG
       let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
           match ImageBuffer::from_raw(target_width, target_height, pixels) {
               Some(img) => img,
               None => {
                   return ScreenshotResult {
                       success: false,
                       output_path: output_path.to_string(),
                       width: target_width,
                       height: target_height,
                       error_message: Some("Failed to create image buffer".to_string()),
                   };
               }
           };

       // Save to file
       let path = Path::new(output_path);
       if let Err(e) = img.save(path) {
           return ScreenshotResult {
               success: false,
               output_path: output_path.to_string(),
               width: target_width,
               height: target_height,
               error_message: Some(format!("Failed to save PNG: {}", e)),
           };
       }

       ScreenshotResult {
           success: true,
           output_path: output_path.to_string(),
           width: target_width,
           height: target_height,
           error_message: None,
       }
   }
   ```

3. **Add getter for viewport size in renderer** (`renderer.rs`):
   ```rust
   pub fn get_viewport_size(&self) -> (u32, u32) {
       (self.texture_size.width, self.texture_size.height)
   }
   ```

4. **Register module in `rust/src/api/mod.rs`**:
   ```rust
   pub mod screenshot_api;
   ```

5. **Regenerate FFI bindings**:
   ```bash
   flutter_rust_bridge_codegen generate
   ```

6. **Test Rust side** (add integration test):
   ```rust
   // rust/tests/screenshot_test.rs
   #[test]
   fn test_bgra_to_rgba_conversion() {
       let mut pixels = vec![0u8, 128u8, 255u8, 255u8]; // BGRA: Blue=0, Green=128, Red=255
       for chunk in pixels.chunks_exact_mut(4) {
           chunk.swap(0, 2);
       }
       assert_eq!(pixels, vec![255u8, 128u8, 0u8, 255u8]); // RGBA: Red=255, Green=128, Blue=0
   }
   ```

**Estimated complexity**: Medium - new module, external crate integration.

---

### Phase 3: Screenshot CLI Integration

**Goal**: Wire screenshot API through HTTP server to CLI.

**Files to modify**:
- `lib/ai_assistant/http_server.dart`
- `bin/atomcad_cli.dart`

**Steps**:

1. **Add `/screenshot` endpoint to HTTP server** (`http_server.dart`):
   ```dart
   import 'package:flutter_cad/src/rust/api/screenshot_api.dart' as screenshot_api;

   // In _handleRequest switch:
   case '/screenshot':
     await _handleScreenshot(request);
     break;

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
     final width = params['width'] != null ? int.tryParse(params['width']!) : null;
     final height = params['height'] != null ? int.tryParse(params['height']!) : null;

     // Optional: background color
     List<int>? bgColor;
     if (params['background'] != null) {
       bgColor = params['background']!.split(',').map((s) => int.parse(s.trim())).toList();
     }

     // Call Rust API
     final result = screenshot_api.captureScreenshot(
       outputPath: outputPath,
       width: width,
       height: height,
       backgroundRgb: bgColor != null ? Uint8List.fromList(bgColor) : null,
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
   ```

2. **Add `screenshot` command to CLI** (`atomcad_cli.dart`):
   ```dart
   // In argument parser setup:
   final screenshotParser = ArgParser()
     ..addOption('output', abbr: 'o', help: 'Output PNG file path', mandatory: true)
     ..addOption('width', abbr: 'w', help: 'Image width in pixels')
     ..addOption('height', abbr: 'h', help: 'Image height in pixels')
     ..addOption('background', help: 'Background color as R,G,B (0-255)');

   // Command handler:
   Future<void> _runScreenshot(String serverUrl, ArgResults args) async {
     final queryParams = <String, String>{
       'output': args['output'],
     };

     if (args['width'] != null) queryParams['width'] = args['width'];
     if (args['height'] != null) queryParams['height'] = args['height'];
     if (args['background'] != null) queryParams['background'] = args['background'];

     final uri = Uri.parse('$serverUrl/screenshot').replace(queryParameters: queryParams);
     final response = await http.get(uri);

     final result = jsonDecode(response.body);
     if (result['success'] == true) {
       print('Screenshot saved: ${result['output_path']} (${result['width']}x${result['height']})');
     } else {
       stderr.writeln('Error: ${result['error']}');
       exit(1);
     }
   }
   ```

3. **Update server documentation** (`http_server.dart` class doc):
   Add to the endpoint list:
   ```
   /// - `GET /camera?eye=x,y,z&target=x,y,z&up=x,y,z&orthographic=true` - Control camera
   /// - `GET /screenshot?output=<path>&width=<w>&height=<h>` - Capture viewport to PNG
   ```

4. **Test end-to-end**:
   ```bash
   # Basic screenshot
   atomcad-cli screenshot --output test.png

   # With resolution
   atomcad-cli screenshot --output hires.png --width 1920 --height 1080

   # Full workflow
   atomcad-cli edit --code="sphere1 = sphere { radius: 10 }"
   atomcad-cli camera --eye 30,30,30 --target 0,0,0 --up 0,0,1 --orthographic
   atomcad-cli screenshot --output sphere.png
   ```

**Estimated complexity**: Low - straightforward wiring.

---

### Phase 4: Polish and Documentation

**Goal**: Error handling, edge cases, documentation.

**Tasks**:

1. **Error handling improvements**:
   - Validate output path is writable
   - Handle case where atomCAD is not running gracefully
   - Validate resolution limits (e.g., max 4096x4096)

2. **Add convenience features**:
   - `atomcad-cli camera --get` - Just return current camera state without modifying
   - `atomcad-cli screenshot --output auto` - Auto-generate timestamped filename

3. **Update skill definition** for AI agents (`atomcad` skill):
   Add camera and screenshot documentation to the skill.

4. **Integration test**:
   Create test that builds geometry, positions camera, takes screenshot, and verifies PNG exists.

5. **Documentation**:
   - Update this plan document with actual implementation details
   - Add examples to CLI help text
   - Document in AI agent skill

---

## Testing Checklist

### Camera Command
- [ ] `--eye`, `--target`, `--up` correctly parsed and applied
- [ ] `--orthographic` and `--perspective` toggle projection mode
- [ ] `--ortho-height` controls zoom in orthographic mode
- [ ] Combined parameters work in single command
- [ ] Returns current camera state after modification
- [ ] Error when atomCAD not running

### Screenshot Command
- [ ] Basic screenshot to valid path works
- [ ] Custom width/height produces correct resolution
- [ ] Original viewport restored after custom resolution screenshot
- [ ] BGRA→RGBA conversion correct (colors render properly)
- [ ] Invalid path returns clear error
- [ ] Error when atomCAD not running
- [ ] Large resolution (e.g., 4096x4096) works without crash

### Integration
- [ ] Full workflow: edit → camera → screenshot produces valid image
- [ ] Multiple screenshots in sequence work correctly
- [ ] Screenshots while geometry is animating/updating work
