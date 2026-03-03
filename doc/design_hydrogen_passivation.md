# Hydrogen Passivation Design

General-purpose hydrogen passivation for arbitrary `AtomicStructure` instances.

## Motivation

atomCAD has two existing H-passivation implementations, both lattice-fill specific:

1. **`lattice_fill/hydrogen_passivation.rs`** — Uses motif data to know where missing bond partners should be and places H in that exact direction. Only works with `PlacedAtomTracker` data from lattice filling.

2. **`lattice_fill/surface_reconstruction.rs`** — Specialized for diamond/silicon (100) 2x1 dimers.

Neither works on arbitrary structures (hand-built in atom_edit, imported from XYZ, or modified by deleting atoms). This design fills that gap with a general-purpose `add_hydrogens()` function that works on any `AtomicStructure`.

## Algorithm

### Overview

For each non-hydrogen atom, determine how many bonds it *should* have based on element and hybridization, compare with how many it *currently* has, and place hydrogens in the open directions at the correct bond length.

### Detailed Steps

```
add_hydrogens(structure, options) -> HydrogenPassivationResult

Step 1: Analysis (immutable scan)
    atom_ids = snapshot of all current atom IDs
    placements = []

    for atom_id in atom_ids:
        atom = structure.get_atom(atom_id)

        // Skip atoms that should not be passivated
        if atom.atomic_number <= 0: continue       // delete markers, parameters
        if atom.atomic_number == 1: continue        // don't passivate H itself
        if options.selected_only && !atom.is_selected(): continue
        if options.skip_already_passivated && atom.is_hydrogen_passivation(): continue

        hybridization = detect_hybridization(structure, atom_id, None)
        max_bonds = covalent_max_neighbors(atom.atomic_number, hybridization)
        current = count_active_neighbors(structure, atom_id)
        needed = max_bonds - current
        if needed <= 0: continue

        existing_dirs = gather_bond_directions(structure, atom)

        h_bond_len = lookup_xh_bond_length(atom.atomic_number)
        open_dirs = compute_open_directions(hybridization, existing_dirs, needed)
        positions = open_dirs.map(|d| atom.position + d * h_bond_len)

        placements.push((atom_id, positions))

Step 2: Mutation (add atoms and bonds)
    h_count = 0
    for (parent_id, positions) in placements:
        for pos in positions:
            h_id = structure.add_atom(1, pos)
            structure.get_atom_mut(h_id).set_hydrogen_passivation(true)
            structure.add_bond(parent_id, h_id, BOND_SINGLE)
            h_count += 1

    return HydrogenPassivationResult { hydrogens_added: h_count }
```

The two-step approach (analyze immutably, then mutate) follows the same borrow pattern used throughout the codebase.

### Geometry: `compute_open_directions()`

Reuses the guided placement geometry from `guided_placement.rs` in a deterministic, non-interactive mode. No FreeSphere or FreeRing — we always pick concrete positions.

#### sp3 (tetrahedral, 109.47 deg)

| Existing bonds | Open slots | Method |
|---|---|---|
| 0 | 4 | Standard tetrahedral orientation: `[1,1,1], [-1,-1,1], [-1,1,-1], [1,-1,-1]` normalized |
| 1 | 3 | Try `find_dihedral_reference()` first. If found: call `compute_sp3_case1_with_dihedral()`, take only the 3 Primary (staggered) positions. If not found: fall back to cone at 70.53 deg from -bond_dir with `arbitrary_perpendicular()` as dihedral reference, place 3 positions at 0 deg, 120 deg, 240 deg around the cone. |
| 2 | 2 | Reuse `sp3_case2` logic: two directions symmetric about the b1-b2 plane, each at 109.47 deg from both existing bonds |
| 3 | 1 | Reuse `sp3_case3` logic: `d4 = -normalize(b1 + b2 + b3)` |

#### sp2 (trigonal planar, 120 deg)

| Existing bonds | Open slots | Method |
|---|---|---|
| 0 | 3 | Equilateral triangle in the XY plane: directions at 0 deg, 120 deg, 240 deg from `+X` |
| 1 | 2 | Pick arbitrary plane normal perpendicular to bond. Two directions at +/-120 deg from existing bond in that plane |
| 2 | 1 | Reuse `sp2_case2` logic: `d3 = -normalize(b1 + b2)` |

