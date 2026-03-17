# atom_edit Energy Minimization Integration Plan

## 1. Overview

Wire the UFF energy minimizer into the `atom_edit` node so users can minimize atomic structures directly from the editing interface. This involves:

1. A new Rust function that minimizes an `atom_edit` diff with frozen atom support
2. A new API endpoint exposed via Flutter Rust Bridge
3. Flutter UI additions (minimize button with freeze mode) in the atom_edit property panel

**Depends on**: Phase 20 (vdW) should be complete first so the minimizer produces physically meaningful results.

**Implementation phases**: Two phases, split at the FRB codegen boundary.
- **Phase A** (Rust): Steps 1–3, validate with `cargo build`
- **Phase B** (Flutter): Steps 4–6, validate with `flutter_rust_bridge_codegen generate` + `flutter analyze`

Each phase is small enough for one AI session (~100 lines Rust, ~60 lines Dart).

---

## 2. Freeze Modes

Two modes, matching the plan in `doc/energy_minimization_plan.md` Section 5:

### Mode 1: Freeze Base Atoms

Only atoms in the diff (user-edited/added) are free to move. All base atoms (from the input structure) are frozen at their original positions. This is the safe default — the user's edits get refined without disturbing the surrounding structure.

**Use case**: User adds a few atoms or adjusts positions, then minimizes to get correct bond lengths/angles while keeping the base structure intact.

### Mode 2: Free All

All atoms (base + diff) are free to move. The entire neighborhood relaxes together.

**Use case**: User wants the whole structure to find its energy minimum after an edit. More physically realistic but moves more atoms.

---

## 3. The Anchor Problem

When base atoms move (Mode 2), they must be added to the diff with anchors so that `apply_diff()` can still match them. This is the same mechanism that `apply_transform()` already uses (atom_edit.rs:438-468):

```rust
// Pattern from apply_transform() — reused for minimization:
let new_diff_id = self.diff.add_atom(atomic_number, new_position);
self.diff.set_anchor_position(new_diff_id, old_position);
```

For diff atoms that already exist (user-edited atoms), only their position is updated. Their anchor was already set on the first `move_in_diff()` call and is preserved.

---

## 4. Rust Implementation

### 4.1 New function: `minimize_atom_edit()` in `atom_edit.rs`

Location: `rust/src/structure_designer/nodes/atom_edit/atom_edit.rs`

This is a module-level free function (like `transform_selected`, `delete_selected_atoms_and_bonds`, etc.) that operates on StructureDesigner.

```rust
/// Freeze mode for atom_edit minimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimizeFreezeMode {
    /// Only diff atoms move; base atoms are frozen.
    FreezeBase,
    /// All atoms move freely.
    FreeAll,
}

/// Minimizes the atomic structure in the active atom_edit node.
///
/// Returns the minimization result message, or an error string.
pub fn minimize_atom_edit(
    structure_designer: &mut StructureDesigner,
    freeze_mode: MinimizeFreezeMode,
) -> Result<String, String>
```

**New imports needed** (add to top of atom_edit.rs):
```rust
use crate::crystolecule::simulation::minimize_with_force_field;
use crate::crystolecule::simulation::MinimizationConfig;
use crate::crystolecule::simulation::topology::MolecularTopology;
use crate::crystolecule::simulation::uff::UffForceField;
```

**Algorithm — concrete method calls:**

