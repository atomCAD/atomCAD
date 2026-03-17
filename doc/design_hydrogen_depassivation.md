# Hydrogen Depassivation Design

General-purpose hydrogen removal for arbitrary `AtomicStructure` instances — the inverse of hydrogen passivation.

## Motivation

Hydrogen passivation (`add_hydrogens()`) is already implemented. The inverse operation — removing hydrogens — is needed for:

1. **Iterative editing:** After passivation, the user may want to modify the structure (add/remove atoms, change geometry) and re-passivate. Removing old hydrogens first avoids stacking artifacts.
2. **Import cleanup:** Structures imported from XYZ or MOL files often include hydrogen atoms the user wants to strip before working with the bare framework.
3. **Node network workflows:** A `remove_hydrogen` → transform → `add_hydrogen` pipeline lets users modify bare structures non-destructively.

The algorithm is trivially simple compared to passivation: find hydrogen atoms, delete them and their bonds. No hybridization detection, geometry computation, or bond length lookup is needed.

## Algorithm

### Overview

Scan all atoms, identify hydrogen atoms that match the removal criteria, then delete them. Two-phase approach (analyze immutably, then mutate) follows the same borrow pattern as `add_hydrogens()`.

### Detailed Steps

```
remove_hydrogens(structure, options) -> RemoveHydrogensResult

Step 1: Analysis (immutable scan)
    atom_ids = snapshot of all current atom IDs
    ids_to_remove = []

    for atom_id in atom_ids:
        atom = structure.get_atom(atom_id)

        // Only consider hydrogen atoms
        if atom.atomic_number != 1: continue

        // Selection filter
        if options.selected_only:
            // Remove this H if:
            //   (a) the H atom itself is selected, OR
            //   (b) any neighbor (bonded atom) of the H is selected
            is_self_selected = atom.is_selected()
            has_selected_neighbor = atom.bonds.any(|b| {
                structure.get_atom(b.other_atom_id())
                    .map_or(false, |n| n.is_selected())
            })
            if !is_self_selected && !has_selected_neighbor: continue

        ids_to_remove.push(atom_id)

Step 2: Mutation (remove atoms)
    for atom_id in ids_to_remove:
        structure.delete_atom(atom_id)
        // delete_atom() also removes bonds from neighboring atoms

    return RemoveHydrogensResult { hydrogens_removed: ids_to_remove.len() }
```

### What to Remove

- Only atoms with `atomic_number == 1` (hydrogen)
- All hydrogen atoms regardless of the `is_hydrogen_passivation()` flag — manually placed H, passivation-added H, and imported H are all removed

### What NOT to Remove

- Non-hydrogen atoms (any `atomic_number != 1`)
- Delete markers (`atomic_number == 0`)
- Parameter elements (`atomic_number < 0`)
- Hydrogen atoms that don't match the selection filter (when `selected_only` is true)

### Options

```rust
pub struct RemoveHydrogensOptions {
    /// Only remove H atoms that are themselves selected or bonded to a selected atom.
    pub selected_only: bool,
}
```

### Result

```rust
pub struct RemoveHydrogensResult {
    /// Number of hydrogen atoms removed.
    pub hydrogens_removed: usize,
}
```

## File Location

Same file as passivation: `rust/src/crystolecule/hydrogen_passivation.rs`

The function `remove_hydrogens()` lives alongside `add_hydrogens()` since they are inverse operations sharing the same module. No new module declaration needed — the file already exists and is registered in `crystolecule/mod.rs`.

## Integration: atom_edit Node Action

Hydrogen depassivation in atom_edit is a one-shot operation (like passivation and minimization), not a persistent tool.

### Architecture

New function in the existing file: `rust/src/structure_designer/nodes/atom_edit/hydrogen_passivation.rs`

Follows the same three-step borrow pattern as `add_hydrogen_atom_edit()`.

### Three-Step Implementation

```
pub fn remove_hydrogen_atom_edit(
    structure_designer: &mut StructureDesigner,
    selected_only: bool,
) -> Result<String, String>

Phase 1: Gather (immutable borrows)
    - Get the active atom_edit_data (verify not in diff view)
    - Get the eval cache (provenance maps)
    - Get the result structure from the selected node
    - Run remove_hydrogens() on a CLONE with the appropriate options
      to identify which result-atom IDs are hydrogen atoms to remove
    - For each H atom to remove:
        - Look up provenance to determine AtomSource
        - Collect removal instructions as owned data:
            - DiffAdded(diff_id): remove from diff entirely
            - DiffMatchedBase { diff_id, .. }: remove from diff + add delete marker
            - BasePassthrough(base_id): need to add delete marker at atom position

Phase 2: No additional computation needed

Phase 3: Mutate (mutable borrow on atom_edit_data)
    For each H atom removal instruction:
        - DiffAdded: call atom_edit_data.remove_from_diff(diff_id)
        - DiffMatchedBase: call atom_edit_data.replace_with_delete_marker(diff_id, position)
        - BasePassthrough: call atom_edit_data.mark_for_deletion(position)
    Clear selection (hydrogen atoms may have been selected)
    Return summary message: "Removed N hydrogen atoms"
```

