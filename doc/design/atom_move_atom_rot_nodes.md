# Design: atom_move and atom_rot Nodes

## Overview

This document describes the design for two new atomic structure transformation nodes: `atom_move` and `atom_rot`. These nodes provide simple, composable transformations in free (continuous) space for atomic structures.

### Motivation

The existing `atom_trans` node combines translation and rotation in a single node with complex semantics:
- It rotates around the **transformed frame's axes** (intrinsic rotation), which is not intuitive for many users
- It combines two operations that users often want to apply separately
- The euler angle representation can lead to gimbal lock issues

The new nodes address these issues by:
- **Separating concerns**: `atom_move` handles translation only, `atom_rot` handles rotation only
- **Using world-aligned axes**: Both nodes operate in the fixed Cartesian coordinate system
- **Simpler mental model**: Each node does one thing well

### Implementation Pattern

**IMPORTANT**: These nodes follow the same pattern as `lattice_move` and `lattice_rot` (see `rust/src/structure_designer/nodes/lattice_move.rs` and `rust/src/structure_designer/nodes/lattice_rot.rs` for reference):

- **No frame transform manipulation**: The nodes directly transform atom positions, they do NOT use or modify the `frame_transform` field
- **Cumulative through structure**: When chaining nodes, each applies its transformation to the already-transformed atoms from upstream
- **Gadget at translation/pivot**: The gadget is positioned at the transformation value (translation vector for `atom_move`, pivot point for `atom_rot`), starting from origin
- **Simple sync_data**: The gadget's `sync_data` directly sets the node data values without computing relative transforms

### Deprecation of atom_trans

The `atom_trans` node will be deprecated:
- It remains in the codebase for backward compatibility
- Existing networks using `atom_trans` will continue to work
- The node type will be marked as **private** (`public: false`) so it cannot be added to new networks via the add-node dialog
- Users are encouraged to use `atom_move` and `atom_rot` for new designs

---

## atom_move Node

### Purpose

Translates an atomic structure by a vector in world space (Cartesian coordinates). The translation is absolute and world-aligned, similar to how `lattice_move` works in lattice space.

### Data Structure (Rust)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomMoveData {
    #[serde(with = "dvec3_serializer")]
    pub translation: DVec3,  // Translation vector in angstroms
}
```

### Evaluation Cache

```rust
#[derive(Debug, Clone)]
pub struct AtomMoveEvalCache {
    // Currently empty, but reserved for future gadget needs
    // (e.g., if we want to show the input structure's bounding box)
}
```

### Node Type Definition

```rust
NodeType {
    name: "atom_move".to_string(),
    description: "Translates an atomic structure by a vector in world space (Cartesian coordinates).
The translation is specified in angstroms along the X, Y, and Z axes.
This node operates in continuous space, unlike lattice_move which operates in discrete lattice space.".to_string(),
    summary: None,
    category: NodeTypeCategory::AtomicStructure,
    parameters: vec![
        Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::Atomic,
        },
        Parameter {
            id: None,
            name: "translation".to_string(),
            data_type: DataType::Vec3,
        },
    ],
    output_type: DataType::Atomic,
    public: true,
    node_data_creator: || Box::new(AtomMoveData {
        translation: DVec3::ZERO,
    }),
    node_data_saver: generic_node_data_saver::<AtomMoveData>,
    node_data_loader: generic_node_data_loader::<AtomMoveData>,
}
```

### Parameters (Input Pins)

| Pin Index | Name        | Type   | Description                          | Default      |
|-----------|-------------|--------|--------------------------------------|--------------|
| 0         | molecule    | Atomic | Input atomic structure (required)    | -            |
| 1         | translation | Vec3   | Translation vector in angstroms      | (0, 0, 0)    |

The `translation` input pin shadows the `translation` property in `AtomMoveData`. If a wire is connected to the pin, the wired value is used; otherwise, the stored property value is used.

### Evaluation Logic

Following the `lattice_move` pattern, the evaluation directly transforms atoms without using frame transforms:

```rust
fn eval(&self, ...) -> NetworkResult {
    // 1. Get input atomic structure
    let input = evaluate_arg_required(0);  // molecule

    // 2. Get translation (from pin or property)
    let translation = evaluate_or_default(1, self.translation, extract_vec3);

    // 3. Store eval cache for gadget (empty, reserved for future use)
    if network_stack.len() == 1 {
        context.selected_node_eval_cache = Some(Box::new(AtomMoveEvalCache {}));
    }

    // 4. Apply translation directly to atoms (NO frame transform manipulation)
    let mut result = input.clone();
    result.transform(&DQuat::IDENTITY, &translation);

    return NetworkResult::Atomic(result);
}
```

**Key difference from `atom_trans`**: This node does NOT manipulate `frame_transform`. The translation is applied directly to atom positions. When chaining multiple `atom_move` nodes, each one adds its translation to the already-translated atoms.

### Gadget Design

The `AtomMoveGadget` displays an XYZ axis gizmo that is **always world-aligned** (not rotated with the structure).

#### Visual Appearance
- Three colored cylinders representing X (red), Y (green), Z (blue) axes
- Arrow heads at the positive ends
- Small sphere at the origin
- **Positioned at the translation vector value** (i.e., gadget position = `self.translation`, starting from world origin)

#### Interaction
- **Hit test**: Detect which axis handle is being clicked
- **Drag**: Move along the selected axis in world space
- **Snapping**: No snapping (continuous movement)

#### Implementation

```rust
#[derive(Clone)]
pub struct AtomMoveGadget {
    pub translation: DVec3,
    pub dragged_handle_index: Option<i32>,
    pub start_drag_offset: f64,
    pub start_drag_translation: DVec3,
}