#### sp1 (linear, 180 deg)

| Existing bonds | Open slots | Method |
|---|---|---|
| 0 | 2 | Arbitrary axis: `+X, -X` |
| 1 | 1 | `d2 = -bond_dir` (directly opposite) |

#### Dihedral-aware placement (sp3 case 1→3)

When a dihedral reference is available (the bonded neighbor has other neighbors), passivation reuses the same `find_dihedral_reference()` + `compute_sp3_case1_with_dihedral()` path as guided placement. This produces H positions that are staggered relative to the neighboring structure — the same result as if the user had manually placed atoms using guided placement and clicked on staggered (Primary) dots.

When no dihedral reference is available (the bonded neighbor has no other neighbors), we fall back to a deterministic arbitrary reference.

#### Arbitrary reference for deterministic fallback

For cases where guided placement would use interactive modes (FreeSphere, FreeRing), or where no dihedral reference exists, we pick a deterministic reference:

```rust
fn arbitrary_perpendicular(v: DVec3) -> DVec3 {
    let ref_axis = if v.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    v.cross(ref_axis).normalize()
}
```

This pattern already exists in the degenerate-case handling of `sp3_case3` and `sp3_case2`.

### Behavior on Non-Ideal Geometry

The algorithm handles distorted structures (non-ideal bond angles) gracefully because the underlying guided placement geometry adapts:

- **"Opposite centroid" cases** (sp3: 3->1, sp2: 2->1): Fully adaptive. Computes `d = -normalize(sum of existing dirs)`. No ideal angle is enforced — the new direction simply points away from existing bonds. Works correctly regardless of how distorted the input is.

- **sp3 case 2->2**: Enforces 109.47 deg from each existing bond individually. Due to the `mid = normalize(b1+b2)` symmetry, both existing bonds are treated equally. The angle *between the two new H's* floats to absorb geometric error. Has a `.clamp(-1.0, 1.0)` safety for extreme distortions.

- **Case 1->N** (cone-based): Uses hardcoded ideal cone angle from the single existing bond. Always self-consistent since there's only one reference direction.

- **Case 0** (bare atom): Arbitrary deterministic orientation. Always valid.

If the user wants physically optimal H positions on distorted structures, they can run energy minimization after passivation (already available as a one-click action).

### X-H Bond Length Table

A hardcoded table for common elements, with covalent radii sum as fallback. All values are rounded experimental bond lengths (within 0.005 A of reference data).

| Bond | Length (A) | Reference (A) | Reference source |
|---|---|---|---|
| C-H | 1.09 | 1.087 | Calculla bond lengths table; matches existing `C_H_BOND_LENGTH` |
| N-H | 1.01 | 1.012 | Calculla bond lengths table |
| O-H | 0.96 | 0.958 | Calculla bond lengths table |
| Si-H | 1.48 | 1.480 | Calculla bond lengths table |
| P-H | 1.42 | 1.420 | Calculla bond lengths table |
| S-H | 1.34 | 1.336 | Calculla bond lengths table |
| B-H | 1.19 | 1.19 | Wikipedia (diborane terminal B-H) |
| Ge-H | 1.53 | 1.527 | NIST CCCBDB (germane) |

Fallback: `covalent_radius(element) + covalent_radius(H)` from `atomic_constants.rs`.

### What NOT to Passivate

- Hydrogen atoms (atomic_number == 1)
- Delete markers (atomic_number == 0)
- Parameter elements (atomic_number < 0)
- Already-saturated atoms (current bonds >= max)
- Noble gases (max == 0)
- Atoms already flagged with `is_hydrogen_passivation()` (by default; controlled by option flag)

### Options

```rust
pub struct AddHydrogensOptions {
    /// Only passivate atoms that are currently selected.
    pub selected_only: bool,

    /// Skip atoms already flagged as hydrogen-passivated.
    /// Default: true.
    pub skip_already_passivated: bool,
}
```

### Result

```rust
pub struct AddHydrogensResult {
    /// Number of hydrogen atoms added.
    pub hydrogens_added: usize,
}
```

## File Location

New file: `rust/src/crystolecule/hydrogen_passivation.rs`

