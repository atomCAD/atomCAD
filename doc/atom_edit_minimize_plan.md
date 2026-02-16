# atom_edit Energy Minimization Integration Plan

## 1. Overview

Wire the UFF energy minimizer into the `atom_edit` node so users can minimize atomic structures directly from the editing interface. This involves:

1. A new Rust function that minimizes an `atom_edit` diff with frozen atom support
2. A new API endpoint exposed via Flutter Rust Bridge
3. Flutter UI additions (minimize button with freeze mode) in the atom_edit property panel

**Depends on**: Phase 20 (vdW) should be complete first so the minimizer produces physically meaningful results.

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

When base atoms move (Mode 2), they must be added to the diff with anchors so that `apply_diff()` can still match them. This is the same mechanism that `apply_transform()` already uses (atom_edit.rs:442-468):

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
/// Steps:
/// 1. Evaluate the input structure (base) from pin 0
/// 2. Apply the current diff to get the full structure
/// 3. Build topology and force field from the full structure
/// 4. Determine frozen atoms based on freeze_mode
/// 5. Run L-BFGS minimization
/// 6. Write moved positions back into the diff
///    - Diff atoms: update position via set_atom_position
///    - Base atoms that moved (FreeAll mode): add to diff with anchor
///
/// Returns the minimization result message, or an error string.
pub fn minimize_atom_edit(
    structure_designer: &mut StructureDesigner,
    freeze_mode: MinimizeFreezeMode,
) -> Result<String, String>
```

**Algorithm detail:**

```
Phase 1: Gather info (immutable borrows)
  - Get AtomEditData (immutable)
  - Get eval cache (AtomEditEvalCache) for provenance maps
  - Get evaluated result structure (AtomicStructure from selected node output)
  - Build MolecularTopology from result structure
  - Build UffForceField from topology
  - Determine frozen set:
    FreezeBase → frozen = all topology indices whose atom_ids map to base atoms
                 (i.e., atoms NOT in diff_to_result provenance)
    FreeAll    → frozen = empty
  - Record original positions for all atoms

Phase 2: Minimize (no borrows on structure_designer)
  - Clone positions from topology
  - Run minimize_with_force_field()

Phase 3: Write back (mutable borrow)
  - Get AtomEditData (mutable)
  - For each atom that moved (position changed beyond threshold):
    - If atom is a diff atom (exists in diff_to_result):
      Find the diff atom ID, call set_atom_position(diff_id, new_pos)
    - If atom is a base atom (exists in base_to_result, FreeAll mode):
      Add to diff: diff.add_atom(atomic_number, new_pos)
      Set anchor: diff.set_anchor_position(new_id, old_pos)
  - Return result message
```

**Key implementation notes:**

1. The three-phase pattern (gather → compute → mutate) matches `transform_selected` and avoids borrow conflicts.

2. Use `AtomEditEvalCache.provenance` to map between result atom IDs and diff/base atom IDs. The provenance has:
   - `base_to_result: HashMap<u32, u32>` — base atom → result atom
   - `diff_to_result: HashMap<u32, u32>` — diff atom → result atom
   We need the reverse: result atom → (diff or base atom).

3. Build reverse maps at gather time:
   ```rust
   let result_to_diff: HashMap<u32, u32> = provenance.diff_to_result
       .iter().map(|(&d, &r)| (r, d)).collect();
   let result_to_base: HashMap<u32, (u32, i16, DVec3)> = ...;
   ```

4. Movement threshold: only write back atoms that moved more than 1e-6 Å. This avoids cluttering the diff with atoms that didn't meaningfully change.

5. The topology's `atom_ids` maps topology index → result structure atom ID. Use this plus the reverse maps to identify each atom.

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

Following the established pattern:

```rust
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

### 4.4 FRB Codegen

After adding the API function and type, run:
```bash
flutter_rust_bridge_codegen generate
```

This generates the Dart bindings in `lib/src/rust/api/structure_designer/atom_edit_api.dart`.

