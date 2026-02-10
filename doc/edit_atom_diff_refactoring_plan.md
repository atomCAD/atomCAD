# New `atom_edit` Node: Diff-Based Atomic Structure Editing

## Motivation

The current `edit_atom` node uses a command stack pattern: every user interaction (select, add atom, delete, replace, move) is stored as a command object. Evaluation replays all commands sequentially. This has several drawbacks:

- **Selection is persisted in the command history.** UI-only operations (clicking to select atoms) are stored forever and serialized to `.cnnd` files. A node with 20 real edits might have 100+ selection commands.
- **Commands reference atoms by ID.** Atom IDs are assigned during evaluation and depend on the input structure. If the input changes (upstream node modified), IDs can silently point to wrong atoms.
- **The history is opaque.** Users see "ops: 47" but cannot understand what the node does without mentally replaying the sequence.
- **Not composable.** The edit cannot be extracted and reapplied elsewhere (e.g., stamping a defect into a crystal lattice at multiple positions).

Rather than refactoring `edit_atom` in-place, we create a **new `atom_edit` node** with a clean diff-based design. The old `edit_atom` node remains unchanged and will be deprecated gradually. This avoids the need for `.cnnd` migration and allows the two nodes to coexist during the transition.

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

### Delete Markers

**Atom delete marker:**
- A constant `pub const DELETED_SITE_ATOMIC_NUMBER: i16 = 0;` on `AtomicStructure` or at module level.
- `Atom::is_delete_marker(&self) -> bool` helper method.
- In diff visualization mode, render atom delete markers as red solid spheres.

**Bond delete marker:**
- A constant `pub const BOND_DELETED: u8 = 0;` alongside existing bond order constants (`BOND_SINGLE=1` through `BOND_METALLIC=7`). Bond order 0 is currently unused and semantically means "no bond."
- `InlineBond::is_delete_marker(&self) -> bool` helper (returns `self.bond_order() == BOND_DELETED`).
- A bond with order 0 in the diff means "explicitly delete this base bond." This parallels `atomic_number = 0` for atom deletion.
- Bond delete markers are only meaningful in diff structures (`is_diff = true`). Non-diff structures should never contain them.

### Diff Application Algorithm

`apply_diff()` lives in the **crystolecule module** (`atomic_structure_diff.rs`), since diff application is a fundamental operation on atomic structures, not specific to the node network or the `atom_edit` node. Future stamp nodes and any other consumer can use it directly.

**Signature:**
```rust
pub fn apply_diff(base: &AtomicStructure, diff: &AtomicStructure, tolerance: f64) -> DiffApplicationResult
```

**Return type:**
```rust
pub struct DiffApplicationResult {
    pub result: AtomicStructure,
    pub provenance: DiffProvenance,
    pub stats: DiffStats,
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

pub struct DiffStats {
    pub atoms_added: u32,
    pub atoms_deleted: u32,
    pub atoms_modified: u32,
    pub bonds_added: u32,
    pub bonds_deleted: u32,
}
```

The provenance is a zero-cost byproduct of the matching that `apply_diff()` already performs internally. Returning it enables the selection model (Phase 7) and interaction functions (Phase 4) to know the origin of every result atom without re-running the matching. The stats are also computed as a byproduct of the same matching pass — no separate `diff_stats()` function needed.

**Algorithm:**

1. **Atom matching:** For each atom in the diff:
   - Determine match position: if `diff.anchor_positions` contains this atom's ID, use the anchor position; otherwise use the atom's own position.
   - Find the nearest unmatched atom in the base structure within `tolerance` of the match position.
   - Use greedy nearest-first assignment: process diff atoms sorted by closest match distance to avoid ambiguity.
   - **Limitation:** Greedy matching can produce suboptimal results when diff atoms are clustered near multiple base atoms. For small diffs (the expected use case), this is acceptable. If edge cases arise, the matching can be upgraded to bipartite matching (Hungarian algorithm) later without changing the public API.

