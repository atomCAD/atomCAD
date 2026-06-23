# atom_edit/ - Agent Instructions

Non-destructive atom editing node with diff-based architecture. Edits are represented as a single `AtomicStructure` diff applied to the input structure via `apply_diff()`.

This module implements both the `atom_edit` and `motif_edit` node types. `motif_edit` is a mode of `AtomEditData` (`is_motif_mode = true`) that outputs a `Motif` instead of a phase type. It adds: fractional coordinate conversion via a unit_cell input (defaults to cubic diamond), parameter elements, cross-cell bonds, and ghost atom generation for neighboring cells. See `doc/design_motif_edit.md`.

**Phase-aware I/O (lattice-space refactoring):** The `atom_edit` node declares its input as `DataType::HasAtoms` (abstract — accepts both Crystal and Molecule) and uses `OutputPinDefinition::same_as_input("result", "molecule")` on pin 0 so a Crystal input produces a Crystal output and a Molecule input produces a Molecule output. Pin 1 (`diff`) is always `DataType::Molecule`. The concrete input variant is captured in `CachedInput`/`InputWrapperKind` (Crystal retains its `Structure` and geo_tree_root; Molecule retains its geo_tree_root) and re-wrapped on output. `motif_edit` keeps the same `HasAtoms` input but has fixed `Motif` + `Molecule` output pins.

## Module Structure

```
atom_edit/
├── mod.rs                # Module declarations + backward-compat re-exports
├── types.rs              # Shared type definitions (enums, structs, constants)
├── atom_edit_data.rs     # AtomEditData struct, NodeData impl, accessors, utilities
├── diff_recorder.rs      # DiffRecorder, AtomDelta, BondDelta, AtomState (undo support)
├── selection.rs          # Ray-based and marquee atom/bond selection
├── operations.rs         # Shared mutation operations (delete, replace, transform, drag, bond order)
├── default_tool.rs       # Default tool pointer event state machine
├── add_atom_tool.rs      # Add Atom tool interaction
├── add_bond_tool.rs      # Add Bond tool interaction
├── measurement.rs        # Read-only measurement queries (distance, angle, dihedral)
├── modify_measurement.rs # Modify distance/angle/dihedral by moving atoms
├── minimization.rs       # UFF energy minimization (batch + continuous during drag)
├── hydrogen_passivation.rs # General-purpose hydrogen passivation
├── atom_edit_gadget.rs   # XYZ selection gadget (translation gizmo)
├── guideline.rs          # Pure placement-guideline geometry + `Guideline` type (#368)
├── guideline_tool.rs     # Guideline TOOL pointer state machine (#368, Phase 2 viewport)
└── text_format.rs        # Human-readable diff text format (AI integration)
```

## Key Types (in `types.rs`)

| Type | Purpose |
|------|---------|
| `AtomEditTool` | Enum: `Default`, `AddAtom`, `AddBond` |
| `AddAtomToolState` | Enum: `Idle`, `GuidedPlacement`, `GuidedFreeSphere`, `GuidedFreeRing` |
| `DefaultToolInteractionState` | State machine: Idle → Pending → Dragging/Marquee |
| `AtomEditSelection` | Provenance-based selection (base + diff atom IDs) |
| `AtomEditEvalCache` | Provenance maps from most recent `apply_diff()` |
| `AddBondInteractionState` | State machine: Idle → Pending → Dragging |
| `AddBondToolState` | Bond order (1-7) + interaction state for AddBond tool |
| `DiffAtomKind` | Classification: DeleteMarker, MatchedBase, PureAddition |
| `BondDeletionInfo` | Info for deleting bonds via diff |
| `BondEndpointInfo` | Info for changing bond order (diff IDs + identity for promotion) |
| `AddBondMoveResult` | Rubber-band preview data returned by `add_bond_pointer_move` |

## Core Data (`atom_edit_data.rs`)

`AtomEditData` implements `NodeData` and contains:
- **Persistent state** (serialized): `diff`, `output_diff`, `tolerance`, `show_anchor_arrows`, `include_base_bonds_in_diff`, `error_on_stale_entries`
- **Transient state** (not serialized): `selection`, `active_tool`, `last_stats`

Per-atom metadata (frozen flags, hybridization overrides) is stored **inline on `Atom.flags`** of diff atoms. When a base atom needs an override, it is promoted to a real diff atom and the flag is set directly. During evaluation, `apply_diff()` copies flags from diff atoms to result atoms via `copy_atom_metadata()`. No external maps needed. Downstream consumers (UFF typer, hydrogen passivation) read overrides from `Atom.flags`.

Key methods:
- Diff mutations: `add_atom_to_diff`, `mark_for_deletion`, `move_in_diff`, etc.
- Batch operations: `apply_delete_result_view`, `apply_replace`, `apply_transform`
- Tool management: `set_active_tool`, `set_selected_element`

Accessor helpers:
- `get_active_atom_edit_data()` — immutable access
- `get_selected_atom_edit_data_mut()` — mutable access (marks node changed)
- `get_atom_edit_data_mut_transient()` — mutable without marking changed (for interaction state)

## Three-Phase Borrow Pattern

