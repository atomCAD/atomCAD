# atom_edit Node Undo/Redo Design

## Overview

This document designs the undo/redo system for the `atom_edit` node, which was deferred from the global undo/redo system (see `doc/design_global_undo_redo.md`). The `atom_edit` node has complex, potentially large state (an `AtomicStructure` diff with atoms, bonds, and anchor positions) that requires specialized incremental undo commands rather than the opaque `SetNodeData` snapshots used for simpler nodes.

**Current state:** `atom_edit` mutations bypass `set_node_network_data()` — they mutate the node data in-place via `get_selected_atom_edit_data_mut()`. This means `atom_edit` operations currently have **no undo support at all**. The `SetNodeData` command only fires for the generic data setter path, which atom_edit's interactive tools do not use.

## Design Principles

1. **Fully incremental** — Each command stores only the minimum state needed to reverse and re-apply that specific operation. No full-diff or full-node snapshots. A simple "add atom" stores ~50 bytes regardless of whether the diff has 10 or 10,000 atoms.
2. **Integrate with the global undo stack** — Commands implement `UndoCommand` and live in the global `StructureDesigner::undo_stack`. No separate per-node stack.
3. **Drag coalescing** — Screen-plane drags produce many intermediate positions; these are coalesced into a single undo step.
4. **Selection is not undoable** — Consistent with the global undo design.
5. **Recording at the mutation layer** — Low-level diff mutation methods on `AtomEditData` record deltas when recording is active. This is automatic and precise — no manual delta construction at each call site.

## Core Delta Types

All diff mutations are represented as ordered lists of atom and bond deltas. These are the fundamental building blocks.

### AtomDelta

Represents a single atom's change. Stores the complete before and after state.

```rust
/// State of an atom in the diff at a point in time.
#[derive(Debug, Clone)]
struct AtomState {
    atomic_number: i16,
    position: DVec3,
    anchor: Option<DVec3>,
}

/// A change to a single atom in the diff.
#[derive(Debug, Clone)]
struct AtomDelta {
    atom_id: u32,
    /// None if atom didn't exist before (was added).
    before: Option<AtomState>,
    /// None if atom doesn't exist after (was removed).
    after: Option<AtomState>,
}
```

**Three cases:**
| `before` | `after` | Meaning |
|----------|---------|---------|
| `None` | `Some(s)` | Atom was **added** with state `s` |
| `Some(s)` | `None` | Atom was **removed** (had state `s`) |
| `Some(a)` | `Some(b)` | Atom was **modified** from `a` to `b` |

### BondDelta

Represents a single bond's change.

```rust
/// A change to a bond in the diff.
#[derive(Debug, Clone)]
struct BondDelta {
    atom_id1: u32,
    atom_id2: u32,
    /// None if bond didn't exist before.
    old_order: Option<u8>,
    /// None if bond doesn't exist after.
    new_order: Option<u8>,
}
```

### FrozenDelta

Represents changes to the frozen atom sets.

```rust
#[derive(Debug, Clone)]
struct FrozenDelta {
    /// (provenance, atom_id) pairs added to frozen sets.
    added: Vec<(FrozenProvenance, u32)>,
    /// (provenance, atom_id) pairs removed from frozen sets.
    removed: Vec<(FrozenProvenance, u32)>,
}

#[derive(Debug, Clone, Copy)]
enum FrozenProvenance { Base, Diff }
```

## Command Types

### 1. AtomEditMutationCommand

The primary command for all operations that modify the diff structure. Stores the ordered list of deltas.

```rust
struct AtomEditMutationCommand {
    description: String,
    network_name: String,
    node_id: u64,
    atom_deltas: Vec<AtomDelta>,
    bond_deltas: Vec<BondDelta>,
}
```

- **undo:** Apply deltas in **reverse order**, restoring each delta's `before` state:
  - `Added` (before=None, after=Some): Delete the atom by ID
  - `Removed` (before=Some, after=None): Re-add the atom with its original ID and state
  - `Modified` (before=Some, after=Some): Restore position/atomic_number/anchor to `before` values
  - For bonds: restore `old_order` (remove if was None, add/change if was Some)
- **redo:** Apply deltas in **forward order**, restoring each delta's `after` state:
  - `Added`: Re-add the atom with its original ID and state
  - `Removed`: Delete the atom
  - `Modified`: Set position/atomic_number/anchor to `after` values
  - For bonds: apply `new_order`
- **refresh:** `NodeDataChanged(vec![node_id])`

**Memory per delta:** An `AtomDelta` is ~80 bytes (id + two optional 40-byte states). A `BondDelta` is ~16 bytes.

### 2. AtomEditToggleFlagCommand

For simple boolean flag toggles that don't modify the diff.

