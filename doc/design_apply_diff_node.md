# Implementation Plan: `apply_diff` Node, Diff-Aware Transforms, Lattice Atomic Transforms

This document is an implementation plan with three sequential phases. Each phase includes
its own tests and should be fully working (cargo test passes) before moving to the next.

## Motivation

The `atom_edit` node can output a raw diff structure (`output_diff = true`) instead of the
applied result. This makes diffs first-class values in the node network. To complete this
workflow we need:

1. **Diff-aware transforms** so `atom_move`, `atom_rot`, and `atom_trans` work correctly on
   diff structures (moving the defect to a new location before application).
2. An **`apply_diff` node** that takes a base structure and a diff and produces the result.
3. **Lattice-coordinate atomic transforms** (`atom_lattice_move`, `atom_lattice_rot`) so
   users can place defects using integer lattice coordinates and discrete symmetry rotations.

### Target Pipeline

```
                                                         +---------------+
+-----------+                                            |               |
| unit_cell +---> atom_fill ---> [Diamond] ------------->|  apply_diff   |---> [Diamond with
| + motif   |                                            |               |      relocated defect]
+-----------+                                            +-------+-------+
      |                                                          |
      |  +--------------------+     +------------------+         |
      |  | atom_edit          |     |                  |         |
      +->| (output_diff=ON)   +---->| atom_lattice_move+-------->+
         | [defect patch]     |     | t: (3, 4, 2)    |
         +--------------------+     +------------------+
```

---

## Background: How Diffs Work

An `AtomicStructure` with `is_diff = true` encodes three kinds of changes:

| Diff Atom Type     | `atomic_number` | `anchor_position` | Semantics                              |
|--------------------|-----------------|--------------------|-----------------------------------------|
| **Addition**       | >= 1            | None               | New atom placed at `atom.position`     |
| **Deletion**       | 0 (marker)      | Optional           | Removes base atom matched by position  |
| **Modification**   | >= 1            | Some(base_pos)     | Replaces/moves the matched base atom   |

`apply_diff(base, diff, tolerance)` in `crystolecule/atomic_structure_diff.rs` uses greedy
nearest-first matching: each diff atom's match position (`anchor.unwrap_or(atom.position)`)
is compared against base atom positions within a tolerance. Matched atoms get
replaced/deleted; unmatched diff atoms get added; unmatched base atoms pass through.

Key types returned by `apply_diff()`:

- `DiffApplicationResult` — contains `result`, `provenance`, and `stats`
- `DiffProvenance` — maps every result atom back to its origin (base passthrough, matched, or added)
- `DiffStats` — counts of atoms added/deleted/modified, bonds added/deleted, orphaned entries

---

## Phase 1: Fix `AtomicStructure::transform()` to Include Anchors

### Problem

`AtomicStructure::transform()` only transforms atom positions. It does **not** transform
`anchor_positions`. This is correct for `transform_atom()` (single-atom edits within a diff),
but wrong for `transform()` (rigid-body transform of the whole structure).

If a diff is moved without updating anchors, the anchors still point at the old location.
When `apply_diff` later runs, it matches anchors against base atoms at the **old** position —
the diff effectively stays in place despite being "moved."

### Call Site Analysis

All callers of `transform()` move the **entire structure as a rigid body**:

| Call site              | Context                                |
|------------------------|----------------------------------------|
| `atom_move.rs:95`      | Translate entire structure              |
| `atom_rot.rs:149-151`  | Rotate entire structure around pivot    |
| `atom_trans.rs:124,129` | Re-frame entire structure              |

The single-atom method `transform_atom()` is used only by:
- `transform()` itself (iterating all atoms)
- `edit_atom/commands/transform_command.rs` (dragging selected atoms within the diff)

In the drag case, anchors correctly stay put — the anchor records "where this atom came
from in the base." So the separation is clean:

- **`transform()`** = rigid-body transform of whole structure -> anchors must move too
- **`transform_atom()`** = move one atom within the structure -> anchors stay

### Step 1.1: Modify `AtomicStructure::transform()`

**File:** `rust/src/crystolecule/atomic_structure/mod.rs`

Append anchor transformation to `transform()`:

```rust
pub fn transform(&mut self, rotation: &DQuat, translation: &DVec3) {
    let atom_ids: Vec<u32> = self.atom_ids().cloned().collect();
    for atom_id in atom_ids {
        self.transform_atom(atom_id, rotation, translation);
    }
    // Also transform anchor positions (for diff structures).
    // When anchor_positions is empty (non-diff structures) this is a no-op.
    for pos in self.anchor_positions.values_mut() {
        *pos = rotation.mul_vec3(*pos) + *translation;
    }
}
```

