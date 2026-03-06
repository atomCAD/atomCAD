# Design: UNCHANGED Atom Marker for Bond-Only Diffs

**Status:** Plan
**Related:** [diff_representation_and_apply_diff.md](diff_representation_and_apply_diff.md)

---

## 1. Problem Statement

When a user adds, removes, or changes a bond between two existing base atoms in the `atom_edit` node, the current code creates **identity entries** in the diff: full copies of the atoms (with their real `atomic_number` and `position`) that serve purely as bond endpoints.

This causes two problems:

1. **Semantic incorrectness.** The identity atoms are indistinguishable from genuinely modified atoms. `apply_diff` counts them as `atoms_modified` even though the user only changed a bond.

2. **Unnecessary atom replacement.** When matched, the diff atom *replaces* the base atom in the result. Since the identity entry is an exact copy, the output is identical, but the intent ("only the bond changed") is lost. This prevents future optimizations that could skip atom processing when only bonds are affected.

---

## 2. Solution: UNCHANGED Atomic Number

Introduce a new special atomic number constant, `UNCHANGED_ATOMIC_NUMBER`, that means: *"this atom exists in the base structure and is not being modified by this diff; it is present only as a bond endpoint reference."*

### 2.1 The Fifth Diff Atom Kind

The existing four kinds (from `diff_representation_and_apply_diff.md` section 2.2) become five:

| Kind | `atomic_number` | Anchor | Meaning |
|------|-----------------|--------|---------|
| **Addition** | > 0 | none | Brand new atom |
| **Delete marker** | = 0 | — | Marks base atom for removal |
| **Replacement** | > 0 | = position | Change element, same position |
| **Move** | > 0 | != position | Reposition atom |
| **Unchanged** | = -1 | none | Bond endpoint reference only |

### 2.2 Constant Value

```rust
/// Atomic number used as an "unchanged" marker in diff structures.
/// An atom with this atomic number means "match the base atom at this position
/// but do not modify it." Used when only bonds between existing atoms change.
pub const UNCHANGED_ATOMIC_NUMBER: i16 = -1;
```

**Why -1?**
- `atomic_number` is `i16`, so negative values are available.
- Real elements are 1..=118. Atomic number 0 is already taken by `DELETED_SITE_ATOMIC_NUMBER`.
- -1 is the natural sentinel: clearly invalid as a real element, easy to check, adjacent to the existing delete marker convention.

### 2.3 No Anchor Required

UNCHANGED atoms do **not** need an anchor position. Their `atomic_number = -1` is sufficient to identify them as a special marker, just as `atomic_number = 0` identifies delete markers (which also have no anchor).

**Matching:** UNCHANGED atoms are placed at the base atom's position. The existing matching algorithm in `match_diff_atoms()` uses the atom's own position when no anchor is present (line 218-221: `match_pos = anchor_position.unwrap_or(diff_atom.position)`), so matching works correctly without an anchor.

**Orphan detection:** If the base atom no longer exists (deleted upstream), the UNCHANGED atom will be unmatched. The `apply_diff` unmatched-diff-atoms loop must check `is_unchanged_marker()` (just as it already checks `is_delete_marker()`) and skip the atom. This correctly drops the bond reference when the base atom is gone.

**Classification:** `classify_diff_atom()` checks `atomic_number` before checking for anchors, so UNCHANGED atoms (no anchor) are correctly classified as `Unchanged` rather than `PureAddition`.

---

## 3. Display

UNCHANGED atoms need to be visible in the **diff view** so the user can see which base atoms are referenced as bond endpoints. They should look clearly distinct from both real atoms and delete markers.

### 3.1 Visual Properties

| Property | Value | Rationale |
|----------|-------|-----------|
| **Color** | `Vec3::new(0.4, 0.6, 0.9)` — light blue | Clearly informational/reference. Impossible to confuse with Carbon's near-black `(0.18, 0.18, 0.18)`, which is the most common element in atomCAD. Distinct from delete markers (red) and anchor arrows (orange). |
| **Radius** | 0.4 Angstroms | Slightly smaller than delete markers (0.5 A) to convey that these are reference points, not active edits. Large enough to be clearly visible and clickable. |
| **Roughness** | 0.7 | Very matte, ghostly appearance. More matte than delete markers (0.5) to further differentiate. |
| **Metallic** | 0.0 | Consistent with delete markers. |

### 3.2 Selection Color

When selected, UNCHANGED atoms should follow the same convention as delete markers: turn bright magenta `Vec3::new(1.0, 0.2, 1.0)` with roughness 0.15.

### 3.3 Constants to Add

In `rust/src/display/atomic_tessellator.rs`:

```rust
// color for unchanged atom markers in diff structures (light blue)
const UNCHANGED_MARKER_COLOR: Vec3 = Vec3::new(0.4, 0.6, 0.9);
// fixed radius for unchanged atom markers (Angstroms)
const UNCHANGED_MARKER_RADIUS: f64 = 0.4;
// roughness for unchanged atom markers (matte/ghostly)
const UNCHANGED_MARKER_ROUGHNESS: f32 = 0.7;
```

---

## 4. Implementation Plan

Tests are written alongside the code in each phase, not as a separate step. Test files go in `rust/tests/` per project convention.

### Phase 1: Core — Data Model + `apply_diff` + Display

**Data model** (`crystolecule/atomic_structure/mod.rs`, `atom.rs`):

1. Add constant `pub const UNCHANGED_ATOMIC_NUMBER: i16 = -1;` next to `DELETED_SITE_ATOMIC_NUMBER`.
2. Add method `Atom::is_unchanged_marker(&self) -> bool` next to `is_delete_marker()`.
3. Add method `Atom::is_special_marker(&self) -> bool` that returns `true` for both delete and unchanged markers.

**`apply_diff` algorithm** (`crystolecule/atomic_structure_diff.rs`):

**Change 1 — matched diff atoms** (Step 2, around line 113-135):

```
Current logic:
  if diff_atom.is_delete_marker() → delete base atom
  else → use diff atom (replacement/move)

New logic:
  if diff_atom.is_delete_marker() → delete base atom
  else if diff_atom.is_unchanged_marker() → keep base atom as-is (pass through)
       but still register diff_id → result_id mapping for bond resolution
  else → use diff atom (replacement/move)
```

Key details:
- The base atom passes through unchanged (position, atomic_number, all properties from base).
- The match is still recorded in `diff_to_result` so bond resolution (Phase 3a and 3b) works correctly.
- `stats.atoms_modified` is NOT incremented. Add a new stat field `stats.unchanged_references` to track these.
- Provenance: consider adding `DiffUnchanged { diff_id, base_id }` to the provenance enum if it exists.

**Change 2 — unmatched diff atoms** (around line 137-163):

After the existing `is_delete_marker()` check (line 141), add an `is_unchanged_marker()` check:

```rust
if diff_atom.is_unchanged_marker() {
    // Unmatched UNCHANGED marker → base atom no longer exists.
    // Drop this reference (and any bonds attached to it).
    stats.orphaned_tracked_atoms += 1;
    continue;
}
```

This replaces the anchor-based orphan detection for UNCHANGED atoms. The existing anchor-based orphan detection (line 147) continues to handle anchored atoms (moves/replacements) as before.

**Display** (`display/atomic_tessellator.rs`):

In `get_atom_color_and_material()` and `get_displayed_atom_radius()`:
- Add `is_unchanged_marker()` checks after the existing `is_delete_marker()` checks.
- Return the UNCHANGED marker color, radius, and roughness.
- Selection color: same magenta as delete markers.

**`DiffAtomKind`** (`structure_designer/nodes/atom_edit/types.rs`):

Add `Unchanged` variant. Update `classify_diff_atom()` to check `is_unchanged_marker()` before the anchor check:

```rust
pub fn classify_diff_atom(diff: &AtomicStructure, diff_id: u32) -> DiffAtomKind {
    if let Some(atom) = diff.get_atom(diff_id) {
        if atom.is_delete_marker() {
            DiffAtomKind::DeleteMarker
        } else if atom.is_unchanged_marker() {
            DiffAtomKind::Unchanged
        } else if diff.has_anchor_position(diff_id) {
            DiffAtomKind::MatchedBase
        } else {
            DiffAtomKind::PureAddition
        }
    } else {
        DiffAtomKind::PureAddition
    }
}
```

The `is_unchanged_marker()` check must come before the anchor check. This is important because if an UNCHANGED atom is later promoted to a real atom (Phase 2 promotion), it gains an anchor and transitions to `MatchedBase` naturally.

**Tests for Phase 1:**

*Data model (`tests/crystolecule/atomic_structure_test.rs` or similar):*

1. `is_unchanged_marker()` returns `true` for `UNCHANGED_ATOMIC_NUMBER`, `false` for 0, 6, 1.
2. `is_delete_marker()` returns `false` for `UNCHANGED_ATOMIC_NUMBER` (not confused).
3. `is_special_marker()` returns `true` for both 0 and -1, `false` for real elements.

*`classify_diff_atom` (`tests/structure_designer/`):*

