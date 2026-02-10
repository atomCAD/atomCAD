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
    show_anchor_arrows: bool,                       // When true + is_diff, tessellator renders anchor arrows
}
```

- `AtomicStructure::new()` → `is_diff = false`, empty anchor map, `show_anchor_arrows = false` (existing behavior, unchanged)
- `AtomicStructure::new_diff()` → `is_diff = true`, empty anchor map, `show_anchor_arrows = false` (new constructor for diffs)
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
pub fn apply_diff(base: &AtomicStructure, diff: &AtomicStructure, tolerance: f64) -> DiffApplicationResult
```

**Return type:**
```rust
pub struct DiffApplicationResult {
    pub result: AtomicStructure,
    pub provenance: DiffProvenance,
}

pub struct DiffProvenance {
    /// result_atom_id → where it came from
    pub sources: FxHashMap<u32, AtomSource>,
    /// base_atom_id → result_atom_id (reverse lookup for base pass-throughs and matched atoms)
    pub base_to_result: FxHashMap<u32, u32>,
    /// diff_atom_id → result_atom_id (reverse lookup for diff atoms present in result)
    pub diff_to_result: FxHashMap<u32, u32>,
}

pub enum AtomSource {
    /// Base atom NOT touched by the diff (pass-through)
    BasePassthrough(u32),               // base_atom_id
    /// Diff atom that matched a base atom (replacement or move)
    DiffMatchedBase { diff_id: u32, base_id: u32 },
    /// Diff atom with no base match (new addition)
    DiffAdded(u32),                     // diff_atom_id
}
```

The provenance is a zero-cost byproduct of the matching that `apply_diff()` already performs internally. Returning it enables the selection model (Phase 7) and interaction functions (Phase 4) to know the origin of every result atom without re-running the matching.

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