impl Tessellatable for AtomMoveGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        // Use xyz_gadget_utils with:
        // - Identity rotation (world-aligned)
        // - Position at self.translation
        // - No rotation handles (translation only)
        xyz_gadget_utils::tessellate_xyz_gadget(
            &mut output.mesh,
            &UnitCellStruct::cubic_identity(),  // Unit scale for world coords
            DQuat::IDENTITY,                     // World-aligned
            &self.translation,
            false,                               // No rotation handles
        );
    }
}

impl Gadget for AtomMoveGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        xyz_gadget_utils::xyz_gadget_hit_test(
            &UnitCellStruct::cubic_identity(),
            DQuat::IDENTITY,
            &self.translation,
            &ray_origin,
            &ray_direction,
            false,  // No rotation handles
        )
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        self.dragged_handle_index = Some(handle_index);
        self.start_drag_translation = self.translation;
        // Calculate initial offset along the axis
        self.start_drag_offset = calculate_axis_offset(...);
    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        // Calculate new offset and apply delta to translation
        let axis_direction = get_world_axis_direction(handle_index);
        let new_offset = calculate_axis_offset(...);
        let delta = new_offset - self.start_drag_offset;

        self.translation = self.start_drag_translation + axis_direction * delta;
    }

    fn end_drag(&mut self) {
        self.dragged_handle_index = None;
    }
}

impl NodeNetworkGadget for AtomMoveGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(atom_move_data) = data.as_any_mut().downcast_mut::<AtomMoveData>() {
            atom_move_data.translation = self.translation;
        }
    }
}
```

### Editor Design (Flutter)

```
┌─────────────────────────────────────────┐
│ Atom Move Properties                    │
│ atom_move                          [?]  │
├─────────────────────────────────────────┤
│                                         │
│ Translation (Å)                         │
│ ┌─────────┬─────────┬─────────┐        │
│ │ X: 0.00 │ Y: 0.00 │ Z: 0.00 │        │
│ └─────────┴─────────┴─────────┘        │
│                                         │
└─────────────────────────────────────────┘
```

#### Implementation

```dart
class AtomMoveEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIAtomMoveData? data;
  final StructureDesignerModel model;
  // ...
}

