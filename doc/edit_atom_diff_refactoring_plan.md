# Edit Atom Node Refactoring: Command Stack to Diff Representation

## Motivation

The current `edit_atom` node uses a command stack pattern: every user interaction (select, add atom, delete, replace, move) is stored as a command object. Evaluation replays all commands sequentially. This has several drawbacks:

- **Selection is persisted in the command history.** UI-only operations (clicking to select atoms) are stored forever and serialized to `.cnnd` files. A node with 20 real edits might have 100+ selection commands.
- **Commands reference atoms by ID.** Atom IDs are assigned during evaluation and depend on the input structure. If the input changes (upstream node modified), IDs can silently point to wrong atoms.
- **The history is opaque.** Users see "ops: 47" but cannot understand what the node does without mentally replaying the sequence.
- **Not composable.** The edit cannot be extracted and reapplied elsewhere (e.g., stamping a defect into a crystal lattice at multiple positions).

## New Design: Diff as AtomicStructure

Replace the command stack with a single **diff** represented as an `AtomicStructure`. The `AtomicStructure` type itself is extended with diff capabilities: an `is_diff` flag and an `anchor_positions` map. This means diffs flow through the existing node network infrastructure (`NetworkResult::Atomic`) with zero friction — all existing code that processes AtomicStructure (visualization, serialization, iteration, hit testing) works on diffs automatically.

### AtomicStructure Extensions

The diff capabilities are added directly to `AtomicStructure` rather than a separate wrapper type (rationale below):

```rust
pub struct AtomicStructure {
    // ... existing fields ...
    is_diff: bool,                                  // Whether this structure represents a diff
    anchor_positions: FxHashMap<u32, DVec3>,         // diff_atom_id → base match position (for moved atoms)
}
```

- `AtomicStructure::new()` → `is_diff = false`, empty anchor map (existing behavior, unchanged)
- `AtomicStructure::new_diff()` → `is_diff = true`, empty anchor map (new constructor for diffs)
- `is_diff()` getter, `set_is_diff()` setter
- `anchor_position(atom_id)` getter, `set_anchor_position(atom_id, pos)` setter, `remove_anchor_position(atom_id)` remover
- `has_anchor_position(atom_id)` convenience method

### The Delete Site Marker

- A constant `pub const DELETED_SITE_ATOMIC_NUMBER: i16 = 0;` on `AtomicStructure` or at module level.
- `Atom::is_delete_marker(&self) -> bool` helper method.
- In diff visualization mode, render delete markers with a distinct appearance (e.g., red translucent sphere with X overlay, or a specific "void" glyph).

### Diff Application Algorithm

`apply_diff()` lives in the **crystolecule module** (`atomic_structure_diff.rs`), since diff application is a fundamental operation on atomic structures, not specific to the node network or the edit_atom node. Future stamp nodes and any other consumer can use it directly.

**Signature:**
```rust
pub fn apply_diff(base: &AtomicStructure, diff: &AtomicStructure, tolerance: f64) -> AtomicStructure
```

**Algorithm:**

1. **Atom matching:** For each atom in the diff:
   - Determine match position: if `diff.anchor_positions` contains this atom's ID, use the anchor position; otherwise use the atom's own position.
   - Find the nearest unmatched atom in the base structure within `tolerance` of the match position.
   - Use greedy nearest-first assignment: process diff atoms sorted by closest match distance to avoid ambiguity.