This mirrors the existing `delete_selected_in_result_view()` pattern from `operations.rs`, but targets hydrogen atoms specifically instead of selected atoms.

### Gathering Removal Targets

Rather than running the full `remove_hydrogens()` algorithm on a clone (which would do unnecessary work deleting atoms from a structure we throw away), the gather phase can directly scan the result structure for hydrogen atoms and apply the selection filter:

```rust
// In Phase 1 (immutable):
let mut h_atoms_to_remove: Vec<HRemovalInfo> = Vec::new();

for &atom_id in result_structure.atom_ids().copied().collect::<Vec<_>>().iter() {
    let atom = match result_structure.get_atom(atom_id) {
        Some(a) => a,
        None => continue,
    };
    if atom.atomic_number != 1 { continue; }

    if selected_only {
        let self_selected = atom.is_selected();
        let neighbor_selected = atom.bonds.iter()
            .filter(|b| !b.is_delete_marker())
            .any(|b| {
                result_structure.get_atom(b.other_atom_id())
                    .map_or(false, |n| n.is_selected())
            });
        if !self_selected && !neighbor_selected { continue; }
    }

    let source = eval_cache.provenance.sources.get(&atom_id).cloned();
    h_atoms_to_remove.push(HRemovalInfo {
        result_id: atom_id,
        position: atom.position,
        source,
    });
}
```

### API

New function in `rust/src/api/structure_designer/atom_edit_api.rs`:

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_remove_hydrogen(selected_only: bool) -> String {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = atom_edit::remove_hydrogen_atom_edit(
                    &mut cad_instance.structure_designer,
                    selected_only,
                );
                refresh_structure_designer_auto(cad_instance);
                match result {
                    Ok(msg) => msg,
                    Err(e) => format!("Error: {}", e),
                }
            },
            "Error: no active instance".to_string(),
        )
    }
}
```

### Module Registration

No new module file needed. The function is added to the existing `hydrogen_passivation.rs` file in `atom_edit/`. The existing `mod hydrogen_passivation;` declaration and re-export in `atom_edit/mod.rs` already cover it.

### UI: atom_edit Editor Panel

The depassivation buttons live in the **same** "Hydrogen Passivation" collapsible section as the existing passivation buttons. The section title stays "Hydrogen Passivation" — it covers both adding and removing hydrogen.

#### Updated Layout

```
┌─────────────────────────────────────────┐
│  Hydrogen Passivation               [v] │
│  ┌──────────────┐  ┌──────────────────┐ │
│  │ Add H        │  │ Add H            │ │
│  │ all          │  │ selected (Ctrl+H)│ │
│  └──────────────┘  └──────────────────┘ │
│  Added 12 hydrogen atoms                │
│                                         │
│  ┌──────────────┐  ┌──────────────────┐ │
│  │ Remove H     │  │ Remove H         │ │
│  │ all          │  │ selected (^⇧H)   │ │
│  └──────────────┘  └──────────────────┘ │
│  Removed 8 hydrogen atoms               │
│                                         │
└─────────────────────────────────────────┘
```

Two new `ElevatedButton.icon` buttons in a second `Row`, below the existing passivation buttons:

| Button | Label | Icon | `selected_only` | Enabled When |
|--------|-------|------|-----------------|-------------|
| Remove H all | `'Remove H\nall'` | `Icons.blur_off` | `false` | Always |
| Remove H selected | `'Remove H\nselected (Ctrl+Shift+H)'` | `Icons.filter_center_focus_outlined` | `true` | `_stagedData?.hasSelectedAtoms` |

Each button calls:

```dart
widget.model.atomEditRemoveHydrogen(selectedOnly: false);  // or true
```

Below the remove buttons, a separate status text shows `widget.model.lastRemoveHydrogenMessage`. Red if starts with "Error", grey otherwise.

The outlined icon variants (`blur_off`, `filter_center_focus_outlined`) visually distinguish removal from addition while keeping the icon metaphor consistent.

#### Model Method

In `structure_designer_model.dart`:

```dart
String _lastRemoveHydrogenMessage = '';
String get lastRemoveHydrogenMessage => _lastRemoveHydrogenMessage;