2. **Atom effects:**
   - **Match found + normal atom:** The base atom is replaced by the diff atom's properties (element, position). This handles element replacement, position changes (via anchors), or both.
   - **Match found + delete marker (atomic_number = 0):** The matched base atom is removed.
   - **No match + normal atom:** The atom is added to the result (new atom insertion).
   - **No match + delete marker:** Ignored (trying to delete something that doesn't exist).

3. **Bond resolution:** Base bonds survive by default; the diff can override or explicitly delete them.

   The algorithm iterates bonds, not atom pairs (avoiding O(n²)):

   **Step 3a — Base bond pass-through:** For each bond in the base structure between atoms `(base_a, base_b)`:
   - If neither base atom was matched by a diff atom: copy bond to result unchanged (using remapped result atom IDs).
   - If exactly one was matched: copy bond to result unchanged. The matched atom inherits base bonds to non-diff neighbors automatically.
   - If both were matched by diff atoms `(diff_a, diff_b)`: check if the diff contains a bond between `diff_a` and `diff_b`:
     - If diff bond with `bond_order > 0`: use the diff's bond (override).
     - If diff bond with `bond_order = 0` (delete marker): no bond in the result (explicit deletion).
     - If no diff bond between them: use the base bond (bonds survive by default).

   **Step 3b — New diff bonds:** For each bond in the diff between `(diff_a, diff_b)` that was NOT already processed in step 3a (i.e., at least one of the diff atoms is an addition with no base match, or the bond is between two diff atoms that didn't both match base atoms that were bonded):
   - If `bond_order > 0`: add the bond to the result (new bond).
   - If `bond_order = 0`: skip (delete marker for a bond that doesn't exist in the base — no-op).

   This design means:
   - Replacing the element of two bonded atoms preserves their bond automatically — no interaction function bookkeeping needed.
   - Moving two bonded atoms preserves their bond automatically.
   - New bonds between diff atoms are added explicitly in the diff.
   - Bond deletion requires both atoms in the diff and a delete marker bond (`bond_order = 0`) between them.

   **Identity entries in the diff:** When an interaction function needs to add a bond delete marker between two base atoms, both must be present in the diff as endpoints. If a base atom isn't already in the diff, it's added as an "identity entry" — same position, same element, no anchor. During `apply_diff()`, this identity entry matches its base counterpart and produces a `DiffMatchedBase` with identical properties. This is harmless — the result is the same atom with the same properties, and the bond resolution correctly processes the delete marker between them.

4. **Unmatched base atoms:** Pass through to the result with their original properties and bonds.

5. **Result:** A `DiffApplicationResult` containing a new `AtomicStructure` with `is_diff = false` (fully resolved), a `DiffProvenance` mapping every result atom to its origin, and `DiffStats` summarizing the changes.

### Position Matching Tolerance

- Default tolerance: **0.1 Angstrom** (well below typical bond lengths ~1.0-1.5 A, well above numerical noise ~1e-10).
- Configurable per `atom_edit` node if edge cases arise, but the default should work for virtually all scenarios.
- Matching algorithm: greedy nearest-first assignment. For each diff atom, find the nearest unmatched base atom within tolerance of the match position. Process diff atoms in order of closest match distance first. The base structure's existing spatial grid (4.0 Å cells) can be used for efficient neighbor lookups.

### Handling Atom Movement (Anchor Positions)

When a user moves an atom from position P_old to P_new:

1. The atom is added to (or updated in) the diff at position P_new.
2. An anchor position is recorded: `diff.set_anchor_position(atom_id, P_old)`.
3. During `apply_diff()`, the anchor position P_old is used for matching against the base, then the atom is placed at P_new in the result.

**Why this is better than delete+add:**
- A single atom in the diff represents the move (not a delete marker + a separate new atom).
- Bonds to non-diff neighbors are preserved automatically (base bonds survive by default). No need to include neighbors in the diff.
- Example: Moving a carbon in diamond (4 bonds) requires just 1 atom in the diff with an anchor. Delete+add would require 1 delete marker + 1 new atom + 4 neighbors + all their inter-bonds (~6 atoms, ~10+ bonds).
- Moving two bonded atoms simultaneously works correctly: both enter the diff with anchors, and their mutual bond survives from the base (no explicit bond needed in the diff).

**Anchor lifecycle:**
- First move: anchor is set to the base atom's current position.
- Subsequent moves: anchor stays at the original base position; only the diff atom's position updates.
- Move back to original position: anchor and position coincide. Could be cleaned up (atom removed from diff) or left as-is (harmless identity edit).
- Moving an already-added atom (no base match): no anchor needed — just update position directly.

### Why enrich AtomicStructure rather than a separate AtomicDiff type

Once an AtomicStructure contains delete markers (atomic_number = 0), it already carries diff semantics. The anchor map is a natural extension. A separate wrapper type (`AtomicDiff`) would create a parallel world: a new `NetworkResult` variant, duplicate visualization paths, a new `DataType` for pins, and constant unwrapping. Enriching AtomicStructure avoids all of this while adding negligible memory overhead (~56 bytes for non-diff structures: one bool + an empty hashmap).

**Downstream node safety:** When `output_diff = true`, the `atom_edit` node outputs a structure with `is_diff = true`. Downstream nodes that don't understand diffs (e.g., `atom_fill`, `atom_union`) will process the structure literally — delete markers (atomic_number = 0) would be treated as regular atoms, which produces wrong results. This is acceptable because `output_diff` is a debugging/visualization toggle, not a normal data flow mode. The primary output (`output_diff = false`) always produces a fully-resolved non-diff structure. A future enhancement could add a type-system-level guard (e.g., a `DiffAtomic` DataType variant) if diff structures need to flow to stamp nodes, but this is out of scope for the initial implementation.

## Data Model Changes

### Rust: `AtomicStructure` (extended)

```rust
pub struct AtomicStructure {
    // ... existing fields (atoms, grid, num_atoms, num_bonds, decorator, frame_transform) ...
    is_diff: bool,
    anchor_positions: FxHashMap<u32, DVec3>,
}
```

### Rust: `AtomEditData` (new struct)

```rust
AtomEditData {
    // Persistent (serialized to .cnnd)
    diff: AtomicStructure,                  // The diff (is_diff = true)
    output_diff: bool,                      // When true, output the diff instead of the result
    show_anchor_arrows: bool,               // When true + output_diff, render anchor arrows in diff view
    tolerance: f64,                         // Positional matching tolerance (default 0.1)

    // Transient (NOT serialized)
    selection: AtomEditSelection,           // Current selection state
    active_tool: AtomEditTool,              // Current editing tool
}
```

### Rust: `AtomEditSelection` (new, transient)

Selection is stored by **provenance** (base/diff atom IDs) rather than result atom IDs. This makes selection stable across re-evaluations, since base IDs are immutable and diff IDs are under our control.

```rust
pub struct AtomEditSelection {
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

### Rust: `AtomEditEvalCache` (new, transient)

Follows the existing eval cache pattern (see `FacetShellEvalCache`, `DrawingPlaneEvalCache`, etc.). Stored in `NodeSceneData.selected_node_eval_cache` during evaluation, retrieved via `structure_designer.get_selected_node_eval_cache()`.

```rust
#[derive(Debug, Clone)]
pub struct AtomEditEvalCache {
    pub provenance: DiffProvenance,
    pub stats: DiffStats,
}
```

The provenance maps result atom IDs to their base/diff origins. Used by selection and interaction functions to determine atom provenance without re-running the matching algorithm. The stats are used by `get_subtitle()` and `get_atom_edit_data()` without re-running `apply_diff()`.

### Relationship to old `edit_atom` node

The old `edit_atom` node and all its infrastructure (`EditAtomData`, `EditAtomCommand`, `commands/` directory, `edit_atom_data_serialization.rs`, `edit_atom_api.rs`) remain **completely untouched**. The new `atom_edit` node is a separate, parallel implementation:

- `AtomEditTool` enum (Default, AddAtom, AddBond) — same tools as `EditAtomTool`, fresh implementation
- Tool-specific state (replacement_atomic_number, last_atom_id, etc.) — same concept, fresh structs
- Atom selection rendering: `atom.is_selected()` flag → magenta color (unchanged tessellator behavior, shared)
- Bond selection rendering: `decorator.is_bond_selected()` → magenta color (unchanged, shared)
- `calc_selection_transform()` utility — shared, called explicitly after selection changes

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

2. **`rust/src/crystolecule/atomic_structure/atom.rs`** — Add atom delete marker helper:
   - `pub fn is_delete_marker(&self) -> bool` (returns `self.atomic_number == DELETED_SITE_ATOMIC_NUMBER`)

   **`rust/src/crystolecule/atomic_structure/inline_bond.rs`** — Add bond delete marker:
   - Add `pub const BOND_DELETED: u8 = 0;` alongside existing `BOND_SINGLE=1` through `BOND_METALLIC=7`
   - Add `pub fn is_delete_marker(&self) -> bool` (returns `self.bond_order() == BOND_DELETED`)

   **`rust/src/crystolecule/atomic_structure/atomic_structure_decorator.rs`** — Add anchor arrow rendering hint:
   - Add `pub show_anchor_arrows: bool` field (default `false`)
   - This is a transient rendering hint, consistent with the decorator's existing role (display states, bond selection, `from_selected_node`)

### Phase 2: Diff Application Algorithm

Implement the core diff application logic in the crystolecule module. This is independent of the node network and usable by `atom_edit`, future stamp nodes, or any other consumer.

**Files to create:**

3. **`rust/src/crystolecule/atomic_structure_diff.rs`** (new file) — The core diff application:
   - `pub fn apply_diff(base: &AtomicStructure, diff: &AtomicStructure, tolerance: f64) -> DiffApplicationResult`
   - `DiffApplicationResult` struct: `result: AtomicStructure`, `provenance: DiffProvenance`, `stats: DiffStats`
   - `DiffProvenance` struct: `sources: FxHashMap<u32, AtomSource>`, `base_to_result: FxHashMap<u32, u32>`, `diff_to_result: FxHashMap<u32, u32>`
   - `AtomSource` enum: `BasePassthrough(u32)`, `DiffMatchedBase { diff_id, base_id }`, `DiffAdded(u32)`
   - `DiffStats` struct: `atoms_added: u32, atoms_deleted: u32, atoms_modified: u32, bonds_added: u32, bonds_deleted: u32` — computed as a byproduct of the matching, not via a separate function
   - Internal: `match_diff_atoms()` — greedy nearest-first positional matching with anchor support
   - Internal: `resolve_bonds()` — bond resolution using the two-step algorithm described above (step 3a: base bond pass-through, step 3b: new diff bonds)

4. **`rust/src/crystolecule/mod.rs`** — Add `pub mod atomic_structure_diff;`

5. **`rust/tests/crystolecule/atomic_structure_diff_test.rs`** (new file) — Comprehensive tests:
   - Add atom to structure (no match → added)
   - Delete atom by position match (delete marker)
   - Replace element at matched position
   - Move atom via anchor position (verify bonds to non-diff neighbors preserved)
   - Bond resolution: both atoms in diff, diff bond with order > 0 overrides base bond
   - Bond resolution: both atoms in diff, diff bond delete marker (order = 0) removes base bond
   - Bond resolution: both atoms in diff, no diff bond → base bond survives by default
   - Bond resolution: one atom in diff, one not → base bond survives
   - Bond resolution: neither atom in diff → base bond untouched
   - Bond resolution: replacing element of two bonded atoms preserves their bond (no explicit bond in diff needed)
   - Bond resolution: identity entry in diff (same position/element, no anchor) matches base atom correctly
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
   - Stats: counts match expected values for each test case

6. **`rust/tests/crystolecule.rs`** — Register the new test module

### Phase 3: AtomEditData Implementation

Create the new `atom_edit` node with diff-based data model. The eval method delegates to `apply_diff()` from the crystolecule module.

**Files to create:**

7. **`rust/src/structure_designer/nodes/atom_edit/`** — New node directory:

   **`rust/src/structure_designer/nodes/atom_edit/mod.rs`**:
   - `pub mod atom_edit;`

   **`rust/src/structure_designer/nodes/atom_edit/atom_edit.rs`** — `AtomEditData` implementing `NodeData`:
   - `AtomEditData` struct with fields: `diff: AtomicStructure`, `output_diff: bool`, `show_anchor_arrows: bool`, `tolerance: f64`, `selection: AtomEditSelection`, `active_tool: AtomEditTool`
   - `AtomEditSelection` struct (provenance-based selection state, see Data Model section)
   - `AtomEditEvalCache` struct (follows existing eval cache pattern)
   - `AtomEditTool` enum: `Default(DefaultToolState)`, `AddAtom(AddAtomToolState)`, `AddBond(AddBondToolState)`
   - `NodeData::eval()` implementation:
     - When `output_diff` is false:
       1. Call `apply_diff(input, &self.diff, self.tolerance)` → `DiffApplicationResult { result, provenance, stats }`
       2. Apply selection to result: for each ID in `selection.selected_base_atoms`, look up `provenance.base_to_result` → if found, set `atom.set_selected(true)`; same for `selection.selected_diff_atoms` via `provenance.diff_to_result`. Apply `selection.selected_bonds` to result decorator. Silently skip any stale IDs that don't appear in the provenance maps (eval is `&self`, so selection can't be mutated here; stale entries are harmless and cleaned up lazily during the next selection interaction).
       3. Store provenance and stats in eval cache: `if network_stack.len() == 1 { context.selected_node_eval_cache = Some(Box::new(AtomEditEvalCache { provenance, stats })); }`
       4. Return result
     - When `output_diff` is true: return a clone of `self.diff` with `decorator().show_anchor_arrows` set according to `self.show_anchor_arrows`
   - Direct diff mutation methods:
     - `add_atom_to_diff(atomic_number, position)` — calls `self.diff.add_atom()`
     - `mark_for_deletion(match_position)` — adds atom with `DELETED_SITE_ATOMIC_NUMBER` at match_position
     - `replace_in_diff(match_position, new_atomic_number)` — adds/updates atom in diff
     - `move_in_diff(atom_id, new_position)` — sets anchor if needed, updates position
     - `add_bond_in_diff(atom_id1, atom_id2, order)` — adds bond in diff structure
     - `delete_bond_in_diff(atom_id1, atom_id2)` — adds bond delete marker (`bond_order = 0`) in diff, ensuring both atoms are in the diff
     - `remove_from_diff(diff_atom_id)` — removes atom from diff (and its anchor if any)
   - `get_subtitle()`: read `DiffStats` from the eval cache (`AtomEditEvalCache.stats`) to show "+N, -M, ~K" summary
   - `clone_box()`: clone diff (including anchor_positions) + selection + other fields
   - `get_parameter_metadata()`: requires "molecule" input (same as edit_atom)

**Files to modify:**

8. **`rust/src/structure_designer/nodes/mod.rs`** — Add `pub mod atom_edit;`

9. **`rust/src/structure_designer/node_type_registry.rs`** — Register the new `atom_edit` node type:
   - Name: `"AtomEdit"`
   - Category: same as edit_atom (atomic editing)
   - Parameters: one "molecule" input of type `DataType::Atomic`
   - Output: `DataType::Atomic`
   - `node_data_creator`: `|| Box::new(AtomEditData::new())`
   - `node_data_saver` / `node_data_loader`: new serialization functions (see Phase 6)

### Phase 4: Interaction Functions

Implement the public interaction functions for the `atom_edit` node. These directly mutate the diff and update selection.

**Files to modify:**

10. **`rust/src/structure_designer/nodes/atom_edit/atom_edit.rs`** (interaction functions):

   **Provenance access:** Interaction functions retrieve the `DiffProvenance` from the eval cache via `structure_designer.get_selected_node_eval_cache()` → `downcast_ref::<AtomEditEvalCache>()`. This follows the same pattern used by `facet_shell::select_facet_by_ray()`, `drawing_plane::provide_gadget()`, etc. The provenance is always fresh because it was computed during the most recent evaluation.

   - `select_atom_or_bond_by_ray()` — Ray hit test on the evaluated result → `result_atom_id`. Look up `provenance.sources[result_atom_id]` to determine origin. Store the atom's base or diff ID in `atom_edit_data.selection`:
     - `BasePassthrough(base_id)` → add/toggle `base_id` in `selection.selected_base_atoms`
     - `DiffMatchedBase { diff_id, .. }` → add/toggle `diff_id` in `selection.selected_diff_atoms`
     - `DiffAdded(diff_id)` → add/toggle `diff_id` in `selection.selected_diff_atoms`
     - Handle `SelectModifier` (Replace/Expand/Toggle) as before.
     - For bond hits: store `BondReference` in `selection.selected_bonds` (result-space).
     - Recalculate `selection.selection_transform`: retrieve the evaluated result `AtomicStructure` from the scene, map selected base/diff IDs to result IDs via provenance, then call `calc_selection_transform()` on those result atoms. This requires the evaluated result to be available — which it is, since the scene holds it after the most recent evaluation.
     - No diff mutation. Selection is purely transient.

   - `add_atom_by_ray()` — Calculate position (ray-plane intersection, same as current edit_atom), then call `atom_edit_data.add_atom_to_diff(atomic_number, position)`. Clear `selection.selected_bonds` (diff changed).

   - `draw_bond_by_ray()` — Same two-click workflow. On second click, call `atom_edit_data.add_bond_in_diff(atom_id1, atom_id2, 1)`. The atom IDs are diff-internal IDs. If bonding involves a base atom not yet in the diff, it must be added to the diff first as an identity entry (same position and element, no anchor) so the bond can reference diff atom IDs. Clear `selection.selected_bonds`.

   - `delete_selected_atoms_and_bonds()` — Iterate selection by provenance category:
     - For each `base_id` in `selection.selected_base_atoms`: add a delete marker at that atom's position via `mark_for_deletion()`. Remove from `selected_base_atoms`.
     - For each `diff_id` in `selection.selected_diff_atoms`:
       - If the diff atom is a pure addition (no anchor, no base match): remove from diff via `remove_from_diff()`.
       - If the diff atom matched a base atom (has anchor or was a replacement): replace with delete marker (keep position/anchor for matching).
       - Remove from `selected_diff_atoms`.
     - For selected bonds: ensure both endpoint atoms are in the diff (add as identity entries at current positions if not already present). Add a bond delete marker (`bond_order = 0`) between them via `delete_bond_in_diff()`. This works uniformly whether the bond came from the base or was previously added in the diff — a delete marker suppresses any bond between the two atoms. Clear `selected_bonds`.

   - `replace_selected_atoms()` — For each selected atom:
     - `diff_id` in `selected_diff_atoms`: update its `atomic_number` in the diff. Selection unchanged.
     - `base_id` in `selected_base_atoms`: add to diff with the new `atomic_number` at the base atom's position. Move from `selected_base_atoms` to `selected_diff_atoms` (new diff ID). Clear `selected_bonds`.

   - `transform_selected(abs_transform)` — Compute relative delta: `relative = abs_transform.delta_from(selection_transform)`. Then for each selected atom:
     - `diff_id` in `selected_diff_atoms`:
       - Read current position from diff: `P_old = self.diff.get_atom(diff_id).position()`
       - Compute `P_new = relative.rotation * P_old + relative.translation`
       - Update position in diff. Anchor (if any) stays at original base position. Selection unchanged.
     - `base_id` in `selected_base_atoms`: look up `provenance.base_to_result[base_id]` → read position and element from result. Add to diff at new position (`relative` applied to old position), set anchor to old position. Move from `selected_base_atoms` to `selected_diff_atoms` (new diff ID).
     - Update `selection.selection_transform` algebraically: `selection_transform = selection_transform.apply_to_new(relative)` (same composition as existing `edit_atom` TransformCommand — no position re-lookup needed since the result hasn't been re-evaluated yet). Clear `selected_bonds`.

   **Key design point:** Interaction functions know atom provenance from the `AtomEditSelection` itself — `selected_base_atoms` and `selected_diff_atoms` are separate sets. The eval cache provenance is only needed during `select_atom_or_bond_by_ray()` to classify a newly-clicked result atom. For mutation functions (delete, replace, transform), the selection already tells us which atoms are base vs. diff.

   **Distinguishing pure additions from matched diff atoms in delete:** The `delete_selected_atoms_and_bonds()` function needs to know if a diff atom is a pure addition or matched a base atom. This can be determined by checking `diff.has_anchor_position(diff_id)` (moved atom has anchor) or looking up `provenance.sources` for any result atom sourced from this `diff_id` to check if it's `DiffMatchedBase` vs `DiffAdded`. Alternatively, the interaction function can simply check: does the eval cache provenance contain a `DiffMatchedBase` entry for this `diff_id`? If yes → it matched a base atom (replace with delete marker). If the provenance shows `DiffAdded` → it's a pure addition (remove from diff entirely).

### Phase 5: API Layer

**Files to create:**

11. **`rust/src/api/structure_designer/atom_edit_api.rs`** (new file) — API functions for the `atom_edit` node:
    - `atom_edit_select_by_ray()` — wraps `select_atom_or_bond_by_ray()`
    - `atom_edit_add_atom_by_ray()` — wraps `add_atom_by_ray()`
    - `atom_edit_draw_bond_by_ray()` — wraps `draw_bond_by_ray()`
    - `atom_edit_delete_selected()` — wraps `delete_selected_atoms_and_bonds()`
    - `atom_edit_replace_selected()` — wraps `replace_selected_atoms()`
    - `atom_edit_transform_selected()` — wraps `transform_selected()`
    - `atom_edit_toggle_output_diff()` — toggles `atom_edit_data.output_diff`
    - `atom_edit_toggle_show_anchor_arrows()` — toggles `atom_edit_data.show_anchor_arrows`
    - `get_active_atom_edit_tool()` / `set_active_atom_edit_tool()` — tool state
    - `set_atom_edit_default_data()` / `set_atom_edit_add_atom_data()` — tool configuration
    - All functions follow the existing API pattern: `with_mut_cad_instance` → get node data → downcast to `AtomEditData` → call method → `refresh_structure_designer_auto()`
    - All functions marked `#[flutter_rust_bridge::frb(sync)]`

**Files to modify:**

12. **`rust/src/api/structure_designer/mod.rs`** — Add `pub mod atom_edit_api;`

13. **`rust/src/api/structure_designer/structure_designer_api_types.rs`** — Add new types (alongside existing `APIEditAtomData`):
    ```rust
    pub struct APIAtomEditData {
        pub active_tool: APIAtomEditTool,
        pub bond_tool_last_atom_id: Option<u32>,
        pub replacement_atomic_number: Option<i16>,
        pub add_atom_tool_atomic_number: Option<i16>,
        pub has_selected_atoms: bool,
        pub has_selection: bool,
        pub selection_transform: Option<APITransform>,
        pub output_diff: bool,
        pub show_anchor_arrows: bool,
        pub diff_stats: APIDiffStats,
    }

    pub enum APIAtomEditTool {
        Default,
        AddAtom,
        AddBond,
    }

    pub struct APIDiffStats {
        pub atoms_added: u32,
        pub atoms_deleted: u32,
        pub atoms_modified: u32,
        pub bonds_added: u32,
        pub bonds_deleted: u32,
    }
    ```

14. **`rust/src/api/structure_designer/structure_designer_api.rs`** — Add `get_atom_edit_data()`:
    - Similar to existing `get_edit_atom_data()` but for the new node
    - Read `has_selected_atoms`, `has_selection` from the **evaluated result** atomic structure (correctly reflects only visible selected atoms, excluding any stale IDs in `AtomEditSelection`)
    - Read `selection_transform` from `atom_edit_data.selection.selection_transform`
    - Read `output_diff`, `show_anchor_arrows` from `atom_edit_data`
    - Read `diff_stats` from the eval cache (`AtomEditEvalCache.stats`)

### Phase 6: Serialization

**Files to create:**

15. **`rust/src/structure_designer/serialization/atom_edit_data_serialization.rs`** (new file):
    - Serialize the diff `AtomicStructure` including atoms, bonds, `is_diff`, and `anchor_positions`
    - Serialize `output_diff: bool`, `show_anchor_arrows: bool`, and `tolerance: f64`
    - Active tool state, `AtomEditSelection`, and `AtomEditEvalCache` are NOT serialized (transient UI state)

**Files to modify:**

16. **`rust/src/structure_designer/serialization/mod.rs`** — Add `pub mod atom_edit_data_serialization;`

17. **`rust/src/crystolecule/atomic_structure/mod.rs`** (serialization aspect) — The `AtomicStructure` serialization (used by `.cnnd` save/load) must be updated to persist:
    - `is_diff` flag
    - `anchor_positions` map (only when `is_diff = true`)
    - These fields are optional in the format — absent means `is_diff = false`, empty anchors (backward compatible)

**No migration needed:** The old `edit_atom` node retains its own serialization unchanged. Old `.cnnd` files with `edit_atom` nodes continue to work. New `.cnnd` files with `atom_edit` nodes use the new serialization. No conversion between formats is needed.

### Phase 7: Selection Model

The selection model is the most architecturally significant difference from the old `edit_atom` design. In `edit_atom`, selection was a command in the history (persistent, serialized, replayed). In `atom_edit`, selection is fully transient UI state stored separately from the diff.

#### Old `edit_atom` selection system (for reference)

| Aspect | `edit_atom` implementation |
|--------|----------------------|
| Atom selection storage | `Atom.flags` bit 0 (`ATOM_FLAG_SELECTED`), set during command replay |
| Bond selection storage | `HashSet<BondReference>` in `AtomicStructureDecorator.selected_bonds` |
| Selection trigger | `SelectCommand` added to command history |
| Selection transform | Two redundant copies: on decorator (set by commands) and on `EditAtomData` (synced by `refresh_scene_dependent_edit_atom_data()`) |
| Serialization | SelectCommands serialized in `.cnnd` history — **the core problem** |
| Undo/redo | Selection changes are undoable (they're commands) |
| Rendering | `atom.is_selected()` → magenta, lower roughness; `decorator.is_bond_selected()` → magenta |

#### New `atom_edit` selection system

18. **`AtomEditSelection` — provenance-based selection state:**

    Selection is stored as two sets of atom IDs categorized by **provenance** (origin), not by result atom IDs:

    ```rust
    pub struct AtomEditSelection {
        pub selected_base_atoms: HashSet<u32>,      // Base atom IDs (stable — input doesn't change)
        pub selected_diff_atoms: HashSet<u32>,      // Diff atom IDs (stable — we control the diff)
        pub selected_bonds: HashSet<BondReference>,  // Result-space bond refs (cleared on diff mutation)
        pub selection_transform: Option<Transform>,  // Cached, recalculated after selection changes
    }
    ```

    **Why provenance-based:** Result atom IDs are assigned fresh by `apply_diff()` on each evaluation. If the diff changes (user adds an atom), result IDs can shift. Base atom IDs and diff atom IDs are stable across re-evaluations — base IDs because the input is immutable during editing, diff IDs because they're internal to the diff `AtomicStructure` we control.

    **Bond exception:** Bond selection is stored in result space and cleared on any diff mutation. Bond selection is typically short-lived (select bond → delete), and tracking bond provenance across base+diff would be complex for little benefit.

19. **`AtomEditEvalCache` — provenance via eval cache:**

    The eval cache stores the `DiffProvenance` and `DiffStats` computed during the most recent `apply_diff()` call. This follows the established eval cache pattern used by 14+ existing nodes (e.g., `FacetShellEvalCache`, `DrawingPlaneEvalCache`).

    ```rust
    #[derive(Debug, Clone)]
    pub struct AtomEditEvalCache {
        pub provenance: DiffProvenance,
        pub stats: DiffStats,
    }
    ```

    **Lifecycle:**
    - **Populated:** During `AtomEditData::eval()` when `network_stack.len() == 1` (root-level evaluation), via `context.selected_node_eval_cache = Some(Box::new(AtomEditEvalCache { provenance, stats }))`.
    - **Retrieved:** By `select_atom_or_bond_by_ray()` and other interaction functions via `structure_designer.get_selected_node_eval_cache()` → `downcast_ref::<AtomEditEvalCache>()`.
    - **Invalidated:** Automatically when the node is re-evaluated (new provenance replaces old). Also invalidated when node data changes and downstream caches are cleared.
    - **Cached when invisible:** Travels with `NodeSceneData` into the invisible node cache, restored on visibility without re-evaluation.

    **Why eval cache, not a field on AtomEditData:** The provenance is a byproduct of evaluation, not part of the node's persistent or UI state. The eval cache is the established mechanism for passing evaluation-computed values to interaction functions. Storing it on `AtomEditData` would conflate transient evaluation state with node data.

20. **Selection flow — user clicks to select:**

    1. Ray hit test on evaluated result → `result_atom_id`
    2. Retrieve `AtomEditEvalCache` from eval cache → `provenance`
    3. Look up `provenance.sources[result_atom_id]`:
       - `BasePassthrough(base_id)` → add/toggle `base_id` in `selection.selected_base_atoms`
       - `DiffMatchedBase { diff_id, .. }` → add/toggle `diff_id` in `selection.selected_diff_atoms`
       - `DiffAdded(diff_id)` → add/toggle `diff_id` in `selection.selected_diff_atoms`
    4. Handle `SelectModifier` (Replace clears all first, Expand adds, Toggle inverts)
    5. Recalculate `selection.selection_transform`: map all selected base/diff IDs to result atom IDs via provenance maps, retrieve those atoms' positions from the evaluated result structure, call `calc_selection_transform()`.

21. **Selection rendering — during eval:**

    After `apply_diff()` produces the result and provenance:
    1. For each `base_id` in `selection.selected_base_atoms`: look up `provenance.base_to_result[base_id]` → if found, `result_id`, set `result.get_atom_mut(result_id).set_selected(true)`. If `base_id` not in map (atom was deleted by a delete marker in the diff), silently skip — eval is `&self` and cannot mutate selection. Stale entries are harmless and cleaned up lazily during the next selection interaction.
    2. For each `diff_id` in `selection.selected_diff_atoms`: look up `provenance.diff_to_result[diff_id]` → if found, set `result.get_atom_mut(result_id).set_selected(true)`. Same stale-skip logic.
    3. Apply `selection.selected_bonds` to result decorator.
    4. Tessellator renders selection exactly as before — no tessellator changes needed for selection (only for diff visualization in Phase 8).
    5. The API reads `has_selected_atoms` / `has_selection` from the **result** (where flags are correctly set only for existing atoms), not from `AtomEditSelection` (which may contain stale IDs). This ensures the UI doesn't show active Delete/Replace buttons for a phantom selection.

22. **Selection updates during mutations:**

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

23. **Input change handling:**

    If the input structure changes (upstream node modified), `selected_base_atoms` may contain stale IDs. The selection rendering step (item 21 above) handles this gracefully: stale IDs that don't appear in the provenance maps are silently skipped. This is acceptable because when the input changes, the user's spatial context has shifted anyway.

24. **What is NOT serialized:**

    - `AtomEditSelection` — entirely transient, reset to empty on load
    - `AtomEditEvalCache` — eval cache is always transient (rebuilt on evaluation)
    - `active_tool` — reset to Default on load (same as current `edit_atom` behavior)
    - Selection is NOT undoable (no undo/redo in `atom_edit`). This is intentional. If undo is added later (via diff snapshots), selection would still be separate.

### Phase 8: Diff Visualization

The rendering system uses PBR materials with `albedo`, `roughness`, and `metallic` — no opacity channel, no alpha blending, no depth sorting for transparency. All atoms render as solid spheres with element-based colors (from `ATOM_INFO`). Selected atoms override the color to magenta with lower roughness. The decorator provides `AtomDisplayState` (Normal, Marked, SecondaryMarked) for crosshair overlays. Bonds render as cylinders.

The diff visualization leverages this existing infrastructure with **minimal changes** — no new render pipelines, no transparency, no new display states.

25. **`rust/src/display/atomic_tessellator.rs`** — Three small changes:

    **a) Delete marker rendering (atomic_number = 0):**
    - In `get_atom_color_and_material()`, add a check: if `atom.atomic_number == 0` (delete marker), return a fixed color and radius instead of looking up `ATOM_INFO` (which has no entry for atomic_number 0).
    - Color: solid red, e.g., `Vec3::new(0.9, 0.1, 0.1)`. Roughness: `0.5`. Metallic: `0.0`.
    - Radius: fixed `0.5` Angstrom (a reasonable small sphere; covalent radii range ~0.3–1.5 A).
    - No transparency, no X overlay, no special glyph — just a red sphere. The renderer already handles arbitrary colors per atom; this is a one-line branch.

    **b) Bond delete marker rendering (bond_order = 0):**
    - In bond tessellation, add a check: if `bond.bond_order() == 0` (delete marker) and the structure has `is_diff = true`, skip rendering the bond (or optionally render as a thin red dashed/dotted line for visualization). In non-diff structures, bond_order = 0 should never occur.
    - This is a one-line branch in the bond tessellation loop.

    **c) Normal diff atoms (additions, replacements, moves):**
    - Rendered with their **standard element color** — no special color coding for "added" vs. "modified". A carbon in the diff looks like any other carbon.
    - This requires **zero rendering changes**. The tessellator already uses `atom.atomic_number` to look up the element color.

    **d) Anchor arrow visualization (optional, controlled by `show_anchor_arrows: bool` on `AtomicStructureDecorator`):**
    - `show_anchor_arrows` is stored on `AtomEditData` (the node data). When `output_diff = true`, the eval method copies this flag to the output `AtomicStructure`'s decorator before returning it: `diff_clone.decorator_mut().show_anchor_arrows = self.show_anchor_arrows`. This is the single source of truth — the flag on the decorator is a transient rendering hint set during eval, not a persistent field.
    - When `is_diff = true` and `decorator().show_anchor_arrows = true`, the tessellator iterates `anchor_positions`. For each entry `(atom_id, anchor_pos)`:
      1. Render a small delete-marker-style sphere at `anchor_pos` (same red color, smaller radius e.g. `0.3` A) to show "this is where the atom was matched".
      2. Render a thin cylinder from `anchor_pos` to the atom's current position (reusing the existing bond cylinder tessellation with a small radius e.g. `0.05` A, colored e.g. orange `Vec3::new(1.0, 0.6, 0.0)`).
    - This reuses existing geometry primitives (sphere + cylinder) with no new rendering infrastructure.
    - **Impostor compatibility:** Both tessellation paths (triangle mesh and impostor) are supported. The impostor path's `AtomImpostorMesh::add_atom_quad()` and `BondImpostorMesh::add_bond_quad()` take arbitrary positions, radii, and colors — no atom references needed. The anchor arrow loop adds to the same impostor meshes that regular atoms and bonds use.
    - When `show_anchor_arrows = false` or `is_diff = false`: no extra geometry, zero performance impact.
    - `show_anchor_arrows` field on `AtomicStructureDecorator`: Add a `show_anchor_arrows: bool` field (default `false`). This field is NOT serialized — it's a transient rendering hint set only during eval output. This is consistent with the decorator's existing role as the home for rendering/UI hints (`atom_display_states`, `from_selected_node`, `selected_bonds`) that travel with the structure. The tessellator already reads the decorator for display states and bond selection, so accessing `decorator().show_anchor_arrows` is zero friction.

    **Total rendering changes:** One branch in `get_atom_color_and_material()` for atomic_number=0, one branch in bond tessellation for bond_order=0, plus an optional anchor arrow loop in both `tessellate_atomic_structure()` and `tessellate_atomic_structure_impostors()`. No shader changes, no pipeline changes, no new Material fields.

