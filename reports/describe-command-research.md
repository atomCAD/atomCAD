# Research: `describe` Command Improvement

## Essential Reading for Implementers

Before implementing, read these files to understand the patterns:

1. **`rust/src/structure_designer/text_format/node_type_introspection.rs`** - Current describe implementation (the file to modify)
2. **`rust/src/structure_designer/node_data.rs`** - NodeData trait (add new method here)
3. **`rust/src/structure_designer/nodes/atom_fill.rs`** - Example showing Pattern B (hardcoded `motif` default), Pattern C (required `shape`), and Pattern D (literal-only `element_values`)
4. **`rust/src/structure_designer/nodes/sphere.rs`** - Example showing Pattern A (property-backed `center`, `radius`) and Pattern B (hardcoded `unit_cell`)

---

## Problem Statement

The `atomcad-cli describe <node>` command outputs incomplete and misleading information about node inputs. The current format also exposes internal implementation details (parameters vs properties) that aren't meaningful to CLI users.

**Current `describe atom_fill` output:**
```
Parameters (input pins):
  shape        : Geometry  [no default - wire only]
  motif        : Motif     [no default - wire only]
  m_offset     : Vec3      [default: (0.0, 0.0, 0.0)]
  passivate    : Bool      [default: true]
  rm_single    : Bool      [default: false]
  surf_recon   : Bool      [default: false]
  invert_phase : Bool      [default: false]

Properties (not wirable):
  parameter_element_value_definition : String  [default: ""]
```

**Problems:**
1. `motif` says "wire only" but actually has a default (`DEFAULT_ZINCBLENDE_MOTIF`)
2. The "Parameters" vs "Properties" distinction is an implementation detail, not useful to CLI users
3. No indication of which inputs can only be wired vs only set as literals

---

## Proposed Output Format

**New `describe atom_fill` output:**
```
Node: atom_fill
Category: AtomicStructure
Description: Converts a 3D geometry into an atomic structure...

Inputs:
  shape          : Geometry  [required, wire-only]
  motif          : Motif     [default: cubic zincblende, wire-only]
  m_offset       : Vec3      [default: (0.0, 0.0, 0.0)]
  passivate      : Bool      [default: true]
  rm_single      : Bool      [default: false]
  surf_recon     : Bool      [default: false]
  invert_phase   : Bool      [default: false]
  element_values : String    [default: "", literal-only]

Output: Atomic
```

### Terminology

- **wire-only**: This input can only be connected to another node's output. There is no text literal representation for this type (e.g., `Geometry`, `Atomic`, `Motif`).
- **literal-only**: This input can only be set as a literal value in the text format. It has no input pin and cannot be connected to other nodes.
- Inputs without either marker can be set as a literal OR wired to another node.

### Key Changes

1. Single "Inputs" section instead of "Parameters" + "Properties"
2. `[required]` for inputs that must be provided
3. `[default: <value>]` shows the actual default (including hardcoded constants)
4. `wire-only` / `literal-only` markers where applicable

---

## Technical Background

### Four Input Patterns in the Codebase

#### Pattern A: Property-Backed Default
The input has a stored property; when not connected, uses the property value.
```rust
let radius = network_evaluator.evaluate_or_default(
    ..., 1, self.radius, NetworkResult::extract_int  // ← Uses stored property
);
```
**Examples:** `sphere.center`, `sphere.radius`, `atom_fill.passivate`

#### Pattern B: Hardcoded Constant Default
The input has no stored property but uses a hardcoded constant when not connected.
```rust
let unit_cell = network_evaluator.evaluate_or_default(
    ..., 2, UnitCellStruct::cubic_diamond(), ...  // ← Hardcoded constant
);
```
**Examples:** `sphere.unit_cell`, `atom_fill.motif`
**Current bug:** These show as "wire only" but actually have defaults!

#### Pattern C: Required Input
The input has no default; evaluation fails if not connected.
```rust
let shape_val = network_evaluator.evaluate_arg_required(..., 0);
```
**Examples:** `atom_fill.shape`, `diff.base`, `union.shapes`

#### Pattern D: Literal-Only (Stored Property, No Input Pin)
A property exposed via `get_text_properties()` but not in `parameters`.
```rust
// In get_text_properties only, no matching Parameter
("parameter_element_value_definition".to_string(), TextValue::String(...))
```
**Examples:** `atom_fill.element_values`, `expr.expression`

### Current Implementation Flaw

