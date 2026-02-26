# atom_edit/ - Agent Instructions

Non-destructive atom editing node with diff-based architecture. Edits are represented as a single `AtomicStructure` diff applied to the input structure via `apply_diff()`.

## Module Structure

```
atom_edit/
├── mod.rs                # Module declarations + backward-compat re-exports
├── types.rs              # Shared type definitions (enums, structs, constants)
├── atom_edit_data.rs     # AtomEditData struct, NodeData impl, accessors, utilities
├── selection.rs          # Ray-based and marquee atom/bond selection
├── operations.rs         # Shared mutation operations (delete, replace, transform, drag, bond order)
├── default_tool.rs       # Default tool pointer event state machine
├── add_atom_tool.rs      # Add Atom tool interaction
├── add_bond_tool.rs      # Add Bond tool interaction
├── measurement.rs        # Read-only measurement queries (distance, angle, dihedral)
├── modify_measurement.rs # Modify distance/angle/dihedral by moving atoms
├── minimization.rs       # UFF energy minimization
├── atom_edit_gadget.rs   # XYZ selection gadget (translation gizmo)
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
- **Persistent state** (serialized): `diff`, `output_diff`, `tolerance`, etc.
- **Transient state** (not serialized): `selection`, `active_tool`, `last_stats`

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

## Adding Features

- **New tool**: Create `my_tool.rs`, add `mod my_tool;` in `mod.rs`, add `pub use super::my_tool::*;` to the re-export block.
- **New operation**: Add to `operations.rs` if tool-agnostic, or to the specific tool file.
- **New type**: Add to `types.rs` if shared across files, or keep local if single-file.