5. **Result:** A `DiffApplicationResult` containing a new `AtomicStructure` with `is_diff = false` (fully resolved) and a `DiffProvenance` mapping every result atom to its origin.

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
    show_anchor_arrows: bool,
}
```

### Rust: `EditAtomData` (replaces current struct)

```rust
EditAtomData {
    // Persistent (serialized to .cnnd)
    diff: AtomicStructure,                  // The diff (is_diff = true)
    output_diff: bool,                      // When true, output the diff instead of the result
    show_anchor_arrows: bool,               // When true + output_diff, render anchor arrows in diff view
    tolerance: f64,                         // Positional matching tolerance (default 0.1)

    // Transient (NOT serialized)
    selection: EditAtomSelection,           // Current selection state
    active_tool: EditAtomTool,              // Current editing tool
}
```

**Removed:** `history: Vec<Box<dyn EditAtomCommand>>`, `next_history_index: usize`, `selection_transform: Option<Transform>`

### Rust: `EditAtomSelection` (new, transient)

Selection is stored by **provenance** (base/diff atom IDs) rather than result atom IDs. This makes selection stable across re-evaluations, since base IDs are immutable and diff IDs are under our control.

```rust
pub struct EditAtomSelection {
    /// Base atoms selected (by base atom ID — stable, input doesn't change during editing)
    pub selected_base_atoms: HashSet<u32>,
    /// Diff atoms selected (by diff atom ID — stable, we control the diff)
    pub selected_diff_atoms: HashSet<u32>,
    /// Bond selection in result space (cleared on any diff mutation)
    pub selected_bonds: HashSet<BondReference>,
    /// Cached selection transform (recalculated after selection changes)
    pub selection_transform: Option<Transform>,
}
```

### Rust: `EditAtomEvalCache` (new, transient)

Follows the existing eval cache pattern (see `FacetShellEvalCache`, `DrawingPlaneEvalCache`, etc.). Stored in `NodeSceneData.selected_node_eval_cache` during evaluation, retrieved via `structure_designer.get_selected_node_eval_cache()`.

```rust
#[derive(Debug, Clone)]
pub struct EditAtomEvalCache {
    pub provenance: DiffProvenance,
}
```

The provenance maps result atom IDs to their base/diff origins. Used by selection and interaction functions to determine atom provenance without re-running the matching algorithm.

### What stays the same

- `EditAtomTool` enum (Default, AddAtom, AddBond) — tools still needed for interaction
- Tool-specific state (replacement_atomic_number, last_atom_id, etc.)
- Atom selection rendering: `atom.is_selected()` flag → magenta color (unchanged tessellator behavior)
- Bond selection rendering: `decorator.is_bond_selected()` → magenta color (unchanged)
- `calc_selection_transform()` utility — still used, called explicitly after selection changes

### What gets removed

- `EditAtomCommand` trait and all 6 command implementations
- `commands/` subdirectory entirely
- `edit_atom_data_serialization.rs` (replaced by simpler serialization)
- Undo/redo logic (`undo()`, `redo()`, `can_undo()`, `can_redo()`, `next_history_index`)
- `SelectCommand` — selection is no longer a command; it's direct mutation of `EditAtomSelection`
- `selection_transform` on `EditAtomData` — moved into `EditAtomSelection`
- `refresh_scene_dependent_edit_atom_data()` — no longer needed; selection transform lives in `EditAtomSelection` and is read directly by the API layer

## Implementation Plan

### Phase 1: AtomicStructure Diff Extensions

Extend the core `AtomicStructure` type in the crystolecule module with diff capabilities. This is the foundation that everything else builds on.

**Files to modify:**

1. **`rust/src/crystolecule/atomic_structure/mod.rs`** — Add diff fields and methods to `AtomicStructure`:
   - Add `is_diff: bool` field (default `false`)
   - Add `anchor_positions: FxHashMap<u32, DVec3>` field (default empty)
   - Add `show_anchor_arrows: bool` field (default `false`) — controls optional anchor arrow visualization during tessellation
   - Add `pub const DELETED_SITE_ATOMIC_NUMBER: i16 = 0;`
   - Add `new_diff()` constructor (creates empty structure with `is_diff = true`)
   - Add getters/setters: `is_diff()`, `set_is_diff()`, `show_anchor_arrows()`, `set_show_anchor_arrows()`, `anchor_position(atom_id)`, `set_anchor_position(atom_id, pos)`, `remove_anchor_position(atom_id)`, `has_anchor_position(atom_id)`
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
   - `pub fn apply_diff(base: &AtomicStructure, diff: &AtomicStructure, tolerance: f64) -> DiffApplicationResult`
   - `DiffApplicationResult` struct: `result: AtomicStructure`, `provenance: DiffProvenance`
   - `DiffProvenance` struct: `sources: FxHashMap<u32, AtomSource>`, `base_to_result: FxHashMap<u32, u32>`, `diff_to_result: FxHashMap<u32, u32>`
   - `AtomSource` enum: `BasePassthrough(u32)`, `DiffMatchedBase { diff_id, base_id }`, `DiffAdded(u32)`
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
   - Provenance: added atom has `DiffAdded` source
   - Provenance: passthrough base atom has `BasePassthrough` source
   - Provenance: replaced atom has `DiffMatchedBase` source
   - Provenance: `base_to_result` and `diff_to_result` reverse maps are correct
   - Provenance: deleted atom absent from all maps (removed from result)

6. **`rust/tests/crystolecule.rs`** — Register the new test module

### Phase 3: EditAtomData Refactoring

Replace the command stack in EditAtomData with the diff. The eval method now delegates to `apply_diff()` from the crystolecule module.

**Files to modify:**

7. **`rust/src/structure_designer/nodes/edit_atom/edit_atom.rs`** — Replace `EditAtomData`:
   - Remove `history: Vec<Box<dyn EditAtomCommand>>`, `next_history_index: usize`, `selection_transform: Option<Transform>`
   - Add `diff: AtomicStructure` (initialized with `AtomicStructure::new_diff()`), `output_diff: bool`, `tolerance: f64`
   - Add `selection: EditAtomSelection` (transient, not serialized)
   - Add `EditAtomEvalCache` struct (follows existing eval cache pattern, see `FacetShellEvalCache` etc.)
   - Rewrite `eval()`:
     - When `output_diff` is false:
       1. Call `apply_diff(input, &self.diff, self.tolerance)` → `DiffApplicationResult { result, provenance }`
       2. Apply selection to result: for each ID in `selection.selected_base_atoms`, look up `provenance.base_to_result` → if found, set `atom.set_selected(true)`; same for `selection.selected_diff_atoms` via `provenance.diff_to_result`. Apply `selection.selected_bonds` to result decorator. Silently skip any stale IDs that don't appear in the provenance maps (eval is `&self`, so selection can't be mutated here; stale entries are harmless and cleaned up lazily during the next selection interaction).
       3. Store provenance in eval cache: `if network_stack.len() == 1 { context.selected_node_eval_cache = Some(Box::new(EditAtomEvalCache { provenance })); }`
       4. Return result
     - When `output_diff` is true: return a clone of `self.diff`
   - Replace command-based mutation methods with direct diff mutation:
     - `add_atom_to_diff(atomic_number, position)` — calls `self.diff.add_atom()`
     - `mark_for_deletion(match_position)` — adds atom with `DELETED_SITE_ATOMIC_NUMBER` at match_position
     - `replace_in_diff(match_position, new_atomic_number)` — adds/updates atom in diff
     - `move_in_diff(atom_id, new_position)` — sets anchor if needed, updates position
     - `add_bond_in_diff(atom_id1, atom_id2, order)` — adds bond in diff structure
     - `remove_from_diff(diff_atom_id)` — removes atom from diff (and its anchor if any)
   - Remove `undo()`, `redo()`, `can_undo()`, `can_redo()`, `add_command()`
   - Keep `active_tool`
   - Update `get_subtitle()`: use `diff_stats()` to show "+N, -M, ~K" summary
   - Update `clone_box()`: clone diff (including anchor_positions) + selection + other fields
   - Update `get_parameter_metadata()`: unchanged (still requires "molecule" input)

8. **Remove command infrastructure:**
   - Delete `rust/src/structure_designer/nodes/edit_atom/edit_atom_command.rs`
   - Delete `rust/src/structure_designer/nodes/edit_atom/commands/` directory (all 7 files)
   - Update `rust/src/structure_designer/nodes/edit_atom/mod.rs`: remove `pub mod commands;` and `pub mod edit_atom_command;`

### Phase 4: Interaction Functions Refactoring

Rewrite the public interaction functions that were previously creating commands. These stay in the edit_atom module (they are specific to edit_atom's interactive workflow) but now directly mutate the diff.

**Files to modify:**

9. **`rust/src/structure_designer/nodes/edit_atom/edit_atom.rs`** (interaction functions):

   **Provenance access:** Interaction functions retrieve the `DiffProvenance` from the eval cache via `structure_designer.get_selected_node_eval_cache()` → `downcast_ref::<EditAtomEvalCache>()`. This follows the same pattern used by `facet_shell::select_facet_by_ray()`, `drawing_plane::provide_gadget()`, etc. The provenance is always fresh because it was computed during the most recent evaluation.

   - `select_atom_or_bond_by_ray()` — Ray hit test on the evaluated result → `result_atom_id`. Look up `provenance.sources[result_atom_id]` to determine origin. Store the atom's base or diff ID in `edit_atom_data.selection`:
     - `BasePassthrough(base_id)` → add/toggle `base_id` in `selection.selected_base_atoms`
     - `DiffMatchedBase { diff_id, .. }` → add/toggle `diff_id` in `selection.selected_diff_atoms`
     - `DiffAdded(diff_id)` → add/toggle `diff_id` in `selection.selected_diff_atoms`
     - Handle `SelectModifier` (Replace/Expand/Toggle) as before.
     - For bond hits: store `BondReference` in `selection.selected_bonds` (result-space).
     - Recalculate `selection.selection_transform` from positions of selected result atoms.
     - No diff mutation. Selection is purely transient.

   - `add_atom_by_ray()` — Calculate position (ray-plane intersection, same as current), then call `edit_atom_data.add_atom_to_diff(atomic_number, position)`. Clear `selection.selected_bonds` (diff changed).

   - `draw_bond_by_ray()` — Same two-click workflow. On second click, call `edit_atom_data.add_bond_in_diff(atom_id1, atom_id2, 1)`. The atom IDs are diff-internal IDs. If bonding involves a base atom not yet in the diff, it must be added to the diff first (at its current position, no anchor) so both atoms are "from the diff" for bond resolution. Clear `selection.selected_bonds`.

   - `delete_selected_atoms_and_bonds()` — Iterate selection by provenance category:
     - For each `base_id` in `selection.selected_base_atoms`: add a delete marker at that atom's position via `mark_for_deletion()`. Remove from `selected_base_atoms`.
     - For each `diff_id` in `selection.selected_diff_atoms`:
       - If the diff atom is a pure addition (no anchor, no base match): remove from diff via `remove_from_diff()`.
       - If the diff atom matched a base atom (has anchor or was a replacement): replace with delete marker (keep position/anchor for matching).
       - Remove from `selected_diff_atoms`.
     - For bonds: if both atoms are in the diff, remove the bond from the diff. Otherwise, ensure both atoms are in the diff and omit the bond. Clear `selected_bonds`.

   - `replace_selected_atoms()` — For each selected atom:
     - `diff_id` in `selected_diff_atoms`: update its `atomic_number` in the diff. Selection unchanged.
     - `base_id` in `selected_base_atoms`: add to diff with the new `atomic_number` at the base atom's position. Move from `selected_base_atoms` to `selected_diff_atoms` (new diff ID). Clear `selected_bonds`.

   - `transform_selected()` — For each selected atom:
     - `diff_id` in `selected_diff_atoms`:
       - If added atom (no anchor): update position directly. Selection unchanged.
       - If matched atom (has anchor): update position, anchor stays at original base position. Selection unchanged.
     - `base_id` in `selected_base_atoms`: add to diff at base atom's position, set anchor to base position, update position to new position. Move from `selected_base_atoms` to `selected_diff_atoms` (new diff ID).
     - Recalculate `selection.selection_transform`. Clear `selected_bonds`.

   **Key design point:** Interaction functions know atom provenance from the `EditAtomSelection` itself — `selected_base_atoms` and `selected_diff_atoms` are separate sets. The eval cache provenance is only needed during `select_atom_or_bond_by_ray()` to classify a newly-clicked result atom. For mutation functions (delete, replace, transform), the selection already tells us which atoms are base vs. diff.

### Phase 5: API Layer

**Files to modify:**

10. **`rust/src/api/structure_designer/edit_atom_api.rs`** — Update API functions:
    - Remove `edit_atom_undo()`, `edit_atom_redo()`
    - Add `toggle_edit_atom_output_diff()` — toggles `edit_atom_data.output_diff`
    - Add `toggle_edit_atom_show_anchor_arrows()` — toggles `edit_atom_data.show_anchor_arrows`
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
        pub show_anchor_arrows: bool,
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
    - Read `has_selected_atoms`, `has_selection` from the **evaluated result** atomic structure (same as current code — this correctly reflects only visible selected atoms, excluding any stale IDs in `EditAtomSelection`)
    - Read `selection_transform` from `edit_atom_data.selection.selection_transform`
    - Remove `refresh_scene_dependent_edit_atom_data()` — no longer needed

### Phase 6: Serialization

**Files to modify:**

13. **`rust/src/structure_designer/serialization/edit_atom_data_serialization.rs`** — Complete rewrite:
    - Serialize the diff `AtomicStructure` including atoms, bonds, `is_diff`, and `anchor_positions`
    - Serialize `output_diff: bool` and `tolerance: f64`
    - Active tool state, `EditAtomSelection`, and `EditAtomEvalCache` are NOT serialized (transient UI state)
    - AtomicStructure already has serialization infrastructure for `.cnnd` — extend it to include anchor positions when `is_diff = true`

14. **`rust/src/crystolecule/atomic_structure/mod.rs`** (serialization aspect) — The `AtomicStructure` serialization (used by `.cnnd` save/load) must be updated to persist:
    - `is_diff` flag
    - `anchor_positions` map (only when `is_diff = true`)
    - `show_anchor_arrows` flag (only when `is_diff = true`)
    - These fields are optional in the format — absent means `is_diff = false`, empty anchors, `show_anchor_arrows = false` (backward compatible)

15. **`rust/src/structure_designer/node_type_registry.rs`** — Update the `edit_atom` node type entry's `node_data_saver` and `node_data_loader` to use the new serialization.

16. **Migration of old `.cnnd` files:**
    - Detect old format by presence of `history` key in JSON
    - Replay the old commands on an empty `AtomicStructure` to produce the net effect
    - Convert the result into a diff: compare against an empty base to identify all atoms as "added"
    - This is a lossy but correct migration — the diff captures the net effect of all commands
    - Selection commands are discarded (they were transient anyway)

### Phase 7: Selection Model

The selection model is the most architecturally significant change from the command stack design. In the old model, selection was a command in the history (persistent, serialized, replayed). In the new model, selection is fully transient UI state stored separately from the diff.

#### Current selection system (being replaced)

| Aspect | Current implementation |
|--------|----------------------|
| Atom selection storage | `Atom.flags` bit 0 (`ATOM_FLAG_SELECTED`), set during command replay |
| Bond selection storage | `HashSet<BondReference>` in `AtomicStructureDecorator.selected_bonds` |
| Selection trigger | `SelectCommand` added to command history |
| Selection transform | Two redundant copies: on decorator (set by commands) and on `EditAtomData` (synced by `refresh_scene_dependent_edit_atom_data()`) |
| Serialization | SelectCommands serialized in `.cnnd` history — **the core problem** |
| Undo/redo | Selection changes are undoable (they're commands) |
| Rendering | `atom.is_selected()` → magenta, lower roughness; `decorator.is_bond_selected()` → magenta |

#### New selection system

17. **`EditAtomSelection` — provenance-based selection state:**

    Selection is stored as two sets of atom IDs categorized by **provenance** (origin), not by result atom IDs:

    ```rust
    pub struct EditAtomSelection {
        pub selected_base_atoms: HashSet<u32>,      // Base atom IDs (stable — input doesn't change)
        pub selected_diff_atoms: HashSet<u32>,      // Diff atom IDs (stable — we control the diff)
        pub selected_bonds: HashSet<BondReference>,  // Result-space bond refs (cleared on diff mutation)
        pub selection_transform: Option<Transform>,  // Cached, recalculated after selection changes
    }
    ```

    **Why provenance-based:** Result atom IDs are assigned fresh by `apply_diff()` on each evaluation. If the diff changes (user adds an atom), result IDs can shift. Base atom IDs and diff atom IDs are stable across re-evaluations — base IDs because the input is immutable during editing, diff IDs because they're internal to the diff `AtomicStructure` we control.

    **Bond exception:** Bond selection is stored in result space and cleared on any diff mutation. Bond selection is typically short-lived (select bond → delete), and tracking bond provenance across base+diff would be complex for little benefit.

18. **`EditAtomEvalCache` — provenance via eval cache:**

    The eval cache stores the `DiffProvenance` computed during the most recent `apply_diff()` call. This follows the established eval cache pattern used by 14+ existing nodes (e.g., `FacetShellEvalCache`, `DrawingPlaneEvalCache`).

    ```rust
    #[derive(Debug, Clone)]
    pub struct EditAtomEvalCache {
        pub provenance: DiffProvenance,
    }
    ```

    **Lifecycle:**
    - **Populated:** During `EditAtomData::eval()` when `network_stack.len() == 1` (root-level evaluation), via `context.selected_node_eval_cache = Some(Box::new(EditAtomEvalCache { provenance }))`.
    - **Retrieved:** By `select_atom_or_bond_by_ray()` and other interaction functions via `structure_designer.get_selected_node_eval_cache()` → `downcast_ref::<EditAtomEvalCache>()`.
    - **Invalidated:** Automatically when the node is re-evaluated (new provenance replaces old). Also invalidated when node data changes and downstream caches are cleared.
    - **Cached when invisible:** Travels with `NodeSceneData` into the invisible node cache, restored on visibility without re-evaluation.

    **Why eval cache, not a field on EditAtomData:** The provenance is a byproduct of evaluation, not part of the node's persistent or UI state. The eval cache is the established mechanism for passing evaluation-computed values to interaction functions. Storing it on `EditAtomData` would conflate transient evaluation state with node data.

19. **Selection flow — user clicks to select:**

    1. Ray hit test on evaluated result → `result_atom_id`
    2. Retrieve `EditAtomEvalCache` from eval cache → `provenance`
    3. Look up `provenance.sources[result_atom_id]`:
       - `BasePassthrough(base_id)` → add/toggle `base_id` in `selection.selected_base_atoms`
       - `DiffMatchedBase { diff_id, .. }` → add/toggle `diff_id` in `selection.selected_diff_atoms`
       - `DiffAdded(diff_id)` → add/toggle `diff_id` in `selection.selected_diff_atoms`
    4. Handle `SelectModifier` (Replace clears all first, Expand adds, Toggle inverts)
    5. Recalculate `selection.selection_transform` via `calc_selection_transform()` on the result atoms

20. **Selection rendering — during eval:**

    After `apply_diff()` produces the result and provenance:
    1. For each `base_id` in `selection.selected_base_atoms`: look up `provenance.base_to_result[base_id]` → if found, `result_id`, set `result.get_atom_mut(result_id).set_selected(true)`. If `base_id` not in map (atom was deleted by a delete marker in the diff), silently skip — eval is `&self` and cannot mutate selection. Stale entries are harmless and cleaned up lazily during the next selection interaction.
    2. For each `diff_id` in `selection.selected_diff_atoms`: look up `provenance.diff_to_result[diff_id]` → if found, set `result.get_atom_mut(result_id).set_selected(true)`. Same stale-skip logic.
    3. Apply `selection.selected_bonds` to result decorator.
    4. Tessellator renders selection exactly as before — no tessellator changes needed for selection (only for diff visualization in Phase 8).
    5. The API reads `has_selected_atoms` / `has_selection` from the **result** (where flags are correctly set only for existing atoms), not from `EditAtomSelection` (which may contain stale IDs). This ensures the UI doesn't show active Delete/Replace buttons for a phantom selection.

21. **Selection updates during mutations:**

    When interaction functions mutate the diff, they update the selection to stay consistent:

    | Mutation | Selection update |
    |----------|-----------------|
    | Delete base atom (add delete marker) | Remove `base_id` from `selected_base_atoms` |
    | Delete diff-added atom (remove from diff) | Remove `diff_id` from `selected_diff_atoms` |
    | Delete diff-matched atom (replace with delete marker) | Remove `diff_id` from `selected_diff_atoms` |
    | Replace base atom (add to diff with new element) | Move from `selected_base_atoms` to `selected_diff_atoms` (new diff ID) |
    | Replace diff atom (update element in diff) | No change (same `diff_id`) |
    | Transform base atom (add to diff with anchor) | Move from `selected_base_atoms` to `selected_diff_atoms` (new diff ID) |
    | Transform diff atom (update position in diff) | No change (same `diff_id`) |
    | Any diff mutation | Clear `selected_bonds` |

22. **Input change handling:**

    If the input structure changes (upstream node modified), `selected_base_atoms` may contain stale IDs. The selection rendering step (item 20 above) handles this gracefully: stale IDs that don't appear in the provenance maps are silently cleaned up. This is acceptable because when the input changes, the user's spatial context has shifted anyway.

23. **What is NOT serialized:**

    - `EditAtomSelection` — entirely transient, reset to empty on load
    - `EditAtomEvalCache` — eval cache is always transient (rebuilt on evaluation)
    - `active_tool` — reset to Default on load (same as current behavior)
    - Selection is NOT undoable (no command to undo). This is intentional — the plan removes undo/redo entirely. If undo is added later (via diff snapshots), selection would still be separate.

24. **Replaces from current model:**

    | Current | New |
    |---------|-----|
    | `SelectCommand` in command history | Direct mutation of `EditAtomSelection` |
    | Selection serialized in `.cnnd` | Selection NOT serialized |
    | Selection reconstructed by command replay | Selection applied to result from base/diff ID sets via provenance |
    | `selection_transform` on decorator + `EditAtomData` | Single location: `EditAtomSelection.selection_transform` |
    | `refresh_scene_dependent_edit_atom_data()` | No longer needed — API reads from `EditAtomSelection` directly |
    | `match_diff_atoms()` called per interaction | Provenance cached in eval cache; only needed at selection time |

### Phase 8: Diff Visualization

The rendering system uses PBR materials with `albedo`, `roughness`, and `metallic` — no opacity channel, no alpha blending, no depth sorting for transparency. All atoms render as solid spheres with element-based colors (from `ATOM_INFO`). Selected atoms override the color to magenta with lower roughness. The decorator provides `AtomDisplayState` (Normal, Marked, SecondaryMarked) for crosshair overlays. Bonds render as cylinders.

The diff visualization leverages this existing infrastructure with **minimal changes** — no new render pipelines, no transparency, no new display states.

25. **`rust/src/display/atomic_tessellator.rs`** — Two small changes to `get_atom_color_and_material()`:

    **a) Delete marker rendering (atomic_number = 0):**
    - In `get_atom_color_and_material()`, add a check: if `atom.atomic_number == 0` (delete marker), return a fixed color and radius instead of looking up `ATOM_INFO` (which has no entry for atomic_number 0).
    - Color: solid red, e.g., `Vec3::new(0.9, 0.1, 0.1)`. Roughness: `0.5`. Metallic: `0.0`.
    - Radius: fixed `0.5` Angstrom (a reasonable small sphere; covalent radii range ~0.3–1.5 A).
    - No transparency, no X overlay, no special glyph — just a red sphere. The renderer already handles arbitrary colors per atom; this is a one-line branch.

    **b) Normal diff atoms (additions, replacements, moves):**
    - Rendered with their **standard element color** — no special color coding for "added" vs. "modified". A carbon in the diff looks like any other carbon.
    - This requires **zero rendering changes**. The tessellator already uses `atom.atomic_number` to look up the element color.

    **c) Anchor arrow visualization (optional, controlled by `show_anchor_arrows: bool` on `AtomicStructure`):**
    - When `is_diff = true` and `show_anchor_arrows = true`, the tessellator iterates `anchor_positions`. For each entry `(atom_id, anchor_pos)`:
      1. Render a small delete-marker-style sphere at `anchor_pos` (same red color, smaller radius e.g. `0.3` A) to show "this is where the atom was matched".
      2. Render a thin cylinder from `anchor_pos` to the atom's current position (reusing the existing bond cylinder tessellation with a small radius e.g. `0.05` A, colored e.g. orange `Vec3::new(1.0, 0.6, 0.0)`).
    - This reuses existing geometry primitives (sphere + cylinder) with no new rendering infrastructure.
    - **Impostor compatibility:** Both tessellation paths (triangle mesh and impostor) are supported. The impostor path's `AtomImpostorMesh::add_atom_quad()` and `BondImpostorMesh::add_bond_quad()` take arbitrary positions, radii, and colors — no atom references needed. The anchor arrow loop adds to the same impostor meshes that regular atoms and bonds use. The anchor sphere is an `add_atom_quad()` call; the arrow cylinder is an `add_bond_quad(anchor_pos, atom_pos, thin_radius, orange_color)` call.
    - Default: `show_anchor_arrows = false`. The flag lives on `AtomicStructure` alongside `is_diff` and `anchor_positions`, so it's available at tessellation time.
    - When `show_anchor_arrows = false` or `is_diff = false`: no extra geometry, zero performance impact.

    **Total rendering changes:** One branch in `get_atom_color_and_material()` for atomic_number=0, plus an optional anchor arrow loop in both `tessellate_atomic_structure()` and `tessellate_atomic_structure_impostors()`. No shader changes, no pipeline changes, no new Material fields.

26. **Display mode in the edit_atom node:**
    - When `output_diff = false` (default): the node outputs the applied result. Rendering shows the final structure with standard element colors. This is the normal editing workflow.
    - When `output_diff = true`: the node outputs the diff itself. The `is_diff = true` flag causes delete markers to render as red spheres. Added/modified atoms render with their normal element colors. If `show_anchor_arrows` is enabled, movement arrows are shown.
    - The user toggles `show_anchor_arrows` independently of `output_diff` (both are on the node data). Arrows are only meaningful when viewing the diff, but the flag is harmless on non-diff structures (no anchors → no arrows).

### Phase 9: Flutter UI Changes

**Files to modify:**

27. **`lib/structure_designer/node_data/edit_atom_editor.dart`** — UI changes:
    - **Remove:** Undo/Redo buttons from the header row
    - **Add:** Output mode toggle in the header — a segmented button or toggle switch labeled "Result" / "Diff"
    - **Add:** "Show Arrows" checkbox/toggle — visible when in Diff output mode, toggles `show_anchor_arrows`
    - **Add:** Diff statistics display below the header — show e.g., "+3 atoms, -1 atom, ~2 modified" from `APIDiffStats`
    - **Keep:** Tool selector (Default, AddAtom, AddBond) — tools are unchanged in concept
    - **Keep:** Element selector — used for add/replace operations
    - **Keep:** Replace/Delete buttons — they now mutate the diff directly
    - **Keep:** Transform controls — they now update positions in the diff
    - **Contextual delete button:**
      - If a diff-added atom is selected: "Remove from diff" semantics (removes it entirely)
      - If a base-matched atom is selected: "Mark for deletion" semantics (adds delete marker)
      - The button can remain labeled "Delete Selected" for simplicity; the underlying behavior differs based on context

28. **`lib/structure_designer/structure_designer_model.dart`** — Update model methods:
    - Remove `editAtomUndo()`, `editAtomRedo()`
    - Add `toggleEditAtomOutputDiff()` — calls new API function
    - Add `toggleEditAtomShowAnchorArrows()` — calls new API function
    - Update `refreshFromKernel()` data fetch to populate new `APIEditAtomData` fields (output_diff, show_anchor_arrows, diff_stats)

29. **`lib/structure_designer/structure_designer_viewport.dart`** — No changes expected. Ray-cast interactions call the same API functions; the implementations change internally but the viewport code doesn't need to know.

### Phase 10: Text Format Integration

30. **`rust/src/structure_designer/text_format/`** — If edit_atom exposes text properties:
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
| `rust/src/structure_designer/nodes/edit_atom/edit_atom.rs` | New data model (`EditAtomData` with `EditAtomSelection`), new eval using `apply_diff()` with eval cache (`EditAtomEvalCache`), provenance-based selection, new diff-mutating interaction functions |
| `rust/src/structure_designer/serialization/edit_atom_data_serialization.rs` | Complete rewrite for diff-based serialization |
| `rust/src/api/structure_designer/edit_atom_api.rs` | Remove undo/redo, add output toggle |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Update `APIEditAtomData`, add `APIDiffStats` |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Update `get_edit_atom_data()`, remove `refresh_scene_dependent_edit_atom_data()` |
| `rust/src/structure_designer/structure_designer.rs` | Remove `refresh_scene_dependent_edit_atom_data()` |
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
| `rust/src/display/atomic_tessellator.rs` | One branch for delete marker color/radius in `get_atom_color_and_material()`, optional anchor arrow loop (sphere + cylinder reuse) |
| `rust/tests/crystolecule.rs` | Register new test module |

## Dependency Order

The implementation phases have a clear dependency chain:

```
Phase 1: AtomicStructure extensions (is_diff, anchors, delete marker constant)
    ↓