void atomEditRemoveHydrogen({required bool selectedOnly}) {
    _lastRemoveHydrogenMessage =
        atom_edit_api.atomEditRemoveHydrogen(selectedOnly: selectedOnly);
    refreshFromKernel();
    notifyListeners();
}
```

This follows the exact same pattern as `atomEditAddHydrogen()`.

### Keyboard Shortcut: Ctrl+Shift+H

In `structure_designer_viewport.dart`, add a new shortcut handler directly after the existing Ctrl+H block:

```dart
// Ctrl+Shift+H: Remove hydrogen from selected atoms (Default tool only)
if (event is KeyDownEvent &&
    HardwareKeyboard.instance.isControlPressed &&
    HardwareKeyboard.instance.isShiftPressed &&
    event.logicalKey == LogicalKeyboardKey.keyH) {
  final tool = atom_edit_api.getActiveAtomEditTool();
  if (tool == APIAtomEditTool.default_) {
    widget.graphModel.atomEditRemoveHydrogen(selectedOnly: true);
    renderingNeeded();
    return KeyEventResult.handled;
  }
}
```

**Important ordering:** The Ctrl+Shift+H handler must come **before** the Ctrl+H handler in the event dispatch chain, because Ctrl+Shift+H also matches `isControlPressed`. If Ctrl+H is checked first, it would consume the event before the shift check.

The shortcut mirrors the passivation shortcut (Ctrl+H) with the Shift modifier indicating the reverse operation.

## Integration: Standalone remove_hydrogen Node

A pure-functional node that takes an `AtomicStructure` input and outputs a copy with all hydrogen atoms removed. Follows the `add_hydrogen` node pattern.

### Node Definition

New file: `rust/src/structure_designer/nodes/remove_hydrogen.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrogenDepassivateData {}

