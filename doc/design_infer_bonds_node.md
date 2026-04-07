# Design: `infer_bonds` Node

## Overview

A standalone node that performs bond inference on an `AtomicStructure` input. Extracts the bond inference functionality currently embedded in `import_cif` into a reusable, composable node.

## Node Signature

| | Name | DataType |
|---|---|---|
| **Input pin 0** | `molecule` | `Atomic` |
| **Input pin 1** | `additive` | `Bool` |
| **Input pin 2** | `bond_tolerance` | `Float` |
| **Output pin 0** | (primary) | `Atomic` |

- **Name:** `infer_bonds`
- **Category:** `AtomicStructure`
- **Description:** Infers covalent bonds between atoms based on interatomic distances and covalent radii.
- **Summary:** Infer bonds from distances

## Parameters

### `molecule` (required)
The input atomic structure.

### `additive` (optional, default: `false`)
Controls whether existing bonds are preserved:
- `false` (default): All existing bonds are cleared before inference. The output contains only inferred bonds. Predictable, idempotent — the standard "rebond from scratch" mode.
- `true`: Existing bonds are kept. Inferred bonds are added only where no bond already exists (the underlying `auto_create_bonds_with_tolerance` already skips existing bonds via `has_bond_between`). Useful for layering multiple inference passes with different tolerances or supplementing partial bond data.

Stored as `InferBondsData.additive`.

### `bond_tolerance` (optional, default: `1.15`)
Multiplier applied to the sum of covalent radii of two atoms to determine the maximum bonding distance:

```
max_bond_distance = (radius_a + radius_b) * bond_tolerance
```

- `< 1.0`: Only very close atoms bond (stricter than standard covalent radii)
- `1.15` (default): Standard covalent bond detection
- `> 1.15`: Increasingly permissive, may detect second-nearest neighbors

Stored as `InferBondsData.bond_tolerance` and editable via text properties and the UI field.

## Behavior

1. Evaluate input `molecule`. Return error if missing.
2. Evaluate `additive` — use wired value if connected, else `self.additive`.
3. Evaluate `bond_tolerance` — use wired value if connected, else `self.bond_tolerance`.
4. Clone the input `AtomicStructure`.
5. If `additive` is `false`: clear all existing bonds on the clone.
6. Call `auto_create_bonds_with_tolerance(&mut structure, bond_tolerance)`.
7. Return the structure with inferred bonds.

When `additive` is `false` (default), the node is idempotent — applying it twice produces the same result regardless of whether the input already had bonds. When `additive` is `true`, the existing bonds are preserved and new bonds are added only where none exist.

## Node Data Properties and Input Pin Override Pattern

Each optional parameter (`additive`, `bond_tolerance`) exists in **two forms**:

1. **Node data property** — a field on `InferBondsData` (Rust struct, serialized to `.cnnd`). Editable via the property editor UI panel when the node is selected. This is the default value.
2. **Input pin** — a wirable input on the node. When a wire is connected to the pin, its value **overrides** the node data property.

This is a standard pattern used throughout the codebase (see `import_cif` for the reference implementation). The `eval()` method resolves each parameter like this:

```rust
// Pattern: evaluate_arg returns NetworkResult::None when pin is unconnected
let additive = match network_evaluator.evaluate_arg(..., 1) {
    NetworkResult::Bool(b) => b,      // pin connected → use wired value
    _ => self.additive,                // pin unconnected → use node data property
};
```

The node data properties are exposed to three consumers:
- **Property editor UI** (Flutter widget) — for interactive editing
- **Text format** (`get_text_properties` / `set_text_properties`) — for AI text-based editing
- **Subtitle** (`get_subtitle`) — shows current values on the node in the graph when pins are not connected

### Text properties

Only serialize non-default values (same convention as `import_cif`):

```rust
fn get_text_properties(&self) -> Vec<(String, TextValue)> {
    let mut props = Vec::new();
    if self.additive {  // default is false, only serialize when true
        props.push(("additive".to_string(), TextValue::Bool(true)));
    }
    if (self.bond_tolerance - 1.15).abs() > 1e-10 {  // default is 1.15
        props.push(("bond_tolerance".to_string(), TextValue::Float(self.bond_tolerance)));
    }
    props
}
```

### Subtitle

Shows property values on the node body when pins are not connected. Uses `connected_input_pins` parameter to suppress values that are overridden by wires:

```rust
fn get_subtitle(&self, connected_input_pins: &HashSet<String>) -> Option<String> {
    let mut parts = Vec::new();
    if self.additive && !connected_input_pins.contains("additive") {
        parts.push("additive".to_string());
    }
    if !connected_input_pins.contains("bond_tolerance") {
        parts.push(format!("tolerance: {:.2}", self.bond_tolerance));
    }
    if parts.is_empty() { None } else { Some(parts.join(", ")) }
}
```