All interaction functions follow this pattern to avoid Rust borrow conflicts:

1. **Phase 1: Gather** — immutable borrows on `StructureDesigner` to collect owned data
2. **Phase 2: Compute** — process gathered data (no borrows held)
3. **Phase 3: Mutate** — mutable borrow on `StructureDesigner` to apply changes

## Provenance System

Selection is stored by provenance (base/diff atom IDs), not result atom IDs. The `AtomEditEvalCache` maps between spaces:
- `provenance.base_to_result` — base atom ID → result atom ID
- `provenance.diff_to_result` — diff atom ID → result atom ID
- `provenance.sources` — result atom ID → `AtomSource` (Base/DiffMatched/DiffAdded)

In **diff view** (`output_diff = true`), atom IDs from hit tests are diff-native — no provenance needed.

## Anchor Invariant (CRITICAL)

Diff atoms have an optional **anchor position** that tells `apply_diff` which base atom they correspond to. The anchor invariant:

- **Anchors are set ONLY at promotion time** — when a base atom is first added to the diff via `add_atom()` + `set_anchor_position()`. This happens in `drag_selected_by_delta`, `apply_transform`, `apply_position_updates`, `atom_edit_gadget::sync_data`, and similar functions that promote base atoms.
- **`move_in_diff` MUST NOT set anchors.** It only changes the atom's position.
- **Pure addition atoms** (created by AddAtom, with no base counterpart) must NEVER have an anchor. `apply_diff` treats anchored-but-unmatched atoms as "orphaned tracked atoms" and **drops them from the result**.

If you need to move an atom that is already in the diff, call `move_in_diff`. If you need to move a base atom that is not yet in the diff, first promote it (`add_atom` + `set_anchor_position`), then call `move_in_diff`.

## Tool Files

- **`default_tool.rs`**: Pointer event state machine (`pointer_down` → `pointer_move` → `pointer_up`). Handles click-select, drag threshold, screen-plane dragging, and marquee selection. Clicking an already-selected bond cycles its order (single→double→triple→single; specialized orders enter cycle at single).
- **`add_atom_tool.rs`**: Add Atom tool with guided placement. Click empty space → free placement (ray-plane intersection). Click existing atom → guided placement: compute candidate positions via `crystolecule::guided_placement`, show guide dots, click dot to place and bond. Three guided modes: `GuidedPlacement` (fixed dots for sp3/sp2/sp1 cases with 2+ bonds), `GuidedFreeSphere` (bare atom, cursor-tracked dot on wireframe sphere), `GuidedFreeRing` (single bond without dihedral reference, rotating dots on cone ring). Guide dot hit testing (`GUIDE_DOT_HIT_RADIUS = 0.3 A`) runs before atom hit testing. Design doc: `doc/atom_edit/guided_atom_placement.md`.
- **`add_bond_tool.rs`**: Drag-to-bond interaction with configurable bond order (1-7). State machine: Idle → Pending (pointer down on atom) → Dragging (drag threshold exceeded) → bond creation on pointer up over target atom, or cancel on empty/same atom. `pointer_move` performs lightweight ray-cast only (no evaluation/tessellation) and returns `AddBondMoveResult` for Flutter's 2D rubber-band overlay. Design doc: `doc/atom_edit/design_bond_creation_and_order.md`.
- **`operations.rs`**: Includes `change_bond_order` (single bond), `change_selected_bonds_order` (batch), and `cycle_bond_order` (single→double→triple→single). Both change functions handle result view (provenance promotion) and diff view (direct edit).

## Backward Compatibility

The `mod.rs` uses an inline `pub mod atom_edit { ... }` with re-exports so that existing import paths like `atom_edit::atom_edit::AtomEditData` continue to work. Do not remove or rename this re-export module.

## Undo/Redo Integration

atom_edit bypasses the generic `SetNodeData` command and uses incremental delta-based undo. Design doc: `doc/design_atom_edit_undo.md`.

### DiffRecorder (`diff_recorder.rs`)

`AtomEditData` has an optional `recorder: Option<DiffRecorder>` that captures `AtomDelta` and `BondDelta` entries as mutations happen. Call `begin_recording()` before a mutation and `end_recording()` after — the recorder automatically coalesces redundant deltas (e.g., multiple moves of the same atom during a drag).

### Recorded Mutation Methods

Two layers of mutation methods exist:
- **Recording methods** (on `AtomEditData`): `add_atom_to_diff`, `remove_from_diff`, `move_in_diff`, `add_bond_in_diff`, `set_atomic_number_recorded`, `set_anchor_recorded`, `add_atom_recorded`, `add_bond_recorded`, `set_position_recorded`, `delete_bond_recorded`, `set_frozen_recorded`, `set_hybridization_override_recorded`, `set_flags_recorded`. Called during user actions.
- **Non-recording methods** (on `AtomicStructure`/diff directly): `diff.add_atom_with_id`, `diff.delete_atom`, `diff.set_atom_position`, etc. Called by undo/redo execution only.

When adding new diff mutations, use the recording variants. If mutating `self.diff` directly, add a `*_recorded` wrapper.

### API Integration Pattern