26. **Display mode in the `atom_edit` node:**
    - When `output_diff = false` (default): the node outputs the applied result. Rendering shows the final structure with standard element colors. This is the normal editing workflow.
    - When `output_diff = true`: the node outputs the diff itself. The `is_diff = true` flag causes atom delete markers to render as red spheres and bond delete markers to be skipped (or rendered as red lines). Added/modified atoms render with their normal element colors. If `show_anchor_arrows` is enabled, movement arrows are shown.
    - The user toggles `show_anchor_arrows` independently of `output_diff` (both are on the node data). Arrows are only meaningful when viewing the diff, but the flag is harmless on non-diff structures (no anchors → no arrows).

### Phase 9: Flutter UI Changes

**Files to create:**

27. **`lib/structure_designer/node_data/atom_edit_editor.dart`** (new file) — Editor widget for the `atom_edit` node:
    - **Output mode toggle** in the header — a segmented button or toggle switch labeled "Result" / "Diff"
    - **"Show Arrows" checkbox/toggle** — visible when in Diff output mode, toggles `show_anchor_arrows`
    - **Diff statistics display** below the header — show e.g., "+3 atoms, -1 atom, ~2 modified" from `APIDiffStats`
    - **Tool selector** (Default, AddAtom, AddBond) — same concept as edit_atom
    - **Element selector** — used for add/replace operations
    - **Replace/Delete buttons** — they mutate the diff directly
    - **Transform controls** — they update positions in the diff
    - **No undo/redo buttons** — the `atom_edit` node does not have undo/redo
    - **Contextual delete button:**
      - If a diff-added atom is selected: "Remove from diff" semantics (removes it entirely)
      - If a base-matched atom is selected: "Mark for deletion" semantics (adds delete marker)
      - The button can remain labeled "Delete Selected" for simplicity; the underlying behavior differs based on context