No changes needed in `atom_move`, `atom_rot`, or `atom_trans` — they already call
`transform()` and will get correct behavior automatically.

### Step 1.2: Tests

**File:** `rust/tests/crystolecule/atomic_structure_test.rs` (or new file if needed)

**Test: `transform_moves_anchor_positions`**
1. Create an `AtomicStructure` with `is_diff = true`
2. Add an atom at (1, 0, 0) with anchor at (0, 0, 0)
3. Call `transform(&DQuat::IDENTITY, &DVec3::new(5.0, 0.0, 0.0))`
4. Assert atom position is (6, 0, 0)
5. Assert anchor position is (5, 0, 0)

**Test: `transform_rotates_anchor_positions`**
1. Create a diff structure with atom at (2, 0, 0) and anchor at (1, 0, 0)
2. Call `transform(&DQuat::from_rotation_z(PI/2), &DVec3::ZERO)`
3. Assert atom position is approximately (0, 2, 0)
4. Assert anchor position is approximately (0, 1, 0)

**Test: `transform_atom_does_not_move_anchors`**
1. Create a diff structure with atom at (1, 0, 0) and anchor at (0, 0, 0)
2. Call `transform_atom(atom_id, &DQuat::IDENTITY, &DVec3::new(5.0, 0.0, 0.0))`
3. Assert atom position is (6, 0, 0)
4. Assert anchor position is still (0, 0, 0) — unchanged

**Test: `transform_with_no_anchors_is_noop`**
1. Create a normal (non-diff) structure with atoms
2. Call `transform(...)` — verify it works exactly as before (no crash, correct positions)

### Verification

```bash
cd rust && cargo test atomic_structure_test && cargo clippy
```

---

## Phase 2: `apply_diff` Node

### Node Specification

| Property      | Value                |
|---------------|----------------------|
| **Name**      | `apply_diff`         |
| **Category**  | `AtomicStructure`    |
| **Output**    | `DataType::Atomic`   |
| **Public**    | `true`               |

### Input Pins

| #  | Name        | Type    | Required | Description                                    |
|----|-------------|---------|----------|------------------------------------------------|
| 0  | `base`      | Atomic  | Yes      | The base structure to apply the diff onto      |
| 1  | `diff`      | Atomic  | Yes      | The diff structure (`is_diff = true`)          |
| 2  | `tolerance` | Float   | No       | Matching tolerance in Angstroms (default 0.1)  |

### Text Properties

| Name             | Type  | Default | Description                                       |
|------------------|-------|---------|---------------------------------------------------|
| `tolerance`      | Float | 0.1     | Positional matching tolerance in Angstroms         |
| `error_on_stale` | Bool  | false   | Error if orphaned atoms/bonds/delete-markers exist |

### Data Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyDiffData {
    pub tolerance: f64,
    pub error_on_stale: bool,
}
```

### Evaluation Logic

```
1. Evaluate pin 0 -> base (required, must be Atomic)
2. Evaluate pin 1 -> diff (required, must be Atomic)
3. Validate: if diff.is_diff() == false, return NetworkResult::Error
   ("apply_diff: input on 'diff' pin is not a diff structure (is_diff = false)")
4. Get tolerance from pin 2 or property (default 0.1)
5. Call apply_diff(&base, &diff, tolerance) -> DiffApplicationResult
6. If error_on_stale is true:
     Check stats for orphaned_tracked_atoms, unmatched_delete_markers, orphaned_bonds
     If any > 0: return NetworkResult::Error with diagnostic message
