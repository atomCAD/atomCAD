# Design: `apply_diff` Node and Diff-Aware Transforms

## Motivation

The `atom_edit` node can output a raw diff structure (`output_diff = true`) instead of the
applied result. This makes diffs first-class values in the node network. To complete this
workflow we need:

1. An **`apply_diff` node** that takes a base structure and a diff and produces the result.
2. **Diff-aware transforms** so `atom_move`, `atom_rot`, and `atom_trans` work correctly on
   diff structures (moving the defect to a new location before application).

### Example Pipeline

```
                                                    +---------------+
+-----------+                                       |               |
| unit_cell +---> atom_fill ---> [Diamond] -------->|  apply_diff   |---> [Diamond with
| + motif   |                                       |               |      relocated defect]
+-----------+                                       +-------+-------+
                                                            |
+--------------------+     +-----------+     +----------+   |
| atom_edit          |     |           |     |          |   |
| (output_diff=ON)   +---->| atom_move +---->| atom_rot +---+
| [defect patch]     |     | (5,0,0)   |     | 45deg Z  |
+--------------------+     +-----------+     +----------+
```

A defect is authored once in `atom_edit`, repositioned/rotated freely, then applied to any
base structure. Multiple `apply_diff` nodes could apply the same diff at different locations.

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

## Part 1: Fix `AtomicStructure::transform()` to Include Anchors

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

### Change

In `crystolecule/atomic_structure/mod.rs`, append anchor transformation to `transform()`:

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

---

## Part 2: `apply_diff` Node

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
3. Get tolerance from pin 2 or property (default 0.1)
4. Call apply_diff(&base, &diff, tolerance) -> DiffApplicationResult
5. If error_on_stale is true:
     Check stats for orphaned_tracked_atoms, unmatched_delete_markers, orphaned_bonds
     If any > 0: return NetworkResult::Error with diagnostic message
6. Return NetworkResult::Atomic(result)
```

### Subtitle

Display a compact summary from `DiffStats`: e.g. `"+3 -2 ~1"` (3 added, 2 deleted,
1 modified). This requires caching the stats in an eval cache struct.

### File Location

`rust/src/structure_designer/nodes/apply_diff.rs`

Registration in `node_type_registry.rs` under `create_built_in_node_types()`.

---

## Implementation Checklist

1. **`AtomicStructure::transform()`** — add anchor transformation loop
2. **`apply_diff` node** — new file `nodes/apply_diff.rs`
3. **Register node** — add to `nodes/mod.rs` and `node_type_registry.rs`
4. **Tests:**
   - Unit test: `transform()` moves anchors correctly
   - Unit test: `transform_atom()` does NOT move anchors (verify existing behavior)
   - Integration test: atom_move on a diff, then apply_diff, verify correct result
   - Integration test: atom_rot on a diff, then apply_diff, verify correct result
   - Node snapshot test for `apply_diff`
