# Design: Inline Atom Metadata in Diff

**Status:** Proposed
**Author:** Claude (with user direction)
**Date:** 2025-03-25

## Problem

Per-atom metadata in `AtomEditData` (frozen flags, hybridization overrides) is stored in **parallel maps external to the atoms**, split by provenance:

```rust
// Current: 4 separate maps on AtomEditData
pub frozen_base_atoms: HashSet<u32>,
pub frozen_diff_atoms: HashSet<u32>,
pub hybridization_override_base_atoms: HashMap<u32, u8>,
pub hybridization_override_diff_atoms: HashMap<u32, u8>,
```

Every operation that touches atoms must manually handle all maps:

| Touchpoint | What must be done per map |
|---|---|
| **Promotion** (base→diff) | Migrate entry from `*_base_*` to `*_diff_*` |
| **Eval pin 0** (result) | Apply via `base_to_result` and `diff_to_result` provenance |
| **Eval pin 1** (diff) | Apply directly to diff clone |
| **Undo/redo** | Separate command types (`AtomEditFrozenChangeCommand`, `AtomEditHybridizationChangeCommand`) |
| **Serialization** | Save/load each map independently |
| **UI (hover/panel)** | Read from the correct map depending on view |

Adding a new per-atom property (e.g., charge, label, color tag) requires touching **all 6+ sites**. Forgetting any one produces a subtle, hard-to-reproduce bug — as demonstrated by the frozen-not-visible-in-diff-view and hybridization-not-migrated-on-promotion bugs.

## Root Cause

`Atom` already has a `flags` field with bits for frozen and hybridization:

```rust
pub struct Atom {
    pub flags: u16,  // bit 0: selected, bit 1: H passivation, bit 2: frozen, bits 3-4: hybridization
    // ...
}
```

The metadata *belongs on the atom*, but it's stored externally because:

1. Base atoms live in the **input structure** (read-only — owned by the upstream node's eval result).
2. The external maps use **stable provenance IDs** (base or diff atom IDs) that survive across evaluations.

But the diff structure is already the canonical place for "edits applied to base atoms." Overrides should live there too.

## Proposed Solution

**Store all per-atom overrides on the diff atoms themselves, using `Atom.flags`.**

### Core Idea

When the user sets an override on any atom (base or diff), the atom is promoted to the diff if not already there, and the flag is set directly on the diff `Atom.flags`. No external maps needed.

### Promotion on Override

If the user sets an override on a **base atom** (one not yet in the diff):

1. Create an UNCHANGED marker in the diff for that base atom (same pattern as `add_bond_tool.rs::resolve_atom_to_diff_id`).
2. Set the flag on the new diff atom's `flags`.
3. Migrate selection from base to diff (same as existing promotion).

This is consistent with the existing principle: any edit to a base atom creates a diff entry.

### Eval Changes

**Pin 0 (result):** `apply_diff` already calls `merge_atom_metadata(target_id, diff_atom, base_atom)` which ORs flags from both sources (line 237 of `atomic_structure/mod.rs`). If the override is on the diff atom's flags, it automatically appears on the result atom. **No manual provenance-mapping loop needed.**

**Pin 1 (diff):** The diff clone is `self.diff.clone()`. Flags are already on the atoms. **No manual application loop needed.**

### What Gets Removed

```rust
// DELETE from AtomEditData:
pub frozen_base_atoms: HashSet<u32>,
pub frozen_diff_atoms: HashSet<u32>,
pub hybridization_override_base_atoms: HashMap<u32, u8>,
pub hybridization_override_diff_atoms: HashMap<u32, u8>,

// DELETE from eval():
// - The 4 provenance-mapping loops (pin 0: frozen base/diff, hyb base/diff)
// - The 2 direct-application loops (pin 1: frozen diff, hyb diff)

// DELETE:
// - promote_base_atom_metadata() helper
// - All promote_base_atom_metadata() call sites in 4 promotion functions

// DELETE or SIMPLIFY:
// - AtomEditFrozenChangeCommand (frozen changes become regular diff mutations)
// - AtomEditHybridizationChangeCommand (hybridization changes become regular diff mutations)
// - FrozenDelta, HybridizationDelta types
// - Separate serialization fields for frozen/hybridization maps
```

### What Changes

**Setting an override (API layer):**

```rust
// Before (current):
fn atom_edit_set_hybridization_override(...) {
    for &base_id in &data.selection.selected_base_atoms {
        data.hybridization_override_base_atoms.insert(base_id, value);
        // + undo delta tracking
    }
    for &diff_id in &data.selection.selected_diff_atoms {
        data.hybridization_override_diff_atoms.insert(diff_id, value);
        // + undo delta tracking
    }
}

// After (proposed):
fn atom_edit_set_hybridization_override(...) {
    // Promote any selected base atoms to diff first
    promote_selected_base_atoms_to_diff(data);
    // Now all selected atoms are diff atoms — set flags directly
    for &diff_id in &data.selection.selected_diff_atoms {
        data.diff.set_atom_hybridization_override(diff_id, value);
        // Recorded by DiffRecorder (flag change = atom state change)
    }
}
```

Same pattern for freeze/unfreeze.

**Undo/redo:**

Flag changes become part of `AtomEditMutationCommand` via the `DiffRecorder`. The recorder already captures `AtomState` (atomic_number, position, anchor). Extend `AtomState` to include `flags`:

```rust
pub struct AtomState {
    pub atomic_number: i16,
    pub position: DVec3,
    pub anchor: Option<DVec3>,
    pub flags: u16,  // NEW: captures frozen, hybridization, etc.
}
```

When undoing, the recorder restores the atom's full state including flags. No separate command types needed.

**Serialization:**

The diff `AtomicStructure` is already serialized (atom positions, bonds, anchors). Flags are part of each atom. If the serialization format doesn't currently persist `Atom.flags`, add it. The 4 external map fields in `SerializableAtomEditData` become unnecessary.

Migration: on load, if the old format has `frozen_base_atoms` / `hybridization_override_*_atoms` fields, apply them to the diff atoms as part of the loader (one-time migration).

**Drag skip (frozen check):**

Currently `drag_selected_by_delta` and `apply_transform` check `frozen_diff_atoms.contains(&id)`. After the change, read directly from the atom:

```rust
// Before:
.filter(|&&id| !self.frozen_diff_atoms.contains(&id))

// After:
.filter(|&&id| !self.diff.get_atom(id).map_or(false, |a| a.is_frozen()))
```

### Hover / UI

The hover tooltip reads `atom.hybridization_override()` and `atom.is_frozen()` from the evaluated output structure. Since flags flow through `apply_diff` → `merge_atom_metadata` automatically, the hover works without any special handling. Same for both pin 0 and pin 1.

## Implementation Plan

### Phase 1: Extend DiffRecorder to capture flags

- Add `flags: u16` to `AtomState` in `diff_recorder.rs`.
- Update `AtomEditMutationCommand` undo/redo to restore flags.
- Add recording wrappers: `set_frozen_recorded`, `set_hybridization_override_recorded`.

### Phase 2: Move overrides onto diff atoms

- Change `atom_edit_set_hybridization_override` and `atom_edit_toggle_frozen` to promote base atoms and set flags on diff atoms.
- Remove `hybridization_override_base_atoms`, `hybridization_override_diff_atoms`, `frozen_base_atoms`, `frozen_diff_atoms` from `AtomEditData`.
- Remove the 6 manual application loops from `eval()`.
- Remove `promote_base_atom_metadata()` and its 4 call sites.
- Update `drag_selected_by_delta`, `apply_transform` frozen checks to read from atom.

### Phase 3: Remove old undo commands

- Remove `AtomEditFrozenChangeCommand`, `AtomEditHybridizationChangeCommand`.
- Remove `FrozenDelta`, `HybridizationDelta`, `FrozenProvenance`, `HybridizationProvenance` types.
- Update API functions to use `with_atom_edit_undo` instead of pushing separate commands.

### Phase 4: Serialization migration

- Remove the 4 map fields from `SerializableAtomEditData`.
- Ensure `Atom.flags` are persisted in the diff's serialized atoms (may already be the case — verify).
- Add backward-compat migration in the loader: read old map fields if present, apply to diff atoms.

### Phase 5: Tests

- Update existing tests that directly manipulate the external maps.
- Verify that the existing `atom_edit_undo_test.rs` tests still pass (most should need only import/setup changes).
- Remove tests for deleted command types.

## Benefits

- **Adding a new per-atom property:** Add a bit to `Atom.flags`, add a setter. Done. No maps, no provenance loops, no separate undo commands, no serialization fields.
- **No promotion bugs:** There's nothing to forget to migrate — the flag lives on the atom that gets promoted.
- **No eval bugs:** `merge_atom_metadata` already ORs flags. No manual loops to miss.
- **Simpler undo:** One command type handles all diff mutations including flag changes.
- **Less code:** ~200-300 lines of map management, undo commands, and provenance loops removed.

## Risks

- **UNCHANGED markers:** Setting an override on a base atom creates an UNCHANGED diff entry. This slightly inflates the diff, but the same pattern is already used by `add_bond_tool` and `hydrogen_passivation` — it's an established pattern, not a new concern.
- **Serialization backward compat:** Needs a migration path for existing `.cnnd` files with the old map format. Straightforward — apply maps to diff atoms on load.
- **Flag bits exhaustion:** `Atom.flags` is `u16` with 5 bits used (selected, H passivation, frozen, 2 for hybridization). 11 bits remain. If many more per-atom properties are needed, consider a side-table per `AtomicStructure` (not per `AtomEditData`). But 11 bits is plenty for foreseeable needs.