**Location:** `rust/src/structure_designer/text_format/node_type_introspection.rs`

```rust
let default_info = if let Some((_, default_val)) = prop_map.get(&param.name) {
    format!("[default: {}]", default_val)
} else {
    "[no default - wire only]".to_string()  // ← Cannot distinguish B from C!
};
```

The code assumes no matching property = no default, but Pattern B has hardcoded defaults.

---

## Inventory of Affected Inputs

### Hardcoded Defaults (Pattern B) - Currently Broken

| Node | Input | Hardcoded Default | Display As |
|------|-------|-------------------|------------|
| `sphere` | `unit_cell` | `UnitCellStruct::cubic_diamond()` | `[default: cubic diamond]` |
| `cuboid` | `unit_cell` | `UnitCellStruct::cubic_diamond()` | `[default: cubic diamond]` |
| `half_space` | `unit_cell` | `UnitCellStruct::cubic_diamond()` | `[default: cubic diamond]` |
| `facet_shell` | `unit_cell` | `UnitCellStruct::cubic_diamond()` | `[default: cubic diamond]` |
| `drawing_plane` | `unit_cell` | `UnitCellStruct::cubic_diamond()` | `[default: cubic diamond]` |
| `atom_fill` | `motif` | `DEFAULT_ZINCBLENDE_MOTIF` | `[default: cubic zincblende]` |
| `circle` | `drawing_plane` | `DrawingPlane::default()` | `[default: XY plane]` |
| `rect` | `drawing_plane` | `DrawingPlane::default()` | `[default: XY plane]` |
| `polygon` | `drawing_plane` | `DrawingPlane::default()` | `[default: XY plane]` |
| `half_plane` | `drawing_plane` | `DrawingPlane::default()` | `[default: XY plane]` |
| `reg_poly` | `drawing_plane` | `DrawingPlane::default()` | `[default: XY plane]` |

### Required Inputs (Pattern C) - Currently Correct but Wrong Wording

| Node | Input | Notes |
|------|-------|-------|
| `atom_fill` | `shape` | Geometry input |
| `atom_trans` | `molecule` | Atomic structure input |
| `atom_cut` | `molecule` | Atomic structure input |
| `edit_atom` | `molecule` | Atomic structure input |
| `relax` | `molecule` | Atomic structure input |
| `diff` | `base`, `sub` | Geometry inputs |
| `diff_2d` | `base`, `sub` | Geometry2D inputs |
| `union` | `shapes` | Array input |
| `union_2d` | `shapes` | Array input |
| `intersect` | `shapes` | Array input |
| `intersect_2d` | `shapes` | Array input |
| `extrude` | `shape_2d` | 2D geometry input |
| `geo_trans` | `shape` | Geometry input |
| `lattice_move` | `geometry` | Geometry input |
| `lattice_rot` | `geometry` | Geometry input |
| `lattice_symop` | `geometry` | Geometry input |
| `map` | `xs`, `f` | Array and function inputs |

---

## Solution: Add `get_parameter_metadata()` Trait Method

Other approaches were considered (adding metadata to `Parameter` struct, convention-based inference, separate description method), but this approach offers the best balance of minimal invasiveness and accuracy.

### 1. Add Method to NodeData Trait

**File:** `rust/src/structure_designer/node_data.rs`

```rust
trait NodeData {
    // Existing methods...

    /// Returns metadata for inputs not derivable from get_text_properties().
    /// Maps input name -> (is_required: bool, default_description: Option<String>)
    ///
    /// - If input has a matching property in get_text_properties(), no entry needed
    /// - If input is required (Pattern C), return (true, None)
    /// - If input has hardcoded default (Pattern B), return (false, Some("description"))
    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        HashMap::new()  // Default: all non-property inputs are required
    }
}
```

### 2. Implement for Affected Nodes

**File:** `rust/src/structure_designer/nodes/sphere.rs`
```rust
fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
    let mut m = HashMap::new();
    m.insert("unit_cell".to_string(), (false, Some("cubic diamond".to_string())));
    m
}
```

**File:** `rust/src/structure_designer/nodes/atom_fill.rs`
```rust
fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
    let mut m = HashMap::new();
    m.insert("shape".to_string(), (true, None));  // required
    m.insert("motif".to_string(), (false, Some("cubic zincblende".to_string())));
    m
}
```

Similar implementations needed for: `cuboid`, `half_space`, `facet_shell`, `drawing_plane`, `circle`, `rect`, `polygon`, `half_plane`, `reg_poly`, and all nodes with required inputs.