Phase 2: apply_diff() + DiffProvenance in crystolecule + tests
    ↓
Phase 3: EditAtomData refactoring (uses apply_diff, introduces EditAtomEvalCache)
Phase 7: Selection model (EditAtomSelection, provenance-based)  ← integrated with Phase 3/4
    ↓
Phase 4: Interaction functions (uses EditAtomSelection + eval cache provenance)
    ↓
Phase 5: API layer (exposes new interaction functions, reads from EditAtomSelection)
Phase 6: Serialization (persists new data model)    ← can be parallel with Phase 5
    ↓
Phase 8: Diff visualization (rendering)
    ↓
Phase 9: Flutter UI (consumes API changes)
    ↓
Phase 10: Text format (lower priority, can be deferred)
```

**Note:** Phase 7 is not a standalone late phase — the selection model is part of the core EditAtomData design (Phase 3) and the interaction functions (Phase 4). It's listed as a separate phase for documentation clarity, but implementation-wise `EditAtomSelection` and `EditAtomEvalCache` are built alongside Phase 3/4.

## Open Questions for Future Sessions

1. **Diff composition** — When two edit_atom nodes are chained, the second operates on the result of the first. Could their diffs be composed into one? This is a future optimization.

2. **Multi-output support** — When the node can output both the result and the diff simultaneously, the `output_diff` boolean becomes unnecessary. This depends on broader node network infrastructure.

3. **Stamping workflow** — The full UX for "apply this diff at multiple positions in a crystal" (e.g., T-center defect stamping) is a separate design task. The diff will flow as a regular `NetworkResult::Atomic` with `is_diff = true` into a future stamp node that calls `apply_diff()`.

4. **Anchor position alternatives** — The current anchor map approach works well. If we discover cases where a richer position-transformation model is needed (e.g., parametric displacements), this can be extended later without changing the fundamental architecture.