This lives at the module level alongside `guided_placement.rs`, NOT inside `lattice_fill/`. The lattice-fill-specific version remains separate since it uses motif/tracker data that the general algorithm does not need.

The guided placement geometry functions (`sp3_case2`, `sp3_case3`, `sp2_case2`, cone computation, `arbitrary_perpendicular`, etc.) will need to be made `pub(crate)` in `guided_placement.rs`, or the shared direction-computation logic extracted into helper functions callable from both modules.

Add `pub mod hydrogen_passivation;` to `crystolecule/mod.rs`.

## Integration: atom_edit Node Action

Hydrogen passivation in atom_edit is a one-shot operation (like minimize), not a persistent tool (like AddAtom).

### Architecture

- New file: `rust/src/structure_designer/nodes/atom_edit/hydrogen_passivation.rs`
- Follow the `minimization.rs` pattern: a single public function that takes `&mut StructureDesigner` and uses the three-step borrow pattern.

### Three-Step Implementation

```
pub fn add_hydrogen_atom_edit(
    structure_designer: &mut StructureDesigner,
    selected_only: bool,
) -> Result<String, String>

Gather (immutable borrows)
    - Get the active atom_edit_data (verify not in diff view)
    - Get the eval cache (provenance maps)
    - Get the result structure from the selected node
    - Call add_hydrogens() on a CLONE of the result structure
      with the appropriate options
    - Collect the new H atoms and their parent bonds as owned data
    - For each new H: determine parent atom provenance (base or diff)

Compute: No computation needed beyond the gather step

Mutate (mutable borrow on atom_edit_data)
    For each (parent_result_id, h_position) from the analysis:
        - Look up parent's provenance:
            - DiffAdded or DiffMatchedBase: parent is already in diff,
              use its diff_id directly
            - BasePassthrough: parent is NOT in diff yet,
              promote it (add_atom + set_anchor_position)
        - Add H atom to diff: diff.add_atom(1, h_position)
        - Flag H atom: set_hydrogen_passivation(true)
        - Create bond in diff: diff.add_bond_checked(parent_diff_id, h_id, BOND_SINGLE)
    Return summary message: "Added N hydrogen atoms"
```

Key implementation detail: the core algorithm runs on the *result* structure (base + diff applied), but all mutations go into the *diff*. Parent atoms that only exist in the base must be promoted to the diff (with anchor) before bonding. This is the same pattern used in `minimize_atom_edit` when `FreeAll` mode moves base atoms.

### API

New function in `rust/src/api/structure_designer/atom_edit_api.rs`:

```rust
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_add_hydrogen(selected_only: bool) -> String {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let result = atom_edit::add_hydrogen_atom_edit(
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

In `atom_edit/mod.rs`, add the module declaration alongside the others:
```rust
mod hydrogen_passivation;
```

Inside the existing `pub mod atom_edit { ... }` re-export block, add:
```rust
pub use super::hydrogen_passivation::*;
```

### UI: atom_edit Editor Panel

The hydrogen passivation UI lives in the Default tool tab of `atom_edit_editor.dart`, as a collapsible section below the existing "Energy Minimization" section. This follows the same `ExpansionTile`-in-a-`Card` pattern.

#### Layout

In the `build()` method of `_AtomEditEditorState`, after the Energy Minimization section (line ~163) and before the Transform section, add:

```dart
// Hydrogen Passivation section (Default tool only)
if (activeTool == APIAtomEditTool.default_)
  _buildCollapsibleAddHydrogenSection(),
```

#### Section Builder

```dart
Widget _buildCollapsibleAddHydrogenSection() {
  return Card(
    child: ExpansionTile(
      title: const Text('Hydrogen Passivation'),
      initiallyExpanded: false,
      children: [_buildAddHydrogenSectionContent()],
    ),
  );
}
```

#### Section Content: Two Buttons

```
┌─────────────────────────────────────────┐
│  Hydrogen Passivation               [v] │
│  ┌──────────────┐  ┌──────────────────┐ │
│  │ 🔘 Add H     │  │ 🎯 Add H        │ │
│  │    all        │  │    selected      │ │
│  └──────────────┘  └──────────────────┘ │
│  Added 12 hydrogen atoms                │
│                                         │
└─────────────────────────────────────────┘
```

Two `ElevatedButton.icon` buttons in a `Row`:

| Button | Label | Icon | `selected_only` | Enabled When |
|--------|-------|------|-----------------|-------------|
| Add H all | `'Add H\nall'` | `Icons.blur_on` | `false` | Always |
| Add H selected | `'Add H\nselected'` | `Icons.filter_center_focus` | `true` | `_stagedData?.hasSelectedAtoms` |

Each button calls:

```dart
widget.model.atomEditAddHydrogen(selectedOnly: false);  // or true
```

Below the buttons, a status text shows `widget.model.lastAddHydrogenMessage` (same pattern as `lastMinimizeMessage`). Red if starts with "Error", grey otherwise.

#### Model Method

In `structure_designer_model.dart`:

```dart
String _lastAddHydrogenMessage = '';
String get lastAddHydrogenMessage => _lastAddHydrogenMessage;