**Files to modify:**

28. **`lib/structure_designer/structure_designer_model.dart`** — Add model methods for `atom_edit`:
    - `atomEditSelectByRay()`, `atomEditAddAtomByRay()`, `atomEditDrawBondByRay()`, etc.
    - `atomEditDeleteSelected()`, `atomEditReplaceSelected()`, `atomEditTransformSelected()`
    - `toggleAtomEditOutputDiff()` — calls new API function
    - `toggleAtomEditShowAnchorArrows()` — calls new API function
    - Update `refreshFromKernel()` data fetch to populate `APIAtomEditData` fields
    - The existing `edit_atom` model methods remain unchanged

29. **`lib/structure_designer/structure_designer_viewport.dart`** — Add interaction dispatch for `atom_edit` node:
    - When the selected node is an `atom_edit` node, ray-cast interactions should call the `atom_edit_*` API functions instead of the `edit_atom` equivalents
    - The dispatch can be based on the node type name

### Phase 10: Text Format Integration

30. **`rust/src/structure_designer/text_format/`** — If `atom_edit` exposes text properties:
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
| `rust/src/structure_designer/nodes/atom_edit/mod.rs` | Module declaration for new node |
| `rust/src/structure_designer/nodes/atom_edit/atom_edit.rs` | `AtomEditData` implementing `NodeData`, interaction functions, selection model |
| `rust/src/structure_designer/serialization/atom_edit_data_serialization.rs` | Serialization for `AtomEditData` |
| `rust/src/api/structure_designer/atom_edit_api.rs` | Public API functions for `atom_edit` node |
| `lib/structure_designer/node_data/atom_edit_editor.dart` | Flutter editor widget for `atom_edit` node |