class _AtomMoveEditorState extends State<AtomMoveEditor> {
  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Atom Move Properties',
            nodeTypeName: 'atom_move',
          ),
          const SizedBox(height: 16),
          Vec3Input(
            label: 'Translation (Å)',
            value: widget.data!.translation,
            onChanged: (newValue) {
              widget.model.setAtomMoveData(
                widget.nodeId,
                APIAtomMoveData(translation: newValue),
              );
            },
          ),
        ],
      ),
    );
  }
}
```

### Subtitle

```rust
fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
    if connected_input_pins.contains("translation") {
        return None;
    }
    Some(format!("({:.2}, {:.2}, {:.2})",
        self.translation.x, self.translation.y, self.translation.z))
}
```

---

## atom_rot Node

### Purpose

Rotates an atomic structure around an axis in world space by a specified angle. The rotation is always around axes defined in the fixed Cartesian coordinate system (not the structure's local frame).

### Data Structure (Rust)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomRotData {
    pub angle: f64,  // Rotation angle in radians (displayed as degrees in UI)
    #[serde(with = "dvec3_serializer")]
    pub rot_axis: DVec3,  // Rotation axis direction (will be normalized)
    #[serde(with = "dvec3_serializer")]
    pub pivot_point: DVec3,  // Point around which rotation occurs (in angstroms)
}
```

### Evaluation Cache

```rust
#[derive(Debug, Clone)]
pub struct AtomRotEvalCache {
    pub pivot_point: DVec3,  // The actual evaluated pivot point (may be overridden by input pin)
    pub rot_axis: DVec3,  // The actual evaluated rotation axis (normalized)
}
```

### Node Type Definition

```rust
NodeType {
    name: "atom_rot".to_string(),
    description: "Rotates an atomic structure around an axis in world space.
The rotation is performed around the specified axis direction, centered at the pivot point.
The axis is always interpreted in the fixed Cartesian coordinate system (world space).
The rotation angle is specified in degrees in the UI.".to_string(),
    summary: None,
    category: NodeTypeCategory::AtomicStructure,
    parameters: vec![
        Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::Atomic,
        },
        Parameter {
            id: None,
            name: "angle".to_string(),
            data_type: DataType::Float,
        },
        Parameter {
            id: None,
            name: "rot_axis".to_string(),
            data_type: DataType::Vec3,
        },
        Parameter {
            id: None,
            name: "pivot_point".to_string(),
            data_type: DataType::Vec3,
        },
    ],
    output_type: DataType::Atomic,
    public: true,
    node_data_creator: || Box::new(AtomRotData {
        angle: 0.0,
        rot_axis: DVec3::new(0.0, 0.0, 1.0),  // Default: Z-axis
        pivot_point: DVec3::ZERO,
    }),
    node_data_saver: generic_node_data_saver::<AtomRotData>,
    node_data_loader: generic_node_data_loader::<AtomRotData>,
}
```

### Parameters (Input Pins)

| Pin Index | Name          | Type   | Description                              | Default      |
|-----------|---------------|--------|------------------------------------------|--------------|
| 0         | molecule      | Atomic | Input atomic structure (required)        | -            |
| 1         | angle         | Float  | Rotation angle (radians internally)      | 0.0          |
| 2         | rot_axis | Vec3   | Axis direction (auto-normalized)         | (0, 0, 1)    |
| 3         | pivot_point   | Vec3   | Center of rotation in angstroms          | (0, 0, 0)    |

All input pins (except molecule) shadow their corresponding properties in `AtomRotData`.

### Evaluation Logic

Following the `lattice_rot` pattern (see `rust/src/structure_designer/nodes/lattice_rot.rs`), the evaluation directly transforms atoms without using frame transforms:

```rust
fn eval(&self, ...) -> NetworkResult {
    // 1. Get input atomic structure
    let input = evaluate_arg_required(0);  // molecule

    // 2. Get parameters (from pins or properties)
    let angle = evaluate_or_default(1, self.angle, extract_float);
    let rot_axis = evaluate_or_default(2, self.rot_axis, extract_vec3);
    let pivot_point = evaluate_or_default(3, self.pivot_point, extract_vec3);

    // 3. Normalize the rotation axis
    let normalized_axis = rot_axis.normalize_or_zero();
    if normalized_axis == DVec3::ZERO {
        // Invalid axis - return input unchanged or error
        return NetworkResult::Atomic(input);
    }

    // 4. Store eval cache for gadget
    if network_stack.len() == 1 {
        context.selected_node_eval_cache = Some(Box::new(AtomRotEvalCache {
            pivot_point,
            rot_axis: normalized_axis,
        }));
    }

    // 5. Create rotation quaternion
    let rotation_quat = DQuat::from_axis_angle(normalized_axis, angle);

    // 6. Apply rotation around pivot point directly to atoms (NO frame transform manipulation)
    // This is: translate to origin, rotate, translate back
    let mut result = input.clone();

    // For each atom: new_pos = pivot + rotation * (old_pos - pivot)
    // Which is equivalent to: translate by -pivot, rotate, translate by +pivot
    result.transform(&DQuat::IDENTITY, &(-pivot_point));  // Move pivot to origin
    result.transform(&rotation_quat, &DVec3::ZERO);       // Rotate around origin
    result.transform(&DQuat::IDENTITY, &pivot_point);     // Move back

    return NetworkResult::Atomic(result);
}
```

**Key difference from `atom_trans`**: This node does NOT manipulate `frame_transform`. The rotation is applied directly to atom positions around the pivot point. When chaining multiple `atom_rot` nodes, each one rotates the already-transformed atoms.

### Gadget Design

The `AtomRotGadget` displays the rotation axis as an arrow with a draggable interaction for changing the angle. The implementation follows the pattern from `lattice_rot` (see `rust/src/structure_designer/nodes/lattice_rot.rs`).

#### Visual Appearance

```
                    ▲
                    │  (Arrow head - yellow)
                    │
                    │  (Cylinder - yellow, showing rotation axis)
                    │
                    ●  (Pivot point - red sphere)
                    │
                    │
                    ▼

        ╭─────────────────╮
       ╱                   ╲   (Optional: arc showing current rotation angle)
      ╱                     ╲
```

- **Yellow cylinder**: The rotation axis, passing through the pivot point (which comes from `self.pivot_point`, not relative to any input)
- **Arrow head**: At the positive end of the axis, indicating direction
- **Red sphere**: The pivot point (positioned at `self.pivot_point` in world space)
- **Optional arc/ring**: Visual indicator of the current rotation angle (can be added in future iteration)

#### Interaction

The gadget allows **continuous dragging to change the angle**:

1. **Hit test**: Detect clicks on the arrow/axis cylinder
2. **Start drag**: Record the initial mouse position projected onto a plane perpendicular to the rotation axis
3. **Drag**: Calculate the angular change based on mouse movement around the axis
4. **End drag**: Finalize the angle change

#### Implementation