4. Classify all five kinds: `DeleteMarker` (atomic_number=0), `Unchanged` (atomic_number=-1, no anchor), `MatchedBase` (real element + anchor), `PureAddition` (real element, no anchor). Also verify a promoted UNCHANGED atom (atomic_number>0, has anchor) classifies as `MatchedBase`.

*`apply_diff` — matched UNCHANGED (`tests/crystolecule/atomic_structure_diff_test.rs`):*

5. **Bond addition via UNCHANGED:** base has atoms A(C), B(N) with no bond. Diff has two UNCHANGED atoms at A, B positions with a single bond. Result: A(C), B(N) with the bond. Atoms have base positions and atomic numbers (not -1).
6. **Bond deletion via UNCHANGED:** base has A-B single bond. Diff has two UNCHANGED atoms with `BOND_DELETED`. Result: A, B with no bond, atoms unchanged.
7. **Bond order change via UNCHANGED:** base has A-B single bond. Diff has two UNCHANGED atoms with double bond. Result: A-B double bond, atoms unchanged.
8. **Stats correctness:** `atoms_modified=0`, `unchanged_references=2`, `bonds_added=1` (or `bonds_deleted` as appropriate). Verify `atoms_added` and `atoms_deleted` are 0.
9. **Provenance:** UNCHANGED atoms produce correct `diff_to_result` and `base_to_result` mappings. The result atom source is `DiffUnchanged` (or whichever variant is chosen) with both `diff_id` and `base_id`.

*`apply_diff` — unmatched UNCHANGED (orphan detection):*

10. **Full orphan:** base is empty. Diff has one UNCHANGED atom with a bond to an added atom. UNCHANGED atom is orphaned (`orphaned_tracked_atoms += 1`), added atom survives, bond is dropped (`orphaned_bonds += 1`).
11. **Partial orphan:** base has atom A but not B. Diff has UNCHANGED at A and UNCHANGED at B with a bond. A matches (passes through), B is orphaned, bond is dropped. Result has only A.

*`apply_diff` — mixed diffs:*

12. **UNCHANGED alongside real edits:** base has A, B, C, D. Diff: delete A, move B (anchor + new position), add E, UNCHANGED C and D with a bond. Result: B at new position, C, D with bond, E added, A gone. Stats: `atoms_deleted=1`, `atoms_modified=1`, `atoms_added=1`, `unchanged_references=2`, `bonds_added=1`.
13. **UNCHANGED + bond to added atom:** base has A. Diff has UNCHANGED at A + added atom E + bond between them. Result: A and E bonded. Tests cross-kind bond resolution.
14. **UNCHANGED + bond to moved atom:** base has A, B. Diff has UNCHANGED at A + move B (real element, anchor at old B, new position) + bond between them. Result: A at original pos, B at new pos, bonded.

*Display (`tests/display/` or integration):*

15. **UNCHANGED marker visual:** `get_displayed_atom_radius()` returns `UNCHANGED_MARKER_RADIUS` (0.4) for an UNCHANGED atom. `get_atom_color_and_material()` returns light blue `(0.4, 0.6, 0.9)`, roughness 0.7, metallic 0.0.
16. **UNCHANGED marker selected:** when selected, returns magenta `(1.0, 0.2, 1.0)`, roughness 0.15.
17. **Not confused with delete:** verify delete marker still returns red, radius 0.5.

### Phase 2: atom_edit — Use UNCHANGED for Identity Entries + Promotion

**Identity entry creation** (`operations.rs`, `atom_edit_data.rs`, `add_bond_tool.rs`):

All places that currently create identity entries by calling `diff.add_atom(atomic_number, position)` with the real atomic number must use `UNCHANGED_ATOMIC_NUMBER` instead. No anchor is needed.

Affected locations:

1. **`change_bond_order_result_view`** (operations.rs ~481-490): Promoting base endpoints for bond order change.
   ```
   Before: diff.add_atom(an, pos)
   After:  diff.add_atom(UNCHANGED_ATOMIC_NUMBER, pos)
   ```

2. **`change_selected_bonds_order_result_view`** (operations.rs ~617-641): Batch bond order change with dedup HashMap.
   ```
   Before: diff.add_atom(an, pos)
   After:  diff.add_atom(UNCHANGED_ATOMIC_NUMBER, pos)
   ```
   Note: the dedup key changes from `(an, pos_bits)` to just `(pos_bits)` since `an` is always `-1`. Alternatively, keep the key structure unchanged — dedup still works correctly since positions are unique.

3. **`delete_selected_in_result_view`** (operations.rs ~92-112) and **`apply_delete_result_view`** (atom_edit_data.rs ~427-437): Bond deletion identity entries.
   ```
   Before: diff.add_atom(an, pos)
   After:  diff.add_atom(UNCHANGED_ATOMIC_NUMBER, pos)
   ```

