# atom_edit.rs Refactoring Plan: Extract Testable Modules

## Motivation

`rust/src/structure_designer/nodes/atom_edit/atom_edit.rs` is ~1970 lines and mixes three concerns:

1. **Data model + diff mutations** (structs, `impl AtomEditData`, tool management)
2. **Text format** (serialize/parse diff to human-readable text)
3. **Interaction functions** (select, delete, replace, transform — depend on `StructureDesigner`)

The interaction functions follow a pattern: gather data from `StructureDesigner` (immutable borrows) → plan mutations → execute mutations on `AtomEditData`. The "execute mutations" step is pure data logic that could be unit tested, but currently it's entangled with the `StructureDesigner` gathering code.

This plan extracts (1) text format code into its own file and (2) core mutation logic into testable methods on `AtomEditData`, with new unit tests.

## Current File Structure

```
rust/src/structure_designer/nodes/atom_edit/
├── mod.rs           # Just `pub mod atom_edit;`
└── atom_edit.rs     # Everything (1968 lines)

rust/tests/structure_designer/
└── atom_edit_text_format_test.rs  # 55 tests for text format only
```

## Target File Structure

```
rust/src/structure_designer/nodes/atom_edit/
├── mod.rs                  # pub mod atom_edit; pub mod text_format;
├── atom_edit.rs            # Data model + NodeData impl + interaction functions (~1200 lines)
└── text_format.rs          # Text format serialize/parse (~320 lines, extracted)

rust/tests/structure_designer/
├── atom_edit_text_format_test.rs  # Existing 55 tests (update import path)
└── atom_edit_mutations_test.rs    # NEW: unit tests for core mutation logic
```

## Extraction 1: Text Format → `text_format.rs`

### What to move

Extract lines 313–647 of `atom_edit.rs` into a new file `text_format.rs`. These are:

**Public functions (currently in `atom_edit.rs`):**
- `pub fn serialize_diff(diff: &AtomicStructure) -> String` (line 392)
- `pub fn parse_diff_text(text: &str) -> Result<AtomicStructure, String>` (line 457)

**Private helper functions:**
- `fn element_symbol(atomic_number: i16) -> String` (line 318)
- `fn normalize_element_symbol(s: &str) -> String` (line 327)
- `fn format_position(pos: &DVec3) -> String` (line 340)
- `fn format_float(value: f64) -> String` — used by `format_position`, defined near text format code
- `fn bond_order_name(order: u8) -> &'static str` (line 350)
- `fn parse_bond_order_name(name: &str) -> Option<u8>` (line 364)
- `fn resolve_element(symbol: &str) -> Option<i16>` (line 535)
- `fn parse_element_and_position(text: &str) -> Result<(String, DVec3), String>` (line 545)
- `fn parse_modification(text: &str) -> Result<(String, DVec3, Option<DVec3>), String>` (line 557)
- `fn parse_position(text: &str) -> Result<DVec3, String>` (line 586)
- `fn parse_bond_line(text: &str) -> Result<(usize, usize, u8), String>` (line 619)
- `fn parse_atom_pair(text: &str) -> Result<(usize, usize), String>` (line 633)

### New file: `text_format.rs`

```rust
//! Human-readable text format for atom_edit diffs.
//!
//! Format:
//! - `+El @ (x, y, z)` — atom addition
//! - `~El @ (x, y, z)` — atom replacement
//! - `~El @ (x, y, z) [from (ox, oy, oz)]` — atom move
//! - `- @ (x, y, z)` — atom delete marker
//! - `bond A-B order_name` — bond
//! - `unbond A-B` — bond delete marker

use glam::f64::DVec3;
use crate::crystolecule::atomic_structure::AtomicStructure;
// ... remaining imports from the moved code ...

// All moved functions go here, keeping the same visibility.
// serialize_diff and parse_diff_text remain `pub`.
// All helpers remain private (module-private, not pub).
```

### Changes to `atom_edit.rs`