### 3. Update Introspection Logic

**File:** `rust/src/structure_designer/text_format/node_type_introspection.rs`

```rust
pub fn describe_node_type(node_type_name: &str, registry: &NodeTypeRegistry) -> String {
    let node_type = match registry.get_node_type(node_type_name) {
        Some(nt) => nt,
        None => return format!("Node type '{}' not found\n", node_type_name),
    };

    let default_data = (node_type.node_data_creator)();
    let text_props = default_data.get_text_properties();
    let metadata = default_data.get_parameter_metadata();

    // Build property map
    let prop_map: HashMap<String, (String, String)> = text_props
        .iter()
        .map(|(name, value)| (name.clone(), (value.inferred_data_type().to_string(), value.to_text())))
        .collect();

    // Get parameter names for filtering literal-only properties
    let param_names: HashSet<&str> = node_type.parameters.iter().map(|p| p.name.as_str()).collect();

    let mut output = String::new();

    // Header
    writeln!(output, "Node: {}", node_type.name).unwrap();
    writeln!(output, "Category: {:?}", node_type.category).unwrap();
    writeln!(output, "Description: {}", node_type.description).unwrap();
    writeln!(output).unwrap();
    writeln!(output, "Inputs:").unwrap();

    // Process parameters (wirable inputs)
    for param in &node_type.parameters {
        let type_str = param.data_type.to_string();
        let is_wire_only = is_wire_only_type(&param.data_type);

        let default_info = if let Some((_, default_val)) = prop_map.get(&param.name) {
            format!("[default: {}]", default_val)
        } else if let Some((is_required, default_desc)) = metadata.get(&param.name) {
            if *is_required {
                "[required]".to_string()
            } else {
                match default_desc {
                    Some(desc) => format!("[default: {}]", desc),
                    None => "[has default]".to_string(),
                }
            }
        } else {
            "[required]".to_string()  // Fallback: assume required
        };

        let wire_only_marker = if is_wire_only { ", wire-only" } else { "" };

        writeln!(output, "  {:width$} : {}  {}{}",
            param.name, type_str, default_info, wire_only_marker,
            width = max_name_len).unwrap();
    }

    // Process literal-only properties (not in parameters)
    for (name, value) in &text_props {
        if !param_names.contains(name.as_str()) {
            let type_str = value.inferred_data_type().to_string();
            writeln!(output, "  {:width$} : {}  [default: {}, literal-only]",
                name, type_str, value.to_text(), width = max_name_len).unwrap();
        }
    }

    writeln!(output).unwrap();
    writeln!(output, "Output: {}", node_type.output_type).unwrap();

    output
}

fn is_wire_only_type(data_type: &DataType) -> bool {
    matches!(data_type,
        DataType::Geometry | DataType::Geometry2D | DataType::Atomic |
        DataType::Motif | DataType::UnitCell | DataType::DrawingPlane |
        DataType::Array(_) | DataType::Function
    )
}
```

---

## Files to Modify

1. **`rust/src/structure_designer/node_data.rs`** - Add `get_parameter_metadata()` trait method with default impl
2. **`rust/src/structure_designer/text_format/node_type_introspection.rs`** - Update `describe_node_type()` logic
3. **Node files requiring `get_parameter_metadata()` implementation:**
   - `sphere.rs`, `cuboid.rs`, `half_space.rs`, `facet_shell.rs`, `drawing_plane.rs`
   - `atom_fill.rs`, `atom_trans.rs`, `atom_cut.rs`, `edit_atom.rs`, `relax.rs`
   - `circle.rs`, `rect.rs`, `polygon.rs`, `half_plane.rs`, `reg_poly.rs`
   - `diff.rs`, `diff_2d.rs`, `union.rs`, `union_2d.rs`, `intersect.rs`, `intersect_2d.rs`
   - `extrude.rs`, `geo_trans.rs`, `lattice_move.rs`, `lattice_rot.rs`, `lattice_symop.rs`
   - `map.rs`

---

## Implementation Notes

- The `get_parameter_metadata()` method only needs to be implemented for nodes that have:
  - Hardcoded defaults (Pattern B) - to provide the default description
  - Required inputs (Pattern C) - to mark them as required (though the fallback handles this)
- Nodes with only property-backed defaults (Pattern A) don't need to implement this method
- The `is_wire_only_type()` helper determines which types cannot be expressed as literals