## Implementation

### Step 1: Prerequisite — `clear_all_bonds()` on `AtomicStructure`

No method exists to clear all bonds at once. Add to `rust/src/crystolecule/atomic_structure/mod.rs` in the `impl AtomicStructure` block:

```rust
pub fn clear_all_bonds(&mut self) {
    for slot in self.atoms.iter_mut() {
        if let Some(atom) = slot {
            atom.bonds.clear();
        }
    }
    self.num_bonds = 0;
}
```

### Step 2: Node implementation — `rust/src/structure_designer/nodes/infer_bonds.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferBondsData {
    pub additive: bool,
    pub bond_tolerance: f64,
}

impl Default for InferBondsData {
    fn default() -> Self {
        Self {
            additive: false,
            bond_tolerance: 1.15,
        }
    }
}
```

**`NodeData` trait methods to implement:**

| Method | Notes |
|---|---|
| `eval()` | Core logic (see eval() pseudocode below) |
| `clone_box()` | `Box::new(self.clone())` |
| `get_text_properties()` | Expose `additive` as `Bool` (only if true), `bond_tolerance` as `Float` (only if non-default). See pattern above. |
| `set_text_properties()` | Set `additive` and `bond_tolerance` from `HashMap<String, TextValue>`. See `import_cif` lines 227-240 for exact pattern. |
| `get_subtitle()` | Show mode and tolerance when pins not connected. See pattern above. |
| `get_parameter_metadata()` | Mark `molecule` as `(true, None)` (required). `additive` and `bond_tolerance` are `(false, None)` (optional). |

**`get_node_type()` function:**

```rust
pub fn get_node_type() -> NodeType {
    NodeType {
        name: "infer_bonds".to_string(),
        description: "Infers covalent bonds between atoms based on interatomic distances \
                      and covalent radii, scaled by a tolerance multiplier.".to_string(),
        summary: Some("Infer bonds from distances".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter { id: None, name: "molecule".to_string(), data_type: DataType::Atomic },
            Parameter { id: None, name: "additive".to_string(), data_type: DataType::Bool },
            Parameter { id: None, name: "bond_tolerance".to_string(), data_type: DataType::Float },
        ],
        output_pins: OutputPinDefinition::single(DataType::Atomic),
        public: true,
        node_data_creator: || Box::new(InferBondsData::default()),
        node_data_saver: generic_node_data_saver::<InferBondsData>,
        node_data_loader: generic_node_data_loader::<InferBondsData>,
    }
}
```

### Step 3: Node registration

1. Add `pub mod infer_bonds;` in `rust/src/structure_designer/nodes/mod.rs`
2. Import and register in `rust/src/structure_designer/node_type_registry.rs`:
   ```rust
   use super::nodes::infer_bonds::get_node_type as infer_bonds_get_node_type;
   // in create_built_in_node_types():
   ret.add_node_type(infer_bonds_get_node_type());
   ```

### Step 4: API types and getter/setter — Rust side

Follow the `import_cif` pattern. Files to modify:

**`rust/src/api/structure_designer/structure_designer_api_types.rs`** — add API data type:

```rust
pub struct APIInferBondsData {
    pub additive: bool,
    pub bond_tolerance: f64,
}
```

**`rust/src/api/structure_designer/structure_designer_api.rs`** — add getter and setter functions:

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn get_infer_bonds_data(node_id: u64) -> Option<APIInferBondsData> {
    // Downcast node data to InferBondsData, return APIInferBondsData
    // Follow exact pattern of get_import_cif_data() at line 3402
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_infer_bonds_data(node_id: u64, data: APIInferBondsData) {
    // Create InferBondsData from API type, call set_node_network_data + refresh
    // Follow exact pattern of set_import_cif_data() at line 3432
}
```

### Step 5: Regenerate FFI bindings

```powershell
flutter_rust_bridge_codegen generate
```

This generates the Dart API types and function bindings in `lib/src/rust/`.

### Step 6: Flutter property editor — `lib/structure_designer/node_data/infer_bonds_editor.dart`

Create a `StatefulWidget` that displays:
- **Checkbox** for `additive` (label: "Additive", subtitle: "Keep existing bonds and add new ones")
- **Text field** for `bond_tolerance` (label: "Bond Tolerance", validated as positive number)