```rust
struct AtomEditToggleFlagCommand {
    description: String,
    network_name: String,
    node_id: u64,
    flag: AtomEditFlag,
    old_value: bool,
    new_value: bool,
}

enum AtomEditFlag {
    OutputDiff,
    ShowAnchorArrows,
    IncludeBaseBondsInDiff,
    ErrorOnStaleEntries,
}
```

### 3. AtomEditFrozenChangeCommand

For freeze/unfreeze operations.

```rust
struct AtomEditFrozenChangeCommand {
    description: String,
    network_name: String,
    node_id: u64,
    delta: FrozenDelta,
}
```

- **undo:** Remove `delta.added`, re-add `delta.removed`.
- **redo:** Add `delta.added`, remove `delta.removed`.

## Prerequisite: add_atom_with_id

`AtomicStructure::add_atom()` always appends (ID = `atoms.len() + 1`). Undo of an atom removal (and redo of an atom addition) must re-add the atom with its **original ID**. A new method is needed:

```rust
impl AtomicStructure {
    /// Add an atom with a specific ID. Used by undo/redo.
    /// Panics if the slot is already occupied.
    /// Extends the atoms Vec with padding if needed.
    pub fn add_atom_with_id(
        &mut self,
        id: u32,
        atomic_number: i16,
        position: DVec3,
    ) -> u32 {
        let index = (id - 1) as usize;
        // Extend with None padding if needed
        while self.atoms.len() <= index {
            self.atoms.push(None);
        }
        assert!(self.atoms[index].is_none(), "Slot {} already occupied", id);
        let atom = Atom { id, atomic_number, position, bonds: SmallVec::new(), flags: 0, in_crystal_depth: 0.0 };
        self.atoms[index] = Some(atom);
        self.num_atoms += 1;
        self.add_atom_to_grid(id, &position);
        id
    }
}
```

This parallels the `add_node_with_id` added to `NodeNetwork` for the global undo system.

## Delta Recording Mechanism

### Recording at the Mutation Layer

The low-level diff mutation methods on `AtomEditData` (`add_atom_to_diff`, `remove_from_diff`, `move_in_diff`, `add_bond_in_diff`, etc.) are the single choke point for all diff modifications. We add an optional recorder that captures deltas as mutations happen.

```rust
/// Captures diff deltas during a recording session.
#[derive(Debug, Default)]
pub struct DiffRecorder {
    pub atom_deltas: Vec<AtomDelta>,
    pub bond_deltas: Vec<BondDelta>,
}

impl AtomEditData {
    /// The active recorder. When Some, mutations are recorded.
    recorder: Option<DiffRecorder>,

    pub fn begin_recording(&mut self) {
        self.recorder = Some(DiffRecorder::default());
    }

    pub fn end_recording(&mut self) -> Option<DiffRecorder> {
        self.recorder.take()
    }
}
```

**Two layers of mutation methods:**

1. **Recording methods** (on `AtomEditData`) — `add_atom_to_diff`, `remove_from_diff`, `move_in_diff`, `add_bond_in_diff`, `set_atomic_number_recorded`, etc. These are called during **user actions**. When `recorder` is active, they capture deltas alongside performing the mutation.

2. **Non-recording methods** (on `AtomicStructure` / the diff directly) — `diff.add_atom_with_id`, `diff.delete_atom`, `diff.set_atom_position`, `diff.add_bond`, etc. These are called by **undo/redo execution** (`apply_undo`/`apply_redo`). They modify the diff without touching the recorder, since we never want undo/redo to produce new deltas.

This separation is natural: the recording methods already exist as `AtomEditData` wrappers around the `AtomicStructure` primitives. Undo/redo simply uses the primitives directly.

Each recording method captures its delta **before and after** performing the operation:

```rust
pub fn add_atom_to_diff(&mut self, atomic_number: i16, position: DVec3) -> u32 {
    self.selection.clear_bonds();
    let id = self.diff.add_atom(atomic_number, position);

    // Record: atom didn't exist before, exists now
    if let Some(ref mut rec) = self.recorder {
        rec.atom_deltas.push(AtomDelta {
            atom_id: id,
            before: None,
            after: Some(AtomState { atomic_number, position, anchor: None }),
        });
    }
    id
}

pub fn remove_from_diff(&mut self, diff_atom_id: u32) {
    // Capture before-state
    let before_state = if self.recorder.is_some() {
        self.diff.get_atom(diff_atom_id).map(|a| {
            let anchor = self.diff.anchor_position(diff_atom_id).copied();
            (AtomState { atomic_number: a.atomic_number, position: a.position, anchor },
             a.bonds.iter().map(|b| (b.other_atom_id(), b.bond_order())).collect::<Vec<_>>())
        })
    } else { None };

    self.selection.clear_bonds();
    self.diff.delete_atom(diff_atom_id);
    self.diff.remove_anchor_position(diff_atom_id);

    // Record atom removal + bond removals
    if let Some(ref mut rec) = self.recorder {
        if let Some((atom_state, bonds)) = before_state {
            rec.atom_deltas.push(AtomDelta {
                atom_id: diff_atom_id,
                before: Some(atom_state),
                after: None,
            });
            for (other_id, order) in bonds {
                // Only record bond removal once (for the atom being deleted)
                // The other endpoint's bond list is updated by delete_atom internally
                if diff_atom_id < other_id {
                    rec.bond_deltas.push(BondDelta {
                        atom_id1: diff_atom_id,
                        atom_id2: other_id,
                        old_order: Some(order),
                        new_order: None,
                    });
                }
            }
        }
    }
}

pub fn move_in_diff(&mut self, atom_id: u32, new_position: DVec3) {
    // Capture old position
    let old_state = if self.recorder.is_some() {
        self.diff.get_atom(atom_id).map(|a| AtomState {
            atomic_number: a.atomic_number,
            position: a.position,
            anchor: self.diff.anchor_position(atom_id).copied(),
        })
    } else { None };

    self.selection.clear_bonds();
    self.diff.set_atom_position(atom_id, new_position);

    if let Some(ref mut rec) = self.recorder {
        if let Some(old) = old_state {
            rec.atom_deltas.push(AtomDelta {
                atom_id,
                before: Some(old.clone()),
                after: Some(AtomState { position: new_position, ..old }),
            });
        }
    }
}

pub fn add_bond_in_diff(&mut self, atom_id1: u32, atom_id2: u32, order: u8) {
    // Capture old bond state
    let old_order = if self.recorder.is_some() {
        self.diff.get_atom(atom_id1).and_then(|a| {
            a.bonds.iter().find(|b| b.other_atom_id() == atom_id2).map(|b| b.bond_order())
        })
    } else { None };

    self.selection.clear_bonds();
    self.diff.add_bond_checked(atom_id1, atom_id2, order);

    if let Some(ref mut rec) = self.recorder {
        let (a, b) = if atom_id1 < atom_id2 { (atom_id1, atom_id2) } else { (atom_id2, atom_id1) };
        rec.bond_deltas.push(BondDelta {
            atom_id1: a, atom_id2: b,
            old_order,
            new_order: Some(order),
        });
    }
}
```

Similarly for `mark_for_deletion` (records an addition with atomic_number=0), `replace_in_diff` (records an addition), and methods on `AtomicStructure` called directly (like `set_atomic_number`, `set_anchor_position`).

**Methods that call other recorded methods** (like `convert_to_delete_marker` which calls `remove_from_diff` + `mark_for_deletion`) automatically produce the correct compound delta — the inner calls each record their own deltas, and the result is an ordered sequence of [Remove(old_atom), Add(delete_marker)].

### Direct AtomicStructure Mutations

Some code paths mutate `self.diff` directly (e.g., `self.diff.set_atomic_number(...)`, `self.diff.set_anchor_position(...)`) without going through AtomEditData wrapper methods. These occur in:

- `apply_replace` — calls `self.diff.set_atomic_number()` and `self.diff.set_anchor_position()` directly
- `apply_transform` — calls `self.diff.set_atomic_number()` and `self.diff.set_anchor_position()` directly
- `drag_selected_by_delta` (in operations.rs) — calls `self.diff.set_atomic_number()`, `self.diff.set_anchor_position()`, `self.diff.add_atom()`
- `apply_position_updates` (in modify_measurement.rs) — similar pattern
- `minimization.rs` — calls `self.diff.set_atom_position()` and `self.diff.add_atom()` + `set_anchor_position()`
- `hydrogen_passivation.rs` — calls `self.diff.add_atom()`, `self.diff.add_bond_checked()`, etc.

For these, we add recording wrapper methods on `AtomEditData` that combine the mutation with delta recording:

```rust
impl AtomEditData {
    /// Set an atom's atomic_number with recording.
    pub fn set_atomic_number_recorded(&mut self, atom_id: u32, atomic_number: i16) {
        if let Some(ref mut rec) = self.recorder {
            if let Some(atom) = self.diff.get_atom(atom_id) {
                let old_an = atom.atomic_number;
                let pos = atom.position;
                let anchor = self.diff.anchor_position(atom_id).copied();
                if old_an != atomic_number {
                    rec.atom_deltas.push(AtomDelta {
                        atom_id,
                        before: Some(AtomState { atomic_number: old_an, position: pos, anchor }),
                        after: Some(AtomState { atomic_number, position: pos, anchor }),
                    });
                }
            }
        }
        self.diff.set_atomic_number(atom_id, atomic_number);
    }

    /// Set an atom's anchor position with recording.
    pub fn set_anchor_recorded(&mut self, atom_id: u32, anchor: DVec3) {
        if let Some(ref mut rec) = self.recorder {
            if let Some(atom) = self.diff.get_atom(atom_id) {
                let old_anchor = self.diff.anchor_position(atom_id).copied();
                if old_anchor != Some(anchor) {
                    rec.atom_deltas.push(AtomDelta {
                        atom_id,
                        before: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor: old_anchor,
                        }),
                        after: Some(AtomState {
                            atomic_number: atom.atomic_number,
                            position: atom.position,
                            anchor: Some(anchor),
                        }),
                    });
                }
            }
        }
        self.diff.set_anchor_position(atom_id, anchor);
    }

    /// Add atom directly to diff with recording. For use by code that
    /// bypasses add_atom_to_diff (minimization, hydrogen passivation, etc.)
    pub fn add_atom_recorded(&mut self, atomic_number: i16, position: DVec3) -> u32 {
        let id = self.diff.add_atom(atomic_number, position);
        if let Some(ref mut rec) = self.recorder {
            rec.atom_deltas.push(AtomDelta {
                atom_id: id,
                before: None,
                after: Some(AtomState { atomic_number, position, anchor: None }),
            });
        }
        id
    }

    /// Add bond with recording. For use by code that calls diff.add_bond_checked directly.
    pub fn add_bond_recorded(&mut self, atom_id1: u32, atom_id2: u32, order: u8) {
        let old_order = self.diff.get_atom(atom_id1).and_then(|a| {
            a.bonds.iter().find(|b| b.other_atom_id() == atom_id2).map(|b| b.bond_order())
        });
        self.diff.add_bond_checked(atom_id1, atom_id2, order);
        if let Some(ref mut rec) = self.recorder {
            let (a, b) = if atom_id1 < atom_id2 { (atom_id1, atom_id2) } else { (atom_id2, atom_id1) };
            rec.bond_deltas.push(BondDelta { atom_id1: a, atom_id2: b, old_order, new_order: Some(order) });
        }
    }

    /// Set atom position with recording. For use by minimization etc.
    pub fn set_position_recorded(&mut self, atom_id: u32, new_position: DVec3) {
        if let Some(ref mut rec) = self.recorder {
            if let Some(atom) = self.diff.get_atom(atom_id) {
                let anchor = self.diff.anchor_position(atom_id).copied();
                rec.atom_deltas.push(AtomDelta {
                    atom_id,
                    before: Some(AtomState { atomic_number: atom.atomic_number, position: atom.position, anchor }),
                    after: Some(AtomState { atomic_number: atom.atomic_number, position: new_position, anchor }),
                });
            }
        }
        self.diff.set_atom_position(atom_id, new_position);
    }
}
```

The existing code paths (`minimization.rs`, `hydrogen_passivation.rs`, `operations.rs`, `modify_measurement.rs`) are updated to call these recorded variants instead of `self.diff.*` directly. The non-recording paths (`add_atom_to_diff`, `move_in_diff`, etc.) are also updated to use the recorded variants internally, so recording is centralized.

### Delta Optimization: Coalescing Redundant Deltas

A single operation may produce multiple deltas for the same atom (e.g., `apply_replace` on a base atom with an UNCHANGED marker: first changes atomic_number, then sets anchor). These can be coalesced after recording ends:

```rust
impl DiffRecorder {
    /// Coalesce consecutive deltas for the same atom into a single delta.
    /// Only coalesces Modified+Modified pairs for the same atom_id.
    pub fn coalesce(&mut self) { ... }
}
```

Coalescing rules:
- Two consecutive `Modified` deltas for the same atom → merge into one (`before` from first, `after` from second)
- `Added` followed by `Modified` for same atom → single `Added` with the final state
- `Modified` followed by `Removed` for same atom → single `Removed` with the original before-state