```
Phase 1: Gather info (immutable borrows, inside a block that returns owned data)
  let (topology, force_field, frozen_indices, result_to_source) = {

    // 1. Get AtomEditData (immutable)
    let atom_edit_data = get_active_atom_edit_data(structure_designer)?;
      // Returns Option<&AtomEditData> — use .ok_or("No active atom_edit node")?

    // 2. Get eval cache → provenance maps
    let eval_cache = structure_designer.get_selected_node_eval_cache()
        .ok_or("No eval cache")?;
    let eval_cache = eval_cache.downcast_ref::<AtomEditEvalCache>()
        .ok_or("Wrong eval cache type")?;

    // 3. Get evaluated result structure (full base+diff applied)
    let result_structure = structure_designer
        .get_atomic_structure_from_selected_node()
        .ok_or("No result structure")?;

    // 4. Build topology and force field
    let topology = MolecularTopology::from_structure(result_structure);
    let force_field = UffForceField::from_topology(&topology)?;

    // 5. Build result_atom_id → AtomSource map for write-back
    //    provenance.sources: FxHashMap<u32, AtomSource> has this directly
    //    AtomSource::Base { base_id } or AtomSource::Diff { diff_id }
    //    We need: topology_index → AtomSource, via topology.atom_ids
    let result_to_source: Vec<(u32, AtomSource)> = topology.atom_ids.iter()
        .filter_map(|&result_id| {
            eval_cache.provenance.sources.get(&result_id)
                .map(|source| (result_id, source.clone()))
        })
        .collect();

    // 6. Determine frozen set (topology indices)
    let frozen_indices: Vec<usize> = match freeze_mode {
        MinimizeFreezeMode::FreezeBase => {
            topology.atom_ids.iter().enumerate()
                .filter(|(_, &result_id)| {
                    !eval_cache.provenance.diff_to_result.values()
                        .any(|&r| r == result_id)
                })
                .map(|(i, _)| i)
                .collect()
        }
        MinimizeFreezeMode::FreeAll => Vec::new(),
    };

    (topology, force_field, frozen_indices, result_to_source)
  };

Phase 2: Minimize (no borrows on structure_designer)
  let mut positions = topology.positions.clone();
  let config = MinimizationConfig::default();
  let result = minimize_with_force_field(
      &force_field, &mut positions, &config, &frozen_indices,
  );

Phase 3: Write back (mutable borrow)
  let atom_edit_data = get_selected_atom_edit_data_mut(structure_designer)
      .ok_or("No active atom_edit node")?;
  // For each atom, check if position changed beyond threshold (1e-6 Å)
  for (topo_idx, &atom_id) in topology.atom_ids.iter().enumerate() {
      let new_pos = DVec3::new(
          positions[topo_idx * 3],
          positions[topo_idx * 3 + 1],
          positions[topo_idx * 3 + 2],
      );
      let old_pos = DVec3::new(
          topology.positions[topo_idx * 3],
          topology.positions[topo_idx * 3 + 1],
          topology.positions[topo_idx * 3 + 2],
      );
      if (new_pos - old_pos).length() < 1e-6 { continue; }

      match &result_to_source[...for this atom...] {
          AtomSource::Diff { diff_id } => {
              atom_edit_data.diff.set_atom_position(*diff_id, new_pos);
          }
          AtomSource::Base { base_id } => {
              // FreeAll mode only — add base atom to diff with anchor
              let atomic_number = topology.atomic_numbers[topo_idx];
              let new_diff_id = atom_edit_data.diff.add_atom(
                  atomic_number as i16, new_pos,
              );
              atom_edit_data.diff.set_anchor_position(new_diff_id, old_pos);
          }
      }
  }
  // Return human-readable message
  Ok(format!("Minimization {} after {} iterations (energy: {:.4} kcal/mol)",
      if result.converged { "converged" } else { "stopped" },
      result.iterations, result.energy))
```

**Key implementation notes:**

1. The three-phase pattern (gather → compute → mutate) matches `transform_selected` (atom_edit.rs:1615-1671) and avoids borrow conflicts.

2. Use `provenance.sources` (type `FxHashMap<u32, AtomSource>`) for the result→source mapping. This is simpler than building reverse maps from `diff_to_result` / `base_to_result`, since `AtomSource` already carries the source atom ID:
   ```rust
   pub enum AtomSource {
       Base { base_id: u32 },
       Diff { diff_id: u32 },
   }
   ```

3. For `FreezeBase` mode, the frozen set is all topology indices whose result atom ID does NOT appear as a value in `provenance.diff_to_result`. An alternative (possibly cleaner): iterate `topology.atom_ids` and check `provenance.sources[result_id]` — if `AtomSource::Base`, it's frozen.

