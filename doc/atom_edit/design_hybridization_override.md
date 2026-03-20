# Design: Per-Atom Hybridization Override for UFF Minimization

## Problem

When a user builds a molecule with nitrogen in sp3 (tetrahedral) geometry, then corrects the bonding to sp2 (planar), running UFF minimization reverts the atom to sp3 because the UFF typer infers hybridization purely from bond orders. If the bonds are all single bonds (e.g., three single bonds on N), the typer returns `N_3` (sp3) regardless of the user's intended geometry.

**Concrete example:** Building caffeine. The user initially places N atoms as sp3 (wrong), then manually flattens them to sp2 (correct). Running "minimize" reassigns them as `N_3` and pushes them back to tetrahedral geometry.

## Design Principle

The UFF typer is ported from RDKit and extensively tested — we do not modify its heuristic logic. Instead, we allow the user to **override the hybridization** on individual atoms. The philosophy: when the user explicitly sets hybridization to something other than `Auto`, they know what they are doing, and that override should be respected by all downstream consumers (guided placement, hydrogen passivation, **and UFF minimization**).

## Current State

| Subsystem | Uses hybridization? | Respects user override? |
|-----------|---------------------|------------------------|
| Guided placement | Yes — `detect_hybridization()` | Yes — via `hybridization_override` parameter passed from Flutter |
| Hydrogen passivation | Yes — `detect_hybridization()` | No — always passes `None` |
| UFF typer | Yes — `assign_uff_type()` | **No** — purely bond-order-based |

The `APIHybridization` enum (`Auto`, `Sp3`, `Sp2`, `Sp1`) exists in the Flutter UI, but the chosen value is **transient** — it is never stored on the atom. It only affects the current guided placement session.

`Atom.flags` is a `u16` with bits 0-2 used (selected, hydrogen_passivation, frozen). Bits 3-4 are available and can encode the hybridization override (2 bits, 4 values: Auto/Sp3/Sp2/Sp1) — allowing the override to flow with the atom through the node network.

## Design

### 1. Storage: dual representation (Atom.flags + AtomEditData maps)

Hybridization override has two storage layers:

1. **Runtime: `Atom.flags` bits 3-4** — the override travels with the atom through the node network. Any node that receives an `AtomicStructure` (including the relax node) can read the override directly from each atom's flags. This is how the frozen flag already works (bit 2).
2. **Persistence: `AtomEditData`-level maps** — `SerializableAtom` is deliberately minimal `(id, atomic_number, position)` and does not include flags. For `.cnnd` serialization, overrides are stored as separate lists in `SerializableAtomEditData`, following the same pattern as frozen atoms.

The maps are the **source of truth within atom_edit**. During evaluation (applying diff to base), the maps are consulted to set `Atom.flags` hybridization bits on the **result** atoms. The result `AtomicStructure` then flows downstream carrying the bits.

#### Runtime: Atom.flags

`Atom.flags` is a `u16` with bits 0-2 currently used (selected, hydrogen_passivation, frozen). Bits 3-4 encode the hybridization override:

```rust
// In atom.rs
const ATOM_FLAG_HYBRIDIZATION_MASK: u16 = 0b11 << 3;
const ATOM_FLAG_HYBRIDIZATION_SHIFT: u16 = 3;

impl Atom {
    /// Returns the hybridization override (0=Auto, 1=Sp3, 2=Sp2, 3=Sp1).
    #[inline]
    pub fn hybridization_override(&self) -> u8 {
        ((self.flags & ATOM_FLAG_HYBRIDIZATION_MASK) >> ATOM_FLAG_HYBRIDIZATION_SHIFT) as u8
    }

    #[inline]
    pub fn set_hybridization_override(&mut self, hybridization: u8) {
        self.flags = (self.flags & !ATOM_FLAG_HYBRIDIZATION_MASK)
            | (((hybridization as u16) & 0b11) << ATOM_FLAG_HYBRIDIZATION_SHIFT);
    }
}
```

Atoms created by nodes that don't set hybridization (lattice fill, import, etc.) have bits 3-4 = 0 (Auto), preserving existing behavior.

#### Persistence: AtomEditData maps

The existing codebase stores per-atom editing metadata at the `AtomEditData` level for persistence. The frozen flag is the precedent:

- `frozen_base_atoms: HashSet<u32>` — base atom IDs that are frozen
- `frozen_diff_atoms: HashSet<u32>` — diff atom IDs that are frozen