This is optional (correctness doesn't depend on it) but reduces memory usage.

## Undo/Redo Execution

Undo/redo operates entirely through **non-recording `AtomicStructure` methods** (`diff.add_atom_with_id`, `diff.delete_atom`, `diff.set_atom_position`, `diff.set_atomic_number`, `diff.set_anchor_position`, `diff.remove_anchor_position`, `diff.add_bond`, `diff.delete_bond`, `diff.add_bond_checked`). These are the low-level primitives that the recording methods wrap. Since undo/redo never goes through the recording layer, it never produces new deltas — even if a `DiffRecorder` were somehow active (which it won't be).

### Accessing AtomEditData

Commands access the node data through `UndoContext`:

```rust
fn get_atom_edit_data_mut<'a>(
    ctx: &'a mut UndoContext,
    network_name: &str,
    node_id: u64,
) -> Option<&'a mut AtomEditData> {
    let network = ctx.network_mut(network_name)?;
    let node = network.nodes.get_mut(&node_id)?;
    node.data.as_any_mut().downcast_mut::<AtomEditData>()
}
```

### Undo Execution

Process deltas in **reverse order**. For each delta, restore the `before` state:

```rust
fn apply_undo(data: &mut AtomEditData, atom_deltas: &[AtomDelta], bond_deltas: &[BondDelta]) {
    // Reverse bond deltas first (bonds reference atoms that must still exist)
    for delta in bond_deltas.iter().rev() {
        match (delta.old_order, delta.new_order) {
            (None, Some(_)) => {
                // Bond was added → remove it
                data.diff.delete_bond(&BondReference { atom_id1: delta.atom_id1, atom_id2: delta.atom_id2 });
            }
            (Some(order), None) => {
                // Bond was removed → re-add it
                data.diff.add_bond(delta.atom_id1, delta.atom_id2, order);
            }
            (Some(old), Some(_)) => {
                // Bond order changed → restore old order
                data.diff.add_bond_checked(delta.atom_id1, delta.atom_id2, old);
            }
            (None, None) => {} // no-op
        }
    }

    // Reverse atom deltas
    for delta in atom_deltas.iter().rev() {
        match (&delta.before, &delta.after) {
            (None, Some(_)) => {
                // Atom was added → delete it
                data.diff.delete_atom(delta.atom_id);
                data.diff.remove_anchor_position(delta.atom_id);
            }
            (Some(state), None) => {
                // Atom was removed → re-add with original ID
                data.diff.add_atom_with_id(delta.atom_id, state.atomic_number, state.position);
                if let Some(anchor) = state.anchor {
                    data.diff.set_anchor_position(delta.atom_id, anchor);
                }
            }
            (Some(before), Some(_after)) => {
                // Atom was modified → restore before state
                data.diff.set_atomic_number(delta.atom_id, before.atomic_number);
                data.diff.set_atom_position(delta.atom_id, before.position);
                match before.anchor {
                    Some(anchor) => data.diff.set_anchor_position(delta.atom_id, anchor),
                    None => data.diff.remove_anchor_position(delta.atom_id),
                }
            }
            (None, None) => {} // no-op
        }
    }

    data.clear_input_cache();
}
```

### Redo Execution

Process deltas in **forward order**. For each delta, restore the `after` state. Same logic as undo but using `after` instead of `before`:

```rust
fn apply_redo(data: &mut AtomEditData, atom_deltas: &[AtomDelta], bond_deltas: &[BondDelta]) {
    // Forward atom deltas first (atoms must exist before bonds reference them)
    for delta in atom_deltas.iter() {
        match (&delta.before, &delta.after) {
            (None, Some(state)) => {
                // Atom was added → re-add with original ID
                data.diff.add_atom_with_id(delta.atom_id, state.atomic_number, state.position);
                if let Some(anchor) = state.anchor {
                    data.diff.set_anchor_position(delta.atom_id, anchor);
                }
            }
            (Some(_), None) => {
                // Atom was removed → delete it
                data.diff.delete_atom(delta.atom_id);
                data.diff.remove_anchor_position(delta.atom_id);
            }
            (Some(_before), Some(after)) => {
                // Atom was modified → apply after state
                data.diff.set_atomic_number(delta.atom_id, after.atomic_number);
                data.diff.set_atom_position(delta.atom_id, after.position);
                match after.anchor {
                    Some(anchor) => data.diff.set_anchor_position(delta.atom_id, anchor),
                    None => data.diff.remove_anchor_position(delta.atom_id),
                }
            }
            (None, None) => {}
        }
    }

    // Forward bond deltas
    for delta in bond_deltas.iter() {
        match (delta.old_order, delta.new_order) {
            (None, Some(order)) => {
                data.diff.add_bond(delta.atom_id1, delta.atom_id2, order);
            }
            (Some(_), None) => {
                data.diff.delete_bond(&BondReference { atom_id1: delta.atom_id1, atom_id2: delta.atom_id2 });
            }
            (Some(_), Some(new)) => {
                data.diff.add_bond_checked(delta.atom_id1, delta.atom_id2, new);
            }
            (None, None) => {}
        }
    }

    data.clear_input_cache();
}
```

**Note on ordering:** Undo processes bonds first (in reverse), then atoms (in reverse) — this ensures bonds are removed before the atoms they reference. Redo processes atoms first (forward), then bonds (forward) — atoms must exist before bonds are added.

## User Action to Command Mapping

### Actions Using AtomEditMutationCommand

| User Action | Description | Deltas Produced |
|---|---|---|
| Add atom (free) | "Add atom" | 1 AtomDelta(Added) |
| Add atom (guided) | "Place atom" | 1-2 AtomDelta(Added) + 1 BondDelta(Added) |
| Delete selection (result view) | "Delete atoms" | N AtomDelta(Added for markers, Removed for additions, Remove+Add for conversions) + M BondDelta |
| Delete selection (diff view) | "Delete atoms" | N AtomDelta(Removed or Remove+Add) + M BondDelta(Removed) |
| Replace selection | "Replace atoms" | N AtomDelta(Modified or Added) |
| Drag atoms | "Move atoms" | N AtomDelta(Modified for moves, Added for promotions) |
| Transform (gadget) | "Move atoms" | N AtomDelta(Modified or Added) |
| Add bond | "Add bond" | 0-2 AtomDelta(Added for UNCHANGED promotions) + 1 BondDelta(Added) |
| Change bond order | "Change bond order" | 0-2 AtomDelta(Added for promotions) + 1 BondDelta(OrderChanged) |
| Bond order cycle | "Change bond order" | 0-2 AtomDelta + 1 BondDelta |
| Minimize | "Minimize structure" | N AtomDelta(Modified for moves, Added for promotions) |
| Add hydrogen | "Add hydrogen" | N AtomDelta(Added for H atoms + UNCHANGED markers) + N BondDelta(Added) |
| Remove hydrogen | "Remove hydrogen" | N AtomDelta(Removed or Remove+Add for conversions) + N BondDelta |
| Modify distance | "Modify distance" | N AtomDelta(Modified for moves, Added for promotions) |
| Modify angle | "Modify angle" | Same pattern |
| Modify dihedral | "Modify dihedral" | Same pattern |

### Actions Using AtomEditToggleFlagCommand

| User Action | Flag |
|---|---|
| Toggle diff view | `OutputDiff` |
| Toggle anchor arrows | `ShowAnchorArrows` |
| Toggle base bonds in diff | `IncludeBaseBondsInDiff` |
| Toggle error on stale | `ErrorOnStaleEntries` |

### Actions Using AtomEditFrozenChangeCommand

| User Action | FrozenDelta |
|---|---|
| Freeze selection | added = selected atoms |
| Unfreeze selection | removed = selected atoms |
| Clear frozen | removed = all frozen atoms |

### Actions That Must NOT Push Commands

| Action | Reason |
|---|---|
| Select atom/bond | Transient |
| Marquee select | Transient |
| Switch tool | Transient |
| Set selected element | Transient |
| Toggle gadget visibility | Transient (DefaultToolState) |
| Set/clear measurement mark | Transient UI highlight |
| Frozen to selection | Only changes selection |
| Start/cancel guided placement | Transient tool state |
| Guided placement pointer move | Preview only |
| AddBond pointer down/move | Interaction state only |

## Recording Entry Points

### Pattern: with_atom_edit_undo

A helper wraps each API mutation with recording:

```rust
fn with_atom_edit_undo<F>(
    structure_designer: &mut StructureDesigner,
    description: &str,
    mutation: F,
) where
    F: FnOnce(&mut StructureDesigner),
{
    let (network_name, node_id) = match get_atom_edit_node_info(structure_designer) {
        Some(info) => info,
        None => { mutation(structure_designer); return; },
    };

    // Begin recording
    if let Some(data) = get_atom_edit_data_for_recording(structure_designer) {
        data.begin_recording();
    }

    // Execute the mutation
    mutation(structure_designer);

    // End recording and push command
    if let Some(data) = get_atom_edit_data_for_recording(structure_designer) {
        if let Some(mut recorder) = data.end_recording() {
            recorder.coalesce();
            if !recorder.atom_deltas.is_empty() || !recorder.bond_deltas.is_empty() {
                structure_designer.push_command(AtomEditMutationCommand {
                    description: description.to_string(),
                    network_name,
                    node_id,
                    atom_deltas: recorder.atom_deltas,
                    bond_deltas: recorder.bond_deltas,
                });
            }
        }
    }
}
```

**Note:** `get_atom_edit_data_for_recording` is a variant accessor that gets mutable access WITHOUT calling `mark_node_data_changed` — recording setup is not a data mutation.

### Example: Delete Selected

```rust
// In atom_edit_api.rs
pub fn atom_edit_delete_selected() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            with_atom_edit_undo(
                &mut cad_instance.structure_designer,
                "Delete atoms",
                |sd| atom_edit::delete_selected_atoms_and_bonds(sd),
            );
            refresh_structure_designer_auto(cad_instance);
        });
    }
}
```

No changes needed to `delete_selected_atoms_and_bonds()` itself — the recording happens automatically in the low-level mutation methods.

## Drag Coalescing

### Default Tool Screen-Plane Drag

During a drag, `drag_selected_by_delta()` is called many times. We need all these calls to produce a single undo step.

**Mechanism:** Recording is started when the drag begins and ended when the drag completes. All deltas from all `drag_selected_by_delta()` calls accumulate in a single `DiffRecorder`.

```rust
/// Temporary state held during an atom edit drag.
pub struct PendingAtomEditDrag {
    network_name: String,
    node_id: u64,
    // Recording is active on AtomEditData.recorder throughout the drag
}
```

**Flow:**

1. `default_tool_pointer_move()` detects drag threshold exceeded
2. Call `begin_atom_edit_drag()` → starts recording on AtomEditData, stores `PendingAtomEditDrag`
3. First `drag_selected_by_delta()` call (and all subsequent) — deltas accumulate automatically
4. `default_tool_pointer_up()` → calls `end_atom_edit_drag()`
5. `end_atom_edit_drag()` → ends recording, coalesces deltas, pushes single command

**Coalescing behavior:** Multiple `move_in_diff` calls for the same atom during a drag produce multiple `AtomDelta(Modified)` entries. After coalescing, these merge into a single delta with `before` = position at drag start, `after` = position at drag end. Promotions (first frame only) produce `AtomDelta(Added)` which stays as-is.

**Cancel behavior:** If the drag is cancelled (`pointer_cancel`), the diff has already been modified. `end_atom_edit_drag()` is still called, producing a command. The undo of a cancelled drag restores the original positions.

### XYZ Gadget Drag

Same pattern — `begin_atom_edit_drag()` / `end_atom_edit_drag()` called from the gadget's begin/end lifecycle hooks.

### Handle inside the Rust API Layer

The begin/end calls are handled internally in the Rust API layer's `default_tool_pointer_move` (on Pending→Dragging transition) and `default_tool_pointer_up` (on DragCompleted). No new Flutter API functions needed.

## SetNodeData Suppression

Add `atom_edit` to the suppression check in `set_node_network_data()`:

```rust
let old_data_json = if node_type_name != "edit_atom" && node_type_name != "atom_edit" {
    self.snapshot_node_data(&network_name, node_id)
} else {
    None
};
```

## Clearing the Input Cache

After undo/redo, the `cached_input` may be stale. The undo/redo execution calls `data.clear_input_cache()` after applying deltas (shown in the apply_undo/apply_redo pseudocode above).

## Memory Analysis

### Per-Operation Memory Usage

| Operation | Typical Deltas | Memory |
|---|---|---|
| Add atom | 1 atom | ~80 bytes |
| Add atom (guided) | 2 atoms + 1 bond | ~176 bytes |
| Delete 5 atoms + 3 bonds | 5 atoms + 3 bonds | ~448 bytes |
| Replace 10 atoms | 10 atoms | ~800 bytes |
| Move 20 atoms (drag) | 20 atoms | ~1.6 KB |
| Minimize 500 atoms | 500 atoms | ~40 KB |
| Add hydrogen (100 H) | 100 atoms + 100 bonds | ~9.6 KB |
| Toggle flag | 1 bool | ~50 bytes |

### Comparison with Snapshot Approach

For a diff with 1000 atoms (~80 KB serialized):

| Operation | Incremental | Snapshot (before+after) |
|---|---|---|
| Add 1 atom | 80 bytes | 160 KB |
| Move 5 atoms | 400 bytes | 160 KB |
| Minimize 500 atoms | 40 KB | 160 KB |
| Toggle flag | 50 bytes | 160 KB |

Over 100 undo steps of mixed operations, incremental uses ~50 KB-500 KB total vs snapshot's ~16 MB. The savings are most dramatic for small operations on large diffs.

## Testing Strategy

### Test Helper

```rust
fn assert_atom_edit_undo_redo_roundtrip(
    designer: &mut StructureDesigner,
    action: impl FnOnce(&mut StructureDesigner),
) {
    // Snapshot the full diff state for comparison (test only)
    let before = snapshot_diff_state(designer);

    action(designer);

    let after = snapshot_diff_state(designer);

    // Property 1: do + undo = identity
    assert!(designer.undo());
    assert_eq!(before, snapshot_diff_state(designer));

    // Property 2: do + undo + redo = do
    assert!(designer.redo());
    assert_eq!(after, snapshot_diff_state(designer));

    designer.undo(); // Restore for composability
}

/// Test-only: serialize the full diff + flags + frozen state for comparison.
fn snapshot_diff_state(designer: &StructureDesigner) -> SerializableAtomEditData { ... }
```

### Test Categories

#### Single-Command Tests
```rust
#[test] fn undo_atom_edit_add_atom()
#[test] fn undo_atom_edit_add_atom_guided()
#[test] fn undo_atom_edit_delete_atoms_result_view()
#[test] fn undo_atom_edit_delete_atoms_diff_view()
#[test] fn undo_atom_edit_replace()
#[test] fn undo_atom_edit_add_bond()
#[test] fn undo_atom_edit_change_bond_order()
#[test] fn undo_atom_edit_minimize()
#[test] fn undo_atom_edit_add_hydrogen()
#[test] fn undo_atom_edit_remove_hydrogen()
#[test] fn undo_atom_edit_modify_distance()
#[test] fn undo_atom_edit_modify_angle()
#[test] fn undo_atom_edit_toggle_flag()
#[test] fn undo_atom_edit_frozen_change()
```

#### Drag Coalescing Tests
```rust
#[test] fn undo_atom_edit_drag_is_single_step()
#[test] fn drag_without_movement_creates_no_command()
#[test] fn undo_atom_edit_drag_with_base_promotion()
```

#### Sequence Tests
```rust
#[test] fn undo_atom_edit_sequence_restores_initial_state()
#[test] fn undo_atom_edit_interleaved_with_global()
```

#### Edge Cases
```rust
#[test] fn undo_atom_edit_delete_with_bond_promotions()  // UNCHANGED markers created for bond endpoints
#[test] fn undo_atom_edit_convert_to_delete_marker()     // Atom removed + new marker added
#[test] fn undo_atom_edit_replace_with_unchanged_reuse() // Existing UNCHANGED marker promoted
```

## File Organization

```
rust/src/structure_designer/undo/commands/
├── atom_edit_mutation.rs       # AtomEditMutationCommand, AtomDelta, BondDelta, apply_undo/redo
├── atom_edit_toggle_flag.rs    # AtomEditToggleFlagCommand, AtomEditFlag enum
├── atom_edit_frozen_change.rs  # AtomEditFrozenChangeCommand, FrozenDelta
└── mod.rs                      # Add new modules

rust/src/structure_designer/nodes/atom_edit/
├── diff_recorder.rs            # DiffRecorder, delta coalescing
└── atom_edit_data.rs           # Add recorder field, begin/end_recording, recorded mutation variants

rust/src/crystolecule/atomic_structure/
└── mod.rs                      # Add add_atom_with_id()
```

New field on `StructureDesigner`:
```rust
pub pending_atom_edit_drag: Option<PendingAtomEditDrag>,
```

New field on `AtomEditData`:
```rust
recorder: Option<DiffRecorder>,
```

## Phased Implementation Plan

### Phase A: Infrastructure

1. Add `add_atom_with_id()` to `AtomicStructure`
2. Create `DiffRecorder` and `AtomDelta`/`BondDelta` types
3. Add `recorder` field to `AtomEditData` with `begin_recording()`/`end_recording()`
4. Add recording to core mutation methods: `add_atom_to_diff`, `remove_from_diff`, `move_in_diff`, `add_bond_in_diff`, `mark_for_deletion`, `delete_bond_in_diff`
5. Add recorded wrapper methods: `set_atomic_number_recorded`, `set_anchor_recorded`, `add_atom_recorded`, `add_bond_recorded`, `set_position_recorded`
6. Create `AtomEditMutationCommand` with `apply_undo`/`apply_redo`
7. Create `with_atom_edit_undo` helper
8. Suppress `SetNodeData` for `atom_edit`
9. Tests: `undo_atom_edit_add_atom`, basic undo/redo of single atom add

### Phase B: Simple Operations

1. Wire up: `add_atom_by_ray`, `delete_selected`, `replace_selected`
2. Update `apply_delete_result_view`/`apply_delete_diff_view`/`apply_replace`/`apply_transform` to use recorded methods
3. Wire up: `add_bond_pointer_up`, `change_bond_order`, `change_selected_bonds_order`
4. Tests for each

### Phase C: Drag Coalescing

1. Add `PendingAtomEditDrag` to `StructureDesigner`
2. Implement `begin_atom_edit_drag()`/`end_atom_edit_drag()`
3. Update `drag_selected_by_delta` to use recorded methods
4. Wire into `default_tool_pointer_move`/`pointer_up`
5. Wire into XYZ gadget drag
6. Implement delta coalescing in `DiffRecorder`
7. Tests: drag coalescing, drag with promotions

### Phase D: Complex Operations

1. Update `minimization.rs` to use recorded methods
2. Update `hydrogen_passivation.rs` to use recorded methods
3. Update `modify_measurement.rs` (apply_position_updates) to use recorded methods
4. Update `add_atom_tool.rs` (guided placement) to use recorded methods
5. Tests for minimize, add/remove hydrogen, modify measurement, guided placement

### Phase E: Flags + Frozen + Integration

1. Create `AtomEditToggleFlagCommand` and `AtomEditFrozenChangeCommand`
2. Wire up toggle API functions
3. Wire up frozen state API functions
4. Sequence tests, edge case tests, mixed global+atom_edit tests