- Remove the extracted functions (lines 313–647)
- Remove the `// Phase 10: Text Format Helpers` section header
- Add at the top of the file: `use super::text_format::{serialize_diff, parse_diff_text};`
- The `NodeData` impl's `get_text_properties()` and `set_text_properties()` call `serialize_diff` and `parse_diff_text` — these calls continue to work via the import

### Changes to `mod.rs`

```rust
#![allow(clippy::module_inception)]

pub mod atom_edit;
pub mod text_format;
```

### Changes to existing tests

In `rust/tests/structure_designer/atom_edit_text_format_test.rs`, update the import:

```rust
// Before:
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    parse_diff_text, serialize_diff,
};

// After:
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::text_format::{
    parse_diff_text, serialize_diff,
};
```

All 55 existing tests should pass without other changes.

## Extraction 2: Core Mutation Methods on `AtomEditData`

### The pattern

Each interaction function (delete, replace, transform) has this structure:

```
fn interaction(structure_designer: &mut StructureDesigner, ...) {
    // Phase 1: Gather data (immutable StructureDesigner borrows)
    let gathered_data = { ... eval_cache, result_structure, atom_edit_data ... };

    // Phase 2: Mutate (mutable AtomEditData borrow)
    let atom_edit_data = get_selected_atom_edit_data_mut(structure_designer);
    // ... mutation logic on atom_edit_data.diff and atom_edit_data.selection ...
}
```

Phase 2 only uses `&mut AtomEditData` plus the pre-gathered data. Extract it as a method on `AtomEditData`.

### New methods on `AtomEditData`

Add these methods to the existing `impl AtomEditData` block. They are the "phase 2" of each interaction function — pure data mutations, no `StructureDesigner` dependency.

#### `apply_delete_result_view`

```rust
/// Apply deletion in result view. Called by `delete_selected_in_result_view`
/// after gathering positions and provenance info from StructureDesigner.
///
/// - `base_atoms`: (base_id, position) — adds delete markers at these positions
/// - `diff_atoms`: (diff_id, is_pure_addition) — removes pure additions,
///   converts matched atoms to delete markers
/// - `bonds`: bond deletion info for adding bond delete markers
pub fn apply_delete_result_view(
    &mut self,
    base_atoms: &[(u32, DVec3)],
    diff_atoms: &[(u32, bool)],  // (diff_id, is_pure_addition)
    bonds: &[BondDeletionInfo],
)
```

Move the Phase 2 logic from `delete_selected_in_result_view` (lines ~1669–1708 of current file) into this method. The interaction function becomes:

```rust
fn delete_selected_in_result_view(sd: &mut StructureDesigner) {
    let (base, diff, bonds) = { /* Phase 1: gather from sd */ };
    let data = get_selected_atom_edit_data_mut(sd).unwrap();
    data.apply_delete_result_view(&base, &diff, &bonds);
}
```

#### `apply_delete_diff_view`

```rust
/// Apply deletion in diff view (reversal semantics). Called by
/// `delete_selected_in_diff_view` after gathering selected IDs.
///
/// - `diff_atoms`: (diff_id, DiffAtomKind) — action depends on kind
/// - `bonds`: bond references to remove from diff
pub fn apply_delete_diff_view(
    &mut self,
    diff_atoms: &[(u32, DiffAtomKind)],
    bonds: &[BondReference],
)
```

Move the Phase 2 logic from `delete_selected_in_diff_view` (lines ~1738–1756) into this method.

#### `apply_replace`

```rust
/// Apply element replacement to selected atoms.
///
/// - `atomic_number`: the new element
/// - `base_atoms`: (base_id, position) — adds to diff with new element
pub fn apply_replace(
    &mut self,
    atomic_number: i16,
    base_atoms: &[(u32, DVec3)],
)
```

Move the Phase 2 logic from `replace_selected_atoms` (lines ~1867–1894) into this method. The method handles both diff atom replacements (iterates `selection.selected_diff_atoms`, updates atomic_number in place) and base atom replacements (from the pre-gathered list).

