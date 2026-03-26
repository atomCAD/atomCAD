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

1. Create a **real diff atom** with the base atom's `atomic_number`, `position`, and `anchor=position` (same pattern as drag/transform promotion).
2. Copy base atom's flags via `promote_base_atom_metadata`.
3. Set the override flag on the new diff atom's `flags`.
4. Migrate selection from base to diff (same as existing promotion).

This uses a real diff atom (not an UNCHANGED marker) because UNCHANGED markers mean "bond endpoint reference — do not modify the atom." A flag override *is* a modification. Using a real diff atom with `anchor=position` creates a Replacement entry where the atomic number and position are identical to the base but flags differ. `apply_diff`'s matched-normal-atom path handles this correctly via `copy_atom_metadata(result_id, diff_atom)`.

This is consistent with the existing principle: any edit to a base atom creates a diff entry.

### Promotion Must Copy Base Flags

When a base atom is promoted to diff (drag, transform, minimization, gadget, etc.), the promotion code creates a new diff atom via `add_atom(atomic_number, position)` — which initializes `flags: 0`. Today this is fine because metadata lives in external maps migrated by `promote_base_atom_metadata()`. Under this design, `promote_base_atom_metadata()` must be **reimplemented** (not deleted) to copy `Atom.flags` (except selection bit 0) from the base atom to the new diff atom:

```rust
// Before (current): migrates external maps
pub fn promote_base_atom_metadata(&mut self, base_id: u32, diff_id: u32) {
    if let Some(hyb) = self.hybridization_override_base_atoms.remove(&base_id) {
        self.hybridization_override_diff_atoms.insert(diff_id, hyb);
    }
    if self.frozen_base_atoms.remove(&base_id) {
        self.frozen_diff_atoms.insert(diff_id);
    }
}

// After (proposed): copies flags from base atom to diff atom via recorded method
pub fn promote_base_atom_metadata(&mut self, base_atom: &Atom, diff_id: u32) {
    let flags = (base_atom.flags & !0x1); // all flags except selected
    self.set_flags_recorded(diff_id, flags);
}
```

**Critical:** This must use a recorded method (`set_flags_recorded`), not a direct mutation on `self.diff.get_atom_mut()`. Promotion happens inside recording sessions (drag, transform, etc.). If the flag copy is unrecorded, the `AtomDelta::Added` captures `flags=0`, and on redo the base atom's flags are lost. Using a recorded method generates a `Modified` delta, which the `DiffRecorder::coalesce()` merges with the preceding `Added` into a single `Added` with the correct final flags.

This ensures the diff atom inherits the base atom's frozen/hybridization/passivation state at promotion time. Any subsequent override then modifies the diff atom's flags directly. The 6 existing call sites keep calling `promote_base_atom_metadata` — only the signature and body change.

### Eval Changes

#### `merge_atom_metadata` Must Be Replaced, Not Reused

The existing `merge_atom_metadata` uses OR semantics for flags:

```rust
target.flags = (primary.flags | secondary.flags) & !0x1;
```

This is **wrong** for the proposed design in two ways:

1. **Cannot clear a flag.** If a base atom has `frozen=true` and the user unfreezes it in the diff, the diff atom has `frozen=0`, but OR re-applies `frozen=1` from the base. The user's unfreeze is silently ignored.

2. **Corrupts multi-bit fields.** Hybridization uses bits 3-4 as a 2-bit value (0=Auto, 1=Sp3, 2=Sp2, 3=Sp1). OR merges individual bits: `Sp3(01) | Sp2(10) = Sp1(11)`. Setting Sp2 on an Sp3 atom produces Sp1. Clearing back to Auto is impossible.

Today these bugs are latent because frozen/hybridization flags are never set on atoms — they live in external maps. Moving them onto atoms activates the bugs.

**Fix:** In `apply_diff`, the matched-atom path (diff atom replaces/moves a base atom) should use `copy_atom_metadata(result_id, diff_atom)` instead of `merge_atom_metadata(result_id, diff_atom, base_atom)`. The diff atom is the single source of truth — it already carries forward any base flags from promotion, plus any user overrides applied on top. This is consistent with how `atomic_number` and `position` already work: they come from the diff atom, not merged with the base.

```rust
// Before (apply_diff matched-atom path):
result.merge_atom_metadata(result_id, diff_atom, base_atom);

// After:
result.copy_atom_metadata(result_id, diff_atom);
```