4. **`resolve_to_diff_id`** (add_bond_tool.rs ~103-134): When `BasePassthrough` atoms become diff atoms for bonding.
   ```
   Before: diff.add_atom(atom_info.0, atom_info.1)
   After:  diff.add_atom(UNCHANGED_ATOMIC_NUMBER, atom_info.1)
   ```

**Promotion from UNCHANGED to real atom** (`operations.rs`):

When a user first adds a bond (creating an UNCHANGED entry) then later moves or replaces that atom, the UNCHANGED marker must be promoted to a real diff atom.

**Selection tracking constraint:** Identity entry creation (bond tool, bond order change, bond deletion) adds UNCHANGED atoms to the diff but does **not** update the selection. The atom remains in `selected_base_atoms`, not `selected_diff_atoms`. This means subsequent drag/replace/transform operations hit the **base atom** code path, not the diff atom path. If the base atom path naively creates a new diff atom, it orphans the UNCHANGED entry and loses any bonds attached to it.

**Solution — detect existing diff entries in base-atom promotion:**

`drag_selected_by_delta`, `apply_replace`, and `apply_transform` all have a Phase 1 (immutable borrows) that gathers info about selected base atoms. This phase must be extended to detect base atoms that already have diff entries:

1. For each base atom in `selected_base_atoms`, look up its result atom via `provenance.base_to_result`.
2. Check the result atom's source via `provenance.sources`:
   - If `AtomSource::DiffMatchedBase { diff_id, .. }` → a diff entry already exists. Collect `(base_id, diff_id, atomic_number, position)`.
   - If `AtomSource::BasePassthrough(_)` → no diff entry. Collect `(base_id, atomic_number, position)` as before.

In Phase 2 (mutation), handle the two cases differently:

- **No existing diff entry:** Create a new diff atom with the real `atomic_number`, set anchor, move to `selected_diff_atoms`. This is the existing behavior.
- **Existing diff entry (UNCHANGED or otherwise):** Reuse the existing `diff_id`. If it is an UNCHANGED marker, promote it: set `atomic_number` to the real value and set the anchor position. Then apply the operation (move/replace/transform). Move from `selected_base_atoms` to `selected_diff_atoms` using the existing `diff_id`. All bonds attached to this diff atom are preserved.

After promotion, the atom is a normal `MatchedBase` diff atom — `classify_diff_atom` returns `MatchedBase` because it now has an anchor and `atomic_number > 0`.

**Where the real `atomic_number` comes from:** These functions follow the three-phase borrow pattern. The real `atomic_number` must be gathered in **Phase 1** (immutable borrows) from the result structure, before the mutable `AtomEditData` borrow in Phase 3.

**Note:** This detection also correctly handles the case where a base atom has a non-UNCHANGED diff entry (e.g., from a prior replacement that was undone but the diff entry was retained). In that case, `diff_id` is reused and the existing `atomic_number`/anchor are already correct — only the position update is needed. The current code would create a duplicate in this scenario too, so this fix addresses a pre-existing latent bug as well.

**Text format** (`atom_edit/text_format.rs`):

Add `unchanged @ x y z` representation, mirroring the existing `- @ x y z` delete marker format. No anchor is serialized since UNCHANGED atoms have none.

**Serialization (.cnnd):** No changes needed — existing serialization handles arbitrary atomic numbers. Verify with a roundtrip test.

**Tests for Phase 2:**

*Identity entry creation — verify UNCHANGED is used:*

18. **`resolve_to_diff_id` (AddBond tool):** bond two BasePassthrough atoms. The diff atoms created have `atomic_number = UNCHANGED_ATOMIC_NUMBER` and no anchor.
19. **`change_bond_order_result_view`:** change bond order between two base atoms. Identity entries use UNCHANGED.
20. **`change_selected_bonds_order_result_view`:** batch bond order change. Identity entries use UNCHANGED. Dedup still works (no duplicate UNCHANGED atoms at same position).
21. **`delete_selected_in_result_view` / `apply_delete_result_view`:** delete a bond between two base atoms. Identity entries use UNCHANGED.

*Promotion — drag:*

22. **Single bond, drag one endpoint:** create UNCHANGED entry for bond (A-B), then drag A. UNCHANGED entry for A is reused and promoted (real atomic_number, anchor set). Bond A-B survives. Result shows A at new position with bond.
23. **Multiple bonds, drag shared atom:** atom A has UNCHANGED bonds to B and C. Drag A. Both bonds survive promotion.
24. **Both endpoints dragged:** A and B bonded via UNCHANGED entries. Select both, drag. Both promoted, bond survives.