7. Return NetworkResult::Atomic(result)
```

### Subtitle

Display a compact summary from `DiffStats`: e.g. `"+3 -2 ~1"` (3 added, 2 deleted,
1 modified). This requires caching the stats in an eval cache struct.

### Step 2.1: Create node file

**File:** `rust/src/structure_designer/nodes/apply_diff.rs`

Implement `ApplyDiffData` with the `NodeData` trait. Follow the pattern of other atomic
nodes (e.g. `atom_move.rs`, `atom_union.rs`).

### Step 2.2: Register node

- Add `pub mod apply_diff;` to `rust/src/structure_designer/nodes/mod.rs`
- Add `apply_diff::get_node_type()` to the node list in
  `rust/src/structure_designer/node_type_registry.rs` → `create_built_in_node_types()`

### Step 2.3: Tests

**File:** `rust/tests/structure_designer/` (new test file or extend existing)

**Test: `apply_diff_node_basic`**
Build a network programmatically:
1. Create a base AtomicStructure (e.g. 4 carbon atoms in a line)
2. Create a diff that deletes one atom and adds a new one
3. Wire: [base] -> apply_diff pin 0, [diff] -> apply_diff pin 1
4. Evaluate the network
5. Assert the result has correct atom count (4 - 1 + 1 = 4) and correct positions

**Test: `apply_diff_node_with_moved_diff`** (integration with Phase 1)
Build a network:
1. Create a base structure (e.g. atoms at grid positions)
2. Create a diff with a delete marker at (0,0,0)
3. Wire: [diff] -> atom_move(5,0,0) -> apply_diff pin 1
4. Wire: [base] -> apply_diff pin 0
5. Evaluate — verify the atom at (5,0,0) is deleted, not the one at (0,0,0)

**Test: `apply_diff_node_rejects_non_diff`**
1. Wire a normal AtomicStructure (is_diff = false) into the diff pin
2. Evaluate — assert `NetworkResult::Error` with message about is_diff

**Test: `apply_diff_node_error_on_stale`**
1. Create diff with anchor pointing at nonexistent base position
2. Evaluate with `error_on_stale = true`
3. Assert `NetworkResult::Error` is returned

**Test: `apply_diff_node_snapshot`**
Add to `rust/tests/structure_designer/node_snapshot_test.rs`:
- Snapshot test for `apply_diff` node type (via `cargo insta`)

### Verification

```bash
cd rust && cargo test && cargo clippy
cargo insta review  # if snapshots changed
```

---

## Phase 3: Lattice-Coordinate Transforms for Atomic Structures

### Motivation

With Phases 1 and 2, users can move diffs with `atom_move` and `atom_rot` — but those nodes
require world-space coordinates (Angstroms, radians). For crystal defect placement, users
want to specify positions in **integer lattice coordinates** (e.g. "place defect at unit cell
(3, 4, 2)") and use **discrete symmetry rotations** (e.g. "rotate by step 1 around symmetry
axis 0").

The existing `lattice_move` and `lattice_rot` nodes do exactly this — but only for Geometry,
not for Atomic structures. Duplicating them as separate files would mean ~700 lines of
near-identical code (especially the gadgets).

### Solution: Two Node Types per File, Shared `NodeData`

Each file (`lattice_move.rs`, `lattice_rot.rs`) defines **two** node types using the **same**
`NodeData` struct, distinguished by a boolean flag.

#### Data Structure Changes

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeMoveData {
    #[serde(with = "ivec3_serializer")]
    pub translation: IVec3,
    #[serde(default = "default_lattice_subdivision")]
    pub lattice_subdivision: i32,
    #[serde(default)]  // false for backward compat with existing lattice_move nodes
    pub is_atomic_mode: bool,
}
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatticeRotData {
    pub axis_index: Option<i32>,
    pub step: i32,
    #[serde(with = "ivec3_serializer")]
    pub pivot_point: IVec3,
    #[serde(default)]  // false for backward compat with existing lattice_rot nodes
    pub is_atomic_mode: bool,
}
```

`#[serde(default)]` ensures old .cnnd files (which lack this field) deserialize as
`is_atomic_mode: false` — fully backward compatible.

#### Pin Layout

Shared pins keep identical indices. The `unit_cell` pin is appended last (atom mode only):

**lattice_move / atom_lattice_move:**

| Pin | lattice_move          | atom_lattice_move      |
|-----|-----------------------|------------------------|
| 0   | shape (Geometry)      | molecule (Atomic)      |
| 1   | translation (IVec3)   | translation (IVec3)    |
| 2   | subdivision (Int)     | subdivision (Int)      |
| 3   | —                     | unit_cell (UnitCell)   |

**lattice_rot / atom_lattice_rot:**

| Pin | lattice_rot           | atom_lattice_rot       |
|-----|-----------------------|------------------------|
| 0   | shape (Geometry)      | molecule (Atomic)      |
| 1   | axis_index (Int)      | axis_index (Int)       |
| 2   | step (Int)            | step (Int)             |
| 3   | pivot_point (IVec3)   | pivot_point (IVec3)    |
| 4   | —                     | unit_cell (UnitCell)   |