```rust
#[derive(Clone)]
pub struct AtomRotGadget {
    pub angle: f64,
    pub rot_axis: DVec3,  // Normalized
    pub pivot_point: DVec3,
    pub dragging: bool,
    pub drag_start_angle: f64,
    pub drag_start_mouse_angle: f64,
}

impl Tessellatable for AtomRotGadget {
    fn tessellate(&self, output: &mut TessellationOutput) {
        let output_mesh = &mut output.mesh;

        // Draw rotation axis as a yellow cylinder with arrow head
        let axis_length = 15.0;  // Length in angstroms
        let cylinder_radius = 0.1;

        let half_length = axis_length * 0.5;
        let top_center = self.pivot_point + self.rot_axis * half_length;
        let bottom_center = self.pivot_point - self.rot_axis * half_length;

        let yellow_material = Material::new(
            &Vec3::new(1.0, 1.0, 0.0),
            0.4, 0.8
        );

        // Cylinder for the axis
        tessellate_cylinder(
            output_mesh,
            &top_center,
            &bottom_center,
            cylinder_radius,
            16,
            &yellow_material,
            true,
            Some(&yellow_material),
            Some(&yellow_material),
        );

        // Arrow head at top
        let arrow_base = top_center;
        let arrow_tip = top_center + self.rot_axis * 0.5;
        tessellate_cone(
            output_mesh,
            &arrow_base,
            &arrow_tip,
            cylinder_radius * 3.0,
            16,
            &yellow_material,
        );

        // Red sphere at pivot point
        let red_material = Material::new(
            &Vec3::new(1.0, 0.0, 0.0),
            0.4, 0.0
        );

        tessellate_sphere(
            output_mesh,
            &self.pivot_point,
            0.4,
            12, 12,
            &red_material,
        );
    }
}

impl Gadget for AtomRotGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        // Test intersection with the axis cylinder
        // Return 0 if hit (single handle for rotation)
        if cylinder_ray_intersection(
            ray_origin, ray_direction,
            self.pivot_point, self.rot_axis,
            AXIS_LENGTH, HIT_RADIUS
        ) {
            return Some(0);
        }
        None
    }

    fn start_drag(&mut self, _handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        self.dragging = true;
        self.drag_start_angle = self.angle;

        // Calculate initial mouse angle around the rotation axis
        self.drag_start_mouse_angle = self.calculate_mouse_angle(ray_origin, ray_direction);
    }

    fn drag(&mut self, _handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
        // Calculate current mouse angle around the rotation axis
        let current_mouse_angle = self.calculate_mouse_angle(ray_origin, ray_direction);

        // Calculate delta and apply to angle
        let delta = current_mouse_angle - self.drag_start_mouse_angle;
        self.angle = self.drag_start_angle + delta;
    }

    fn end_drag(&mut self) {
        self.dragging = false;
    }
}

impl AtomRotGadget {
    /// Calculate the angle of the mouse position around the rotation axis
    fn calculate_mouse_angle(&self, ray_origin: DVec3, ray_direction: DVec3) -> f64 {
        // 1. Find intersection of ray with plane perpendicular to axis at pivot
        let plane_normal = self.rot_axis;
        let plane_point = self.pivot_point;

        let denom = ray_direction.dot(plane_normal);
        if denom.abs() < 1e-10 {
            return 0.0;  // Ray parallel to plane
        }

        let t = (plane_point - ray_origin).dot(plane_normal) / denom;
        let intersection = ray_origin + ray_direction * t;

        // 2. Calculate angle from pivot to intersection
        let to_intersection = intersection - self.pivot_point;

        // Create a reference direction perpendicular to the axis
        let ref_dir = if self.rot_axis.dot(DVec3::X).abs() < 0.9 {
            self.rot_axis.cross(DVec3::X).normalize()
        } else {
            self.rot_axis.cross(DVec3::Y).normalize()
        };
        let perp_dir = self.rot_axis.cross(ref_dir);

        // Project intersection onto the perpendicular plane
        let x = to_intersection.dot(ref_dir);
        let y = to_intersection.dot(perp_dir);

        y.atan2(x)
    }
}

impl NodeNetworkGadget for AtomRotGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(atom_rot_data) = data.as_any_mut().downcast_mut::<AtomRotData>() {
            atom_rot_data.angle = self.angle;
        }
    }

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}
```

### Editor Design (Flutter)

