# Plan: Per-Network Camera Settings

## Overview

Save camera view settings (position, orientation, zoom) per node network so that switching between networks restores the user's preferred view for each network.

## Fields to Save Per Network

| Field | Type | Default Value | Description |
|-------|------|---------------|-------------|
| `eye` | `DVec3` | `(0.0, -30.0, 10.0)` | Camera position |
| `target` | `DVec3` | `(0.0, 0.0, 0.0)` | Look-at point |
| `up` | `DVec3` | `(0.0, 0.32, 0.95)` | Up direction |
| `orthographic` | `bool` | `false` | Projection mode |
| `ortho_half_height` | `f64` | `10.0` | Zoom level (orthographic) |
| `pivot_point` | `DVec3` | `(0.0, 0.0, 0.0)` | Rotation center |

**Not saved per network** (remain global/transient):
- `aspect` - computed from viewport size at runtime
- `fovy`, `znear`, `zfar` - constants (may become user preferences later)

## Backward Compatibility Strategy

Per `doc/cnnd_versioning.md`, we use `#[serde(default)]` to make the new field optional. Old `.cnnd` files without `camera_settings` will load successfully and use default camera values.

No version bump required since this is an additive change with defaults.

## Implementation Phases

This implementation is divided into **2 phases** that can be tested independently:

### Phase 1: Data Model + Serialization (Steps 1-5)
- Create `CameraSettings` struct
- Add to `NodeNetwork`
- Add `SerializableCameraSettings` and update serialization
- Update test helpers
- **Verification:** `cargo test` passes, existing roundtrip tests still work

### Phase 2: Runtime Integration (Steps 6-9)
- Add `sync_camera_to_active_network()` helper
- Call from camera-modifying functions
- Load camera on network switch
- **Verification:** Manual testing in the app - camera persists per network

---

## Implementation Steps

### Step 1: Create `SerializableCameraSettings` struct

**File:** `rust/src/structure_designer/serialization/node_networks_serialization.rs`

```rust
use crate::util::serialization_utils::dvec3_serializer;

/// Camera settings that are saved per node network
#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableCameraSettings {
    #[serde(with = "dvec3_serializer")]
    pub eye: DVec3,
    #[serde(with = "dvec3_serializer")]
    pub target: DVec3,
    #[serde(with = "dvec3_serializer")]
    pub up: DVec3,
    pub orthographic: bool,
    pub ortho_half_height: f64,
    #[serde(with = "dvec3_serializer")]
    pub pivot_point: DVec3,
}

impl Default for SerializableCameraSettings {
    fn default() -> Self {
        Self {
            eye: DVec3::new(0.0, -30.0, 10.0),
            target: DVec3::new(0.0, 0.0, 0.0),
            up: DVec3::new(0.0, 0.32, 0.95),
            orthographic: false,
            ortho_half_height: 10.0,
            pivot_point: DVec3::new(0.0, 0.0, 0.0),
        }
    }
}
```

### Step 2: Add camera_settings to `SerializableNodeNetwork`

**File:** `rust/src/structure_designer/serialization/node_networks_serialization.rs`

```rust
#[derive(Serialize, Deserialize)]
pub struct SerializableNodeNetwork {
    pub next_node_id: u64,
    pub node_type: SerializableNodeType,
    pub nodes: Vec<SerializableNode>,
    pub return_node_id: Option<u64>,
    pub displayed_node_ids: Vec<(u64, NodeDisplayType)>,

    // NEW: Optional camera settings (backward compatible)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_settings: Option<SerializableCameraSettings>,
}
```

### Step 3: Add `camera_settings` to runtime `NodeNetwork`

**File:** `rust/src/structure_designer/node_network.rs`

```rust
pub struct NodeNetwork {
    // ... existing fields ...

    /// Camera settings for this network's 3D viewport
    /// When None, uses default camera position
    pub camera_settings: Option<CameraSettings>,
}
```

Create a runtime `CameraSettings` struct (similar to serializable but without serde attributes):

**File:** `rust/src/structure_designer/camera_settings.rs` (new file)

```rust
use glam::f64::DVec3;

/// Camera settings stored per node network
#[derive(Clone, Debug, PartialEq)]
pub struct CameraSettings {
    pub eye: DVec3,
    pub target: DVec3,
    pub up: DVec3,
    pub orthographic: bool,
    pub ortho_half_height: f64,
    pub pivot_point: DVec3,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            eye: DVec3::new(0.0, -30.0, 10.0),
            target: DVec3::new(0.0, 0.0, 0.0),
            up: DVec3::new(0.0, 0.32, 0.95),
            orthographic: false,
            ortho_half_height: 10.0,
            pivot_point: DVec3::new(0.0, 0.0, 0.0),
        }
    }
}
```

### Step 4: Update serialization/deserialization functions