**We follow the same pattern for hybridization overrides**, with one difference: frozen is boolean (present/absent), while hybridization needs a value (sp1/sp2/sp3). So we use `HashMap<u32, u8>` instead of `HashSet<u32>`:

```rust
// In AtomEditData (atom_edit_data.rs)
/// Hybridization overrides for base atoms. Key: base atom ID, Value: 1=Sp3, 2=Sp2, 3=Sp1.
/// Only non-Auto overrides are stored.
pub hybridization_override_base_atoms: HashMap<u32, u8>,
/// Hybridization overrides for diff atoms. Key: diff atom ID, Value: 1=Sp3, 2=Sp2, 3=Sp1.
pub hybridization_override_diff_atoms: HashMap<u32, u8>,
```

**Why base atoms need maps:** Base atoms come from the upstream input — they aren't in the diff `AtomicStructure`, so their overrides can only be stored separately. Diff atoms could theoretically store the override on `Atom.flags` directly in the diff structure, but using maps for both base and diff keeps the persistence layer uniform.

#### Populating Atom.flags during evaluation

During atom_edit evaluation (applying diff to base to produce the result `AtomicStructure`), the evaluator sets hybridization bits on result atoms by consulting the maps. This follows the same pattern as frozen flag propagation — iterate over the override maps (not over all result atoms), and use `base_to_result` / `diff_to_result` provenance maps to find the corresponding result atom ID:

```rust
// In the evaluation path, after building result atoms (same pattern as frozen flags):
for (&base_id, &hyb) in &self.hybridization_override_base_atoms {
    if let Some(&result_id) = diff_result.provenance.base_to_result.get(&base_id) {
        result.set_atom_hybridization_override(result_id, hyb);
    }
}
for (&diff_id, &hyb) in &self.hybridization_override_diff_atoms {
    if let Some(&result_id) = diff_result.provenance.diff_to_result.get(&diff_id) {
        result.set_atom_hybridization_override(result_id, hyb);
    }
}
```

After this, the result `AtomicStructure` carries the overrides on the atoms themselves, and any downstream node can read them.

**Constants** (in `atom.rs` alongside the flag definitions):

```rust
pub const HYBRIDIZATION_AUTO: u8 = 0;
pub const HYBRIDIZATION_SP3: u8 = 1;
pub const HYBRIDIZATION_SP2: u8 = 2;
pub const HYBRIDIZATION_SP1: u8 = 3;
```

### 2. Serialization

Following the frozen pattern exactly. Serializable entry type:

```rust
#[derive(Serialize, Deserialize)]
pub struct HybridizationOverrideEntry {
    pub atom_id: u32,
    /// 1=Sp3, 2=Sp2, 3=Sp1
    pub hybridization: u8,
}
```

Added to `SerializableAtomEditData`:

```rust
pub struct SerializableAtomEditData {
    // ... existing fields ...

    /// Per-atom hybridization overrides for base atoms.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hybridization_override_base_atoms: Vec<HybridizationOverrideEntry>,
    /// Per-atom hybridization overrides for diff atoms.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hybridization_override_diff_atoms: Vec<HybridizationOverrideEntry>,
}
```

**Backward compatibility:** `#[serde(default)]` means old `.cnnd` files without these fields deserialize as empty vectors (all Auto). Files where no overrides are set produce identical JSON to today via `skip_serializing_if`.

### 3. Propagate overrides to `MolecularTopology`

Add a new field to `MolecularTopology`:

```rust
pub struct MolecularTopology {
    // ... existing fields ...
    /// Per-atom hybridization override (0=Auto, 1=Sp3, 2=Sp2, 3=Sp1).
    /// Indexed by topology index, same as `atomic_numbers`.
    pub hybridization_overrides: Vec<u8>,
}
```

Since the override lives on `Atom.flags`, `MolecularTopology::from_structure()` and `from_structure_bonded_only()` read it automatically when building the topology:

```rust
// In from_structure() / from_structure_bonded_only(), alongside existing extraction of
// atomic_numbers and positions:
let hybridization_overrides: Vec<u8> = atoms.iter()
    .map(|a| a.hybridization_override())
    .collect();
```

No manual provenance resolution is needed. Any `AtomicStructure` — whether it came from an atom_edit node, a relax node, or anywhere else — carries the overrides on its atoms, and the topology builder extracts them uniformly.