Follow the `import_cif_editor.dart` pattern:
- `_currentData()` returns current `APIInferBondsData` or default
- Each `_update*()` method creates a new `APIInferBondsData` with the changed field and calls `model.setInferBondsData(nodeId, data)`
- Use `CheckboxListTile` for the boolean (same as `import_cif_editor.dart` lines 232-251)
- Use `StringInput` for the tolerance (same as `import_cif_editor.dart` lines 255-267)

### Step 7: Register the editor in Flutter

**`lib/structure_designer/node_data/node_data_widget.dart`:**
1. Add import: `import 'package:flutter_cad/structure_designer/node_data/infer_bonds_editor.dart';`
2. Add case in `_buildNodeEditor()` switch:
   ```dart
   case 'infer_bonds':
     final inferBondsData = getInferBondsData(nodeId: selectedNode.id);
     return InferBondsEditor(
       nodeId: selectedNode.id,
       data: inferBondsData,
       model: model,
     );
   ```

**`lib/structure_designer/structure_designer_model.dart`:**
Add model methods (follow `setImportCifData` / `getImportCifData` pattern):
```dart
APIInferBondsData? getInferBondsData(BigInt nodeId) {
  return sd_api.getInferBondsData(nodeId: nodeId);
}

void setInferBondsData(BigInt nodeId, APIInferBondsData data) {
  sd_api.setInferBondsData(nodeId: nodeId, data: data);
  refreshFromKernel();
  notifyListeners();
}
```

### Existing function reused

```rust
// crystolecule/atomic_structure_utils.rs
pub fn auto_create_bonds_with_tolerance(structure: &mut AtomicStructure, tolerance_multiplier: f64)
```

## eval() Pseudocode

```rust
fn eval(...) -> EvalOutput {
    let input = network_evaluator.evaluate_arg_required(..., 0);
    if let NetworkResult::Error(_) = input {
        return EvalOutput::single(input);
    }

    let additive = match network_evaluator.evaluate_arg(..., 1) {
        NetworkResult::Bool(b) => b,
        _ => self.additive,
    };

    let bond_tolerance = match network_evaluator.evaluate_arg(..., 2) {
        NetworkResult::Float(f) => f,
        _ => self.bond_tolerance,
    };

    if let NetworkResult::Atomic(structure) = input {
        let mut result = structure.clone();
        if !additive {
            result.clear_all_bonds();
        }
        auto_create_bonds_with_tolerance(&mut result, bond_tolerance);
        EvalOutput::single(NetworkResult::Atomic(result))
    } else {
        EvalOutput::single(NetworkResult::Error("Expected atomic structure".to_string()))
    }
}
```

## Reference files

These files contain the patterns to follow. Read them before implementing:

| File | What to look at |
|---|---|
| `rust/src/structure_designer/nodes/import_cif.rs` | `eval()` parameter resolution pattern (lines 111-133), `get_text_properties` / `set_text_properties` (lines 183-241), `get_subtitle` (lines 172-181) |
| `rust/src/structure_designer/nodes/add_hydrogen.rs` | Simplest Atomic→Atomic node, `get_parameter_metadata`, `clone_box` |
| `rust/src/api/structure_designer/structure_designer_api.rs` | `get_import_cif_data` (line 3402) / `set_import_cif_data` (line 3432) as getter/setter pattern |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | `APIImportCIFData` (line 674) as API type pattern |
| `lib/structure_designer/node_data/import_cif_editor.dart` | Flutter editor with checkbox + text field, `_currentData` / `_update*` pattern |
| `lib/structure_designer/node_data/node_data_widget.dart` | Switch-case registration pattern (line 558 for import_cif) |
| `rust/src/crystolecule/atomic_structure/mod.rs` | Where to add `clear_all_bonds()` |

## Tests

Add `rust/tests/structure_designer/infer_bonds_test.rs`:

1. **Basic inference** — create a structure with two atoms at bonding distance, verify bond is created
2. **Tolerance effect** — same structure, low tolerance produces no bond, high tolerance produces bond
3. **Idempotency (non-additive)** — input with existing bonds produces same result as input without
4. **Additive mode** — input with existing bonds, verify existing bonds preserved and new ones added
5. **Additive does not duplicate** — input already fully bonded, additive adds nothing
6. **Non-additive clears bonds** — input with bonds, `additive: false` removes them before re-inferring
7. **No atoms** — empty structure in, empty structure out (no error)
8. **Text properties roundtrip** — `get_text_properties` / `set_text_properties` with default and non-default values
9. **Node snapshot** — `insta` snapshot for the node type registration

## Complexity Assessment

Low complexity. The node is a thin wrapper around `auto_create_bonds_with_tolerance` plus a `clear_all_bonds` helper. Follows established patterns (`import_cif` for data properties + UI, `add_hydrogen` for Atomic→Atomic node). No gadgets, no multi-output, no custom network interaction.