void atomEditAddHydrogen({required bool selectedOnly}) {
    _lastAddHydrogenMessage =
        atom_edit_api.atomEditAddHydrogen(selectedOnly: selectedOnly);
    refreshFromKernel();
    notifyListeners();
}
```

This follows the exact same pattern as `atomEditMinimize()`.

## Integration: Standalone add_hydrogen Node

A pure-functional node that takes an `AtomicStructure` input and outputs a hydrogen-passivated copy. Follows the `relax` node pattern.

### Node Definition

New file: `rust/src/structure_designer/nodes/add_hydrogen.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrogenPassivateData {}

impl NodeData for HydrogenPassivateData {
    fn eval(&self, ...) -> NetworkResult {
        let input_val = network_evaluator
            .evaluate_arg_required(network_stack, node_id, registry, context, 0);

        if let NetworkResult::Error(_) = input_val {
            return input_val;
        }

        if let NetworkResult::Atomic(mut structure) = input_val {
            let options = AddHydrogensOptions {
                selected_only: false,
                skip_already_passivated: true,
            };
            let result = add_hydrogens(&mut structure, &options);

            if network_stack.len() == 1 {
                context.selected_node_eval_cache = Some(Box::new(
                    HydrogenPassivateEvalCache {
                        message: format!("Added {} hydrogens", result.hydrogens_added),
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
        name: "add_hydrogen".to_string(),
        description: "Adds hydrogen atoms to satisfy valence requirements \
                       of all undersaturated atoms in the input structure."
            .to_string(),
        summary: Some("Add H to open bonds".to_string()),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![Parameter {
            id: None,
            name: "molecule".to_string(),
            data_type: DataType::Atomic,
        }],
        output_type: DataType::Atomic,
        public: true,
        node_data_creator: || Box::new(HydrogenPassivateData {}),
        node_data_saver: generic_node_data_saver::<HydrogenPassivateData>,
        node_data_loader: generic_node_data_loader::<HydrogenPassivateData>,
    }
}
```

### Registration

In `rust/src/structure_designer/nodes/mod.rs`:
```rust
pub mod add_hydrogen;
```

In `rust/src/structure_designer/node_type_registry.rs`:
```rust
use super::nodes::add_hydrogen::get_node_type as add_hydrogen_get_node_type;
// ...
ret.add_node_type(add_hydrogen_get_node_type());
```

### Behavior

- Takes one required `Atomic` input ("molecule")
- Applies `add_hydrogens()` with default options (passivate all non-hydrogen atoms)
- Outputs the passivated structure
- Stateless: no internal data, uses `generic_node_data_saver/loader`
- Stores result message in eval cache for potential UI display

## Reusable Building Blocks from `guided_placement.rs`

The following functions need to be accessible from the new `hydrogen_passivation.rs` module (currently private):

| Function | Current visibility | Needed change |
|---|---|---|
| `detect_hybridization()` | `pub` | No change |
| `covalent_max_neighbors()` | `pub` | No change |
| `count_active_neighbors()` | `fn` (private) | Make `pub(crate)` |
| `gather_bond_directions()` | N/A (inline in `compute_guided_placement()`) | Extract as new `pub(crate)` function |
| `sp3_case3()` | `fn` (private) | Make `pub(crate)` |
| `sp3_case2()` | `fn` (private) | Make `pub(crate)` |
| `sp2_case2()` | `fn` (private) | Make `pub(crate)` |
| `compute_sp3_case1_with_dihedral()` | `pub` | No change |
| `compute_sp3_case1_ring()` | `pub` | No change |
| `find_dihedral_reference()` | `pub` | No change |
| `TETRAHEDRAL_ANGLE` | `const` (private) | Make `pub(crate)` |
| `TRIGONAL_ANGLE` | `const` (private) | Make `pub(crate)` |

### `gather_bond_directions()`

Extracted from the inline code in `compute_guided_placement()` (lines 1214–1229). Returns normalized direction vectors from the anchor atom to each bonded neighbor.

```rust
pub(crate) fn gather_bond_directions(structure: &AtomicStructure, atom: &Atom) -> Vec<DVec3> {
    let anchor_pos = atom.position;
    atom.bonds
        .iter()
        .filter(|b| !b.is_delete_marker())
        .filter_map(|b| {
            structure.get_atom(b.other_atom_id()).map(|neighbor| {
                let dir = neighbor.position - anchor_pos;
                if dir.length_squared() < 1e-12 {
                    DVec3::X // degenerate: atoms at same position
                } else {
                    dir.normalize()
                }
            })
        })
        .collect()
}
```

After extraction, `compute_guided_placement()` should call this function instead of the inline block.

## Testing

Test file: `rust/tests/crystolecule/hydrogen_passivation_test.rs`

Register in `rust/tests/crystolecule.rs`:
```rust
#[path = "crystolecule/hydrogen_passivation_test.rs"]
mod hydrogen_passivation_test;
```

### Test Cases

**Basic passivation:**
- Single carbon with 0-3 existing bonds -> correct number of H's added
- Nitrogen sp3 with 0-2 bonds -> correct H count (max 3, not 4)
- Oxygen sp3 with 0-1 bonds -> correct H count (max 2)
- Halogen with 0 bonds -> 1 H added
- Hydrogen atom -> not passivated (skipped)
- Already-saturated atom -> no H's added

**Geometry verification:**
- sp3 carbon with 3 bonds: H placed at ~109.47 deg from each
- sp3 carbon with 2 bonds: 2 H's placed at ~109.47 deg from each existing bond
- sp2 carbon with 2 bonds: H placed at ~120 deg from each existing bond
- sp1 carbon with 1 bond: H placed at ~180 deg (opposite)

**Bond lengths:**
- C-H bond: ~1.09 A
- N-H bond: ~1.01 A
- O-H bond: ~0.96 A
- Si-H bond: ~1.48 A
- Unknown element: uses covalent radii sum

**Non-ideal geometry:**
- Carbon with bonds at 100 deg instead of 109.47 deg -> H's still placed in reasonable directions
- Carbon with bonds at 120 deg instead of 109.47 deg -> H's still placed, angles adapt

**Options:**
- `selected_only: true` -> only selected atoms get H's
- `skip_already_passivated: true` -> flagged atoms skipped

**Edge cases:**
- Empty structure -> 0 H's added
- Structure with only H atoms -> 0 H's added
- Structure with delete markers -> markers skipped
- Noble gas atoms -> skipped (max 0)

**Methane construction (integration test):**
- Start with bare carbon -> CH4 with tetrahedral geometry
- Verify 4 H's added, all C-H bonds are 1.09 A, all H-C-H angles ~109.47 deg

**Complex molecule:**
- Build ethylene (C=C with 2 H's already) -> add 2 more H's to complete
- Build water (O with 2 H's) -> no H's added (saturated)

## Implementation Plan

The implementation is split into four phases. Each phase is self-contained: it compiles, passes tests, and can be reviewed independently. Tests are included in the earliest phase where they become applicable.

### Phase 1: Core Algorithm + Tests

**Goal:** Implement `add_hydrogens()` in the crystolecule module with full test coverage.

**Visibility changes in `guided_placement.rs`:**
- Make `pub(crate)`: `count_active_neighbors`, `sp3_case2`, `sp3_case3`, `sp2_case2`, `TETRAHEDRAL_ANGLE`, `TRIGONAL_ANGLE`
- These are already `pub`: `detect_hybridization`, `covalent_max_neighbors`, `compute_sp3_case1_with_dihedral`, `compute_sp3_case1_ring`, `find_dihedral_reference`

**New files:**
- `rust/src/crystolecule/hydrogen_passivation.rs` — The core `add_hydrogens()` function, `compute_open_directions()`, X-H bond length table, `AddHydrogensOptions`, `AddHydrogensResult`
- `rust/tests/crystolecule/hydrogen_passivation_test.rs` — All test cases from the Testing section above

**Modified files:**
- `rust/src/crystolecule/mod.rs` — Add `pub mod hydrogen_passivation;`
- `rust/src/crystolecule/guided_placement.rs` — Change visibility of listed functions/constants to `pub(crate)`
- `rust/tests/crystolecule.rs` — Register `hydrogen_passivation_test` module

**Tests to include (all from the Testing section):**
- Basic passivation (H count per element/hybridization)
- Geometry verification (angles for sp3/sp2/sp1)
- Bond lengths (C-H, N-H, O-H, Si-H, fallback)
- Non-ideal geometry (distorted bond angles)
- Options (`selected_only`, `skip_already_passivated`)
- Edge cases (empty, H-only, delete markers, noble gases)
- Methane construction (bare carbon -> CH4)
- Complex molecules (ethylene, water)

**Verification:** `cd rust && cargo test hydrogen_passivation` — all tests pass. `cargo clippy` — no new warnings.

### Phase 2: atom_edit Integration (Rust) + Tests

**Goal:** Wire `add_hydrogens()` into the atom_edit node as a one-shot action, with API exposed to Flutter.

**New files:**
- `rust/src/structure_designer/nodes/atom_edit/hydrogen_passivation.rs` — `add_hydrogen_atom_edit()` function following the `minimization.rs` three-step borrow pattern

**Modified files:**
- `rust/src/structure_designer/nodes/atom_edit/mod.rs` — Add `mod hydrogen_passivation;` and re-export
- `rust/src/api/structure_designer/atom_edit_api.rs` — Add `atom_edit_add_hydrogen(selected_only: bool) -> String`

**No new test file** — the atom_edit integration is a thin wrapper over the core algorithm (already tested in Phase 1). The API function follows the same pattern as `atom_edit_minimize` which is also not unit-tested (it's an API wrapper). Integration testing happens via the Flutter UI in Phase 3.

**Verification:** `cd rust && cargo build` — compiles. `cargo test` — existing tests still pass. `cargo clippy` — no new warnings.

### Phase 3: atom_edit UI (Flutter)

**Goal:** Add the Hydrogen Passivation section to the atom_edit editor panel and regenerate FRB bindings.

**Regenerate bindings:**
- Run `flutter_rust_bridge_codegen generate` (picks up the new `atom_edit_add_hydrogen` API function)

**Modified files:**
- `lib/structure_designer/structure_designer_model.dart` — Add `_lastAddHydrogenMessage`, `lastAddHydrogenMessage` getter, `atomEditAddHydrogen()` method
- `lib/structure_designer/node_data/atom_edit_editor.dart` — Add `_buildCollapsibleAddHydrogenSection()`, `_buildAddHydrogenSectionContent()`, wire into `build()` after the Energy Minimization section

**Verification:** `flutter analyze` — no new issues. Manual testing: open atom_edit node, verify Hydrogen Passivation section appears in Default tool tab, test both buttons on a simple structure.

### Phase 4: Standalone add_hydrogen Node

**Goal:** Create the `add_hydrogen` node as an independent Atomic-in/Atomic-out node.

**New files:**
- `rust/src/structure_designer/nodes/add_hydrogen.rs` — `AddHydrogenData`, `NodeData` impl, `get_node_type()`

**Modified files:**
- `rust/src/structure_designer/nodes/mod.rs` — Add `pub mod add_hydrogen;`
- `rust/src/structure_designer/node_type_registry.rs` — Import and register `add_hydrogen_get_node_type`

**Tests:**
- Add a snapshot test for the new node type in the existing node snapshot test infrastructure (if the project uses `cargo insta` for node snapshots)
- Add a basic evaluation test: create a network with a single carbon atom wired into an `add_hydrogen` node, verify output has 5 atoms (1 C + 4 H) and 4 bonds

**Verification:** `cd rust && cargo build && cargo test` — compiles, all tests pass. `cargo clippy` — no new warnings. The node appears in the node palette under AtomicStructure category (verify after `flutter_rust_bridge_codegen generate` if needed for the palette to pick it up).