**Relax node path:** If the relax node receives its input from an atom_edit node that set hybridization overrides, those overrides are already on the atoms' flags. `from_structure()` extracts them into `topology.hybridization_overrides`, and UFF type assignment respects them. If the input has no overrides (e.g., from a lattice fill node), all flags are 0 (Auto) and behavior is unchanged.

### 4. Override UFF type assignment

Add a new function to `typer.rs`:

```rust
/// Assigns a UFF atom type, optionally overriding the hybridization.
///
/// If `forced_hybridization` is non-zero (1=Sp3, 2=Sp2, 3=Sp1), the type
/// is chosen to match that hybridization regardless of bond orders.
/// Falls back to the standard `assign_uff_type()` if the override is 0 (Auto)
/// or if the element has no alternative types for the requested hybridization.
pub fn assign_uff_type_with_override(
    atomic_number: i16,
    bonds: &[InlineBond],
    forced_hybridization: u8,
) -> Result<&'static str, String> {
    if forced_hybridization == 0 {
        return assign_uff_type(atomic_number, bonds);
    }
    // Try to resolve a type matching the forced hybridization.
    // Only C, N, O, S, B have multiple hybridization variants.
    match resolve_forced_type(atomic_number, bonds, forced_hybridization) {
        Some(label) => Ok(label),
        None => assign_uff_type(atomic_number, bonds), // fallback
    }
}
```

The `resolve_forced_type` helper maps `(element, forced_hybridization)` to the correct UFF label:

| Element | Sp1 | Sp2 | Sp3 |
|---------|-----|-----|-----|
| C (6)   | `C_1` | `C_2` | `C_3` |
| N (7)   | `N_1` | `N_2` | `N_3` |
| O (8)   | `O_1` | `O_2` | `O_3` |
| S (16)  | — | `S_2` | `S_3+N` (use bond-count charge) |
| B (5)   | — | `B_2` | `B_3` |

For elements with no hybridization variants (H, halogens, metals, etc.), the override is silently ignored and the standard assignment is used.

**Note on aromatic types:** `C_R`, `N_R`, `O_R`, `S_R` are aromatic/resonance types. Sp2 override maps to `_2` (not `_R`), because the user is asserting trigonal planar geometry, not aromaticity. The `_2` and `_R` types share the same hybridization digit (2) but may have different equilibrium angles in the parameter table — `_2` is the correct choice for a user-specified sp2 override.

### 5. Update `assign_uff_types` (batch)

```rust
pub fn assign_uff_types_with_overrides(
    atomic_numbers: &[i16],
    bond_lists: &[&[InlineBond]],
    hybridization_overrides: &[u8],
) -> Result<AtomTypeAssignment, String> {
    // Same as assign_uff_types but calls assign_uff_type_with_override
    // using hybridization_overrides[i] for each atom.
}
```

### 6. Update `UffForceField::from_topology_with_frozen`

Change step 2 from:

```rust
let typing = assign_uff_types(&topology.atomic_numbers, &bond_slices)?;
```

to:

```rust
let typing = assign_uff_types_with_overrides(
    &topology.atomic_numbers,
    &bond_slices,
    &topology.hybridization_overrides,
)?;
```

This is the only change needed in the force field construction. All downstream parameter computation (bond stretch, angles, torsions, inversions, vdW) automatically uses the overridden types because they consume the `typing` result.

### 7. Guided placement: when overrides are set

The Add Atom tool has a hybridization selector on the toolbar. Currently this is transient — it affects guide dot geometry but is never persisted. With this feature, the toolbar hybridization also **writes a stored override** on the anchor atom at placement time.

**Behavior by placement mode:**

#### Placing into the void (free placement)

When the user clicks empty space, a new atom is placed at the ray-plane intersection. If the toolbar hybridization is not Auto, the **newly created atom** gets that hybridization override stored. This is forward-looking: the override has no effect yet (a bare atom has no angles), but when the user later bonds to it, guided placement and minimization will use the stored hybridization.

#### Guided placement on an existing atom

When the user clicks an existing atom (the anchor), guide dots appear based on the toolbar hybridization. When the user then **clicks a guide dot to place the new atom**, two things happen atomically (as one undoable action):