*Promotion — replace:*

25. **Replace element:** create UNCHANGED entry for bond (A-B). Replace A's element (C→Si). UNCHANGED entry promoted with new element. Bond survives.

*Promotion — transform:*

26. **Apply transform:** create UNCHANGED entry for bond (A-B). Apply rotation/translation transform to A. UNCHANGED entry promoted. Bond survives.

*Duplicate prevention:*

27. **Existing UNCHANGED entry:** base atom A has UNCHANGED diff entry from prior bond. Select A (in `selected_base_atoms`), drag. Verify: existing diff_id reused, no new diff atom created, bond preserved.
28. **Existing non-UNCHANGED entry:** base atom A already has a MatchedBase diff entry (e.g., from a prior move). Select A (still in `selected_base_atoms`), drag again. Verify: existing diff_id reused, not duplicated.

*Serialization:*

29. **Text format roundtrip:** diff with two UNCHANGED atoms and a bond. Serialize → parse → serialize. Output matches. Format is `unchanged @ (x, y, z)`.
30. **Text format parse rejects bad unchanged:** `unchanged` with extra fields or missing position → parse error.
31. **.cnnd roundtrip:** save and reload a project containing a diff with UNCHANGED atoms. Diff is preserved exactly.

*Backward compatibility / regression:*

32. **Old identity entries still work:** diff with real atomic numbers (not UNCHANGED) as identity entries — the existing replacement path handles them correctly. This is the pre-existing behavior and must not regress.
33. **Delete markers unaffected:** verify all existing delete marker tests still pass (not confused with UNCHANGED).
34. **Existing move/replacement tests unaffected:** verify existing anchored atom tests still pass.

---

## 5. Migration / Backwards Compatibility

Existing `.cnnd` files contain identity entries with real atomic numbers (no UNCHANGED markers). These will continue to work correctly with the updated `apply_diff` — real atomic numbers are processed as before (replacement path). No migration is needed.

New saves will use UNCHANGED markers. Old versions of atomCAD loading new files will see `atomic_number = -1`, which is not a valid element. The behavior depends on how the old code handles unknown elements:
- Display: falls back to default gray atom — acceptable degradation.
- `apply_diff`: treats it as a normal (non-delete) matched atom, replacing base with the diff atom. Since `atomic_number = -1` would produce a nonsensical result, this is a breaking change for old versions. **This is acceptable** since .cnnd is not a stable interchange format.

---

## 6. Summary of Changes by File

| File | Change |
|------|--------|
| `crystolecule/atomic_structure/mod.rs` | Add `UNCHANGED_ATOMIC_NUMBER = -1` |
| `crystolecule/atomic_structure/atom.rs` | Add `is_unchanged_marker()`, `is_special_marker()` |
| `crystolecule/atomic_structure_diff.rs` | Handle UNCHANGED in matched-atom processing and unmatched-atom orphan detection; add `unchanged_references` stat |
| `display/atomic_tessellator.rs` | Add UNCHANGED color/radius/roughness constants and rendering logic |
| `structure_designer/nodes/atom_edit/operations.rs` | Use UNCHANGED for identity entries; handle promotion on drag/replace/transform |
| `structure_designer/nodes/atom_edit/atom_edit_data.rs` | Use UNCHANGED in `apply_delete_result_view` |
| `structure_designer/nodes/atom_edit/add_bond_tool.rs` | Use UNCHANGED in `resolve_to_diff_id` |
| `structure_designer/nodes/atom_edit/types.rs` | Add `Unchanged` variant to `DiffAtomKind` |
| `structure_designer/nodes/atom_edit/text_format.rs` | Add `unchanged` keyword for serialization |
| `tests/crystolecule/` | New test cases for UNCHANGED behavior |
| `doc/atom_edit/diff_representation_and_apply_diff.md` | Update to document fifth atom kind |

---

## 7. Cross-Layer Impact

Adding `unchanged_references` to `DiffStats` (Phase 1) affects the Flutter Rust Bridge API layer:

1. **`rust/src/api/structure_designer/structure_designer_api_types.rs`** — The `DiffStatsApi` mirror struct must add the new field.
2. **`rust/src/api/structure_designer/structure_designer_api.rs`** — The `DiffStats` → `DiffStatsApi` conversion must map the new field.
3. **FRB codegen** — Run `flutter_rust_bridge_codegen generate` to regenerate `lib/src/rust/` bindings.
4. **Flutter UI** (`lib/structure_designer/`) — Update the diagnostic display to show or ignore the new stat.