impl NodeData for HydrogenDepassivateData {
    fn eval(&self, ...) -> NetworkResult {
        let input_val = network_evaluator
            .evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = input_val {
            return input_val;
        }

        if let NetworkResult::Atomic(mut structure) = input_val {
            let options = RemoveHydrogensOptions {
                selected_only: false,
            };
            let result = remove_hydrogens(&mut structure, &options);

            if network_stack.len() == 1 {
                context.selected_node_eval_cache = Some(Box::new(
                    HydrogenDepassivateEvalCache {
                        message: format!("Removed {} hydrogens", result.hydrogens_removed),
                    }
                ));
            }

            NetworkResult::Atomic(structure)
        } else {
            NetworkResult::Atomic(AtomicStructure::new())
        }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_parameter_metadata(&self) -> HashMap<String, (bool, Option<String>)> {
        // molecule input is required
        HashMap::from([("molecule".to_string(), (true, None))])
    }
}

pub fn get_node_type() -> NodeType {
    NodeType {
        name: "remove_hydrogen".to_string(),
        description: "Removes all hydrogen atoms from the input structure."
            .to_string(),
        summary: Some("Strip all H atoms".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::Atomic,
        }],
        output_type: DataType::Atomic,
        public: true,
        node_data_creator: || Box::new(HydrogenDepassivateData {}),
        node_data_saver: generic_node_data_saver::<HydrogenDepassivateData>,
        node_data_loader: generic_node_data_loader::<HydrogenDepassivateData>,
    }
}
```

### Registration

In `rust/src/structure_designer/nodes/mod.rs`:
```rust
pub mod remove_hydrogen;
```

In `rust/src/structure_designer/node_type_registry.rs`:
```rust
use super::nodes::remove_hydrogen::get_node_type as remove_hydrogen_get_node_type;
// ...
ret.add_node_type(remove_hydrogen_get_node_type());
```

### Behavior

- Takes one required `Atomic` input ("molecule")
- Removes all hydrogen atoms (atomic_number == 1) — `selected_only: false`
- Outputs the stripped structure
- Stateless: no internal data, uses `generic_node_data_saver/loader`
- Stores result message in eval cache for potential UI display
- Complementary to `add_hydrogen`: `remove_hydrogen` → `add_hydrogen` is a round-trip that first strips then re-passivates

## Testing

Test cases are added to the existing test file: `rust/tests/crystolecule/hydrogen_passivation_test.rs`

The `remove_hydrogen` node test goes in a new file: `rust/tests/structure_designer/remove_hydrogen_node_test.rs`

### Test Cases for `remove_hydrogens()`

**Basic removal:**
- Structure with 1 C + 4 H (methane) -> remove all H -> 1 C remaining, 0 bonds
- Structure with 2 C + 6 H (ethane) -> remove all H -> 2 C remaining, 1 C-C bond
- Structure with only H atoms -> remove all -> empty structure (0 atoms)
- Structure with no H atoms -> remove 0, structure unchanged
- Empty structure -> remove 0

**Selection filter (`selected_only: true`):**
- Select one carbon in ethane -> remove only its 3 H's, other C's H's remain
- Select a hydrogen directly -> only that H removed
- Select nothing -> 0 removed
- Select all atoms -> all H removed (same as `selected_only: false`)

**Bond cleanup:**
- After removing H from methane, the C atom has 0 bonds (not dangling bond references)
- After removing H from ethane, C-C bond remains intact

**Mixed structures:**
- Structure with C, N, O, H atoms -> only H atoms removed, others untouched
- Structure with delete markers (atomic_number 0) -> markers not removed

**Round-trip with passivation:**
- Bare carbon -> `add_hydrogens()` -> 4 H added -> `remove_hydrogens()` -> back to 1 C, 0 bonds
- Ethylene (C=C + 2H each) -> strip H -> 2 C with double bond -> re-passivate -> same as original

### Test Cases for `remove_hydrogen` node

- Snapshot test for node type registration
- Basic evaluation: methane input -> output has 1 C, 0 H, 0 bonds
- Empty input -> empty output

## Implementation Plan

The implementation is split into three phases. Each phase is self-contained: it compiles, passes tests, and can be reviewed independently.

### Phase 1: Core Algorithm + Tests

**Goal:** Implement `remove_hydrogens()` in the crystolecule module with full test coverage.

**Modified files:**
- `rust/src/crystolecule/hydrogen_passivation.rs` — Add `RemoveHydrogensOptions`, `RemoveHydrogensResult`, and `remove_hydrogens()` function
- `rust/tests/crystolecule/hydrogen_passivation_test.rs` — Add all depassivation test cases from the Testing section

**Verification:** `cd rust && cargo test hydrogen_passivation` — all tests pass. `cargo clippy` — no new warnings.

### Phase 2: atom_edit Integration (Rust) + API + UI + Keyboard Shortcut

**Goal:** Wire `remove_hydrogens()` into the atom_edit node, expose the API, add Flutter UI buttons and keyboard shortcut.

**Modified files (Rust):**
- `rust/src/structure_designer/nodes/atom_edit/hydrogen_passivation.rs` — Add `remove_hydrogen_atom_edit()` function
- `rust/src/api/structure_designer/atom_edit_api.rs` — Add `atom_edit_remove_hydrogen(selected_only: bool) -> String`

**Regenerate bindings:**
- Run `flutter_rust_bridge_codegen generate`

**Modified files (Flutter):**
- `lib/structure_designer/structure_designer_model.dart` — Add `_lastRemoveHydrogenMessage`, `lastRemoveHydrogenMessage` getter, `atomEditRemoveHydrogen()` method
- `lib/structure_designer/node_data/atom_edit_editor.dart` — Add remove H buttons and status text to `_buildAddHydrogenSectionContent()`
- `lib/structure_designer/structure_designer_viewport.dart` — Add Ctrl+Shift+H shortcut handler (before Ctrl+H handler)

**Verification:** `cd rust && cargo build` — compiles. `cargo test` — existing tests pass. `cargo clippy` — no new warnings. `flutter analyze` — no new issues.

### Phase 3: Standalone remove_hydrogen Node

**Goal:** Create the `remove_hydrogen` node as an independent Atomic-in/Atomic-out node.

**New files:**
- `rust/src/structure_designer/nodes/remove_hydrogen.rs` — `HydrogenDepassivateData`, `NodeData` impl, `get_node_type()`

**Modified files:**
- `rust/src/structure_designer/nodes/mod.rs` — Add `pub mod remove_hydrogen;`
- `rust/src/structure_designer/node_type_registry.rs` — Import and register `remove_hydrogen_get_node_type`

**Tests:**
- `rust/tests/structure_designer/remove_hydrogen_node_test.rs` — Snapshot test + basic evaluation test
- Register in `rust/tests/structure_designer.rs`

**Verification:** `cd rust && cargo build && cargo test` — compiles, all tests pass. `cargo clippy` — no new warnings. The node appears in the node palette under AtomicStructure category.