#### `apply_transform`

```rust
/// Apply a relative transform to selected atoms.
///
/// - `relative`: the delta transform to apply
/// - `base_atoms`: (base_id, atomic_number, old_position) — adds to diff with anchor
pub fn apply_transform(
    &mut self,
    relative: &Transform,
    base_atoms: &[(u32, i16, DVec3)],
)
```

Move the Phase 2 logic from `transform_selected` (lines ~1953–1987) into this method. This includes:
- Transforming existing diff atoms (update position, keep anchor)
- Adding base atoms to diff with anchors
- Updating selection (move base→diff)
- Updating selection_transform algebraically

### What stays in the interaction functions

The interaction functions remain as thin wrappers in `atom_edit.rs`:
1. Check `output_diff` for diff-view vs result-view branching
2. Gather data from `StructureDesigner` (eval cache, result structure, provenance)
3. Call the appropriate `AtomEditData` method
4. These are NOT unit tested — they're glue code

### `DiffAtomKind` and `classify_diff_atom` visibility

`DiffAtomKind` enum and `classify_diff_atom` function need to be `pub(super)` (or at minimum accessible from tests). Move them to be associated with `AtomEditData` or make them `pub` since they're part of the data model:

```rust
/// Classification of a diff atom based on its properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffAtomKind {
    DeleteMarker,
    MatchedBase,
    PureAddition,
}

/// Classify a diff atom by inspecting the diff structure directly.
pub fn classify_diff_atom(diff: &AtomicStructure, diff_id: u32) -> DiffAtomKind
```

### `BondDeletionInfo` visibility

Currently a private struct. Needs to be `pub` for the extracted method signature:

```rust
#[derive(Debug, Clone)]
pub struct BondDeletionInfo {
    pub diff_id_a: Option<u32>,
    pub diff_id_b: Option<u32>,
    pub identity_a: Option<(i16, DVec3)>,
    pub identity_b: Option<(i16, DVec3)>,
}
```

### `convert_diff_atom_to_delete_marker`

Currently a free function taking `&mut AtomEditData`. Can become a method on `AtomEditData`:

```rust
impl AtomEditData {
    /// Convert a matched diff atom to a delete marker.
    pub fn convert_to_delete_marker(&mut self, diff_atom_id: u32) { ... }
}
```

## New Test File: `atom_edit_mutations_test.rs`

### File: `rust/tests/structure_designer/atom_edit_mutations_test.rs`

### Registration

Add to `rust/tests/structure_designer.rs`:

```rust
#[path = "structure_designer/atom_edit_mutations_test.rs"]
mod atom_edit_mutations_test;
```

### Imports

```rust
use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::{
    AtomicStructure, DELETED_SITE_ATOMIC_NUMBER,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::{
    BOND_DELETED, BOND_SINGLE,
};
use rust_lib_flutter_cad::crystolecule::atomic_structure::bond_reference::BondReference;
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::{
    AtomEditData, DiffAtomKind, classify_diff_atom, BondDeletionInfo,
};
use rust_lib_flutter_cad::util::transform::Transform;
```

### Test Cases

#### classify_diff_atom tests

```
test_classify_delete_marker
  - Create diff with atom at atomic_number=0 → DiffAtomKind::DeleteMarker

test_classify_matched_base
  - Create diff with atom + anchor → DiffAtomKind::MatchedBase

test_classify_pure_addition
  - Create diff with normal atom, no anchor → DiffAtomKind::PureAddition

test_classify_nonexistent_atom
  - Call with ID not in diff → DiffAtomKind::PureAddition (fallback)
```

#### apply_delete_diff_view tests