Pins 1..N-1 have the same indices in both modes, so `evaluate_or_default` calls for shared
parameters are identical code.

#### Node Type Registration

Each file exports two functions instead of one:

```rust
pub fn get_node_type_lattice_move() -> NodeType {
    NodeType {
        name: "lattice_move".to_string(),
        category: NodeTypeCategory::Geometry3D,
        parameters: vec![
            Parameter { name: "shape".into(),       data_type: DataType::Geometry },
            Parameter { name: "translation".into(), data_type: DataType::IVec3 },
            Parameter { name: "subdivision".into(), data_type: DataType::Int },
        ],
        output_type: DataType::Geometry,
        node_data_creator: || Box::new(LatticeMoveData {
            translation: IVec3::ZERO, lattice_subdivision: 1, is_atomic_mode: false,
        }),
        // ... same saver/loader
    }
}

pub fn get_node_type_atom_lattice_move() -> NodeType {
    NodeType {
        name: "atom_lattice_move".to_string(),
        category: NodeTypeCategory::AtomicStructure,
        parameters: vec![
            Parameter { name: "molecule".into(),    data_type: DataType::Atomic },
            Parameter { name: "translation".into(), data_type: DataType::IVec3 },
            Parameter { name: "subdivision".into(), data_type: DataType::Int },
            Parameter { name: "unit_cell".into(),   data_type: DataType::UnitCell },
        ],
        output_type: DataType::Atomic,
        node_data_creator: || Box::new(LatticeMoveData {
            translation: IVec3::ZERO, lattice_subdivision: 1, is_atomic_mode: true,
        }),
        // ... same saver/loader
    }
}
```

Same pattern for `lattice_rot.rs` with `get_node_type_lattice_rot()` and
`get_node_type_atom_lattice_rot()`.

#### Eval Logic (lattice_move example)

The shared computation (reading translation, subdivision, converting to real-space) is
factored out. Only input extraction and output construction differ:

```rust
fn eval(...) -> NetworkResult {
    let input_val = evaluate_arg_required(..., 0);
    if let NetworkResult::Error(_) = input_val { return input_val; }

    // Shared: read translation (pin 1) and subdivision (pin 2)
    let translation = /* evaluate pin 1 as IVec3 */;
    let subdivision = /* evaluate pin 2 as Int, max(1) */;
    let subdivided = translation.as_dvec3() / subdivision as f64;

    if self.is_atomic_mode {
        if let NetworkResult::Atomic(structure) = input_val {
            let unit_cell = /* evaluate pin 3 as UnitCell */;
            let real_translation = unit_cell.dvec3_lattice_to_real(&subdivided);

            // Store eval cache (same LatticeMoveEvalCache, same code)
            ...

            let mut result = structure.clone();
            result.transform(&DQuat::IDENTITY, &real_translation);
            NetworkResult::Atomic(result)
        } else { runtime_type_error_in_input(0) }
    } else {
        if let NetworkResult::Geometry(shape) = input_val {
            let real_translation = shape.unit_cell.dvec3_lattice_to_real(&subdivided);

            // Store eval cache (same LatticeMoveEvalCache, same code)
            ...

            NetworkResult::Geometry(GeometrySummary { ... })
        } else { runtime_type_error_in_input(0) }
    }
}
```

#### What Is Shared vs. Mode-Specific

| Aspect                  | Shared? | Notes                                             |
|-------------------------|---------|---------------------------------------------------|
| Data struct             | Yes     | Same `LatticeMoveData` / `LatticeRotData`         |
| Serde / .cnnd           | Yes     | Same `generic_node_data_saver/loader`             |
| Text properties         | Yes     | Same fields (translation, subdivision, etc.)      |
| Subtitle                | Yes     | Shows same lattice-coordinate info                |
| Eval: read shared pins  | Yes     | Pins 1..N-1 at same indices                       |
| Eval: lattice-to-real   | Yes     | Same `dvec3_lattice_to_real` call                 |
| Eval cache              | Yes     | Same `LatticeMoveEvalCache` with `unit_cell`      |
| Gadget (tessellation)   | Yes     | `LatticeMoveGadget` takes `UnitCellStruct`, mode-agnostic |
| Gadget (sync_data)      | Yes     | Writes back to same `LatticeMoveData` via downcast |
| Eval: input extraction  | No      | `Geometry(shape)` vs `Atomic(structure)`          |
| Eval: unit_cell source  | No      | `shape.unit_cell` vs dedicated pin                |
| Eval: output            | No      | `GeometrySummary` vs `AtomicStructure::transform` |
| NodeType registration   | No      | Different pin 0 type, category, output type       |
| get_parameter_metadata  | No      | Different required pin names                      |

