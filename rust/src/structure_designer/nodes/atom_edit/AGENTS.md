# atom_edit/ - Agent Instructions

Non-destructive atom editing node with diff-based architecture. Edits are represented as a single `AtomicStructure` diff applied to the input structure via `apply_diff()`.

## Module Structure

```
atom_edit/
├── mod.rs              # Module declarations + backward-compat re-exports
├── types.rs            # Shared type definitions (enums, structs, constants)
├── atom_edit_data.rs   # AtomEditData struct, NodeData impl, accessors, utilities
├── selection.rs        # Ray-based and marquee atom/bond selection
├── operations.rs       # Shared mutation operations (delete, replace, transform, drag)
├── default_tool.rs     # Default tool pointer event state machine
├── add_atom_tool.rs    # Add Atom tool interaction
├── add_bond_tool.rs    # Add Bond tool interaction
├── minimization.rs     # UFF energy minimization
├── atom_edit_gadget.rs # XYZ selection gadget (translation gizmo)
└── text_format.rs      # Human-readable diff text format (AI integration)
```

## Key Types (in `types.rs`)

| Type | Purpose |
|------|---------|
| `AtomEditTool` | Enum: `Default`, `AddAtom`, `AddBond` |
| `AddAtomToolState` | Enum: `Idle`, `GuidedPlacement`, `GuidedFreeSphere`, `GuidedFreeRing` |
| `DefaultToolInteractionState` | State machine: Idle → Pending → Dragging/Marquee |
| `AtomEditSelection` | Provenance-based selection (base + diff atom IDs) |
| `AtomEditEvalCache` | Provenance maps from most recent `apply_diff()` |
| `DiffAtomKind` | Classification: DeleteMarker, MatchedBase, PureAddition |
| `BondDeletionInfo` | Info for deleting bonds via diff |

## Core Data (`atom_edit_data.rs`)

`AtomEditData` implements `NodeData` and contains:
- **Persistent state** (serialized): `diff`, `output_diff`, `tolerance`, etc.
- **Transient state** (not serialized): `selection`, `active_tool`, `last_stats`

Key methods:
- Diff mutations: `add_atom_to_diff`, `mark_for_deletion`, `move_in_diff`, etc.
- Batch operations: `apply_delete_result_view`, `apply_replace`, `apply_transform`
- Tool management: `set_active_tool`, `set_default_tool_atomic_number`

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

## Tool Files

- **`default_tool.rs`**: Pointer event state machine (`pointer_down` → `pointer_move` → `pointer_up`). Handles click-select, drag threshold, screen-plane dragging, and marquee selection.
- **`add_atom_tool.rs`**: Add Atom tool with guided placement. Click empty space → free placement (ray-plane intersection). Click existing atom → guided placement: compute candidate positions via `crystolecule::guided_placement`, show guide dots, click dot to place and bond. Three guided modes: `GuidedPlacement` (fixed dots for sp3/sp2/sp1 cases with 2+ bonds), `GuidedFreeSphere` (bare atom, cursor-tracked dot on wireframe sphere), `GuidedFreeRing` (single bond without dihedral reference, rotating dots on cone ring). Guide dot hit testing (`GUIDE_DOT_HIT_RADIUS = 0.3 A`) runs before atom hit testing. Design doc: `doc/atom_edit/guided_atom_placement.md`.
- **`add_bond_tool.rs`**: Two-click bond creation workflow with provenance resolution.

## Backward Compatibility

The `mod.rs` uses an inline `pub mod atom_edit { ... }` with re-exports so that existing import paths like `atom_edit::atom_edit::AtomEditData` continue to work. Do not remove or rename this re-export module.

## Adding Features

- **New tool**: Create `my_tool.rs`, add `mod my_tool;` in `mod.rs`, add `pub use super::my_tool::*;` to the re-export block.
- **New operation**: Add to `operations.rs` if tool-agnostic, or to the specific tool file.
- **New type**: Add to `types.rs` if shared across files, or keep local if single-file.
