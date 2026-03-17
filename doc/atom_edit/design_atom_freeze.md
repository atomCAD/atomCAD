# Atom Freeze Design

Per-atom freeze flag that prevents frozen atoms from moving during energy minimization.

## Motivation

Users working with complex structures often need to minimize only a subset of atoms while keeping others fixed. Today's options:

1. **Minimize diff** — freezes base atoms, only diff atoms move.
2. **Minimize selected** — only selected atoms move.
3. **Minimize all** — everything moves.

For elaborate workflows, repeatedly building the right selection before each minimization is tedious. A persistent **frozen** flag lets users mark atoms once, then freely run "minimize all" (renamed "minimize unfrozen") knowing those atoms won't move. This is especially useful for:

- Anchoring a rigid framework while relaxing a functional group.
- Freezing a substrate while minimizing an adsorbate.
- Iterative workflows where the frozen set is stable across many minimize operations.

## Design

### 1. Frozen Flag on Atom

Add a new bit flag to `Atom.flags`:

```rust
// atom.rs
const ATOM_FLAG_FROZEN: u16 = 1 << 2;

impl Atom {
    #[inline]
    pub fn is_frozen(&self) -> bool {
        (self.flags & ATOM_FLAG_FROZEN) != 0
    }

    #[inline]
    pub fn set_frozen(&mut self, frozen: bool) {
        if frozen {
            self.flags |= ATOM_FLAG_FROZEN;
        } else {
            self.flags &= !ATOM_FLAG_FROZEN;
        }
    }
}
```

Updated flags comment: `pub flags: u16, // Bit 0: selected, Bit 1: hydrogen passivation, Bit 2: frozen`

### 2. Frozen Tracking in atom_edit

The atom_edit node's base structure is immutable during editing. Like selection, frozen state is tracked by provenance in `AtomEditData`:

```rust
// atom_edit_data.rs (new fields)
pub frozen_base_atoms: HashSet<u32>,
pub frozen_diff_atoms: HashSet<u32>,
```

When building the result structure for evaluation or minimization, the frozen flag is applied to result atoms based on these sets.

#### Serialization

The frozen sets are serialized into the atom_edit node's state in `.cnnd` files, so the frozen configuration persists across save/load cycles. Format: arrays of atom IDs under `"frozen_base_atoms"` and `"frozen_diff_atoms"` keys in the node's JSON state.

### 3. Behavior per Minimize Mode

| Mode | Current behavior | New behavior with frozen flag |
|------|-----------------|-------------------------------|
| **Minimize all** (renamed) | All atoms free | Frozen atoms stay fixed; unfrozen atoms move |
| **Minimize diff** | Base frozen, diff free | Base frozen (as before) + frozen-flagged diff atoms also frozen |
| **Minimize selected** | Selected free, rest frozen | **No change** — frozen flag ignored (see rationale) |
| **Relax node** | All atoms free | Frozen atoms stay fixed; unfrozen atoms move |

#### Rationale: Minimize Selected Ignores Frozen

"Minimize selected" is an explicit, per-operation override where the user says "move exactly these atoms." Honoring the frozen flag here would create confusing behavior: the user selects atoms, clicks minimize, and some selected atoms don't move. If the user wants to exclude frozen atoms from a selection-based minimize, they can simply not select them.

#### Rationale: Minimize Diff Respects Frozen

"Minimize diff" already has a concept of constrained atoms (all base atoms are frozen). Respecting the frozen flag on diff atoms is a natural extension — it lets users freeze specific diff atoms (e.g., a placed anchor atom) while relaxing the rest of the diff.

### 4. Implementation: Frozen Index Computation

In `minimization.rs`, the frozen index computation for `FreeAll` mode changes from:

```rust
MinimizeFreezeMode::FreeAll => Vec::new(),
```

to:

```rust
MinimizeFreezeMode::FreeAll => {
    topology.atom_ids.iter().enumerate()
        .filter(|(_, result_id)| {
            result_structure.get_atom(**result_id)
                .map_or(false, |atom| atom.is_frozen())
        })
        .map(|(i, _)| i)
        .collect()
}
```

For `FreezeBase`, frozen diff atoms are added to the existing base-frozen set:

```rust
MinimizeFreezeMode::FreezeBase => {
    topology.atom_ids.iter().enumerate()
        .filter(|(_, result_id)| {
            let is_base = matches!(
                eval_cache.provenance.sources.get(result_id),
                Some(AtomSource::BasePassthrough(_))
            );
            let is_frozen = result_structure.get_atom(**result_id)
                .map_or(false, |atom| atom.is_frozen());
            is_base || is_frozen
        })
        .map(|(i, _)| i)
        .collect()
}
```

`FreeSelected` remains unchanged (ignores frozen flag).

### 5. Relax Node

`minimize_energy()` in `simulation/mod.rs` gains frozen support by reading atom flags:

```rust
pub fn minimize_energy(
    structure: &mut AtomicStructure,
    vdw_mode: VdwMode,
) -> Result<MinimizationResult, String> {
    let topology = MolecularTopology::from_structure(structure, vdw_mode)?;

    // Collect frozen indices from atom flags
    let frozen: Vec<usize> = topology.atom_ids.iter().enumerate()
        .filter(|(_, &atom_id)| {
            structure.get_atom(atom_id)
                .map_or(false, |atom| atom.is_frozen())
        })
        .map(|(i, _)| i)
        .collect();

    // ... existing minimization code, passing `frozen` to minimize_with_force_field
}
```

This means any node that produces atoms with the frozen flag set will automatically have those atoms frozen during relax evaluation. No changes needed to the relax node itself.