### Step 3.1: Modify `lattice_move.rs`

**File:** `rust/src/structure_designer/nodes/lattice_move.rs`

1. Add `is_atomic_mode: bool` field to `LatticeMoveData` with `#[serde(default)]`
2. Rename `get_node_type()` → `get_node_type_lattice_move()`
3. Add `get_node_type_atom_lattice_move()` function
4. Branch `eval()` on `self.is_atomic_mode`:
   - Atomic mode: extract `NetworkResult::Atomic`, get `unit_cell` from last pin, apply
     `structure.transform(&DQuat::IDENTITY, &real_translation)`
   - Geometry mode: existing code, unchanged
5. Branch `get_parameter_metadata()` to return correct required pins per mode

### Step 3.2: Modify `lattice_rot.rs`

**File:** `rust/src/structure_designer/nodes/lattice_rot.rs`

Same pattern as Step 3.1:
1. Add `is_atomic_mode: bool` field to `LatticeRotData` with `#[serde(default)]`
2. Rename `get_node_type()` → `get_node_type_lattice_rot()`
3. Add `get_node_type_atom_lattice_rot()` function
4. Branch `eval()` on `self.is_atomic_mode`:
   - Atomic mode: extract `NetworkResult::Atomic`, get `unit_cell` from last pin,
     compute real pivot/rotation, apply three-step pivot rotation via
     `structure.transform(...)` calls
   - Geometry mode: existing code, unchanged
5. Branch `get_parameter_metadata()` per mode

### Step 3.3: Register nodes and update call sites

- In `rust/src/structure_designer/node_type_registry.rs`:
  - Update existing `lattice_move::get_node_type()` call →
    `lattice_move::get_node_type_lattice_move()`
  - Update existing `lattice_rot::get_node_type()` call →
    `lattice_rot::get_node_type_lattice_rot()`
  - Add `lattice_move::get_node_type_atom_lattice_move()`
  - Add `lattice_rot::get_node_type_atom_lattice_rot()`

### Step 3.4: Tests

**Test: `atom_lattice_move_basic`**
1. Create an AtomicStructure with an atom at (0, 0, 0)
2. Create a cubic diamond UnitCell (a = 3.567 Å)
3. Evaluate `atom_lattice_move` with translation (1, 0, 0), subdivision 1
4. Assert atom moved to (3.567, 0, 0)

**Test: `atom_lattice_move_subdivision`**
1. Same as above but subdivision = 2, translation = (1, 0, 0)
2. Assert atom moved to (3.567 / 2, 0, 0)

**Test: `atom_lattice_move_diff_preserves_anchors`**
1. Create a diff structure with atom at (0, 0, 0), anchor at (0, 0, 0)
2. Apply `atom_lattice_move` with translation (2, 0, 0) using cubic diamond unit cell
3. Assert atom position moved to (2 * 3.567, 0, 0)
4. Assert anchor position also moved to (2 * 3.567, 0, 0) (via Phase 1 fix)

**Test: `atom_lattice_rot_basic`**
1. Create an AtomicStructure with atom at (3.567, 0, 0) (one unit cell along a)
2. Use cubic diamond UnitCell
3. Evaluate `atom_lattice_rot` with axis_index=0 (4-fold [100]), step=1, pivot=(0,0,0)
4. Assert atom rotated 90° around [100] axis

**Test: `atom_lattice_move_then_apply_diff`** (full integration)
1. Create a base structure (e.g. small diamond crystal)
2. Create a diff with a delete marker at (0, 0, 0) and a new atom at (0.5, 0.5, 0)
3. Wire: diff → atom_lattice_move(2, 0, 0) → apply_diff(base, moved_diff)
4. Evaluate — verify deletion happened at position (2 * a, 0, 0) and new atom placed
   at (2 * a + 0.5, 0.5, 0)

**Test: `lattice_move_geometry_mode_unchanged`** (regression)
1. Run existing lattice_move tests to verify geometry mode still works identically

**Test: Node snapshots**
Add to `rust/tests/structure_designer/node_snapshot_test.rs`:
- Snapshot test for `atom_lattice_move` node type
- Snapshot test for `atom_lattice_rot` node type

### Verification

```bash
cd rust && cargo test && cargo clippy
cargo insta review  # review new + verify unchanged existing snapshots
```