```
┌─────────────────────────────────────────┐
│ Atom Rotation Properties                │
│ atom_rot                           [?]  │
├─────────────────────────────────────────┤
│                                         │
│ Rotation Axis                           │
│ ┌─────────────────────────────────────┐ │
│ │ ▼ Z-axis (0, 0, 1)                  │ │
│ └─────────────────────────────────────┘ │
│                                         │
│ Custom Axis                             │
│ ┌─────────┬─────────┬─────────┐        │
│ │ X: 0.00 │ Y: 0.00 │ Z: 1.00 │        │
│ └─────────┴─────────┴─────────┘        │
│                                         │
│ Angle (degrees)                         │
│ ┌─────────────────────────────────────┐ │
│ │ 0.0                              °  │ │
│ └─────────────────────────────────────┘ │
│                                         │
│ ┌─────────────────────────────────────┐ │
│ │ ○ Current rotation: 0.0°            │ │
│ └─────────────────────────────────────┘ │
│                                         │
│ Pivot Point (Å)                         │
│ ┌─────────┬─────────┬─────────┐        │
│ │ X: 0.00 │ Y: 0.00 │ Z: 0.00 │        │
│ └─────────┴─────────┴─────────┘        │
│                                         │
└─────────────────────────────────────────┘
```

#### Preset Axes Dropdown Options

| Label              | Value           |
|--------------------|-----------------|
| X-axis             | (1, 0, 0)       |
| Y-axis             | (0, 1, 0)       |
| Z-axis             | (0, 0, 1)       |
| -X-axis            | (-1, 0, 0)      |
| -Y-axis            | (0, -1, 0)      |
| -Z-axis            | (0, 0, -1)      |
| Custom...          | (user-defined)  |

#### Implementation

```dart
class AtomRotEditor extends StatefulWidget {
  final BigInt nodeId;
  final APIAtomRotData? data;
  final StructureDesignerModel model;
  // ...
}

class _AtomRotEditorState extends State<AtomRotEditor> {
  // Preset axis options
  static const Map<String, APIVec3?> presetAxes = {
    'X-axis': APIVec3(x: 1, y: 0, z: 0),
    'Y-axis': APIVec3(x: 0, y: 1, z: 0),
    'Z-axis': APIVec3(x: 0, y: 0, z: 1),
    '-X-axis': APIVec3(x: -1, y: 0, z: 0),
    '-Y-axis': APIVec3(x: 0, y: -1, z: 0),
    '-Z-axis': APIVec3(x: 0, y: 0, z: -1),
    'Custom': null,
  };

  String? _getPresetForAxis(APIVec3 axis) {
    for (final entry in presetAxes.entries) {
      if (entry.value != null &&
          (entry.value!.x - axis.x).abs() < 0.001 &&
          (entry.value!.y - axis.y).abs() < 0.001 &&
          (entry.value!.z - axis.z).abs() < 0.001) {
        return entry.key;
      }
    }
    return 'Custom';
  }

  double _radiansToDegrees(double radians) => radians * 180.0 / math.pi;
  double _degreesToRadians(double degrees) => degrees * math.pi / 180.0;

  @override
  Widget build(BuildContext context) {
    final currentPreset = _getPresetForAxis(widget.data!.rotAxis);

    return Padding(
      padding: const EdgeInsets.all(8.0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const NodeEditorHeader(
            title: 'Atom Rotation Properties',
            nodeTypeName: 'atom_rot',
          ),
          const SizedBox(height: 16),

          // Preset axis dropdown
          Text('Rotation Axis', style: Theme.of(context).textTheme.titleSmall),
          const SizedBox(height: 8),
          DropdownButtonFormField<String>(
            value: currentPreset,
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
              contentPadding: EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            ),
            items: presetAxes.keys.map((name) =>
              DropdownMenuItem(value: name, child: Text(name))
            ).toList(),
            onChanged: (String? newValue) {
              if (newValue != null && presetAxes[newValue] != null) {
                _updateRotationAxis(presetAxes[newValue]!);
              }
            },
          ),
          const SizedBox(height: 16),

          // Custom axis input (always visible for fine-tuning)
          Vec3Input(
            label: 'Custom Axis',
            value: widget.data!.rotAxis,
            onChanged: _updateRotationAxis,
          ),
          const SizedBox(height: 16),

          // Angle input (in degrees)
          FloatInput(
            label: 'Angle (degrees)',
            value: _radiansToDegrees(widget.data!.angle),
            onChanged: (newValue) {
              widget.model.setAtomRotData(
                widget.nodeId,
                APIAtomRotData(
                  angle: _degreesToRadians(newValue),
                  rotAxis: widget.data!.rotAxis,
                  pivotPoint: widget.data!.pivotPoint,
                ),
              );
            },
          ),
          const SizedBox(height: 16),

          // Pivot point input
          Vec3Input(
            label: 'Pivot Point (Å)',
            value: widget.data!.pivotPoint,
            onChanged: (newValue) {
              widget.model.setAtomRotData(
                widget.nodeId,
                APIAtomRotData(
                  angle: widget.data!.angle,
                  rotAxis: widget.data!.rotAxis,
                  pivotPoint: newValue,
                ),
              );
            },
          ),
        ],
      ),
    );
  }

  void _updateRotationAxis(APIVec3 newAxis) {
    widget.model.setAtomRotData(
      widget.nodeId,
      APIAtomRotData(
        angle: widget.data!.angle,
        rotAxis: newAxis,
        pivotPoint: widget.data!.pivotPoint,
      ),
    );
  }
}
```