```
test_delete_diff_view_removes_delete_marker
  - Create AtomEditData with a delete marker in the diff
  - Add the delete marker's ID to selection.selected_diff_atoms
  - Call apply_delete_diff_view with [(id, DiffAtomKind::DeleteMarker)]
  - Assert: atom removed from diff, selection cleared

test_delete_diff_view_converts_matched_to_delete_marker
  - Create AtomEditData with a moved atom (has anchor)
  - Add to selection
  - Call apply_delete_diff_view with [(id, DiffAtomKind::MatchedBase)]
  - Assert: atom in diff is now a delete marker at the anchor position,
    original moved atom removed, selection cleared

test_delete_diff_view_removes_pure_addition
  - Create AtomEditData with a pure addition (no anchor)
  - Add to selection
  - Call apply_delete_diff_view with [(id, DiffAtomKind::PureAddition)]
  - Assert: atom removed from diff entirely, selection cleared

test_delete_diff_view_removes_bond_delete_marker
  - Create AtomEditData with two atoms and a bond with BOND_DELETED between them
  - Add bond to selection
  - Call apply_delete_diff_view with bonds=[BondReference{a,b}]
  - Assert: bond removed from diff, atoms still present, selection cleared

test_delete_diff_view_removes_normal_bond
  - Create AtomEditData with two atoms and a BOND_SINGLE between them
  - Call apply_delete_diff_view with bonds=[BondReference{a,b}]
  - Assert: bond removed from diff

test_delete_diff_view_mixed
  - Create diff with: one delete marker, one pure addition, one bond delete marker
  - Select all, call apply_delete_diff_view
  - Assert: delete marker removed, addition removed, bond removed, selection empty
```

#### apply_delete_result_view tests

```
test_delete_result_view_base_atom_creates_delete_marker
  - Create AtomEditData (empty diff)
  - Set selected_base_atoms = {42}
  - Call apply_delete_result_view(base_atoms=[(42, pos)], diff_atoms=[], bonds=[])
  - Assert: diff now contains a delete marker at pos, base_id removed from selection

test_delete_result_view_pure_addition_removes_from_diff
  - Create AtomEditData with an added atom (id=1)
  - Set selected_diff_atoms = {1}
  - Call apply_delete_result_view(base=[], diff=[(1, true)], bonds=[])
  - Assert: atom 1 removed from diff, diff_id removed from selection

test_delete_result_view_matched_atom_becomes_delete_marker
  - Create AtomEditData with a replacement atom (id=1, has anchor)
  - Set selected_diff_atoms = {1}
  - Call apply_delete_result_view(base=[], diff=[(1, false)], bonds=[])
  - Assert: atom 1 converted to delete marker in diff

test_delete_result_view_bond_adds_delete_marker
  - Create AtomEditData with two atoms (id=1, id=2, no bond)
  - Call apply_delete_result_view with bonds that have both diff_ids present
  - Assert: bond with BOND_DELETED added between them

test_delete_result_view_bond_creates_identity_entries
  - Create AtomEditData with empty diff
  - Call apply_delete_result_view with bonds where diff_id_a=None, identity_a=Some(...)
  - Assert: identity atom added to diff, then bond delete marker added
```

#### apply_replace tests

```
test_replace_diff_atoms
  - Create AtomEditData with a Carbon atom in diff
  - Add to selected_diff_atoms
  - Call apply_replace(14 [Silicon], base_atoms=[])
  - Assert: atom's atomic_number is now 14, selection unchanged

test_replace_base_atoms
  - Create AtomEditData (empty diff)
  - Set selected_base_atoms = {42}
  - Call apply_replace(14, base_atoms=[(42, pos)])
  - Assert: new atom in diff with atomic_number=14 at pos,
    base_id removed from selected_base_atoms,
    new diff_id added to selected_diff_atoms

test_replace_delete_marker_in_diff_view
  - Create AtomEditData with a delete marker, add to selected_diff_atoms
  - Call apply_replace(14, base_atoms=[])
  - Assert: atom's atomic_number changed from 0 to 14 (revives the atom as Silicon)
```

#### apply_transform tests