1. The **new atom** is created and bonded to the anchor. The new atom does **not** get a hybridization override (it's a terminal atom; its hybridization will be inferred from future bonds).
2. The **anchor atom** gets the toolbar hybridization stored as its override (unless the toolbar is set to Auto, in which case any existing override on the anchor is left unchanged).

**Why set on the anchor, not the new atom?** The anchor is the atom whose geometry the user is controlling. The guide dots show where the new atom will go *relative to the anchor's hybridization*. The new atom is terminal (one bond) and its hybridization is meaningless until more bonds are added.

**Why at placement time, not at first click?** The first click is exploratory — the user sees guide dots and may change their mind (Escape, click elsewhere). Setting the override only when the atom is actually placed makes the operation atomic and undoable as a single action. If the user clicks the anchor with sp2 selected but then Escapes, no override is written.

**Reset to Auto after placement:** After a guided placement sets the anchor's hybridization override, the toolbar resets to Auto. This prevents the user from accidentally overriding the hybridization of the next atom they click on. (The stored override on the anchor atom is unaffected — only the transient toolbar state resets.) Note: this reset already exists in a different form — the model resets `_hybridizationOverride` to Auto on tool changes (line 872 of `structure_designer_model.dart`). The new behavior extends this to also reset after each guided placement action.

**Priority of hybridization sources during guide dot computation:**

1. Toolbar hybridization (the transient UI selection) — always used for guide dot display, even if the anchor has a stored override. This lets the user "try out" different hybridizations before committing.
2. On placement: the toolbar value is written as the anchor's stored override.

#### Hydrogen passivation

The `hydrogen_passivation.rs` caller currently passes `None` to `detect_hybridization()`. It should read the atom's hybridization override from `Atom.flags` (via `atom.hybridization_override()`) and pass it through. Since the override is on the atom itself, this works regardless of calling context — no `AtomEditData` lookup is needed.

### 8. API layer

Add an API function to set hybridization override on selected atoms:

```rust
pub fn atom_edit_set_hybridization_override(hybridization: APIHybridization) {
    // Reads the current selection from AtomEditData.
    // For each selected base atom: inserts/removes from hybridization_override_base_atoms.
    // For each selected diff atom: inserts/removes from hybridization_override_diff_atoms.
    // Auto removes the entry (restoring default behavior).
    // Wraps with with_atom_edit_undo for undo support.
}
```

The existing `APIHybridization` enum (`Auto`, `Sp3`, `Sp2`, `Sp1`) maps directly.

### 9. Undo support

Following the pattern of `AtomEditFrozenChangeCommand`, create an `AtomEditHybridizationChangeCommand`. The frozen command uses `FrozenDelta` with `added`/`removed` vectors discriminated by `FrozenProvenance`. For hybridization, the same provenance discrimination is needed, but entries also carry a value (the hybridization level). The delta tracks insertions, removals, and value changes separately:

```rust
/// Whether a hybridization override atom ID refers to a base atom or a diff atom.
/// Reuses the same provenance concept as FrozenProvenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridizationProvenance {
    Base,
    Diff,
}

/// Delta representing changes to the hybridization override maps.
#[derive(Debug, Clone)]
pub struct HybridizationDelta {
    /// (provenance, atom_id, new_value) — entries added to the override maps.
    /// On undo: remove these entries. On redo: insert them.
    pub added: Vec<(HybridizationProvenance, u32, u8)>,
    /// (provenance, atom_id, old_value) — entries removed from the override maps.
    /// On undo: re-insert with old_value. On redo: remove them.
    pub removed: Vec<(HybridizationProvenance, u32, u8)>,
    /// (provenance, atom_id, old_value, new_value) — entries whose value changed.
    /// On undo: restore old_value. On redo: apply new_value.
    pub changed: Vec<(HybridizationProvenance, u32, u8, u8)>,
}

pub struct AtomEditHybridizationChangeCommand {
    pub description: String,
    pub network_name: String,
    pub node_id: u64,
    pub delta: HybridizationDelta,
}
```

On undo, `added` entries are removed, `removed` entries are re-inserted with their old values, and `changed` entries are restored to old values. On redo, the inverse. This mirrors the `FrozenDelta` structure but extends it with value tracking for the non-boolean case.

### 10. Flutter UI

#### Shared `HybridizationSelector` widget

The SegmentedButton (compact density, font size 12, `Auto | sp3 | sp2 | sp1` segments) is used by both the Add Atom tool and the Default tool. Extract it into a reusable widget:

```dart
class HybridizationSelector extends StatelessWidget {
  /// Currently selected value(s). Empty set = mixed/indeterminate state.
  final Set<APIHybridization> selected;
  /// Called when the user clicks a segment.
  final ValueChanged<APIHybridization> onChanged;
  /// Whether empty selection is allowed (true for Default tool's mixed state).
  final bool emptySelectionAllowed;
}
```

Place it in `lib/structure_designer/node_data/` alongside the atom_edit editor, or in `lib/common/` if other editors may reuse it in the future.

#### Hybridization selector in the Add Atom tool (existing, refactored)

Replace the inline SegmentedButton with `HybridizationSelector(emptySelectionAllowed: false, ...)`. Behavior is unchanged: controls guide dot geometry. The extension: it now also **writes a stored override on the anchor atom at placement time** (see section 7).

#### Hybridization selector in the Default tool (new)

The hybridization selector is a `SegmentedButton` with four text segments: **Auto | sp3 | sp2 | sp1** (same widget as in the Add Atom tool, defined in `atom_edit_editor.dart`). When the Default tool is active and atoms are selected, this SegmentedButton appears in the toolbar. Its selected segment reflects the selected atoms' stored overrides:

- **All selected atoms have the same override** (including all Auto): that segment is selected (highlighted).
- **Selected atoms have differing overrides**: **no segment is selected** (empty selection). The SegmentedButton supports this via `emptySelectionAllowed: true` — all four segments appear in their unselected/outline state. This is visually distinct from Auto (which highlights the Auto segment) and clearly communicates "these atoms don't agree."

When the user clicks a segment:
- The chosen hybridization is applied to **all selected atoms** via `atom_edit_set_hybridization_override`.
- Clicking `Auto` removes the override from all selected atoms (restoring bond-based inference).
- This is a single undoable action.

When no atoms are selected, the hybridization SegmentedButton is hidden (or disabled/greyed out).

#### Atom info hover popup

The existing atom info popup (shown on hover) should display the hybridization override when it is non-Auto. For example:

```
N (7)  •  3 bonds
Hybridization: sp2 (override)
```

When the atom has Auto (no override), the popup can optionally show the inferred hybridization:

```
N (7)  •  3 bonds
Hybridization: sp3 (auto)
```

This lets the user see at a glance whether an atom has an override and what the effective hybridization is, which is especially useful for diagnosing unexpected minimization results.

### 11. Visual indicator (optional, out of scope)

Atoms with a non-Auto hybridization override could have a subtle visual indicator (e.g., a small badge, colored ring, or text label) so the user can see overrides without hovering. This is a UI-only concern and can be implemented via the existing atom decorator / rendering pipeline. Deferred to a follow-up if users find the hover popup insufficient.

## Data Flow Summary

```
Default tool: user selects atoms, changes hybridization dropdown
  → atom_edit_set_hybridization_override() called
  → AtomEditData.hybridization_override_{base,diff}_atoms updated (persistence)
  → On next evaluation: result Atom.flags bits 3-4 set from maps
  → Persisted in .cnnd as separate lists (like frozen)

Add Atom tool: user clicks anchor, clicks guide dot to place atom
  → New atom created and bonded to anchor
  → Anchor atom gets toolbar hybridization stored as override (map + flags)
  → Both operations captured as one undoable action

Add Atom tool: user clicks void to place bare atom
  → New atom created with toolbar hybridization as override (map + flags)

Evaluation (atom_edit node):
  AtomEditData maps + provenance
    → Result AtomicStructure atoms get hybridization bits set on Atom.flags
    → Result flows downstream carrying overrides on the atoms themselves

Minimization (any path — atom_edit or relax node):
  AtomicStructure received as input
    → MolecularTopology::from_structure() reads Atom.flags → hybridization_overrides
    → assign_uff_types_with_overrides() uses forced types where set
    → UffForceField built with overridden types
    → Minimizer uses correct equilibrium angles/torsions
  (If input atoms have no overrides, all flags are 0 → Auto → existing behavior)

Guided placement (guide dot computation):
  Toolbar hybridization (transient) always used for guide dot display
    → Lets user preview different hybridizations before committing

Hydrogen passivation:
  Reads atom's hybridization override from Atom.flags
    → Passes as hybridization_override parameter to detect_hybridization()

Hover popup:
  Reads atom's hybridization override from Atom.flags
    → Displays "sp2 (override)" or "sp3 (auto)" etc.
```

## Implementation Phases

### Phase A: Storage and serialization
1. Add `Atom.flags` bits 3-4: `hybridization_override()` / `set_hybridization_override()` methods and flag constants in `atom.rs`
2. Add `hybridization_override_base_atoms: HashMap<u32, u8>` and `hybridization_override_diff_atoms: HashMap<u32, u8>` to `AtomEditData`
3. Add `HybridizationOverrideEntry` and fields to `SerializableAtomEditData`
4. Update serialization/deserialization functions (maps ↔ serializable entries)
5. Add `hybridization_overrides: Vec<u8>` to `MolecularTopology`; populate from `Atom.flags` in `from_structure()` and `from_structure_bonded_only()`
6. Update atom_edit evaluation to set `Atom.flags` hybridization bits on result atoms from the AtomEditData maps via provenance
7. Update `from_deserialized()` to accept the new maps

### Phase B: UFF integration
1. Add `assign_uff_type_with_override` and `resolve_forced_type` to `typer.rs`
2. Add `assign_uff_types_with_overrides` to `typer.rs`
3. Update `UffForceField::from_topology_with_frozen` to use overrides from topology
4. Tests: verify that N with 3 single bonds + sp2 override gets `N_2` type and planar equilibrium
5. Integration test: atom_edit with sp2 override → relax node → verify override is respected

### Phase C: Guided placement integration
1. Update `add_atom_tool.rs` placement action: when placing via guide dot, store toolbar hybridization as override on the anchor atom (AtomEditData map; flags set on next evaluation)
2. Update `add_atom_tool.rs` void placement: store toolbar hybridization as override on the new atom
3. Both mutations captured within the existing undo action (placement already uses `with_atom_edit_undo`)
4. Update hydrogen passivation to read override from `Atom.flags` instead of passing `None`
5. Tests: verify anchor gets override on guided placement; verify passivation respects overrides

### Phase D: API and undo
1. Add `AtomEditHybridizationChangeCommand` (following `AtomEditFrozenChangeCommand` pattern)
2. Add `atom_edit_set_hybridization_override` API function with undo wrapping
3. Add `atom_edit_get_selected_hybridization` API function (returns the common override of selected atoms, or a "mixed" sentinel if they differ)
4. Tests: undo/redo of hybridization override changes

### Phase E: Flutter UI
1. Add hybridization dropdown to Default tool toolbar, reflecting selected atoms' overrides
2. Mixed/indeterminate state ("—") when selected atoms disagree
3. Dropdown change calls `atom_edit_set_hybridization_override` for all selected atoms
4. Update atom info hover popup to show hybridization with "(override)" or "(auto)" label
5. Optional: visual indicator for overridden atoms (deferred if popup is sufficient)

## Testing Strategy

- **Unit tests (`uff_typer_test.rs`):** `assign_uff_type_with_override` returns correct types for all (element, override) combinations. Verify fallback to standard assignment for elements without variants.
- **Integration test (atom_edit):** Build a 3-single-bond nitrogen atom, set sp2 override, minimize within atom_edit, verify the atom stays planar (angle ~120°).
- **Integration test (cross-node):** atom_edit with sp2 override → feed output to relax node → verify the override is on the atoms' flags and UFF respects it.
- **Serialization roundtrip:** `.cnnd` save/load preserves hybridization overrides. Old files without the field load as Auto.
- **Undo test:** Set override → undo → verify Auto restored. Redo → verify override restored.
- **Guided placement test:** Place atom via guide dot with sp2 on toolbar → verify anchor atom gets sp2 override stored. Escape without placing → verify no override written.
- **Void placement test:** Place atom into void with sp2 on toolbar → verify new atom gets sp2 override.
- **Hydrogen passivation test:** Stored sp2 override limits hydrogen count correctly.
- **Default tool selector test:** Select atoms with same override → dropdown shows that value. Select atoms with different overrides → dropdown shows mixed state. Change dropdown → all selected atoms updated.

## Backward Compatibility

- **Existing `.cnnd` files:** New fields default to empty via `#[serde(default)]`. No migration needed.
- **Existing API calls:** All existing functions continue to work unchanged. The new `assign_uff_types_with_overrides` is called only from force field construction; the old `assign_uff_types` remains available.
- **Existing behavior:** With all override maps empty (the default) and `Atom.flags` bits 3-4 = 0, the system behaves identically to today.
- **Relax node:** If the relax node's input comes from an atom_edit node with overrides, those overrides flow through on `Atom.flags` and are respected. If the input has no overrides (the common case today), `hybridization_overrides` is all-zeros and behavior is unchanged.
- **Other nodes:** Nodes that create atoms (lattice fill, import, etc.) produce atoms with `Atom.flags` bits 3-4 = 0 (Auto) by default. No changes needed in those nodes.