### Subtitle

```rust
fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
    let show_angle = !connected_input_pins.contains("angle");
    let show_axis = !connected_input_pins.contains("rot_axis");

    if self.angle == 0.0 {
        return None;  // No rotation
    }

    let mut parts = Vec::new();

    if show_angle {
        let degrees = self.angle.to_degrees();
        parts.push(format!("{:.1}°", degrees));
    }

    if show_axis {
        // Show simplified axis name if it's a standard axis
        let axis_name = match (self.rot_axis.x, self.rot_axis.y, self.rot_axis.z) {
            (x, y, z) if (x - 1.0).abs() < 0.001 && y.abs() < 0.001 && z.abs() < 0.001 => "X",
            (x, y, z) if x.abs() < 0.001 && (y - 1.0).abs() < 0.001 && z.abs() < 0.001 => "Y",
            (x, y, z) if x.abs() < 0.001 && y.abs() < 0.001 && (z - 1.0).abs() < 0.001 => "Z",
            (x, y, z) if (x + 1.0).abs() < 0.001 && y.abs() < 0.001 && z.abs() < 0.001 => "-X",
            (x, y, z) if x.abs() < 0.001 && (y + 1.0).abs() < 0.001 && z.abs() < 0.001 => "-Y",
            (x, y, z) if x.abs() < 0.001 && y.abs() < 0.001 && (z + 1.0).abs() < 0.001 => "-Z",
            _ => return Some(format!("{:.1}° custom", self.angle.to_degrees())),
        };
        parts.push(axis_name.to_string());
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}
```

---

## API Types

### Rust API Types

```rust
// In rust/src/api/structure_designer/structure_designer_api_types.rs

#[derive(Debug, Clone)]
pub struct APIAtomMoveData {
    pub translation: APIVec3,
}

#[derive(Debug, Clone)]
pub struct APIAtomRotData {
    pub angle: f64,  // In radians
    pub rot_axis: APIVec3,
    pub pivot_point: APIVec3,
}
```

### Model Methods

```rust
// In rust/src/api/structure_designer/

pub fn set_atom_move_data(&mut self, node_id: u64, data: APIAtomMoveData) {
    // Update node data
}

pub fn set_atom_rot_data(&mut self, node_id: u64, data: APIAtomRotData) {
    // Update node data
}

pub fn get_atom_move_data(&self, node_id: u64) -> Option<APIAtomMoveData> {
    // Get node data
}

pub fn get_atom_rot_data(&self, node_id: u64) -> Option<APIAtomRotData> {
    // Get node data
}
```

---

## Text Properties

Both nodes support text property editing for programmatic access.

### atom_move

```rust
fn get_text_properties(&self) -> Vec<(String, TextValue)> {
    vec![
        ("translation".to_string(), TextValue::Vec3(self.translation)),
    ]
}

fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
    if let Some(v) = props.get("translation") {
        self.translation = v.as_vec3().ok_or_else(|| "translation must be a Vec3".to_string())?;
    }
    Ok(())
}
```