### Files to modify (crystolecule infrastructure)
| File | Changes |
|------|---------|
| `rust/src/crystolecule/atomic_structure/mod.rs` | Add `is_diff`, `anchor_positions`, `DELETED_SITE_ATOMIC_NUMBER`, new constructors and accessors, update Clone/Default/add_atomic_structure/serialization |
| `rust/src/crystolecule/atomic_structure/atomic_structure_decorator.rs` | Add `show_anchor_arrows: bool` field (transient rendering hint, default `false`) |
| `rust/src/crystolecule/atomic_structure/atom.rs` | Add `is_delete_marker()` |
| `rust/src/crystolecule/atomic_structure/inline_bond.rs` | Add `BOND_DELETED` constant and `is_delete_marker()` |
| `rust/src/crystolecule/mod.rs` | Add `pub mod atomic_structure_diff;` |
| `rust/tests/crystolecule.rs` | Register new test module |

### Files to modify (node registration and API plumbing)
| File | Changes |
|------|---------|
| `rust/src/structure_designer/nodes/mod.rs` | Add `pub mod atom_edit;` |
| `rust/src/structure_designer/node_type_registry.rs` | Register `AtomEdit` node type |
| `rust/src/structure_designer/serialization/mod.rs` | Add `pub mod atom_edit_data_serialization;` |
| `rust/src/api/structure_designer/mod.rs` | Add `pub mod atom_edit_api;` |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Add `APIAtomEditData`, `APIAtomEditTool`, `APIDiffStats` |
| `rust/src/api/structure_designer/structure_designer_api.rs` | Add `get_atom_edit_data()` |
| `rust/src/display/atomic_tessellator.rs` | One branch for atom delete marker color/radius, one branch for bond delete marker, optional anchor arrow loop |
| `lib/structure_designer/structure_designer_model.dart` | Add `atom_edit` model methods |
| `lib/structure_designer/structure_designer_viewport.dart` | Add `atom_edit` interaction dispatch |