**File:** `rust/src/structure_designer/serialization/node_networks_serialization.rs`

Update `node_network_to_serializable()`:
```rust
pub fn node_network_to_serializable(...) -> io::Result<SerializableNodeNetwork> {
    // ... existing code ...

    // Convert camera settings if present
    let camera_settings = network.camera_settings.as_ref().map(|cs| {
        SerializableCameraSettings {
            eye: cs.eye,
            target: cs.target,
            up: cs.up,
            orthographic: cs.orthographic,
            ortho_half_height: cs.ortho_half_height,
            pivot_point: cs.pivot_point,
        }
    });

    Ok(SerializableNodeNetwork {
        // ... existing fields ...
        camera_settings,
    })
}
```

Update `serializable_to_node_network()`:
```rust
pub fn serializable_to_node_network(...) -> io::Result<NodeNetwork> {
    // ... existing code ...

    // Convert camera settings if present
    network.camera_settings = serializable.camera_settings.as_ref().map(|scs| {
        CameraSettings {
            eye: scs.eye,
            target: scs.target,
            up: scs.up,
            orthographic: scs.orthographic,
            ortho_half_height: scs.ortho_half_height,
            pivot_point: scs.pivot_point,
        }
    });

    Ok(network)
}
```

### Step 5: Update `NodeNetwork::new()` to initialize camera_settings

**File:** `rust/src/structure_designer/node_network.rs`

```rust
impl NodeNetwork {
    pub fn new(node_type: NodeType) -> Self {
        Self {
            // ... existing fields ...
            camera_settings: None, // Will be populated on first use or from saved file
        }
    }
}
```

### Step 6: Add helper function to sync camera to active network

**File:** `rust/src/api/api_common.rs`

Create a helper function that saves current camera state to the active network. This will be called from every camera-modifying function.

```rust
/// Syncs the current camera state to the active node network's camera_settings.
/// Call this after any camera modification to keep the network's settings up-to-date.
pub fn sync_camera_to_active_network(cad_instance: &mut CADInstance) {
    let camera = &cad_instance.renderer.camera;
    if let Some(network) = cad_instance.structure_designer.get_active_node_network_mut() {
        network.camera_settings = Some(CameraSettings {
            eye: camera.eye,
            target: camera.target,
            up: camera.up,
            orthographic: camera.orthographic,
            ortho_half_height: camera.ortho_half_height,
            pivot_point: camera.pivot_point,
        });
    }
}
```

### Step 7: Call sync helper from all camera-modifying functions

**File:** `rust/src/api/common_api.rs`

Add a call to `sync_camera_to_active_network()` at the end of each camera-modifying function:

- `move_camera()` - after updating eye/target/up
- `set_camera_transform()` - after setting transform
- `set_orthographic_mode()` - after toggling projection mode
- `set_ortho_half_height()` - after changing zoom level
- `set_camera_canonical_view()` - after setting canonical view
- `adjust_camera_target()` - after updating pivot point

Example for `move_camera()`:
```rust
#[flutter_rust_bridge::frb(sync)]
pub fn move_camera(eye: APIVec3, target: APIVec3, up: APIVec3) {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.renderer.move_camera(&from_api_vec3(&eye), &from_api_vec3(&target), &from_api_vec3(&up));
            sync_camera_to_active_network(cad_instance);  // NEW
        });
    }
}
```

### Step 8: Return camera settings from StructureDesigner methods

**Design principle:** StructureDesigner doesn't have access to Renderer (they're siblings in CADInstance). Instead of duplicating camera-loading logic in the API layer, StructureDesigner methods return the camera settings that should be applied, and the API layer applies them.

#### 8a. Modify `set_active_node_network_name()` to return camera settings

**File:** `rust/src/structure_designer/structure_designer.rs`

```rust
/// Sets the active node network and returns the camera settings to apply (if any).
/// The caller is responsible for applying the returned settings to the renderer.
pub fn set_active_node_network_name(&mut self, name: Option<String>) -> Option<CameraSettings> {
    self.active_node_network_name = name;
    // Return camera settings from the newly active network
    self.get_active_node_network()
        .and_then(|n| n.camera_settings.clone())
}
```

#### 8b. Modify `load_node_networks()` to return camera settings

**File:** `rust/src/structure_designer/structure_designer.rs`

```rust
/// Loads node networks from file and returns the camera settings of the first network (if any).
pub fn load_node_networks(&mut self, file_path: &str) -> std::io::Result<Option<CameraSettings>> {
    // ... existing loading code ...

    // At the end, return camera settings from the active network
    Ok(self.get_active_node_network()
        .and_then(|n| n.camera_settings.clone()))
}
```

#### 8c. Add helper to apply camera settings in API layer

**File:** `rust/src/api/api_common.rs`