Then delete `merge_atom_metadata` entirely — no callers remain.

**Pin 0 (result):** `apply_diff` uses `copy_atom_metadata(result_id, diff_atom)` for matched atoms. The diff atom's flags (inherited from base at promotion + any overrides) flow through automatically. **No manual provenance-mapping loop needed.**

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

// DELETE from atomic_structure/mod.rs:
// - merge_atom_metadata() — replaced by copy_atom_metadata() from diff atom

// REIMPLEMENT (same name, new body):
// - promote_base_atom_metadata() — changes from map migration to flag copying
// - All 6 call sites remain, signature changes to take &Atom instead of base_id

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

The hover tooltip reads `atom.hybridization_override()` and `atom.is_frozen()` from the evaluated output structure. Since flags flow through `apply_diff` → `copy_atom_metadata` automatically, the hover works without any special handling. Same for both pin 0 and pin 1.

## Pre-Implementation: Save Backward-Compat Fixture

Before starting any implementation, save a `.cnnd` file that exercises the old format: frozen overrides on both base and diff atoms, hybridization overrides on both base and diff atoms. Place in `rust/tests/fixtures/inline_metadata_migration/`. This becomes the Phase 4 migration test fixture.

## Implementation Plan

Tests are integrated into each phase — written as soon as their prerequisites compile.

### Phase 1: Extend DiffRecorder to capture flags

**Code:**

- Add `flags: u16` to `AtomState` in `diff_recorder.rs`.
- Update `AtomEditMutationCommand` undo/redo to restore flags.
- Add recording wrappers: `set_flags_recorded`, `set_frozen_recorded`, `set_hybridization_override_recorded`.

**Tests — modify:**

- Tests that construct or assert on `AtomState` (e.g., `coalesce_added_modified`, `coalesce_modified_modified`, `coalesce_modified_removed`) must include the new `flags` field.

**Tests — introduce:**

- `recording_set_flags_produces_delta` — `set_flags_recorded` generates a `Modified` delta with correct before/after flags.
- `undo_atom_edit_flag_change` — Record a flag change via `set_frozen_recorded`, undo, verify atom flags are restored.
- `coalesce_added_then_flag_modified` — Record `add_atom_recorded` followed by `set_flags_recorded` on the same atom. Verify `coalesce()` merges them into a single `Added` with the final flags. (This validates the critical pitfall: unrecorded flag copies at promotion would produce `flags=0` in the `Added` delta.)

### Phase 2: Move overrides onto diff atoms

**Code:**

- Change `atom_edit_set_hybridization_override` and `atom_edit_toggle_frozen` to promote base atoms and set flags on diff atoms.
- Remove `hybridization_override_base_atoms`, `hybridization_override_diff_atoms`, `frozen_base_atoms`, `frozen_diff_atoms` from `AtomEditData`.
- Remove the 6 manual application loops from `eval()`.
- Reimplement `promote_base_atom_metadata()`: change from map migration to flag copying (see "Promotion Must Copy Base Flags" section). Update signature at all 6 call sites.
- In `apply_diff` matched-atom path: replace `merge_atom_metadata(result_id, diff_atom, base_atom)` with `copy_atom_metadata(result_id, diff_atom)`. Delete `merge_atom_metadata` from `atomic_structure/mod.rs`. The UNCHANGED path is unchanged — it still copies from the base atom.
- Update `drag_selected_by_delta`, `apply_transform` frozen checks to read from atom.

**Tests — delete (replaced by new tests below, not just removed):**

These tests use `AtomEditFrozenChangeCommand`/`AtomEditHybridizationChangeCommand` + external maps. They are replaced by equivalent tests using the new inline-flag approach:

- `undo_atom_edit_change_hybridization_override`
- `undo_atom_edit_set_hybridization_override_diff_atoms`
- `undo_atom_edit_remove_hybridization_override`
- `undo_atom_edit_hybridization_multiple_atoms`
- `undo_atom_edit_freeze_diff_atoms`
- `undo_atom_edit_unfreeze_atoms`
- `undo_atom_edit_clear_frozen`
- `undo_atom_edit_freeze_already_frozen_is_noop`
- `undo_atom_edit_set_hybridization_already_same_is_noop`

**Tests — modify (external map references → atom flag reads):**