### Files NOT modified (old `edit_atom` remains untouched)
| File | Status |
|------|--------|
| `rust/src/structure_designer/nodes/edit_atom/` (entire directory) | Unchanged, will be deprecated later |
| `rust/src/structure_designer/serialization/edit_atom_data_serialization.rs` | Unchanged |
| `rust/src/api/structure_designer/edit_atom_api.rs` | Unchanged |
| `rust/src/structure_designer/structure_designer.rs` (`refresh_scene_dependent_edit_atom_data()`) | Unchanged, still used by old node |
| `lib/structure_designer/node_data/edit_atom_editor.dart` | Unchanged |

## Dependency Order

The implementation phases have a clear dependency chain:

```
Phase 1: AtomicStructure extensions (is_diff, anchors, atom/bond delete marker constants)
    ↓
Phase 2: apply_diff() + DiffProvenance + DiffStats in crystolecule + tests
    ↓
Phase 3: AtomEditData (new node, uses apply_diff, introduces AtomEditEvalCache)
Phase 7: Selection model (AtomEditSelection, provenance-based)  ← integrated with Phase 3/4
    ↓
Phase 4: Interaction functions (uses AtomEditSelection + eval cache provenance)
    ↓
Phase 5: API layer (exposes new interaction functions, reads from AtomEditSelection)
Phase 6: Serialization (persists new data model)    ← can be parallel with Phase 5
    ↓
Phase 8: Diff visualization (rendering)
    ↓
Phase 9: Flutter UI (consumes API changes)
    ↓
Phase 10: Text format (lower priority, can be deferred)
```