```rust
/// Applies camera settings to the renderer (if Some).
/// Call this after any StructureDesigner method that returns Option<CameraSettings>.
pub fn apply_camera_settings(renderer: &mut Renderer, settings: Option<&CameraSettings>) {
    if let Some(s) = settings {
        renderer.camera.eye = s.eye;
        renderer.camera.target = s.target;
        renderer.camera.up = s.up;
        renderer.camera.orthographic = s.orthographic;
        renderer.camera.ortho_half_height = s.ortho_half_height;
        renderer.camera.pivot_point = s.pivot_point;
        renderer.update_camera_buffer();
    }
}
```

#### 8d. Update API functions to use the helper

**File:** `rust/src/api/structure_designer/structure_designer_api.rs`

```rust
pub fn set_active_node_network(node_network_name: &str) {
    unsafe {
        with_mut_cad_instance(|instance| {
            let camera_settings = instance.structure_designer
                .set_active_node_network_name(Some(node_network_name.to_string()));
            apply_camera_settings(&mut instance.renderer, camera_settings.as_ref());
            refresh_structure_designer_auto(instance);
        });
    }
}

pub fn load_node_networks(file_path: String) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = cad_instance.structure_designer.load_node_networks(&file_path);

                // Apply camera settings returned from load
                if let Ok(camera_settings) = &result {
                    apply_camera_settings(&mut cad_instance.renderer, camera_settings.as_ref());
                }

                refresh_structure_designer_auto(cad_instance);

                match result {
                    Ok(_) => APIResult { success: true, error_message: String::new() },
                    Err(e) => APIResult { success: false, error_message: e.to_string() },
                }
            },
            APIResult { success: false, error_message: "CAD instance not available".to_string() },
        )
    }
}
```

**Benefits of this approach:**
- Camera logic stays close to where network changes happen (in StructureDesigner)
- API layer has single helper function `apply_camera_settings()` - no duplication
- StructureDesigner remains decoupled from Renderer
- Easy to add camera settings return to other methods if needed in the future

### Step 9: Update Flutter side to refresh UI after network switch

**File:** `lib/structure_designer/structure_designer_model.dart`

The Flutter side should call `notifyListeners()` after network switch so the camera control widget updates to reflect the new orthographic/perspective state. This should already happen if `setActiveNodeNetwork()` triggers a refresh.

## File Changes Summary

| File | Change Type |
|------|-------------|
| `rust/src/structure_designer/camera_settings.rs` | **New file** |
| `rust/src/structure_designer/mod.rs` | Add `pub mod camera_settings;` |
| `rust/src/structure_designer/node_network.rs` | Add `camera_settings` field |
| `rust/src/structure_designer/structure_designer.rs` | Modify `set_active_node_network_name()` and `load_node_networks()` to return `Option<CameraSettings>` |
| `rust/src/structure_designer/serialization/node_networks_serialization.rs` | Add `SerializableCameraSettings`, update ser/deser |
| `rust/src/api/api_common.rs` | Add `sync_camera_to_active_network()` and `apply_camera_settings()` helpers |
| `rust/src/api/common_api.rs` | Call sync helper from 6 camera-modifying functions |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Call `apply_camera_settings()` after network changes |

## JSON Format Example

After implementation, a `.cnnd` file will look like:

```json
{
  "node_networks": [
    ["Main", {
      "next_node_id": 5,
      "node_type": { ... },
      "nodes": [ ... ],
      "return_node_id": null,
      "displayed_node_ids": [],
      "camera_settings": {
        "eye": [10.0, -25.0, 15.0],
        "target": [0.0, 0.0, 5.0],
        "up": [0.0, 0.32, 0.95],
        "orthographic": false,
        "ortho_half_height": 10.0,
        "pivot_point": [0.0, 0.0, 5.0]
      }
    }]
  ],
  "version": 2
}
```

Old files without `camera_settings` will continue to work - the field will simply be absent and default camera values will be used.

## Testing

Following existing test patterns in `rust/tests/`:

### 1. Backward Compatibility (existing roundtrip tests)

The existing roundtrip tests in `rust/tests/integration/cnnd_roundtrip_test.rs` already test loading old `.cnnd` files. Since we use `#[serde(default)]`, these will continue to pass - old files without `camera_settings` will deserialize with `None` and use defaults.

**No new test needed** - existing `test_diamond_roundtrip`, `test_hexagem_roundtrip`, etc. cover this.

### 2. Extend roundtrip test to verify camera_settings

**File:** `rust/tests/integration/cnnd_roundtrip_test.rs`

Add camera_settings comparison to the existing `roundtrip_cnnd_file()` function:

```rust
// Inside the network comparison loop:
assert_eq!(
    network1.camera_settings.is_some(),
    network2.camera_settings.is_some(),
    "camera_settings presence mismatch in network '{}'",
    name
);

if let (Some(cs1), Some(cs2)) = (&network1.camera_settings, &network2.camera_settings) {
    assert_eq!(cs1.eye, cs2.eye, "camera eye mismatch in network '{}'", name);
    assert_eq!(cs1.target, cs2.target, "camera target mismatch in network '{}'", name);
    assert_eq!(cs1.up, cs2.up, "camera up mismatch in network '{}'", name);
    assert_eq!(cs1.orthographic, cs2.orthographic, "camera orthographic mismatch in network '{}'", name);
    assert_eq!(cs1.ortho_half_height, cs2.ortho_half_height, "camera ortho_half_height mismatch in network '{}'", name);
    assert_eq!(cs1.pivot_point, cs2.pivot_point, "camera pivot_point mismatch in network '{}'", name);
}
```

### 3. Unit test for deserialization without camera_settings

**File:** `rust/tests/structure_designer/serialization_test.rs`

Add a test that verifies networks without `camera_settings` deserialize correctly:

```rust
#[test]
fn test_network_without_camera_settings_uses_none() {
    let built_ins = create_built_in_node_types();

    // Create a serializable network without camera_settings (simulating old files)
    let serializable = create_serializable_network(vec![
        create_serializable_node(1, "int", Some("myint")),
    ]);

    let network = serializable_to_node_network(&serializable, &built_ins, None).unwrap();

    // camera_settings should be None for old files
    assert!(network.camera_settings.is_none(), "Old files should have no camera_settings");
}
```

Note: The `create_serializable_network()` helper will need to be updated to include `camera_settings: None` field once we add it to `SerializableNodeNetwork`.

---

## Critical Implementation Notes

### Required Imports

**In `node_networks_serialization.rs`:**
```rust
use glam::f64::DVec3;
use crate::util::serialization_utils::dvec3_serializer;
use super::super::camera_settings::CameraSettings;
```

**In `structure_designer.rs`:**
```rust
use super::camera_settings::CameraSettings;
```

**In `api_common.rs`:**
```rust
use crate::structure_designer::camera_settings::CameraSettings;
use crate::renderer::renderer::Renderer;
```

### Test Helper Update (Phase 1)

The `create_serializable_network()` function in `rust/tests/structure_designer/serialization_test.rs` must be updated when `SerializableNodeNetwork` gains the new field:

```rust
fn create_serializable_network(nodes: Vec<SerializableNode>) -> SerializableNodeNetwork {
    SerializableNodeNetwork {
        next_node_id: nodes.len() as u64 + 1,
        node_type: SerializableNodeType { ... },
        nodes,
        return_node_id: None,
        displayed_node_ids: vec![],
        camera_settings: None,  // ADD THIS LINE
    }
}
```

**If this is not updated, tests will fail to compile.**

### Verification Commands

After Phase 1:
```bash
cd rust && cargo test
```

After Phase 2:
```bash
cd rust && cargo test
flutter run  # Manual testing: switch networks, verify camera restores
```

### Edge Cases to Handle

1. **No active network**: `sync_camera_to_active_network()` should silently do nothing if there's no active network
2. **Network without camera_settings**: When loading, if `camera_settings` is `None`, keep the current camera (don't reset to defaults - this preserves continuity when switching to a newly created network)

### HTTP Server / AI Agent Compatibility

The HTTP server (`lib/ai_assistant/http_server.dart`) used by AI agents (atomcad-cli) calls the **same Rust API functions** that we're modifying:

| HTTP Endpoint | Dart Call | Rust API Function | Our Change |
|---------------|-----------|-------------------|------------|
| `GET /camera` | `common_api.moveCamera()` | `move_camera()` | Add `sync_camera_to_active_network()` |
| `GET /camera` | `common_api.setOrthographicMode()` | `set_orthographic_mode()` | Add `sync_camera_to_active_network()` |
| `GET /camera` | `common_api.setOrthoHalfHeight()` | `set_ortho_half_height()` | Add `sync_camera_to_active_network()` |
| `POST /networks/activate` | `sd_api.setActiveNodeNetwork()` | `set_active_node_network()` | Apply returned camera settings (Step 8d) |
| `POST /save` | `sd_api.saveNodeNetworks()` | `save_node_networks()` | Camera already synced |
| `POST /load` | `sd_api.loadNodeNetworks()` | `load_node_networks()` | Apply returned camera settings (Step 8d) |

**Conclusion:** No additional changes needed for HTTP server - the same code paths are used.

### CLI Runner Note

The CLI runner (`rust/src/structure_designer/cli_runner.rs`) directly sets `designer.active_node_network_name` bypassing the API. This is **intentional and fine** because:
- CLI mode is headless (no renderer)
- Camera settings are irrelevant without a viewport
- CLI only evaluates networks and exports results