The `with_atom_edit_undo` helper wraps API mutations:
```
begin_recording → execute mutation → end_recording → push AtomEditMutationCommand
```

Drag operations use `begin_atom_edit_drag()`/`end_atom_edit_drag()` for coalescing across multiple pointer-move frames.

### Commands

- `AtomEditMutationCommand` — incremental atom/bond deltas (all diff mutations, including flag changes like frozen and hybridization override via `set_frozen_recorded` / `set_hybridization_override_recorded`)
- `AtomEditToggleFlagCommand` — boolean flag toggles (output_diff, show_anchor_arrows, etc.)

## Placement Guideline tool (#368)

A **guideline** is a transient line that constrains atom placement (e.g. the equidistant ad-atom site of a Si(111) √3×√3 reconstruction). It is a dedicated **fourth tool** (`AtomEditTool::Guideline(GuidelineTool)`) that fully owns pointer interaction and is self-contained: the guideline lives inside the tool variant, so switching tools (or deselecting the node) drops it. **All three phases are complete** (core / viewport / API+Flutter); the earlier *modal* v1 guideline was removed in Phase 3. Design doc: `doc/atom_edit/design_atom_guidelines.md`.

- **Pure geometry** lives in `guideline.rs`: `Guideline { origin, direction (unit), t }` plus tolerance-based constructors (`from_three_atoms` circumcenter, `from_two_atoms` midpoint, `from_one_atom`) returning `Result<_, GuidelineError>` (`Collinear` / `Coincident` / `ZeroDirection`), and the helpers `decompose` / `point_at` / `closest_t_to_ray`. There is **no** off-line `snapped` bit — picking always snaps the atom onto the line. These carry the bulk of the test coverage (`tests/structure_designer/atom_edit_guideline_test.rs`).
- **Tool state** (`types.rs`): `GuidelineTool { phase, entered_direction, remembered_t, pending }`. `GuidelinePhase` is `Define { defining: Vec<AtomRef> }` (no line yet) or `Active { guideline, picked: Option<AtomRef>, drag }`. `entered_direction` / `remembered_t` persist across Clear / re-Define. `AtomRef` is a provenance-tagged base/diff atom ref stored on the tool — **never** the shared `AtomEditSelection` (which is cleared on tool entry).
- **Mutators** on `AtomEditData` are all named `guideline_*`: `guideline_create_from_defining` (1/2/3 atoms → frozen line), `guideline_place_atom` (free atom → auto-pick → Move), `guideline_pick_atom` (snap + promote-if-base), `guideline_set_position`, `guideline_unpick`, `guideline_tool_clear`, the readers `guideline_active` / `guideline_picked` / `guideline_defining`, and the drag math `guideline_drag_picked_to_ray` / `guideline_drag_ghost_to_ray`. Atom-mutating entries are wrapped in `with_atom_edit_undo` (the line itself is transient/not-undoable; the atom moves it causes are). `guideline_auto_unpick` (undo/redo hook) and `clear_guideline_tool_on_node_deselect` keep the picked/tool state consistent.
- **Pointer state machine** lives in **`guideline_tool.rs`** (`guideline_pointer_{down,move,up}` + `guideline_reset_interaction`), mirroring `default_tool.rs`: pre-threshold press in `GuidelineTool::pending` (`GuidelinePending`), active drag in `GuidelineDragState` (`GhostDragging` / `PickedDragging`). Pick + slide coalesce into one undo step via `begin/end_atom_edit_drag`.
- **Rendering**: `apply_guideline_decoration` reads `guideline_active()` (only the `Active` phase has a line); `apply_guideline_tool_highlight_{diff,result}` mark the defining atoms (Define) / picked atom (Move) via the display-state path. All in `eval(decorate=true)`.
- **API** (`api/.../atom_edit_api.rs`): `APIAtomEditTool::Guideline` (wired in `get_active_tool` / `set_active_tool`); panel fns `guideline_create_from_defining(dir) -> String` (empty or SnackBar error), `guideline_set_position`, `guideline_place_atom`, `guideline_clear`, `guideline_set_entered_direction`; the view builder `get_guideline_tool_view -> Option<APIGuidelineToolView>` (phase / defining_count / can_create / needs_direction / `t`, with `t` derived from the picked atom's live projection in Move); and pointer fns `guideline_pointer_{down,move,up}` + `guideline_reset_interaction`. Panel/pointer fns mark the active node changed so the redecorate refresh runs.

Tests: `tests/structure_designer/atom_edit_guideline_tool_test.rs` (state machine), `..._render_test.rs` (decoration), `..._drag_test.rs` (constrained-drag math), `..._test.rs` (pure geometry).

## Adding Features

- **New tool**: Create `my_tool.rs`, add `mod my_tool;` in `mod.rs`, add `pub use super::my_tool::*;` to the re-export block.
- **New operation**: Add to `operations.rs` if tool-agnostic, or to the specific tool file.
- **New type**: Add to `types.rs` if shared across files, or keep local if single-file.
- **New diff mutation**: Use recording variants (`*_recorded` methods). Wrap the API entry point with `with_atom_edit_undo`.