**Note:** Phase 7 is not a standalone late phase — the selection model is part of the core AtomEditData design (Phase 3) and the interaction functions (Phase 4). It's listed as a separate phase for documentation clarity, but implementation-wise `AtomEditSelection` and `AtomEditEvalCache` are built alongside Phase 3/4.

## Deprecation Plan for `edit_atom`

The old `edit_atom` node remains fully functional during the transition:

1. **Phase 1:** `atom_edit` node is created and available alongside `edit_atom`.
2. **Phase 2:** `edit_atom` is marked as deprecated in the node type registry (hidden from the "add node" menu but still functional in existing files).
3. **Phase 3 (future):** Once all users have migrated, `edit_atom` and all its infrastructure can be removed. This is a separate future task.

No automatic migration from `edit_atom` to `atom_edit` is planned — users manually replace `edit_atom` nodes with `atom_edit` nodes in their networks.

## Open Questions for Future Sessions

1. **Diff composition** — When two `atom_edit` nodes are chained, the second operates on the result of the first. Could their diffs be composed into one? This is a future optimization.

2. **Multi-output support** — When the node can output both the result and the diff simultaneously, the `output_diff` boolean becomes unnecessary. This depends on broader node network infrastructure.

3. **Stamping workflow** — The full UX for "apply this diff at multiple positions in a crystal" (e.g., T-center defect stamping) is a separate design task. The diff will flow as a regular `NetworkResult::Atomic` with `is_diff = true` into a future stamp node that calls `apply_diff()`.

4. **Anchor position alternatives** — The current anchor map approach works well. If we discover cases where a richer position-transformation model is needed (e.g., parametric displacements), this can be extended later without changing the fundamental architecture.

5. **Undo/redo for `atom_edit`** — The initial implementation has no undo/redo. A future enhancement could store diff snapshots (lightweight — just the diff AtomicStructure, not the full result) to enable undo. This is orthogonal to the core diff design and can be added later.