### atom_rot

```rust
fn get_text_properties(&self) -> Vec<(String, TextValue)> {
    vec![
        ("angle".to_string(), TextValue::Float(self.angle)),
        ("rot_axis".to_string(), TextValue::Vec3(self.rot_axis)),
        ("pivot_point".to_string(), TextValue::Vec3(self.pivot_point)),
    ]
}

fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
    if let Some(v) = props.get("angle") {
        self.angle = v.as_float().ok_or_else(|| "angle must be a float".to_string())?;
    }
    if let Some(v) = props.get("rot_axis") {
        self.rot_axis = v.as_vec3().ok_or_else(|| "rot_axis must be a Vec3".to_string())?;
    }
    if let Some(v) = props.get("pivot_point") {
        self.pivot_point = v.as_vec3().ok_or_else(|| "pivot_point must be a Vec3".to_string())?;
    }
    Ok(())
}
```

---

## Parameter Metadata

### atom_move

```rust
fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
    let mut m = HashMap::new();
    m.insert("molecule".to_string(), (true, None));  // required
    m
}
```

### atom_rot

```rust
fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
    let mut m = HashMap::new();
    m.insert("molecule".to_string(), (true, None));  // required
    m
}
```

---

## Migration Notes

### Deprecating atom_trans

1. Change `atom_trans` node type to `public: false`:
   ```rust
   NodeType {
       // ...
       public: false,  // Hidden from add-node dialog
       // ...
   }
   ```

2. Existing `.cnnd` files with `atom_trans` nodes will continue to load and work correctly.

3. Consider adding a migration hint in the node's description:
   ```rust
   description: "[DEPRECATED] Use atom_move and atom_rot instead.
   This node combines translation and rotation with intrinsic euler angles.
   ...".to_string(),
   ```

### Equivalent Operations

To achieve the same result as `atom_trans(translation, rotation)`:

```
molecule → atom_rot(rotation.x, X-axis) → atom_rot(rotation.y, Y-axis) → atom_rot(rotation.z, Z-axis) → atom_move(translation)
```

Note: The order matters due to the intrinsic euler angle semantics of atom_trans. Users should be aware that exact equivalence requires careful attention to rotation order.

---

## Implementation Checklist

### Rust Backend

- [ ] Create `rust/src/structure_designer/nodes/atom_move.rs`
- [ ] Create `rust/src/structure_designer/nodes/atom_rot.rs`
- [ ] Register nodes in `nodes/mod.rs`
- [ ] Register nodes in `node_type_registry.rs`
- [ ] Add API types in `api/structure_designer/structure_designer_api_types.rs`
- [ ] Add API methods for get/set node data
- [ ] Mark `atom_trans` as `public: false`
- [ ] Write tests in `rust/tests/`

### Flutter Frontend

- [ ] Create `lib/structure_designer/node_data/atom_move_editor.dart`
- [ ] Create `lib/structure_designer/node_data/atom_rot_editor.dart`
- [ ] Register editors in node editor factory
- [ ] Add model methods for set/get data
- [ ] Run `flutter_rust_bridge_codegen generate`

### Testing

- [ ] Unit tests for evaluation logic
- [ ] Gadget interaction tests
- [ ] Snapshot tests for node serialization
- [ ] Integration tests with existing atomic structures

---

## Future Enhancements

1. **Visual rotation arc**: Add an arc/ring visualization showing the current rotation angle in the atom_rot gadget.

2. **Snap to angles**: Option to snap to common angles (90°, 45°, 30°, etc.) during gadget drag.

3. **Rotation presets**: Common rotation amounts (90°, 180°, -90°) as quick buttons in the editor.

4. **Axis from selection**: Allow selecting two atoms to define a rotation axis.

5. **Center of mass pivot**: Option to automatically set pivot point to the structure's center of mass.