2. **Atom effects:**
   - **Match found + normal atom:** The base atom is replaced by the diff atom's properties (element, position). This handles element replacement, position changes (via anchors), or both.
   - **Match found + delete marker (atomic_number = 0):** The matched base atom is removed.
   - **No match + normal atom:** The atom is added to the result (new atom insertion).
   - **No match + delete marker:** Ignored (trying to delete something that doesn't exist).

3. **Bond resolution:** For any pair of atoms where **both** were matched or added by the diff, bonds between them are defined entirely by the diff. For pairs where at least one atom was NOT involved in the diff, bonds come from the base structure unchanged. This means:
   - A moved atom retains its base bonds to non-diff neighbors automatically.
   - New bonds between diff atoms are defined in the diff.
   - Bonds between base atoms not mentioned in the diff are untouched.

4. **Unmatched base atoms:** Pass through to the result with their original properties and bonds (governed by the bond rule above).

5. **Result:** A new `AtomicStructure` with `is_diff = false` (it's a fully resolved structure, not a diff).

### Position Matching Tolerance

- Default tolerance: **0.1 Angstrom** (well below typical bond lengths ~1.0-1.5 A, well above numerical noise ~1e-10).
- Configurable per `edit_atom` node if edge cases arise, but the default should work for virtually all scenarios.
- Matching algorithm: greedy nearest-first assignment. For each diff atom, find the nearest unmatched base atom within tolerance of the match position. Process diff atoms in order of closest match distance first.

### Handling Atom Movement (Anchor Positions)

When a user moves an atom from position P_old to P_new:

1. The atom is added to (or updated in) the diff at position P_new.
2. An anchor position is recorded: `diff.set_anchor_position(atom_id, P_old)`.
3. During `apply_diff()`, the anchor position P_old is used for matching against the base, then the atom is placed at P_new in the result.

**Why this is better than delete+add:**
- A single atom in the diff represents the move (not a delete marker + a separate new atom).
- Bonds to non-diff neighbors are preserved automatically via the bond rule. No need to include neighbors in the diff.
- Example: Moving a carbon in diamond (4 bonds) requires just 1 atom in the diff with an anchor. Delete+add would require 1 delete marker + 1 new atom + 4 neighbors + all their inter-bonds (~6 atoms, ~10+ bonds).

**Anchor lifecycle:**
- First move: anchor is set to the base atom's current position.
- Subsequent moves: anchor stays at the original base position; only the diff atom's position updates.
- Move back to original position: anchor and position coincide. Could be cleaned up (atom removed from diff) or left as-is (harmless identity edit).
- Moving an already-added atom (no base match): no anchor needed — just update position directly.

### Why enrich AtomicStructure rather than a separate AtomicDiff type

Once an AtomicStructure contains delete markers (atomic_number = 0), it already carries diff semantics. The anchor map is a natural extension. A separate wrapper type (`AtomicDiff`) would create a parallel world: a new `NetworkResult` variant, duplicate visualization paths, a new `DataType` for pins, and constant unwrapping. Enriching AtomicStructure avoids all of this while adding negligible memory overhead (~56 bytes for non-diff structures: one bool + an empty hashmap).

## Data Model Changes

### Rust: `AtomicStructure` (extended)

```rust
pub struct AtomicStructure {
    // ... existing fields (atoms, grid, num_atoms, num_bonds, decorator, frame_transform) ...
    is_diff: bool,
    anchor_positions: FxHashMap<u32, DVec3>,
}
```

### Rust: `EditAtomData` (replaces current struct)

```rust
EditAtomData {
    diff: AtomicStructure,                  // The diff (is_diff = true)
    active_tool: EditAtomTool,              // Current editing tool (UI state)
    selection_transform: Option<Transform>, // UI state for selected atoms
    output_diff: bool,                      // When true, output the diff instead of the result
    tolerance: f64,                         // Positional matching tolerance (default 0.1)
}
```

**Removed:** `history: Vec<Box<dyn EditAtomCommand>>`, `next_history_index: usize`

### What stays the same

- `EditAtomTool` enum (Default, AddAtom, AddBond) — tools still needed for interaction
- Tool-specific state (replacement_atomic_number, last_atom_id, etc.)
- Selection as UI-only state on the evaluated result's AtomicStructure (atom flags, decorator)

### What gets removed

- `EditAtomCommand` trait and all 6 command implementations
- `commands/` subdirectory entirely
- `edit_atom_data_serialization.rs` (replaced by simpler serialization)
- Undo/redo logic (`undo()`, `redo()`, `can_undo()`, `can_redo()`, `next_history_index`)

## Implementation Plan

### Phase 1: AtomicStructure Diff Extensions

Extend the core `AtomicStructure` type in the crystolecule module with diff capabilities. This is the foundation that everything else builds on.

**Files to modify:**

1. **`rust/src/crystolecule/atomic_structure/mod.rs`** — Add diff fields and methods to `AtomicStructure`:
   - Add `is_diff: bool` field (default `false`)
   - Add `anchor_positions: FxHashMap<u32, DVec3>` field (default empty)
   - Add `pub const DELETED_SITE_ATOMIC_NUMBER: i16 = 0;`
   - Add `new_diff()` constructor (creates empty structure with `is_diff = true`)
   - Add getters/setters: `is_diff()`, `set_is_diff()`, `anchor_position(atom_id)`, `set_anchor_position(atom_id, pos)`, `remove_anchor_position(atom_id)`, `has_anchor_position(atom_id)`
   - Update `Default` and `Clone` implementations to include new fields
   - Update `add_atomic_structure()` to merge anchor positions with remapped IDs
   - Update `to_detailed_string()` to include diff info when `is_diff = true`
   - Update `MemorySizeEstimator` to account for anchor map

2. **`rust/src/crystolecule/atomic_structure/atom.rs`** — Add helper method:
   - `pub fn is_delete_marker(&self) -> bool` (returns `self.atomic_number == DELETED_SITE_ATOMIC_NUMBER`)

### Phase 2: Diff Application Algorithm

Implement the core diff application logic in the crystolecule module. This is independent of the node network and usable by edit_atom, future stamp nodes, or any other consumer.

**Files to create:**

3. **`rust/src/crystolecule/atomic_structure_diff.rs`** (new file) — The core diff application:
   - `pub fn apply_diff(base: &AtomicStructure, diff: &AtomicStructure, tolerance: f64) -> AtomicStructure`
   - Internal: `match_diff_atoms()` — greedy nearest-first positional matching with anchor support
   - Internal: `resolve_bonds()` — bond resolution using the "both from diff → use diff bonds" rule
   - Helper: `diff_stats(diff: &AtomicStructure, base: &AtomicStructure, tolerance: f64) -> DiffStats` — computes statistics (atoms added/deleted/modified) by running the matching without full application
   - `DiffStats` struct: `atoms_added: u32, atoms_deleted: u32, atoms_modified: u32, bonds_in_diff: u32`

4. **`rust/src/crystolecule/mod.rs`** — Add `pub mod atomic_structure_diff;`

5. **`rust/tests/crystolecule/atomic_structure_diff_test.rs`** (new file) — Comprehensive tests:
   - Add atom to structure (no match → added)
   - Delete atom by position match (delete marker)
   - Replace element at matched position
   - Move atom via anchor position (verify bonds to non-diff neighbors preserved)
   - Bond resolution: diff-diff bonds override base bonds
   - Bond resolution: diff-base bonds come from base
   - Bond resolution: base-base bonds untouched
   - Tolerance edge cases (just inside/outside tolerance)
   - No-match delete marker (graceful ignore)
   - Multiple close atoms (greedy assignment correctness)
   - Anchor positions: move + element change combined
   - Empty diff (identity operation)
   - Empty base (all diff atoms added)
   - Result has `is_diff = false`

6. **`rust/tests/crystolecule.rs`** — Register the new test module

### Phase 3: EditAtomData Refactoring

Replace the command stack in EditAtomData with the diff. The eval method now delegates to `apply_diff()` from the crystolecule module.

**Files to modify:**

7. **`rust/src/structure_designer/nodes/edit_atom/edit_atom.rs`** — Replace `EditAtomData`:
   - Remove `history: Vec<Box<dyn EditAtomCommand>>`, `next_history_index: usize`
   - Add `diff: AtomicStructure` (initialized with `AtomicStructure::new_diff()`), `output_diff: bool`, `tolerance: f64`
   - Rewrite `eval()`:
     - When `output_diff` is false: call `apply_diff(input, &self.diff, self.tolerance)` and return result
     - When `output_diff` is true: return a clone of `self.diff`
   - Replace command-based mutation methods with direct diff mutation:
     - `add_atom_to_diff(atomic_number, position)` — calls `self.diff.add_atom()`
     - `mark_for_deletion(match_position)` — adds atom with `DELETED_SITE_ATOMIC_NUMBER` at match_position
     - `replace_in_diff(match_position, new_atomic_number)` — adds/updates atom in diff
     - `move_in_diff(atom_id, new_position)` — sets anchor if needed, updates position
     - `add_bond_in_diff(atom_id1, atom_id2, order)` — adds bond in diff structure
     - `remove_from_diff(diff_atom_id)` — removes atom from diff (and its anchor if any)
   - Remove `undo()`, `redo()`, `can_undo()`, `can_redo()`, `add_command()`
   - Keep `active_tool`, `selection_transform`
   - Update `get_subtitle()`: use `diff_stats()` to show "+N, -M, ~K" summary
   - Update `clone_box()`: clone diff (including anchor_positions) + other fields
   - Update `get_parameter_metadata()`: unchanged (still requires "molecule" input)

8. **Remove command infrastructure:**
   - Delete `rust/src/structure_designer/nodes/edit_atom/edit_atom_command.rs`
   - Delete `rust/src/structure_designer/nodes/edit_atom/commands/` directory (all 7 files)
   - Update `rust/src/structure_designer/nodes/edit_atom/mod.rs`: remove `pub mod commands;` and `pub mod edit_atom_command;`

### Phase 4: Interaction Functions Refactoring

Rewrite the public interaction functions that were previously creating commands. These stay in the edit_atom module (they are specific to edit_atom's interactive workflow) but now directly mutate the diff.

**Files to modify:**

9. **`rust/src/structure_designer/nodes/edit_atom/edit_atom.rs`** (interaction functions):

   - `select_atom_or_bond_by_ray()` — Selection operates on the *evaluated result* structure for display purposes only. Selection flags are transient UI state (set on the result during decoration, not stored in the diff). No diff mutation.

   - `add_atom_by_ray()` — Calculate position (ray-plane intersection, same as current), then call `edit_atom_data.add_atom_to_diff(atomic_number, position)`.

   - `draw_bond_by_ray()` — Same two-click workflow. On second click, call `edit_atom_data.add_bond_in_diff(atom_id1, atom_id2, 1)`. The atom IDs are diff-internal IDs. If bonding involves a base atom not yet in the diff, it must be added to the diff first (at its current position, no anchor) so both atoms are "from the diff" for bond resolution.

   - `delete_selected_atoms_and_bonds()` — For each selected atom in the result:
     - If the atom came from the diff (was added or already modified): remove it from the diff via `remove_from_diff()`
     - If the atom is from the base (unmodified): add a delete marker at its position via `mark_for_deletion()`
     - For bonds: if both atoms are in the diff, remove the bond from the diff. Otherwise, ensure both atoms are in the diff and omit the bond.

   - `replace_selected_atoms()` — For each selected atom:
     - If already in the diff: update its atomic_number in the diff
     - If from the base: add to diff with the new atomic_number at the matched position

   - `transform_selected()` — For each selected atom:
     - If already in the diff (added atom, no base match): update position directly
     - If from the base (or already matched via anchor): set anchor to the base match position (if not already set), update position to the new position

   **Key design point:** Interaction functions need to know which result atoms correspond to diff atoms and which are pass-through base atoms. This requires either:
   - Running the matching as part of the interaction (call `match_diff_atoms()` to get the mapping)
   - Or maintaining a cached mapping from the last evaluation

   The former is cleaner (no stale cache). `match_diff_atoms()` is fast for typical diff sizes.

### Phase 5: API Layer

**Files to modify:**

10. **`rust/src/api/structure_designer/edit_atom_api.rs`** — Update API functions:
    - Remove `edit_atom_undo()`, `edit_atom_redo()`
    - Add `toggle_edit_atom_output_diff()` — toggles `edit_atom_data.output_diff`
    - All other API functions (`select_atom_or_bond_by_ray`, `add_atom_by_ray`, `draw_bond_by_ray`, `delete_selected_atoms_and_bonds`, `replace_selected_atoms`, `transform_selected`) — signatures unchanged, implementations call the rewritten interaction functions

11. **`rust/src/api/structure_designer/structure_designer_api_types.rs`** — Update types:
    ```rust
    pub struct APIEditAtomData {
        pub active_tool: APIEditAtomTool,
        // REMOVED: can_undo, can_redo
        pub bond_tool_last_atom_id: Option<u32>,
        pub replacement_atomic_number: Option<i16>,
        pub add_atom_tool_atomic_number: Option<i16>,
        pub has_selected_atoms: bool,
        pub has_selection: bool,
        pub selection_transform: Option<APITransform>,
        // NEW:
        pub output_diff: bool,
        pub diff_stats: APIDiffStats,
    }

    // NEW:
    pub struct APIDiffStats {
        pub atoms_added: u32,
        pub atoms_deleted: u32,
        pub atoms_modified: u32,
        pub bonds_in_diff: u32,
    }
    ```

12. **`rust/src/api/structure_designer/structure_designer_api.rs`** — Update `get_edit_atom_data()`:
    - Remove `can_undo`, `can_redo` from the returned struct
    - Add `output_diff` from `edit_atom_data.output_diff`
    - Compute and add `diff_stats` by calling `diff_stats()` on the diff and base structure

### Phase 6: Serialization

**Files to modify:**

13. **`rust/src/structure_designer/serialization/edit_atom_data_serialization.rs`** — Complete rewrite:
    - Serialize the diff `AtomicStructure` including atoms, bonds, `is_diff`, and `anchor_positions`
    - Serialize `output_diff: bool` and `tolerance: f64`
    - Active tool state and selection_transform are NOT serialized (transient UI state)
    - AtomicStructure already has serialization infrastructure for `.cnnd` — extend it to include anchor positions when `is_diff = true`

14. **`rust/src/crystolecule/atomic_structure/mod.rs`** (serialization aspect) — The `AtomicStructure` serialization (used by `.cnnd` save/load) must be updated to persist:
    - `is_diff` flag
    - `anchor_positions` map (only when `is_diff = true`)
    - These fields are optional in the format — absent means `is_diff = false` and empty anchors (backward compatible)

15. **`rust/src/structure_designer/node_type_registry.rs`** — Update the `edit_atom` node type entry's `node_data_saver` and `node_data_loader` to use the new serialization.

16. **Migration of old `.cnnd` files:**
    - Detect old format by presence of `history` key in JSON
    - Replay the old commands on an empty `AtomicStructure` to produce the net effect
    - Convert the result into a diff: compare against an empty base to identify all atoms as "added"
    - This is a lossy but correct migration — the diff captures the net effect of all commands
    - Selection commands are discarded (they were transient anyway)

### Phase 7: Selection Model

17. **Selection rethink:**
    - Selection is purely transient UI state — NOT part of the serialized diff.
    - During editing, the user selects atoms in the **evaluated result** (the output of `apply_diff`).
    - The interaction functions (delete, replace, transform) need to map selected result atoms back to either diff atoms or base atoms. This mapping comes from `match_diff_atoms()`.
    - Selection flags on atoms, selection_transform, and selected bonds in the decorator are all transient — they exist during the editing session and are recomputed on each evaluation.
    - The diff's own AtomicStructure may have selection flags set during editing (e.g., selecting atoms within the diff for manipulation), but these are not persisted.

### Phase 8: Diff Visualization

18. **`rust/src/display/atomic_tessellator.rs`** — Rendering support for diff structures:
    - When an `AtomicStructure` has `is_diff = true`, the tessellator uses diff-aware rendering:
      - **Delete markers** (atomic_number = 0): Render as translucent red spheres with X overlay or a distinct "void" glyph. Use a fixed radius (e.g., 1.0 A) since there's no element to determine size.
      - **Normal diff atoms**: Render with standard appearance. Color coding (green outline for added, orange for modified) can be done at the decorator level or via a display mode flag.
      - **Anchor arrows**: For atoms with anchor positions, optionally render a ghost atom at the anchor position connected by a dashed line/arrow to the actual position, showing the movement.
    - When `is_diff = false`, rendering is completely unchanged (no performance impact).

19. **Display mode in the edit_atom node:**
    - When `output_diff = false` (default): the node outputs the applied result. Rendering shows the final structure. This is the normal editing workflow.
    - When `output_diff = true`: the node outputs the diff itself. The `is_diff = true` flag tells the renderer to use diff visualization. The user sees what edits the node contains.
    - A future enhancement could overlay the diff on the base (ghosted base + highlighted diff), but this is not required for the initial refactoring.

### Phase 9: Flutter UI Changes

**Files to modify:**

20. **`lib/structure_designer/node_data/edit_atom_editor.dart`** — UI changes:
    - **Remove:** Undo/Redo buttons from the header row
    - **Add:** Output mode toggle in the header — a segmented button or toggle switch labeled "Result" / "Diff"
    - **Add:** Diff statistics display below the header — show e.g., "+3 atoms, -1 atom, ~2 modified" from `APIDiffStats`
    - **Keep:** Tool selector (Default, AddAtom, AddBond) — tools are unchanged in concept
    - **Keep:** Element selector — used for add/replace operations
    - **Keep:** Replace/Delete buttons — they now mutate the diff directly
    - **Keep:** Transform controls — they now update positions in the diff
    - **Contextual delete button:**
      - If a diff-added atom is selected: "Remove from diff" semantics (removes it entirely)
      - If a base-matched atom is selected: "Mark for deletion" semantics (adds delete marker)
      - The button can remain labeled "Delete Selected" for simplicity; the underlying behavior differs based on context

21. **`lib/structure_designer/structure_designer_model.dart`** — Update model methods:
    - Remove `editAtomUndo()`, `editAtomRedo()`
    - Add `toggleEditAtomOutputDiff()` — calls new API function
    - Update `refreshFromKernel()` data fetch to populate new `APIEditAtomData` fields (output_diff, diff_stats)

22. **`lib/structure_designer/structure_designer_viewport.dart`** — No changes expected. Ray-cast interactions call the same API functions; the implementations change internally but the viewport code doesn't need to know.

### Phase 10: Text Format Integration

23. **`rust/src/structure_designer/text_format/`** — If edit_atom exposes text properties:
    - `get_text_properties()` could expose the diff as a readable summary or structured text
    - `set_text_properties()` could allow editing the diff through text (e.g., for AI-assisted editing)
    - Possible format: `+C @ (1.0, 2.0, 3.0)` for additions, `- @ (4.0, 5.0, 6.0)` for deletions, `~Si @ (7.0, 8.0, 9.0) [from (7.0, 8.5, 9.0)]` for modifications with anchor
    - This is lower priority and can be deferred to a follow-up

## File Summary

### New files
| File | Purpose |
|------|---------|
| `rust/src/crystolecule/atomic_structure_diff.rs` | Diff application algorithm + DiffStats — lives in crystolecule, reusable by any consumer |
| `rust/tests/crystolecule/atomic_structure_diff_test.rs` | Comprehensive tests for diff application |

### Files to heavily modify
| File | Changes |
|------|---------|
| `rust/src/crystolecule/atomic_structure/mod.rs` | Add `is_diff`, `anchor_positions`, `DELETED_SITE_ATOMIC_NUMBER`, new constructors and accessors, update Clone/Default/add_atomic_structure/serialization |
| `rust/src/crystolecule/atomic_structure/atom.rs` | Add `is_delete_marker()` |
| `rust/src/crystolecule/mod.rs` | Add `pub mod atomic_structure_diff;` |
| `rust/src/structure_designer/nodes/edit_atom/edit_atom.rs` | New data model, new eval using `apply_diff()`, new diff-mutating interaction functions |
| `rust/src/structure_designer/serialization/edit_atom_data_serialization.rs` | Complete rewrite for diff-based serialization |
| `rust/src/api/structure_designer/edit_atom_api.rs` | Remove undo/redo, add output toggle |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Update `APIEditAtomData`, add `APIDiffStats` |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Update `get_edit_atom_data()` |
| `lib/structure_designer/node_data/edit_atom_editor.dart` | Remove undo/redo, add output toggle and diff stats |
| `lib/structure_designer/structure_designer_model.dart` | Update model methods |

### Files to delete
| File | Reason |
|------|--------|
| `rust/src/structure_designer/nodes/edit_atom/edit_atom_command.rs` | Command trait no longer needed |
| `rust/src/structure_designer/nodes/edit_atom/commands/add_atom_command.rs` | Replaced by direct diff mutation |
| `rust/src/structure_designer/nodes/edit_atom/commands/add_bond_command.rs` | Replaced by direct diff mutation |
| `rust/src/structure_designer/nodes/edit_atom/commands/delete_command.rs` | Replaced by direct diff mutation |
| `rust/src/structure_designer/nodes/edit_atom/commands/replace_command.rs` | Replaced by direct diff mutation |
| `rust/src/structure_designer/nodes/edit_atom/commands/select_command.rs` | Selection is UI-only state |
| `rust/src/structure_designer/nodes/edit_atom/commands/transform_command.rs` | Replaced by direct diff mutation |
| `rust/src/structure_designer/nodes/edit_atom/commands/mod.rs` | Directory removed |

### Files with minor modifications
| File | Changes |
|------|---------|
| `rust/src/structure_designer/nodes/edit_atom/mod.rs` | Remove command modules |
| `rust/src/structure_designer/node_type_registry.rs` | Update edit_atom description and saver/loader |
| `rust/src/display/atomic_tessellator.rs` | Diff-aware rendering (delete markers, anchor arrows) |
| `rust/tests/crystolecule.rs` | Register new test module |

## Dependency Order

The implementation phases have a clear dependency chain:

```
Phase 1: AtomicStructure extensions (is_diff, anchors, delete marker constant)
    ↓
Phase 2: apply_diff() in crystolecule + tests
    ↓
Phase 3: EditAtomData refactoring (uses apply_diff)
    ↓
Phase 4: Interaction functions (uses new EditAtomData)
    ↓
Phase 5: API layer (exposes new interaction functions)
Phase 6: Serialization (persists new data model)    ← can be parallel with Phase 5
    ↓
Phase 7: Selection model (transient, no persistence)
Phase 8: Diff visualization (rendering)             ← can be parallel with Phase 7
    ↓
Phase 9: Flutter UI (consumes API changes)
    ↓
Phase 10: Text format (lower priority, can be deferred)
```

## Open Questions for Future Sessions

1. **Diff composition** — When two edit_atom nodes are chained, the second operates on the result of the first. Could their diffs be composed into one? This is a future optimization.

2. **Multi-output support** — When the node can output both the result and the diff simultaneously, the `output_diff` boolean becomes unnecessary. This depends on broader node network infrastructure.

3. **Stamping workflow** — The full UX for "apply this diff at multiple positions in a crystal" (e.g., T-center defect stamping) is a separate design task. The diff will flow as a regular `NetworkResult::Atomic` with `is_diff = true` into a future stamp node that calls `apply_diff()`.

4. **Anchor position alternatives** — The current anchor map approach works well. If we discover cases where a richer position-transformation model is needed (e.g., parametric displacements), this can be extended later without changing the fundamental architecture.
