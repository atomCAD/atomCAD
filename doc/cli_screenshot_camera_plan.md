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
- `renderer.render()` returns `Vec<u8>` RGBA pixel data - line 680
- Viewport size control via `set_viewport_size()`

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