In `atom_edit_undo_test.rs`:
- `drag_frozen_base_atom_returns_all_frozen` — promote base atom + set frozen flag on diff atom (was: `frozen_base_atoms.insert()`).
- `drag_frozen_diff_atom_not_moved` — set frozen flag on diff atom directly (was: `frozen_diff_atoms.insert()`).
- `drag_all_frozen_returns_all_frozen` — same pattern.
- `drag_no_frozen_returns_none_frozen` — verify no stale map references.
- `frozen_flag_migrated_on_base_atom_promotion` — verify `promote_base_atom_metadata` copies `Atom.flags` from base to diff (was: verify map entry migration).
- `hybridization_override_migrated_on_base_atom_promotion` — same: verify flags copy, not map migration.
- `frozen_diff_atom_appears_on_pin1_output` — set frozen flag on diff atom directly (was: `frozen_diff_atoms.insert()`), same eval assertion.
- `hybridization_override_appears_on_diff_view_output_atoms` — set hybridization via flag (was: `hybridization_override_diff_atoms.insert()`), same eval assertion.
- `hybridization_override_appears_on_pin1_diff_output` — same pattern.
- `undo_atom_edit_frozen_interleaved_with_mutations` — rewrite to use recorded flag methods instead of separate `AtomEditFrozenChangeCommand`.
- `test_merge_atomic_structure_*` (basic/empty/incremental/undo/with_existing_edits) — update if they reference external maps in setup.

In `atom_edit_mutations_test.rs`:
- `test_apply_transform_skips_frozen_diff_atom` — change from `data.frozen_diff_atoms.insert(id)` to setting frozen flag on the atom.
- `test_apply_transform_all_frozen_diff_atoms_not_moved` — same.

In `continuous_minimization_test.rs`:
- `frozen_atoms_remain_fixed_during_continuous_minimize` — change from `data.frozen_base_atoms.insert(base_id)` to promoting the base atom and setting frozen flag on the diff atom.

**Tests — introduce (validate new semantics):**

- `copy_not_merge_unfreezes_base_atom` — Base atom has `frozen=true`. User unfreezes in diff (diff atom has `frozen=false`). Evaluate pin 0. Verify result atom is NOT frozen. (This is the bug that OR-merge silently reintroduced — validates the `copy_atom_metadata` fix.)
- `copy_not_merge_hybridization_no_corruption` — Base atom has `Sp3`. User sets `Sp2` in diff. Evaluate pin 0. Verify result atom is `Sp2`, not `Sp1` (which OR-merge produces: `01 | 10 = 11`).
- `clearing_hybridization_to_auto_works` — Base atom has `Sp3`. User clears to `Auto` in diff. Evaluate. Verify result atom is `Auto`. (Impossible under OR-merge.)
- `override_on_base_atom_triggers_promotion` — Call `atom_edit_set_hybridization_override` targeting a base atom. Verify a real diff atom (not UNCHANGED) is created with `anchor=position`, same `atomic_number`, and the correct hybridization flag.
- `override_on_existing_diff_atom_no_promotion` — Call `atom_edit_toggle_frozen` on an already-promoted diff atom. Verify no new diff atom is created; existing atom's flags are updated in place.
- `eval_pin0_flags_flow_through` — Set frozen + hybridization on a diff atom. Evaluate pin 0 (result view). Verify both flags appear on the result atom.
- `eval_pin1_flags_flow_through` — Same for pin 1 (diff view). Flags should already be on the cloned diff atoms with no manual application.

### Phase 3: Remove old undo commands

**Code:**

- Remove `AtomEditFrozenChangeCommand`, `AtomEditHybridizationChangeCommand`.
- Remove `FrozenDelta`, `HybridizationDelta`, `FrozenProvenance`, `HybridizationProvenance` types.
- Update API functions to use `with_atom_edit_undo` instead of pushing separate commands.

**Tests — delete:**

- Remove any remaining test helpers that construct `FrozenDelta`, `HybridizationDelta`, `FrozenProvenance`, `HybridizationProvenance`, `AtomEditFrozenChangeCommand`, `AtomEditHybridizationChangeCommand`. (Most tests using these were already rewritten in Phase 2.)

**Tests — introduce (undo via unified mutation command):**