```
test_transform_diff_atoms
  - Create AtomEditData with an atom at (1,0,0) in diff
  - Set selected_diff_atoms, set selection_transform
  - Call apply_transform(relative=translate(1,0,0), base_atoms=[])
  - Assert: atom position is now (2,0,0), anchor unchanged

test_transform_base_atoms_creates_anchors
  - Create AtomEditData (empty diff)
  - Set selected_base_atoms = {42}
  - Call apply_transform(relative=translate(1,0,0), base_atoms=[(42, 6, (1,0,0))])
  - Assert: new atom in diff at (2,0,0) with anchor at (1,0,0),
    base_id removed from selected_base_atoms,
    new diff_id in selected_diff_atoms

test_transform_preserves_existing_anchor
  - Create AtomEditData with atom at (2,0,0) with anchor at (0,0,0)
  - Apply translate(1,0,0)
  - Assert: atom at (3,0,0), anchor still at (0,0,0) (original base position)

test_transform_updates_selection_transform
  - Set up selection_transform, call apply_transform
  - Assert: selection_transform updated algebraically

test_transform_clears_bond_selection
  - Set up bond selection, call apply_transform
  - Assert: selected_bonds is empty
```

#### convert_to_delete_marker tests

```
test_convert_normal_atom_to_delete_marker
  - Create diff with C at (1,0,0), no anchor
  - Call convert_to_delete_marker(id)
  - Assert: old atom removed, new delete marker at (1,0,0)

test_convert_moved_atom_to_delete_marker
  - Create diff with C at (2,0,0) with anchor at (1,0,0)
  - Call convert_to_delete_marker(id)
  - Assert: old atom removed, new delete marker at (1,0,0) — uses anchor position

test_convert_nonexistent_atom_is_noop
  - Call convert_to_delete_marker(999)
  - Assert: diff unchanged
```

## Implementation Notes

### Ordering

1. **Extract text format first** — pure file move, no logic changes, all 55 tests validate correctness immediately
2. **Add mutation methods** — add new `pub` methods to `impl AtomEditData`, initially as duplicates of the inline code
3. **Write mutation tests** — verify the new methods work correctly
4. **Refactor interaction functions** — replace inline mutation code with calls to the new methods
5. **Re-run all tests** — all 949+ existing tests must still pass

### Visibility constraints

The test crate can only access `pub` items. The following must be `pub`:
- `AtomEditData` — already `pub`
- `AtomEditSelection` — already `pub`
- `DiffAtomKind` — currently private, must become `pub`
- `BondDeletionInfo` — currently private, must become `pub`
- `classify_diff_atom` — currently private, must become `pub`
- `convert_to_delete_marker` — currently a free fn, becomes `pub` method
- `apply_delete_result_view`, `apply_delete_diff_view`, `apply_replace`, `apply_transform` — new `pub` methods

### Key imports for tests

The tests need:
```rust
use rust_lib_flutter_cad::structure_designer::nodes::atom_edit::atom_edit::*;
use rust_lib_flutter_cad::crystolecule::atomic_structure::*;
use rust_lib_flutter_cad::crystolecule::atomic_structure::inline_bond::*;
use rust_lib_flutter_cad::crystolecule::atomic_structure::bond_reference::BondReference;
use rust_lib_flutter_cad::util::transform::Transform;
```

### What NOT to change

- `NodeData` trait implementation stays in `atom_edit.rs`
- Interaction functions stay in `atom_edit.rs` (they're the `StructureDesigner` glue)
- `get_active_atom_edit_data` / `get_selected_atom_edit_data_mut` stay in `atom_edit.rs`
- `get_node_type()` stays in `atom_edit.rs`
- Selection helpers (`calc_transform_from_positions`, `apply_modifier_to_set`) stay in `atom_edit.rs`
- The eval cache helpers stay in `atom_edit.rs`
- No changes to `enrich_diff_with_base_bonds` or `apply_diff` (these are in other files)

### Expected line count after refactoring

| File | Before | After |
|------|--------|-------|
| `atom_edit.rs` | ~1970 | ~1200 |
| `text_format.rs` | 0 | ~320 |
| `atom_edit_mutations_test.rs` | 0 | ~400 |
| Total new test count | 55 | ~80 |