---

## 5. Flutter Implementation

### 5.1 Model method in `structure_designer_model.dart`

Add a method to `StructureDesignerModel`:

```dart
String _lastMinimizeMessage = '';

String get lastMinimizeMessage => _lastMinimizeMessage;

void atomEditMinimize(APIMinimizeFreezeMode freezeMode) {
  _lastMinimizeMessage = atomEditApi.atomEditMinimize(freezeMode: freezeMode);
  refreshFromKernel();
  notifyListeners();
}
```

### 5.2 UI in `atom_edit_editor.dart`

Add a "Minimize" section to the Default Tool UI, after the "Transform Selected Atoms" section. The minimize button should always be visible (not just when atoms are selected), since minimization operates on the whole structure.

**Placement**: After the existing tool-specific UI, add a new Card section:

```dart
// In _buildDefaultToolUI(), after the existing Card:
const SizedBox(height: AppSpacing.large),
_buildMinimizeSection(),
```

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
              style: TextStyle(fontSize: 12, color: Colors.grey[600]),
            ),
          ],
        ],
      ),
    ),
  );
}
```

**Note**: The minimize section is NOT tool-dependent — it appears regardless of whether Default, AddAtom, or AddBond tool is active. Place it outside `_buildToolSpecificUI()`, directly in the `build()` method's Column.

---

## 6. Implementation Order

| Step | What | Files |
|------|------|-------|
| 1 | `MinimizeFreezeMode` enum + `minimize_atom_edit()` function | `atom_edit.rs` |
| 2 | `APIMinimizeFreezeMode` enum | `structure_designer_api_types.rs` |
| 3 | `atom_edit_minimize()` API function | `atom_edit_api.rs` |
| 4 | Run `flutter_rust_bridge_codegen generate` | Generated files |
| 5 | Model method `atomEditMinimize()` | `structure_designer_model.dart` |
| 6 | UI minimize section | `atom_edit_editor.dart` |
| 7 | Manual testing | — |

Steps 1-3 are Rust-only and can be compiled/tested before touching Flutter.

---

## 7. Files Modified

| File | Changes |
|------|---------|
| `rust/src/structure_designer/nodes/atom_edit/atom_edit.rs` | Add `MinimizeFreezeMode` enum, `minimize_atom_edit()` function |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | Add `APIMinimizeFreezeMode` enum |
| `rust/src/api/structure_designer/atom_edit_api.rs` | Add `atom_edit_minimize()` function |
| `lib/src/rust/` | Regenerated FRB bindings (auto) |
| `lib/structure_designer/structure_designer_model.dart` | Add `atomEditMinimize()` method, `lastMinimizeMessage` |
| `lib/structure_designer/node_data/atom_edit_editor.dart` | Add minimize section UI |

---

## 8. Testing Strategy

### 8.1 Rust Unit Tests

The core `minimize_atom_edit` function can't easily be unit-tested in isolation (it requires `StructureDesigner` state). However, its building blocks are already well-tested:

- `minimize_energy()` — 47 tests in minimize_test.rs
- `apply_diff()` — tested in atomic_structure_diff tests
- Frozen atom support — tested in minimize_test.rs (frozen dimension tests, frozen UFF tests)

### 8.2 Integration Test

Add a test to `rust/tests/structure_designer/` that:
1. Creates a StructureDesigner with an atom_fill → atom_edit pipeline
2. Adds a displaced atom to the atom_edit diff
3. Calls `minimize_atom_edit` with FreezeBase mode
4. Verifies the diff atom moved toward equilibrium
5. Verifies base atoms are unchanged

### 8.3 Manual Testing

1. Create an atom_fill → atom_edit chain
2. Add a carbon atom near the structure
3. Click "Minimize (freeze base)" — verify the new atom relaxes
4. Click "Minimize (free all)" — verify everything relaxes
5. Switch to diff view — verify anchors exist for moved base atoms
6. Check the result message shows convergence info

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