- `undo_freeze_via_mutation_command` — Freeze a diff atom using the new API (which uses `with_atom_edit_undo`), undo, verify atom is unfrozen.
- `undo_hybridization_via_mutation_command` — Same for hybridization.
- `undo_promotion_for_override` — Set override on a base atom (triggers promotion), undo. Verify the diff atom is removed entirely (the `Added` delta is reversed).
- `redo_flag_override_after_undo` — Full undo→redo cycle for a flag-only operation. Verify flags match the post-override state after redo.

### Phase 4: Serialization migration

**Code:**

- Remove the 4 map fields from `SerializableAtomEditData`.
- Ensure `Atom.flags` are persisted in the diff's serialized atoms (may already be the case — verify).
- Add backward-compat migration in the loader: read old map fields if present, apply to diff atoms.

**Tests — introduce:**

- `flags_roundtrip_serialization` — Create an atom_edit with frozen + hybridization flags on diff atoms. Serialize to `.cnnd`, deserialize. Verify flags survived on the correct atoms.
- `backward_compat_migration_from_external_maps` — Load the fixture saved in the pre-implementation step (old format with `frozen_base_atoms`, `hybridization_override_diff_atoms` maps). Verify the loader applies them to diff atoms correctly. Check both base-provenance overrides (should promote to diff) and diff-provenance overrides (should set flags on existing diff atoms).
- `atom_flags_persist_in_diff_structure` — Serialize/deserialize a bare `AtomicStructure` with non-zero flags. Verify flags round-trip. (Guards against `Atom.flags` being silently dropped by the structure serializer.)

## Benefits

- **Adding a new per-atom property:** Add a bit to `Atom.flags`, add a setter. Done. No maps, no provenance loops, no separate undo commands, no serialization fields.
- **No promotion bugs:** `promote_base_atom_metadata` copies all flags at promotion time — nothing to forget.
- **No eval bugs:** `copy_atom_metadata` from the diff atom is the single code path. No manual loops to miss.
- **Simpler undo:** One command type handles all diff mutations including flag changes.
- **Less code:** ~200-300 lines of map management, undo commands, and provenance loops removed.

## Other `Atom.flags` Bits

### Hydrogen Passivation (bit 1) — Already Inline

The H passivation flag marks hydrogen atoms that were added by the passivation operation (so they can be identified and stripped later). It is **already set directly on diff atoms** at creation time (`hydrogen_passivation.rs` calls `set_atom_hydrogen_passivation(h_id, true)` on the diff atom). There are no external `hydrogen_passivation_base_atoms` / `hydrogen_passivation_diff_atoms` maps on `AtomEditData`. This flag already follows the proposed pattern — **no changes needed**.

### Selection (bit 0) — Intentional Exception

Selection must **not** be moved onto diff atoms. It remains in the external `AtomEditSelection` (with separate `selected_base_atoms` and `selected_diff_atoms` sets). Reasons:

1. **Transient UI state, not persistent data.** `copy_atom_metadata` already strips bit 0 (`& !0x1`) when copying flags through `apply_diff`. Selection is never serialized and never flows through evaluation.

2. **Must not trigger promotion.** Users select base atoms constantly (click, marquee, measure) without intending to edit them. Promoting on selection would pollute the diff with UNCHANGED markers from ordinary navigation.

3. **Precursor to edits, not an edit.** The design's rule is "promote when the user sets an override." Selection is the step *before* an override — it would be circular to require promotion at selection time.

4. **Different lifecycle.** Selection changes on every click, potentially hundreds of times per session. Creating diff entries on every selection change would create unnecessary churn and confuse diff semantics.

## Risks

- **Flag-only overrides create Replacement entries:** Setting an override on a base atom creates a real diff atom (not an UNCHANGED marker) with `anchor=position` and the same `atomic_number`. This is a Replacement where nothing changed except flags — semantically correct but slightly inflates the diff and increments `stats.atoms_modified`. The same pattern is already used by drag/transform promotion, so this is established behavior.
- **Serialization backward compat:** Needs a migration path for existing `.cnnd` files with the old map format. Straightforward — apply maps to diff atoms on load.
- **Flag bits exhaustion:** `Atom.flags` is `u16` with 5 bits used (selected, H passivation, frozen, 2 for hybridization). 11 bits remain. If many more per-atom properties are needed, consider a side-table per `AtomicStructure` (not per `AtomEditData`). But 11 bits is plenty for foreseeable needs.