### 6. UI: Buttons in Energy Minimization Section

Four new buttons added to the Energy Minimization collapsible section in the atom_edit panel, placed **above** the existing minimize buttons:

```
┌─ Energy Minimization ─────────────────────────────────────┐
│                                                             │
│  ┌────────────┐ ┌──────────────┐ ┌────────────┐ ┌───────┐ │
│  │ Selection →│ │ Selection → │ │ Frozen →   │ │ Clear │ │
│  │ Frozen     │ │ Unfrozen    │ │ Selection  │ │Frozen │ │
│  └────────────┘ └──────────────┘ └────────────┘ └───────┘ │
│                                                             │
│  ┌──────────┐ ┌──────────────┐ ┌──────────────────┐       │
│  │ Minimize │ │ Minimize     │ │ Minimize         │       │
│  │ diff     │ │ unfrozen     │ │ selected         │       │
│  └──────────┘ └──────────────┘ └──────────────────┘       │
│                                                             │
│  Status: Converged in 42 iterations (0.3 kcal/mol)         │
└─────────────────────────────────────────────────────────────┘
```

#### Button Definitions

| Button | Enabled when | Action |
|--------|-------------|--------|
| **Selection → Frozen** | Has selected atoms | Sets the frozen flag on all currently selected atoms (additive — does not clear existing frozen atoms) |
| **Selection → Unfrozen** | Has selected atoms | Clears the frozen flag on all currently selected atoms |
| **Frozen → Selection** | Has frozen atoms | Replaces current selection with the set of frozen atoms |
| **Clear Frozen** | Has frozen atoms | Removes the frozen flag from all atoms |

#### Button Rename

"Minimize all" is renamed to **"Minimize unfrozen"** to clearly communicate that frozen atoms are excluded.

### 7. API Functions

New functions in `atom_edit_api.rs`:

```rust
/// Sets the frozen flag on all currently selected atoms (additive).
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_selection_to_frozen() { ... }

/// Clears the frozen flag on all currently selected atoms.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_selection_to_unfrozen() { ... }

/// Replaces the current selection with the set of frozen atoms.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_frozen_to_selection() { ... }

/// Clears the frozen flag from all atoms.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_clear_frozen() { ... }

/// Returns true if any atom has the frozen flag set.
#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_has_frozen_atoms() -> bool { ... }
```

The existing `atom_edit_minimize` API and `APIMinimizeFreezeMode` enum remain unchanged. The frozen flag is respected internally during minimization without needing a new enum variant.

### 8. Staged Data

`APIAtomEditData` gains a new field:

```rust
pub has_frozen_atoms: bool,
```

This enables the Flutter UI to show/hide or enable/disable the frozen-related buttons without an extra API call.

### 9. Hover Tooltip

The atom hover tooltip (`query_hovered_atom_info`) displays info when the user hovers over an atom. Add frozen status to the displayed data.

**Rust API type** (`structure_designer_api_types.rs`): Add field to `APIHoveredAtomInfo`:

```rust
pub is_frozen: bool,
```

**Rust API function** (`structure_designer_api.rs`): Populate from atom flag in `query_hovered_atom_info()`:

```rust
is_frozen: atom.is_frozen(),
```

**Flutter widget** (`lib/common/atom_tooltip.dart`): Add a line to `AtomTooltip` when frozen is true. Display as a short label (e.g., "Frozen" in a distinct color like light blue or amber) after the bond count line. Only shown when `is_frozen == true` — non-frozen atoms show no extra line.

### 10. FRB Codegen

After adding the new API functions and modifying `APIAtomEditData`, run:

```bash
flutter_rust_bridge_codegen generate
```

## Implementation Steps

1. **atom.rs** — Add `ATOM_FLAG_FROZEN` constant and `is_frozen()` / `set_frozen()` methods.
2. **atom_edit_data.rs** — Add `frozen_base_atoms` and `frozen_diff_atoms` hash sets. Initialize as empty. Wire into serialization.
3. **atom_edit result building** — When constructing the result structure, apply frozen flags from the tracking sets.
4. **minimization.rs** — Update `FreeAll` and `FreezeBase` frozen index computation to respect atom frozen flag.
5. **simulation/mod.rs** — Update `minimize_energy()` to read frozen flags from the structure.
6. **atom_edit_api.rs** — Add `selection_to_frozen`, `selection_to_unfrozen`, `frozen_to_selection`, `clear_frozen`, `has_frozen_atoms` API functions.
7. **structure_designer_api_types.rs** — Add `has_frozen_atoms` to `APIAtomEditData`. Add `is_frozen` to `APIHoveredAtomInfo`.
8. **structure_designer_api.rs** — Populate `is_frozen` in `query_hovered_atom_info()`.
9. **atom_tooltip.dart** — Show "Frozen" label when `is_frozen` is true.
10. **FRB codegen** — Regenerate bindings.
11. **atom_edit_editor.dart** — Add the four freeze buttons to the Energy Minimization section. Rename "Minimize all" to "Minimize unfrozen".
12. **Tests** — Add tests for frozen flag behavior in minimization (frozen atoms don't move, unfrozen atoms do).

## Non-Goals

- **Distinct rendering for frozen atoms**: Frozen atoms have no distinct color or icon in the 3D viewport. Users check frozen state via "Frozen → Selection" or by hovering (tooltip shows "Frozen"). A dedicated visual style could be added later if needed.
- **Keyboard shortcut for freeze**: Not planned for initial implementation.
- **Frozen bonds**: Only atoms are frozen. Bond constraints are not in scope.
- **edit_atom node support**: The legacy destructive edit_atom node does not get freeze support. Only the non-destructive atom_edit node is in scope.