4. Movement threshold: only write back atoms that moved more than 1e-6 Å. This avoids cluttering the diff with atoms that didn't meaningfully change.

5. The topology's `atom_ids: Vec<u32>` maps topology index → result structure atom ID. The `atomic_numbers: Vec<u8>` maps topology index → atomic number.

6. **Note on FxHashMap**: The provenance maps use `FxHashMap` (from `rustc_hash`), not `std::collections::HashMap`. Import `AtomSource` from `crate::crystolecule::atomic_structure_diff::AtomSource`.

**~100-150 lines of new code.**

### 4.2 New API type: `APIMinimizeFreezeMode`

Location: `rust/src/api/structure_designer/structure_designer_api_types.rs`

```rust
#[flutter_rust_bridge::frb]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum APIMinimizeFreezeMode {
    FreezeBase,
    FreeAll,
}
```

Add conversion to internal type (or use the API type directly since it's simple enough).

### 4.3 New API function: `atom_edit_minimize()`

Location: `rust/src/api/structure_designer/atom_edit_api.rs`

Following the established pattern (see existing functions like `atom_edit_transform_selected`):

```rust
// New import needed:
use crate::api::structure_designer::structure_designer_api_types::APIMinimizeFreezeMode;
use crate::structure_designer::nodes::atom_edit::atom_edit::MinimizeFreezeMode;

#[flutter_rust_bridge::frb(sync)]
pub fn atom_edit_minimize(freeze_mode: APIMinimizeFreezeMode) -> String {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let internal_mode = match freeze_mode {
                    APIMinimizeFreezeMode::FreezeBase => MinimizeFreezeMode::FreezeBase,
                    APIMinimizeFreezeMode::FreeAll => MinimizeFreezeMode::FreeAll,
                };
                let result = atom_edit::minimize_atom_edit(
                    &mut cad_instance.structure_designer,
                    internal_mode,
                );
                refresh_structure_designer_auto(cad_instance);
                match result {
                    Ok(message) => message,
                    Err(error) => format!("Error: {}", error),
                }
            },
            "Error: no active instance".to_string(),
        )
    }
}
```

**~20 lines of new code.**

### 4.4 Validate Phase A

After steps 1–3:
```bash
cd /c/machine_phase_systems/flutter_cad/rust && cargo build
cd /c/machine_phase_systems/flutter_cad/rust && cargo test
cd /c/machine_phase_systems/flutter_cad/rust && cargo clippy
```

All must pass before proceeding to Phase B.

---

## 5. Flutter Implementation (Phase B)

### 5.0 FRB Codegen

Run first to generate Dart bindings:
```bash
cd /c/machine_phase_systems/flutter_cad && flutter_rust_bridge_codegen generate
```

This generates the Dart bindings including `APIMinimizeFreezeMode` and `atomEditMinimize()` in `lib/src/rust/api/structure_designer/atom_edit_api.dart`.

### 5.1 Model method in `structure_designer_model.dart`

Add a field and method to `StructureDesignerModel`:

```dart
String _lastMinimizeMessage = '';

String get lastMinimizeMessage => _lastMinimizeMessage;

void atomEditMinimize(APIMinimizeFreezeMode freezeMode) {
  _lastMinimizeMessage = atomEditApi.atomEditMinimize(freezeMode: freezeMode);
  refreshFromKernel();
  notifyListeners();
}
```

The `atomEditApi` prefix is already imported in this file (see existing pattern: `atom_edit_api.atomEditDeleteSelected()` etc.). No new import needed — `APIMinimizeFreezeMode` comes from the already-imported `structure_designer_api_types.dart`.

### 5.2 UI in `atom_edit_editor.dart`

Add a "Minimize" section that is **tool-independent** — it appears regardless of which tool (Default, AddAtom, AddBond) is active. This is because minimization operates on the whole structure, not on a selection.

**Placement**: In the `build()` method, add the minimize section **after** `_buildToolSpecificUI()` (line 117):

```dart
// In build(), at the end of the Column's children list:
const SizedBox(height: AppSpacing.large),
// Tool-specific UI elements
_buildToolSpecificUI(),
// ↓↓↓ ADD THESE TWO LINES ↓↓↓
const SizedBox(height: AppSpacing.large),
_buildMinimizeSection(),
```

**New import needed** at top of `atom_edit_editor.dart`:
```dart
import 'package:flutter_cad/src/rust/api/structure_designer/atom_edit_api.dart'
    as atom_edit_api;
```

Actually — the UI calls `widget.model.atomEditMinimize()` which handles the API call internally. The only type needed in the UI file is `APIMinimizeFreezeMode`, which comes from the already-imported `structure_designer_api_types.dart` (line 2). **No new import needed.**

**New method `_buildMinimizeSection()`**:
```dart
Widget _buildMinimizeSection() {
  return Card(
    elevation: 0,
    margin: EdgeInsets.zero,
    color: Colors.grey[50],
    child: Padding(
      padding: const EdgeInsets.all(AppSpacing.medium),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Energy Minimization',
              style: TextStyle(fontWeight: FontWeight.w500)),
          const SizedBox(height: AppSpacing.medium),
          Row(
            children: [
              Expanded(
                child: SizedBox(
                  height: AppSpacing.buttonHeight,
                  child: ElevatedButton.icon(
                    onPressed: () {
                      widget.model.atomEditMinimize(
                        APIMinimizeFreezeMode.freezeBase,
                      );
                    },
                    icon: const Icon(Icons.lock_outline, size: 18),
                    label: const Text('Minimize (freeze base)'),
                    style: AppButtonStyles.primary,
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: SizedBox(
                  height: AppSpacing.buttonHeight,
                  child: ElevatedButton.icon(
                    onPressed: () {
                      widget.model.atomEditMinimize(
                        APIMinimizeFreezeMode.freeAll,
                      );
                    },
                    icon: const Icon(Icons.lock_open, size: 18),
                    label: const Text('Minimize (free all)'),
                    style: AppButtonStyles.primary,
                  ),
                ),
              ),
            ],
          ),
          // Show result message if available
          if (widget.model.lastMinimizeMessage.isNotEmpty) ...[
            const SizedBox(height: AppSpacing.small),
            Text(
              widget.model.lastMinimizeMessage,
              style: TextStyle(
                fontSize: 12,
                color: widget.model.lastMinimizeMessage.startsWith('Error')
                    ? Colors.red[700]
                    : Colors.grey[600],
              ),
            ),
          ],
        ],
      ),
    ),
  );
}
```

**Key UI details:**
- Error messages (prefixed with "Error:") render in red; success messages in grey.
- The minimize section renders for all tools because it's placed in `build()`, not in `_buildToolSpecificUI()`.
- `_stagedData` null check is handled by the early return in `build()` (line 56-58) — `_buildMinimizeSection()` is only called when `_stagedData != null`.
- The result message persists across rebuilds. It is replaced whenever the user clicks either minimize button. If the user switches to a different node, the entire `AtomEditEditor` widget is replaced, so the message is lost naturally.

### 5.3 Validate Phase B

```bash
cd /c/machine_phase_systems/flutter_cad && flutter analyze
dart format lib/structure_designer/structure_designer_model.dart lib/structure_designer/node_data/atom_edit_editor.dart
```

---

## 6. Implementation Order

### Phase A — Rust (one AI session)

| Step | What | Files | Validate |
|------|------|-------|----------|
| A1 | `MinimizeFreezeMode` enum + `minimize_atom_edit()` function | `atom_edit.rs` | `cargo build` |
| A2 | `APIMinimizeFreezeMode` enum | `structure_designer_api_types.rs` | `cargo build` |
| A3 | `atom_edit_minimize()` API function | `atom_edit_api.rs` | `cargo build && cargo test && cargo clippy` |

### Phase B — Flutter (one AI session)

| Step | What | Files | Validate |
|------|------|-------|----------|
| B1 | Run `flutter_rust_bridge_codegen generate` | Generated files | codegen succeeds |
| B2 | Model method `atomEditMinimize()` + `lastMinimizeMessage` | `structure_designer_model.dart` | `flutter analyze` |
| B3 | UI minimize section in `build()` | `atom_edit_editor.dart` | `flutter analyze` |
| B4 | Manual testing | — | See Section 8.3 |

---

## 7. Files Modified

| File | Changes |
|------|---------|
| `rust/src/structure_designer/nodes/atom_edit/atom_edit.rs` | Add `MinimizeFreezeMode` enum, `minimize_atom_edit()` function, new imports for simulation/topology/UFF |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Add `APIMinimizeFreezeMode` enum |
| `rust/src/api/structure_designer/atom_edit_api.rs` | Add `atom_edit_minimize()` function, new imports |
| `lib/src/rust/` | Regenerated FRB bindings (auto) |
| `lib/structure_designer/structure_designer_model.dart` | Add `_lastMinimizeMessage` field, `lastMinimizeMessage` getter, `atomEditMinimize()` method |
| `lib/structure_designer/node_data/atom_edit_editor.dart` | Add `_buildMinimizeSection()` method, add to `build()` Column |

---

## 8. Testing Strategy

### 8.1 Rust Unit Tests

The core `minimize_atom_edit` function can't easily be unit-tested in isolation (it requires `StructureDesigner` state). However, its building blocks are already well-tested:

- `minimize_energy()` — 47 tests in minimize_test.rs
- `apply_diff()` — tested in atomic_structure_diff tests
- Frozen atom support — tested in minimize_test.rs (frozen dimension tests, frozen UFF tests)

### 8.2 Integration Test (deferred — not in Phase A/B)

Could be added to `rust/tests/structure_designer/` later:
1. Creates a StructureDesigner with an atom_fill → atom_edit pipeline
2. Adds a displaced atom to the atom_edit diff
3. Calls `minimize_atom_edit` with FreezeBase mode
4. Verifies the diff atom moved toward equilibrium
5. Verifies base atoms are unchanged

This requires setting up a full StructureDesigner with evaluation, which is complex. The manual testing (8.3) covers this more practically. Can be added as a regression test after the feature is validated.

### 8.3 Manual Testing

1. Create an atom_fill → atom_edit chain
2. Add a carbon atom near the structure
3. Click "Minimize (freeze base)" — verify the new atom relaxes
4. Click "Minimize (free all)" — verify everything relaxes
5. Switch to diff view — verify anchors exist for moved base atoms
6. Check the result message shows convergence info
7. Verify error message (red text) when atom_edit has no connected input
8. Verify both buttons work regardless of active tool (Default, AddAtom, AddBond)

---

## 9. Edge Cases

1. **Empty diff**: Minimize does nothing in FreezeBase mode (all atoms frozen). In FreeAll mode, equivalent to relax node. Return appropriate message.

2. **No input structure**: atom_edit has no connected input → error message.

3. **UFF type errors**: Some atoms may not have UFF parameters. Propagate the error message to the UI (already handled by `UffForceField::from_topology()` returning `Err`).

4. **Large structures**: For structures > 1000 atoms, minimization may take noticeable time. The current synchronous API (`frb(sync)`) will block the UI. This is acceptable for the initial implementation — async can be added later if needed.

5. **Diff-only view**: Minimization operates on the full applied structure, regardless of the current view mode (result vs diff). The UI button should work the same way in both views.

---

## 10. Future Extensions (Not in This Plan)

- **Async minimization** — Run in background, show progress, allow cancellation
- **Per-atom freeze flags** — User selects specific atoms to freeze
- **Interactive dragging** — Minimize in real-time as user drags atoms (IM-UFF)
- **Minimize selected only** — Only atoms in the current selection are free; rest frozen
- **Result display** — Show energy surface visualization or per-atom force vectors
