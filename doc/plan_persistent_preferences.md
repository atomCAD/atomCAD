# Persistent User Preferences Plan

## Overview

Save `StructureDesignerPreferences` to the user's config directory so preferences persist across atomCAD sessions.

## Current State

- `StructureDesignerPreferences` is defined in `rust/src/api/structure_designer/structure_designer_preferences.rs`
- Preferences are stored in-memory in the kernel and reset to defaults on each app launch
- Users must reconfigure preferences every session

## Target State

- Preferences are saved to `<config_dir>/atomCAD/preferences.json`
- On startup, preferences are loaded from this file (if present)
- When preferences change, they are saved automatically

## Platform-Specific Config Directories

Using the `dirs` crate:
- **Windows:** `%APPDATA%\atomCAD\` (e.g., `C:\Users\<user>\AppData\Roaming\atomCAD\`)
- **macOS:** `~/Library/Application Support/atomCAD/`
- **Linux:** `~/.config/atomCAD/`

## Implementation Steps

### Phase 1: Rust Backend

1. **Add `dirs` crate to Cargo.toml**
   ```toml
   dirs = "5.0"
   ```

2. **Add Serialize/Deserialize to preference structs** in `rust/src/structure_designer/preferences.rs` (new file)
   - Move preference structs from `api/structure_designer/structure_designer_preferences.rs` to internal module
   - Add `#[derive(Serialize, Deserialize)]` to all preference structs
   - Add `#[serde(default)]` to all fields for forward compatibility
   - Implement `Default` trait for all preference structs
   - Re-export from API module for FFI compatibility
   - Add a module-level doc comment explaining the versioning strategy (tolerant reader pattern with `#[serde(default)]` on all fields)

3. **Add persistence functions** to `rust/src/structure_designer/preferences.rs`
   - `get_preferences_path() -> Option<PathBuf>` - returns path to preferences.json
   - `load_preferences() -> StructureDesignerPreferences` - loads from file or returns defaults
   - `save_preferences(prefs: &StructureDesignerPreferences) -> Result<(), Error>` - saves to file

4. **Integrate with StructureDesigner**
   - Call `load_preferences()` in `StructureDesigner::new()` to initialize with persisted preferences
   - Call `save_preferences()` in `set_structure_designer_preferences()` API function after applying preferences
   - Create the `atomCAD` config directory if it doesn't exist (use `std::fs::create_dir_all`)

### Phase 2: Flutter Integration

5. **No changes needed** - persistence is transparent to Flutter
   - Preferences load automatically during kernel initialization
   - Preferences save automatically when `set_structure_designer_preferences()` is called
   - Flutter continues using the existing API unchanged

## Versioning Strategy (Tolerant Reader Pattern)

No explicit version field. Use defensive serialization:

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct StructureDesignerPreferences {
    #[serde(default)]
    pub geometry_visualization_preferences: GeometryVisualizationPreferences,
    #[serde(default)]
    pub node_display_preferences: NodeDisplayPreferences,
    // ... etc
}
```

### Handling Changes

| Change Type | Handling |
|-------------|----------|
| Add new field | `#[serde(default)]` provides default when loading old files |
| Remove field | Extra fields in JSON are silently ignored |
| Rename field | Use `#[serde(alias = "old_name")]` for backwards compatibility |

### Error Handling

- **File doesn't exist:** Use defaults silently (first run)
- **File corrupted/invalid JSON:** Use `eprintln!` to log warning, use defaults
- **Missing fields:** `#[serde(default)]` fills in defaults
- **Extra fields:** Silently ignored (forward compatibility)
- **Save failures:** Use `eprintln!` to log warning, don't propagate error to Flutter (preferences not saving is non-critical)
- **Config directory unavailable:** Use defaults, log warning (rare edge case)

## File Format

`preferences.json` example:
```json
{
  "geometry_visualization_preferences": {
    "geometry_visualization": "ExplicitMesh",
    "wireframe_geometry": false,
    "samples_per_unit_cell": 1,
    "sharpness_angle_threshold_degree": 29.0,
    "mesh_smoothing": "SmoothingGroupBased",
    "display_camera_target": false
  },
  "node_display_preferences": {
    "display_policy": "Manual"
  },
  "atomic_structure_visualization_preferences": {
    "visualization": "BallAndStick",
    "rendering_method": "Impostors",
    "ball_and_stick_cull_depth": 8.0,
    "space_filling_cull_depth": 3.0
  },
  "background_preferences": {
    "background_color": { "x": 0, "y": 0, "z": 0 },
    "show_grid": true,
    "grid_size": 200,
    "grid_color": { "x": 90, "y": 90, "z": 90 },
    "grid_strong_color": { "x": 180, "y": 180, "z": 180 },
    "show_lattice_axes": true,
    "show_lattice_grid": false,
    "lattice_grid_color": { "x": 60, "y": 90, "z": 90 },
    "lattice_grid_strong_color": { "x": 100, "y": 150, "z": 150 },
    "drawing_plane_grid_color": { "x": 70, "y": 70, "z": 100 },
    "drawing_plane_grid_strong_color": { "x": 110, "y": 110, "z": 160 }
  },
  "layout_preferences": {
    "layout_algorithm": "Sugiyama",
    "auto_layout_after_edit": true
  }
}
```

## Testing

1. **Unit tests** in `rust/tests/preferences/`
   - Round-trip serialization test
   - Loading with missing fields (forward compatibility)
   - Loading with extra fields (backward compatibility)
   - Corrupted file handling

2. **Manual testing**
   - Change preferences, restart app, verify they persist
   - Delete preferences file, verify defaults are used
   - Test on Windows (primary platform)

## Future Considerations

- **Reset to defaults button** in preferences dialog
- **Import/export preferences** for sharing setups
- **Per-project preferences** (stored in .cnnd file) vs global preferences
