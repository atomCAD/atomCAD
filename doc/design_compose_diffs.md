# Design: Diff Composition (`compose_diffs`) and `atom_composediff` Node

## Problem Statement

When chaining multiple `atom_edit` nodes, each produces a diff that is applied sequentially:

```
base ──▶ apply_diff(diff1) ──▶ apply_diff(diff2) ──▶ apply_diff(diff3) ──▶ result
```

There is no way to **compose** these diffs into a single diff that produces the same result in one step:

```
base ──▶ apply_diff(compose_diffs([diff1, diff2, diff3])) ──▶ result
```

This document designs both:
1. A `compose_diffs` function in the **crystolecule** module (reusable, fundamental operation)
2. An `atom_composediff` node in the **structure designer**

### Correctness Invariant

For any base structure and sequence of diffs:

```
apply_diff(apply_diff(base, diff1), diff2) == apply_diff(base, compose_diffs(diff1, diff2, tolerance))
```

This must hold for all atom operations (add, delete, move, replace), bond operations (add, delete, override), metadata (flags), and all edge cases (orphaned atoms, unmatched delete markers, etc.).

---

## 1. Algorithm: Composing Two Diffs

### 1.1 Key Insight

After `apply_diff(base, diff1)`, the result contains atoms from three origins:

| Origin | Position in result | Known at compose time? |
|--------|-------------------|----------------------|
| diff1 modified atom (matched base) | diff1.atom.position | **Yes** — from diff1 |
| diff1 added atom (pure addition) | diff1.atom.position | **Yes** — from diff1 |
| Base passthrough atom | base.atom.position | **No** — unknown without base |
| diff1 deleted base atom | *(not in result)* | N/A |

diff2 matches against this result by position. We can resolve matches between diff2 and diff1 atoms at compose time (positions known). diff2 atoms that don't match any diff1 atom will match base passthroughs (or be pure additions) — these can be passed through to the composed diff as-is, since their semantics are relative to base positions and will work correctly when applied.

### 1.2 Algorithm Overview

```
compose_two_diffs(diff1, diff2, tolerance) -> composed_diff
```

**Step 1: Match diff2 atoms against diff1 atoms by position.**

For each diff2 atom, compute its match position (anchor if present, else atom position). Search diff1's non-delete-marker atoms within tolerance. Use the same greedy nearest-first matching as `apply_diff`.

Note: diff2 atoms should only match diff1 atoms that would be present in the result of `apply_diff(_, diff1)`. Delete markers in diff1 produce no atoms in the result, so they should be excluded from matching. Unchanged markers in diff1 reference base atoms — from diff2's perspective, the result atom at that position is the base atom, which the composed diff should reference directly.

**Step 2: Classify each diff2 atom and produce composed atoms.**

For each **matched pair** (diff1_atom, diff2_atom):

| diff1 kind | diff2 kind | Composed result | Rationale |
|-----------|-----------|----------------|-----------|
| Modified (has anchor) | Modified/Replace | Atom at **diff2.position**, anchor = **diff1.anchor**, element = **diff2.element**, flags = **diff2.flags** | Tracks original base position via diff1's anchor; applies diff2's final state |
| Modified (has anchor) | Delete marker | **Delete marker** with anchor = **diff1.anchor** | Deletes the original base atom |
| Modified (has anchor) | Unchanged | **diff1 atom as-is** (copy to composed) | diff2 only references it for bonds; diff1's modification stands |
| Pure addition (no anchor) | Modified/Replace | **Pure addition** at **diff2.position**, element = **diff2.element**, flags = **diff2.flags** | Still a new atom, just with diff2's final state |
| Pure addition (no anchor) | Delete marker | **Omit both** (cancellation) | diff1 adds it, diff2 removes it → net zero |
| Pure addition (no anchor) | Unchanged | **diff1 atom as-is** (copy to composed) | diff2 only references it for bonds |
| Unchanged marker | Modified/Replace | Atom at **diff2.position**, anchor = **diff1.match_pos** | diff1 didn't change the base atom, but diff2 does; anchor points to base position |
| Unchanged marker | Delete marker | **Delete marker** with anchor = **diff1.match_pos** | diff1 didn't touch base atom, diff2 deletes it |
| Unchanged marker | Unchanged | **Omit** (double no-op) | Neither diff touches the atom; no need to reference in composed |

For each **unmatched diff1 atom** (not matched by any diff2 atom):
- Copy to composed diff as-is (diff2 doesn't affect it).

For each **unmatched diff2 atom** (doesn't match any diff1 atom):
- Copy to composed diff as-is (it targets base passthroughs or is a pure addition — semantics preserved).

**Step 3: Compose bonds.**

Build a mapping from (diff1_atom_id → composed_atom_id) and (diff2_atom_id → composed_atom_id).

**Pass A — diff1 bonds:**
For each bond in diff1 between atoms a and b (both present in composed diff):
- Look up the corresponding diff2 atoms (if both endpoints are matched by diff2 atoms).
- If both matched and diff2 has a bond between them:
  - Use diff2's bond order (diff2 overrides diff1).
  - Mark this diff2 bond pair as processed.
- If both matched but diff2 has no bond between them:
  - Keep diff1's bond (diff2 doesn't override it).
- If not both matched by diff2:
  - Keep diff1's bond as-is.
- Exception: if either endpoint was cancelled (pure addition + delete), skip the bond.

**Pass B — diff2 bonds (not yet processed):**
For each bond in diff2 not already processed in Pass A:
- Map both endpoints to composed atom IDs.
- If both endpoints are in the composed diff, add the bond (including BOND_DELETED markers).
- If either endpoint is missing (cancelled or orphaned), skip.

**Why this two-pass approach works:** It mirrors the bond resolution in `apply_diff` — diff1 bonds are analogous to "base bonds" (they represent the state after diff1), and diff2 bonds are the "new diff bonds" that override.

### 1.3 Bond Edge Cases

| diff1 bond | diff2 effect | Composed bond |
|-----------|-------------|--------------|
| A-B single | diff2 changes to double | A'-B' double |
| A-B single | diff2 deletes (BOND_DELETED) | A'-B' BOND_DELETED |
| A-B single | diff2 doesn't touch | A'-B' single |
| A-B BOND_DELETED | diff2 doesn't touch | A'-B' BOND_DELETED |
| *(no bond)* | diff2 adds A'-B' single | A'-B' single |
| A-B single | diff2 deletes atom A | No bond (A deleted) |

### 1.4 Metadata (Flags) Composition

Atom flags (frozen, hybridization override, hydrogen passivation) follow last-writer-wins:
- If diff2 modifies an atom, use diff2's flags.
- If only diff1 modifies it, use diff1's flags.
- Unchanged markers don't carry meaningful flags.

### 1.5 Composing N Diffs

`compose_diffs` for N diffs is a left fold:

```
composed = fold(diffs, |acc, next| compose_two_diffs(acc, next, tolerance))
```

This works because composition is **associative**: `compose(compose(d1, d2), d3)` produces the same diff as `compose(d1, compose(d2, d3))` — both produce a diff equivalent to applying d1, d2, d3 in sequence.

The fold must be left-to-right to maintain the sequential application order.

---

## 2. Handling the Unchanged Marker

The unchanged marker (atomic_number = -1) deserves special attention. In a single diff, it means "I have bond changes involving this base atom, but I'm not modifying the atom itself."

**In diff1:** An unchanged marker at position P means diff1 references a base atom at P for bond changes. In the result of `apply_diff(base, diff1)`, the base atom at P passes through (it's in the result at position P).

**When diff2 matches it:** diff2 sees an atom at position P in the result (the actual base atom, not the marker). If diff2 wants to modify it, the composed diff needs to track back to the base atom. Since the unchanged marker matched by position (the match_pos = anchor or position of the unchanged atom), we use that position as the anchor in the composed diff.

**When diff2 doesn't match it:** The unchanged marker in diff1 was only needed for bond resolution between diff1 and the base. In the composed diff, we need to check: do any composed bonds reference this atom? If so, we need an unchanged marker at the same position. If not, it can be omitted. For simplicity, **always carry through** diff1's unchanged markers that aren't matched by diff2 — the extra marker is harmless (it just produces an `unchanged_references` stat when applied).

---

## 3. Implementation: `compose_diffs` in crystolecule

### 3.1 Function Signature

```rust
// In rust/src/crystolecule/atomic_structure_diff.rs

/// Result of composing two diffs.
pub struct DiffCompositionResult {
    /// The composed diff (is_diff = true).
    pub composed: AtomicStructure,
    /// Statistics about the composition.
    pub stats: DiffCompositionStats,
}

pub struct DiffCompositionStats {
    /// diff1 atoms carried through (not touched by diff2).
    pub diff1_passthrough: u32,
    /// diff2 atoms carried through (not matching any diff1 atom).
    pub diff2_passthrough: u32,
    /// Matched pairs where effects were composed.
    pub composed_pairs: u32,
    /// Cancellations (diff1 add + diff2 delete).
    pub cancellations: u32,
}

/// Composes two diffs into a single diff.
///
/// The composed diff, when applied to any base, produces the same result
/// as applying diff1 then diff2:
///   apply_diff(apply_diff(base, diff1), diff2) == apply_diff(base, composed)
///
/// Both inputs must have is_diff = true.
pub fn compose_two_diffs(
    diff1: &AtomicStructure,
    diff2: &AtomicStructure,
    tolerance: f64,
) -> DiffCompositionResult { ... }

/// Composes a sequence of diffs (left fold of compose_two_diffs).
///
/// Returns None if the slice is empty. Returns a clone of the single diff
/// if the slice has length 1.
pub fn compose_diffs(
    diffs: &[&AtomicStructure],
    tolerance: f64,
) -> Option<DiffCompositionResult> { ... }
```

### 3.2 Implementation Notes

- Reuse `match_diff_atoms` (or a variant) for matching diff2 against diff1 — the same greedy nearest-first algorithm.
- When matching diff2 against diff1, **exclude diff1 delete markers** from the searchable set (they produce no atoms in the result).
- For unchanged markers in diff1: their match position is `anchor_position.unwrap_or(atom.position)`, same as any diff atom.
- The composed diff's `is_diff` flag must be `true`.
- Atom IDs in the composed diff are freshly assigned (start from 1). A mapping from diff1/diff2 IDs to composed IDs is needed for bond resolution.

---

## 4. Node Design: `atom_composediff`

### 4.1 Pin Layout

| Pin | Name | Type | Description |
|-----|------|------|-------------|
| Input 0 | `diffs` | `Array(Atomic)` | Array of atomic diff structures to compose |
| Input 1 | `tolerance` | `Float` | Positional matching tolerance (default 0.1) |
| Output 0 | | `Atomic` | The composed diff (is_diff = true) |

The `diffs` input uses `DataType::Array(Box::new(DataType::Atomic))`, which allows multiple wires to be connected (each contributing a diff). The evaluation order is deterministic (sorted by source node ID).

### 4.2 Evaluation Logic

```rust
fn eval(...) -> EvalOutput {
    // 1. Evaluate diffs array (required)
    let diffs_val = evaluate_arg_required(node, 0);
    let diffs_array = extract_array(diffs_val);

    // 2. Validate: all must be diffs
    let mut diff_refs: Vec<&AtomicStructure> = Vec::new();
    for item in &diffs_array {
        let structure = item.extract_atomic()?;
        if !structure.is_diff() {
            return error("all inputs must be diff structures");
        }
        diff_refs.push(structure);
    }

    // 3. Edge cases
    if diff_refs.is_empty() {
        return error("at least one diff required");
    }
    if diff_refs.len() == 1 {
        return single(diff_refs[0].clone());
    }

    // 4. Get tolerance
    let tolerance = evaluate_or_default(node, 1, self.tolerance);

    // 5. Compose
    let result = compose_diffs(&diff_refs, tolerance)?;

    // 6. Optionally check stats for stale entries
    single(result.composed)
}
```

### 4.3 Node Type Registration

```rust
NodeType {
    name: "atom_composediff".to_string(),
    description: "Composes multiple atomic diffs into a single diff. \
        The composed diff, when applied to a base structure, produces the same result \
        as applying each input diff in sequence."
        .to_string(),
    summary: None,
    category: NodeTypeCategory::AtomicStructure,
    parameters: vec![
        Parameter::new("diffs", DataType::Array(Box::new(DataType::Atomic))),
        Parameter::new("tolerance", DataType::Float),
    ],
    output_pins: OutputPinDefinition::single(DataType::Atomic),
    public: true,
    node_data_creator: || Box::new(AtomComposeDiffData { tolerance: 0.1, error_on_stale: false }),
    node_data_saver: generic_node_data_saver::<AtomComposeDiffData>,
    node_data_loader: generic_node_data_loader::<AtomComposeDiffData>,
}
```

### 4.4 Text Format

```
my_composed = atom_composediff { diffs: [edit1.diff, edit2.diff, edit3.diff] }
```

Or with wired inputs:
```
my_composed = atom_composediff {}
    diffs <- edit1.diff
    diffs <- edit2.diff
    diffs <- edit3.diff
```

---

## 5. Testing Strategy

The correctness invariant provides a powerful testing framework: for any base and sequence of diffs, verify that sequential application equals composed application.

All crystolecule tests in `rust/tests/crystolecule/compose_diffs_test.rs`.
All node tests in `rust/tests/structure_designer/atom_composediff_test.rs`.

### 5.1 Test Helper: `assert_structures_equal`

A helper that compares two `AtomicStructure` results for semantic equality:
- Same atom count
- For each atom in result A, there exists an atom in result B at the same position (within tolerance), same atomic_number, same flags
- Same bond count; for each bond in A, there exists a matching bond in B (mapped via position-matched atom pairs), same bond order

This is position-based comparison (not ID-based), since composed vs sequential application may assign different IDs.

```rust
fn assert_structures_equal(a: &AtomicStructure, b: &AtomicStructure, tolerance: f64) {
    assert_eq!(a.num_atoms(), b.num_atoms(), "atom count mismatch");
    // Build position-based bijection between a and b atoms
    // Assert each matched pair has same atomic_number and flags
    // Assert bond sets are identical under the bijection
}
```

### 5.2 Test Helper: `assert_compose_equivalence`

The core equivalence check used by most tests:

```rust
/// Verifies: apply_diff(apply_diff(base, diff1), diff2) == apply_diff(base, compose(diff1, diff2))
fn assert_compose_equivalence(
    base: &AtomicStructure,
    diffs: &[&AtomicStructure],
    tolerance: f64,
) {
    // Sequential application
    let mut sequential = base.clone();
    for diff in diffs {
        sequential = apply_diff(&sequential, diff, tolerance).result;
    }

    // Composed application
    let composed = compose_diffs(diffs, tolerance).unwrap();
    let composed_result = apply_diff(base, &composed.composed, tolerance).result;

    assert_structures_equal(&sequential, &composed_result, tolerance);
}
```

---

### 5.3 Identity and Trivial Cases

#### `compose_empty_diff_is_identity`

Composing any diff with an empty diff (no atoms) should return a diff equivalent to the original.

```
diff1: C at (0,0,0) [pure addition]
diff2: (empty)
tolerance: 0.1

composed: C at (0,0,0) [pure addition]
```

Verify: `apply_diff(base, composed) == apply_diff(base, diff1)` for some base.

#### `compose_single_diff`

`compose_diffs([diff1])` returns a clone of diff1.

#### `compose_two_empty_diffs`

`compose_diffs([empty, empty])` returns an empty diff. Applied to any base, produces the base unchanged.

---

### 5.4 Pure Addition Tests

#### `compose_two_additions_no_overlap`

Both diffs add atoms at different positions. Composed diff contains all additions.

```
Base: C at (0,0,0)

diff1: N at (2,0,0) [pure addition]
diff2: O at (0,2,0) [pure addition]

composed should contain: N at (2,0,0), O at (0,2,0) [both pure additions]

Sequential:  base + diff1 → {C(0,0,0), N(2,0,0)} + diff2 → {C(0,0,0), N(2,0,0), O(0,2,0)}
Composed:    base + composed → {C(0,0,0), N(2,0,0), O(0,2,0)}  ✓
```

#### `compose_addition_with_bond`

diff1 adds two atoms with a bond. diff2 adds a third atom bonded to one of them.

```
Base: (empty)

diff1: C at (0,0,0), C at (1.54,0,0), bond(C1-C2, single)  [both pure additions]
diff2: H at (0,-1.09,0) [pure addition], unchanged at (0,0,0), bond(unchanged-H, single)
       (diff2 adds H bonded to the C that diff1 added at origin)

diff2's unchanged at (0,0,0) matches diff1's C at (0,0,0).
diff1 pure addition + diff2 unchanged → diff1 atom as-is (C at origin).
diff2's H is unmatched → passes through as pure addition.
Bond from diff2 (unchanged→H) maps to composed (C→H).

composed: C at (0,0,0), C at (1.54,0,0), H at (0,-1.09,0)
          bonds: C-C single (from diff1), C-H single (from diff2)

Sequential: {} → {C(0), C(1.54), bond C-C} → {C(0), C(1.54), H(0,-1.09), bonds: C-C, C-H}
Composed:   {} + composed → {C(0), C(1.54), H(0,-1.09), bonds: C-C, C-H}  ✓
```

---

### 5.5 Pure Deletion Tests

#### `compose_two_deletions_different_atoms`

```
Base: C at (0,0,0), N at (2,0,0), O at (0,2,0)

diff1: delete marker at (0,0,0) [deletes C]
diff2: delete marker at (0,2,0) [deletes O — this is a base passthrough position]

diff2 does NOT match diff1 (diff1's delete marker is excluded from matching).
Both pass through to composed.

composed: delete marker at (0,0,0), delete marker at (0,2,0)

Sequential:  {C,N,O} → {N,O} → {N}
Composed:    {C,N,O} + composed → {N}  ✓
```

#### `compose_delete_same_atom_twice`

diff1 deletes atom at P. diff2 also has a delete marker at P.

```
Base: C at (0,0,0)

diff1: delete marker at (0,0,0)
diff2: delete marker at (0,0,0)

diff1's delete marker is excluded from matching, so diff2's delete marker is unmatched.
composed: both delete markers present.

Sequential: {C} → {} → {} (diff2's marker is unmatched_delete_markers, no-op)
Composed:   {C} + composed → {} (diff1's marker deletes C, diff2's marker is unmatched, no-op)  ✓

Stats when applied: unmatched_delete_markers = 1 in both cases.
```

---

### 5.6 Cancellation Tests

#### `compose_add_then_delete_cancels`

diff1 adds an atom, diff2 deletes it. Net effect: nothing.

```
Base: C at (0,0,0)

diff1: N at (2,0,0) [pure addition]
diff2: delete marker at (2,0,0) [matches the N added by diff1]

diff2's delete marker matches diff1's N (distance 0, within tolerance).
diff1 atom is pure addition + diff2 is delete → CANCELLATION. Omit both.

composed: (empty diff)

Sequential: {C} → {C, N} → {C}
Composed:   {C} + empty → {C}  ✓
```

#### `compose_add_then_delete_with_bond_cleanup`

diff1 adds atoms A and B with bond A-B. diff2 deletes A. Bond must also disappear.

```
Base: (empty)

diff1: C at (0,0,0) [add], C at (1.54,0,0) [add], bond(C1-C2, single)
diff2: delete marker at (0,0,0) [matches C1 from diff1]

Cancellation: C1 from diff1 is cancelled.
C2 from diff1 passes through (unmatched by diff2).
Bond C1-C2: C1's endpoint is cancelled → bond is dropped.

composed: C at (1.54,0,0) [pure addition], no bonds

Sequential: {} → {C(0,0,0), C(1.54,0,0), bond} → {C(1.54,0,0)}
Composed:   {} + composed → {C(1.54,0,0)}  ✓
```

#### `compose_add_then_delete_partial_cancellation`

diff1 adds 3 atoms. diff2 deletes 1 of them. Only the deleted one cancels.

```
Base: (empty)

diff1: C at (0,0,0), N at (2,0,0), O at (4,0,0) [all pure additions]
diff2: delete marker at (2,0,0) [matches N]

composed: C at (0,0,0), O at (4,0,0) [pure additions, N cancelled]

Sequential: {} → {C, N, O} → {C, O}
Composed:   {} + composed → {C, O}  ✓
```

---

### 5.7 Chained Modification Tests

#### `compose_move_then_move`

diff1 moves atom from A to B. diff2 moves it from B to C. Composed should move A→C directly.

```
Base: C at (0,0,0), N at (5,0,0)

diff1: C at (1,0,0) with anchor (0,0,0) [move C from origin to (1,0,0)]
diff2: C at (2,0,0) with anchor (1,0,0) [move C from (1,0,0) to (2,0,0)]

diff2 matches diff1's C at (1,0,0) (within tolerance of anchor (1,0,0)).
diff1 is modified (has anchor at (0,0,0)). diff2 is modify.
Composed: C at (2,0,0) with anchor (0,0,0).

composed: C at (2,0,0) anchor=(0,0,0)

Sequential: {C(0,0,0), N(5,0,0)} → {C(1,0,0), N(5,0,0)} → {C(2,0,0), N(5,0,0)}
Composed:   {C(0,0,0), N(5,0,0)} + composed → {C(2,0,0), N(5,0,0)}  ✓
```

#### `compose_move_then_delete`

diff1 moves atom. diff2 deletes it at new position. Composed: delete at original position.

```
Base: C at (0,0,0)

diff1: C at (1,0,0) with anchor (0,0,0) [move]
diff2: delete marker at (1,0,0) [delete the moved atom]

diff2 matches diff1's atom at (1,0,0).
diff1 modified + diff2 delete → delete marker with diff1's anchor.

composed: delete marker, anchor=(0,0,0) — or equivalently positioned at (0,0,0)

Sequential: {C(0,0,0)} → {C(1,0,0)} → {}
Composed:   {C(0,0,0)} + composed → {}  ✓
```

#### `compose_replace_then_replace`

diff1 replaces C with N (same position). diff2 replaces N with O. Composed: C→O.

```
Base: C at (0,0,0)

diff1: N at (0,0,0) anchor=(0,0,0) [replace C→N, same position]
diff2: O at (0,0,0) anchor=(0,0,0) [replace N→O]

diff2 matches diff1's N at (0,0,0).
Both are modified (have anchors). Composed: O at (0,0,0), anchor=(0,0,0).

Sequential: {C(0,0,0)} → {N(0,0,0)} → {O(0,0,0)}
Composed:   {C(0,0,0)} + composed → {O(0,0,0)}  ✓
```

#### `compose_move_then_replace`

diff1 moves atom. diff2 changes its element at new position.

```
Base: C at (0,0,0)

diff1: C at (1,0,0) anchor=(0,0,0) [move]
diff2: N at (1,0,0) anchor=(1,0,0) [replace C→N at (1,0,0)]

diff2 matches diff1 at (1,0,0).
Composed: N at (1,0,0) anchor=(0,0,0) [move + replace combined]

Sequential: {C(0,0,0)} → {C(1,0,0)} → {N(1,0,0)}
Composed:   {C(0,0,0)} + composed → {N(1,0,0)}  ✓
```

#### `compose_add_then_move`

diff1 adds a new atom. diff2 moves it. Composed: add at final position (no anchor).

```
Base: C at (0,0,0)

diff1: N at (2,0,0) [pure addition, no anchor]
diff2: N at (3,0,0) anchor=(2,0,0) [move from (2,0,0) to (3,0,0)]

diff2 matches diff1's N at (2,0,0).
diff1 is pure addition + diff2 is modify → pure addition at diff2's position.

composed: N at (3,0,0) [pure addition, NO anchor]

Sequential: {C(0,0,0)} → {C, N(2,0,0)} → {C, N(3,0,0)}
Composed:   {C(0,0,0)} + composed → {C, N(3,0,0)}  ✓

Note: no anchor in composed because it's still a pure addition (doesn't track a base atom).
```

#### `compose_add_then_replace`

diff1 adds C. diff2 replaces it with N at the same position.

```
Base: (empty)

diff1: C at (1,0,0) [pure addition]
diff2: N at (1,0,0) anchor=(1,0,0) [replace]

diff2 matches diff1 at (1,0,0).
diff1 pure addition + diff2 modify → pure addition at diff2's position, diff2's element.

composed: N at (1,0,0) [pure addition, no anchor]

Sequential: {} → {C(1,0,0)} → {N(1,0,0)}
Composed:   {} + composed → {N(1,0,0)}  ✓
```

#### `compose_move_preserves_bonds_to_other_atoms`

diff1 moves atom A (bonded to base atom B). diff2 moves A again. The composed diff must still have a bond-relevant reference so that A-B bond survives apply_diff bond resolution.

```
Base: C at (0,0,0), C at (1.54,0,0), bond(C1-C2, single)

diff1: C at (0.5,0,0) anchor=(0,0,0) [move C1 slightly]
diff2: C at (1.0,0,0) anchor=(0.5,0,0) [move C1 again]

composed: C at (1.0,0,0) anchor=(0,0,0)

When composed is applied to base: C1 matches via anchor (0,0,0). Result has C1 at (1.0,0,0)
and C2 at (1.54,0,0). Base bond C1-C2 survives (both in result, diff has no bond override).

Sequential: {C(0), C(1.54), bond} → {C(0.5), C(1.54), bond} → {C(1.0), C(1.54), bond}
Composed:   {C(0), C(1.54), bond} + composed → {C(1.0), C(1.54), bond}  ✓
```

---

### 5.8 Unchanged Marker Tests

#### `compose_unchanged_then_modify`

diff1 uses an unchanged marker (for bond changes). diff2 modifies the same atom.

```
Base: C at (0,0,0), C at (1.54,0,0), bond(C1-C2, single)

diff1: unchanged at (0,0,0), N at (3,0,0) [add], bond(unchanged-N, single)
       (diff1 adds a bond from base C to new N, using unchanged marker as reference)

diff2: Si at (0,0,0) anchor=(0,0,0) [replace C with Si]

diff2 matches diff1's unchanged marker at (0,0,0).
diff1 unchanged + diff2 modify → composed atom: Si at (0,0,0) anchor=(0,0,0).
diff1's N at (3,0,0) passes through as pure addition.
Bond between unchanged→N in diff1: now becomes bond between Si→N in composed.

composed: Si at (0,0,0) anchor=(0,0,0), N at (3,0,0) [add], bond(Si-N, single)

Sequential: {C(0), C(1.54), bond C-C}
  → diff1: {C(0), C(1.54), N(3), bonds: C1-C2, C1-N}
  → diff2: {Si(0), C(1.54), N(3), bonds: Si-C2, Si-N}
Composed:   {C(0), C(1.54)} + composed → {Si(0), C(1.54), N(3), bonds: Si-C2, Si-N}  ✓

Note: The base C1-C2 bond survives in both cases because apply_diff bond resolution
preserves base bonds when at most one endpoint is in the diff, or when the diff doesn't
override it. In the composed case, Si matches C1 (via anchor), C2 is a passthrough. The
base bond survives because the composed diff has no bond between Si and C2.
```

#### `compose_unchanged_then_delete`

diff1 has an unchanged marker at P (for bond purposes). diff2 deletes the atom at P.

```
Base: C at (0,0,0), C at (1.54,0,0)

diff1: unchanged at (0,0,0), H at (0,-1.09,0) [add], bond(unchanged-H, single)
diff2: delete marker at (0,0,0)

diff2 matches diff1's unchanged marker.
diff1 unchanged + diff2 delete → delete marker with anchor at (0,0,0) in composed.
diff1's H addition passes through.
Bond unchanged→H: endpoint deleted → bond dropped.

composed: delete marker anchor=(0,0,0), H at (0,-1.09,0) [add], no bonds

Sequential: {C(0), C(1.54)}
  → diff1: {C(0), C(1.54), H(-1.09), bonds: C-C, C-H}
  → diff2: {C(1.54), H(-1.09), no bond between them}
Composed:   {C(0), C(1.54)} + composed → {C(1.54), H(-1.09)}  ✓

Note: H has no bond to anything in either case (the C it was bonded to got deleted).
```

#### `compose_unchanged_then_unchanged_with_bonds`

Both diffs only reference the atom for bond changes. The composed diff must carry
unchanged markers and compose the bond effects.

```
Base: C at (0,0,0), C at (1.54,0,0), C at (3.08,0,0), bonds: C1-C2, C2-C3

diff1: unchanged at (0,0,0), unchanged at (1.54,0,0), bond(0-1.54, BOND_DELETED)
       (diff1 deletes the C1-C2 bond)

diff2: unchanged at (0,0,0), N at (0,2,0) [add], bond(0-N, single)
       (diff2 adds a bond from C1 to a new N)

diff2's unchanged at (0,0,0) matches diff1's unchanged at (0,0,0).
Both unchanged → omit (double no-op on the atom itself).

BUT: we need to carry bonds through. diff1 has BOND_DELETED between (0,0,0) and (1.54,0,0).
diff2 has a new bond from (0,0,0) to N.

For the bond deletion: diff1's unchanged at (1.54,0,0) is unmatched by diff2 → passes through.
diff1's BOND_DELETED between the two unchanged markers passes through.
diff2's bond between unchanged and N: unchanged was omitted (double no-op), BUT the
bond needs to reference the base atom. We need an unchanged marker for bond endpoints.

Revised: the unchanged+unchanged → omit rule only omits if no bonds reference the atom.
If bonds reference it, keep ONE unchanged marker.

composed: unchanged at (0,0,0), unchanged at (1.54,0,0),
          N at (0,2,0) [add],
          bond(0-1.54, BOND_DELETED), bond(0-N, single)

Sequential: {C(0), C(1.54), C(3.08), bonds: C1-C2, C2-C3}
  → diff1: {C(0), C(1.54), C(3.08), bonds: C2-C3}  [C1-C2 deleted]
  → diff2: {C(0), C(1.54), C(3.08), N(0,2,0), bonds: C2-C3, C1-N}
Composed:   {C(0), C(1.54), C(3.08)} + composed
  → C1 matched by unchanged, C2 matched by unchanged, C3 passthrough, N added
  → base bond C1-C2: both matched by diff, diff has BOND_DELETED → deleted
  → base bond C2-C3: only C2 matched → survives
  → diff bond C1-N: both in result → added
  → {C(0), C(1.54), C(3.08), N(0,2,0), bonds: C2-C3, C1-N}  ✓
```

---

### 5.9 Bond Composition Tests

#### `compose_add_bond_then_delete_bond`

diff1 adds a bond between two base atoms. diff2 deletes it. Net: no new bond.

```
Base: C at (0,0,0), C at (1.54,0,0) [no bond between them]

diff1: unchanged at (0,0,0), unchanged at (1.54,0,0), bond(single)
diff2: unchanged at (0,0,0), unchanged at (1.54,0,0), bond(BOND_DELETED)

diff2 unchanged markers match diff1 unchanged markers.
Bond composition: diff1 has bond(single), diff2 has bond(BOND_DELETED) → diff2 overrides.

composed: unchanged at (0,0,0), unchanged at (1.54,0,0), bond(BOND_DELETED)

The base didn't have this bond. diff1 adds it, diff2 removes it. Net: no bond.
In the composed diff, BOND_DELETED applied to a non-existent base bond → no-op. Correct.

Sequential: {C, C, no bond} → {C, C, bond} → {C, C, no bond}
Composed:   {C, C, no bond} + composed → {C, C, no bond}  ✓
```

#### `compose_add_bond_then_change_order`

diff1 adds single bond. diff2 changes to double.

```
Base: C at (0,0,0), C at (1.54,0,0) [no bond]

diff1: unchanged(0,0,0), unchanged(1.54,0,0), bond(single)
diff2: unchanged(0,0,0), unchanged(1.54,0,0), bond(double)

Bond composition: diff2 overrides diff1 → composed bond is double.

composed: unchanged(0,0,0), unchanged(1.54,0,0), bond(double)

Sequential: {C, C} → {C, C, single bond} → {C, C, double bond}
Composed:   {C, C} + composed → {C, C, double bond}  ✓
```

#### `compose_delete_base_bond_passthrough`

diff1 deletes a base bond. diff2 doesn't touch it. Composed should still delete.

```
Base: C at (0,0,0), C at (1.54,0,0), bond(single)

diff1: unchanged(0,0,0), unchanged(1.54,0,0), bond(BOND_DELETED)
diff2: (empty, or unrelated changes)

diff1's atoms unmatched by diff2 → pass through.
diff1's BOND_DELETED passes through.

composed: unchanged(0,0,0), unchanged(1.54,0,0), bond(BOND_DELETED)

Sequential: {C, C, bond} → {C, C} → {C, C}
Composed:   {C, C, bond} + composed → {C, C}  ✓
```

#### `compose_bond_between_added_atoms`

diff1 adds two atoms (no bond). diff2 adds a bond between them.

```
Base: (empty)

diff1: C at (0,0,0), C at (1.54,0,0) [both pure additions, no bond]
diff2: unchanged at (0,0,0), unchanged at (1.54,0,0), bond(single)

diff2's unchanged markers match diff1's additions.
diff1 add + diff2 unchanged → diff1 atoms as-is.
diff2's bond → composed bond.

composed: C at (0,0,0), C at (1.54,0,0), bond(single) [both pure additions]

Sequential: {} → {C, C} → {C, C, bond}
Composed:   {} + composed → {C, C, bond}  ✓
```

#### `compose_bond_with_endpoint_cancelled`

diff1 adds atom A and bonds it to added atom B. diff2 deletes atom A. Bond must disappear.

```
Base: (empty)

diff1: C at (0,0,0), N at (2,0,0), bond(C-N, single) [both additions]
diff2: delete marker at (0,0,0)

Cancellation: C at (0,0,0) cancelled (add + delete).
N at (2,0,0) passes through.
Bond C-N: endpoint C cancelled → bond dropped from composed.

composed: N at (2,0,0) [pure addition, no bonds]

Sequential: {} → {C, N, bond} → {N}
Composed:   {} + composed → {N}  ✓
```

#### `compose_bond_between_mixed_origins`

diff1 adds atom A. diff2 adds bond from A to a base atom B (which diff1 doesn't reference).

```
Base: C at (5,0,0)

diff1: N at (0,0,0) [pure addition]
diff2: unchanged at (0,0,0), unchanged at (5,0,0), bond(single)
       (diff2 bonds the added N to the base C)

diff2's unchanged at (0,0,0) matches diff1's N.
diff1 add + diff2 unchanged → N passes through.
diff2's unchanged at (5,0,0) is unmatched → passes through.
diff2's bond → composed bond.

composed: N at (0,0,0) [add], unchanged at (5,0,0), bond(N-unchanged, single)

Sequential: {C(5)} → {C(5), N(0)} → {C(5), N(0), bond}
Composed:   {C(5)} + composed → {C(5), N(0), bond}  ✓
```

---

### 5.10 Metadata (Flags) Tests

#### `compose_metadata_last_writer_wins`

diff1 sets frozen flag on a moved atom. diff2 clears it.

```
Base: C at (0,0,0)

diff1: C at (1,0,0) anchor=(0,0,0), flags: frozen=true [move + set frozen]
diff2: C at (1,0,0) anchor=(1,0,0), flags: frozen=false [in-place, clear frozen]

diff2 matches diff1 at (1,0,0).
Composed: C at (1,0,0) anchor=(0,0,0), flags: frozen=false (diff2's flags win).

Sequential: {C(0)} → {C(1), frozen} → {C(1), not frozen}
Composed:   {C(0)} + composed → {C(1), not frozen}  ✓
```

#### `compose_metadata_diff1_only`

diff1 sets frozen. diff2 doesn't touch the atom. diff1's flags survive.

```
Base: C at (0,0,0)

diff1: C at (0,0,0) anchor=(0,0,0), flags: frozen=true
diff2: (doesn't touch this atom)

diff1 passes through to composed.

composed: C at (0,0,0) anchor=(0,0,0), flags: frozen=true

Sequential: {C} → {C, frozen} → {C, frozen}
Composed:   {C} + composed → {C, frozen}  ✓
```

#### `compose_metadata_on_pure_addition`

diff1 adds atom with hybridization override. diff2 changes hybridization.

```
Base: (empty)

diff1: C at (1,0,0), flags: hybridization=Sp2 [pure addition]
diff2: C at (1,0,0) anchor=(1,0,0), flags: hybridization=Sp3 [replace-in-place]

Composed: C at (1,0,0), flags: hybridization=Sp3 [pure addition, diff2's flags]

Sequential: {} → {C, Sp2} → {C, Sp3}
Composed:   {} + composed → {C, Sp3}  ✓
```

---

### 5.11 Multi-Diff (3+) Composition Tests

#### `compose_three_diffs_sequential`

Three diffs applied in sequence. Verify all three compose correctly.

```
Base: C at (0,0,0), C at (3,0,0)

diff1: N at (1,0,0) [pure addition]
diff2: O at (2,0,0) [pure addition]
diff3: delete marker at (3,0,0) [deletes second base C]

compose_diffs([diff1, diff2, diff3]):
  step 1: compose(diff1, diff2) → {N at (1,0,0), O at (2,0,0)} [no overlap]
  step 2: compose(above, diff3) → {N at (1,0,0), O at (2,0,0), delete at (3,0,0)}

Sequential: {C(0),C(3)} → {C(0),C(3),N(1)} → {C(0),C(3),N(1),O(2)} → {C(0),N(1),O(2)}
Composed:   {C(0),C(3)} + composed → {C(0),N(1),O(2)}  ✓
```

#### `compose_three_diffs_chained_moves`

An atom is moved three times in sequence.

```
Base: C at (0,0,0)

diff1: C at (1,0,0) anchor=(0,0,0) [move 0→1]
diff2: C at (2,0,0) anchor=(1,0,0) [move 1→2]
diff3: C at (3,0,0) anchor=(2,0,0) [move 2→3]

compose(diff1, diff2): C at (2,0,0) anchor=(0,0,0) [move 0→2]
compose(above, diff3): C at (3,0,0) anchor=(0,0,0) [move 0→3]

Sequential: {C(0)} → {C(1)} → {C(2)} → {C(3)}
Composed:   {C(0)} + composed → {C(3)}  ✓
```

#### `compose_three_diffs_add_move_delete`

diff1 adds atom. diff2 moves it. diff3 deletes it. Net: nothing.

```
Base: C at (0,0,0)

diff1: N at (1,0,0) [pure addition]
diff2: N at (2,0,0) anchor=(1,0,0) [move]
diff3: delete marker at (2,0,0)

compose(diff1, diff2): N at (2,0,0) [pure addition at new position]
compose(above, diff3): CANCELLATION (add + delete)

composed: (empty)

Sequential: {C} → {C, N(1)} → {C, N(2)} → {C}
Composed:   {C} + empty → {C}  ✓
```

#### `compose_three_diffs_interleaved_operations`

Multiple atoms with interleaved operations across all three diffs.

```
Base: C at (0,0,0), N at (2,0,0), O at (4,0,0), Si at (6,0,0)
      bonds: C-N, N-O, O-Si

diff1: delete marker at (0,0,0) [delete C],
       N at (2.5,0,0) anchor=(2,0,0) [move N slightly]

diff2: O at (4,0,0) anchor=(4,0,0) flags:frozen [set frozen on O],
       P at (8,0,0) [add phosphorus]

diff3: delete marker at (2.5,0,0) [delete the moved N],
       Si at (6,1,0) anchor=(6,0,0) [move Si],
       H at (8,1,0) [add H near P for bond]

compose(diff1, diff2):
  - diff2 doesn't match diff1's atoms (different positions, beyond tolerance)
  - All pass through: delete(0,0,0), N at (2.5) anchor=(2), O at (4) anchor=(4) frozen, P at (8)

compose(above, diff3):
  - diff3's delete at (2.5) matches composed N at (2.5) → modified+delete → delete anchor=(2,0,0)
  - diff3's Si at (6,1,0) anchor=(6,0,0) doesn't match any composed atom → passes through
  - diff3's H at (8,1,0) doesn't match (too far from P at 8,0,0 with tol=0.1) → passes through

composed: delete(0,0,0), delete anchor=(2,0,0), O at (4) anchor=(4) frozen,
          P at (8) [add], Si at (6,1,0) anchor=(6,0,0), H at (8,1,0) [add]

Sequential:
  {C(0), N(2), O(4), Si(6), bonds: C-N, N-O, O-Si}
  → diff1: {N(2.5), O(4), Si(6), bonds: N-O(?), O-Si}  [C deleted, N moved]
  → diff2: {N(2.5), O(4,frozen), Si(6), P(8), bonds: ...}
  → diff3: {O(4,frozen), Si(6,1,0), P(8), H(8,1,0)}
Composed: should match ✓

Verify with assert_compose_equivalence.
```

---

### 5.12 Edge Cases

#### `compose_diff2_targets_base_passthrough`

diff2 modifies an atom that diff1 doesn't touch at all.

```
Base: C at (0,0,0), N at (5,0,0)

diff1: O at (10,0,0) [pure addition — doesn't touch either base atom]
diff2: Si at (5,0,0) anchor=(5,0,0) [replace N with Si]

diff2's anchor (5,0,0) doesn't match any diff1 atom.
diff2 is unmatched → passes through to composed as-is.

composed: O at (10,0,0) [add], Si at (5,0,0) anchor=(5,0,0) [replace]

Sequential: {C(0), N(5)} → {C(0), N(5), O(10)} → {C(0), Si(5), O(10)}
Composed:   {C(0), N(5)} + composed → {C(0), Si(5), O(10)}  ✓
```

#### `compose_diff2_orphaned_delete_passthrough`

diff2 has a delete marker at a position that doesn't match any diff1 atom. It targets a base atom.

```
Base: C at (0,0,0), N at (5,0,0)

diff1: H at (2,0,0) [pure addition]
diff2: delete marker at (5,0,0) [targets N in base, doesn't match diff1]

diff2 unmatched → passes through.

composed: H at (2,0,0) [add], delete marker at (5,0,0) [delete]

Sequential: {C, N} → {C, N, H} → {C, H}
Composed:   {C, N} + composed → {C, H}  ✓
```

#### `compose_near_tolerance_boundary`

Two atoms in diff1 are close together. diff2 atom is near the tolerance boundary. Tests greedy matching.

```
tolerance = 0.1

diff1: C at (0,0,0), N at (0.15,0,0)  [both pure additions]
diff2: delete marker at (0.05,0,0)

diff2's delete marker: match position = (0.05,0,0).
  - Distance to C(0,0,0) = 0.05 < 0.1 ✓
  - Distance to N(0.15,0,0) = 0.10 = boundary (not strictly less) ✗

Greedy nearest-first: C is closer → matched.
Cancellation: C cancelled.

composed: N at (0.15,0,0) [add]

Verify with sequential application using the same tolerance.
```

#### `compose_all_diff1_atoms_cancelled`

Every atom in diff1 is cancelled by diff2.

```
diff1: C at (0,0,0), N at (2,0,0) [both pure additions]
diff2: delete at (0,0,0), delete at (2,0,0)

Both cancelled.

composed: (empty diff)

Sequential: base → {base + C, N} → {base}
Composed:   base + empty → base  ✓
```

#### `compose_diff1_delete_not_matchable`

Verifies that diff1 delete markers are excluded from matching.

```
Base: C at (0,0,0)

diff1: delete marker at (0,0,0) [deletes C]
diff2: N at (0,0,0) [pure addition at same position]

diff1's delete marker is excluded from matching.
diff2's N at (0,0,0) is unmatched → passes through as pure addition.
diff1's delete marker also passes through.

composed: delete marker at (0,0,0), N at (0,0,0) [add]

Sequential: {C} → {} → {N(0,0,0)}
Composed:   {C} + composed: delete matches C → deleted. N is pure addition → added.
            Result: {N(0,0,0)}  ✓

This is a subtle but important case: diff1 deletes C, diff2 adds N at the exact same
position. The composed diff must contain both: the delete (to remove base C) and the
addition (to add new N). When applied, the delete marker matches C and removes it, then
N is added as a pure addition (no anchor). The greedy matching ensures the delete marker
claims C first.

But wait: will the delete marker and N both try to match C? The delete marker is at (0,0,0)
and N is at (0,0,0). In apply_diff, the greedy matching would have both candidates at
distance 0 from base C. Which one claims it?

This depends on atom ID ordering in the composed diff. The delete marker should be processed
first (smaller ID if we add diff1 atoms first). After it claims C, N is unmatched → added
as a pure addition. This is correct.

Important: atom ordering in the composed diff matters for greedy matching. diff1 atoms
should generally get lower IDs than diff2 atoms to ensure delete markers from diff1 match
base atoms before diff2 additions at the same position.
```

---

### 5.13 Equivalence Property Tests (Complex Scenarios)

These tests use realistic molecular fragments to validate the full algorithm end-to-end.
Each test calls `assert_compose_equivalence` to verify the invariant.

#### `equivalence_diamond_fragment_two_edits`

1. Build a small diamond fragment base (4 C atoms in tetrahedral arrangement with bonds).
2. diff1: move one atom by 0.5 Å, add an H at a dangling bond.
3. diff2: delete another atom, change a bond order.
4. `assert_compose_equivalence(base, [diff1, diff2], 0.1)`

#### `equivalence_linear_chain_mixed_operations`

1. Base: 6 C atoms in a chain with single bonds: C1-C2-C3-C4-C5-C6.
2. diff1: delete C2, move C4 to new position, add N bonded to C1.
3. diff2: replace C1 with Si (element change), delete N (cancels diff1's addition), add double bond C5-C6.
4. `assert_compose_equivalence(base, [diff1, diff2], 0.1)`

#### `equivalence_bond_heavy`

1. Base: 4 atoms in a ring (C1-C2-C3-C4-C1), all single bonds + C1-C3 cross bond.
2. diff1: delete C1-C3 bond, change C2-C3 to double, add N bonded to C4.
3. diff2: change C2-C3 back to single, add C1-C3 bond (re-add), delete C4-N bond.
4. `assert_compose_equivalence(base, [diff1, diff2], 0.1)`

#### `equivalence_three_diffs_all_operation_types`

1. Base: 8-atom structure with various elements and bonds.
2. diff1: 2 additions, 1 deletion, 1 move.
3. diff2: modify one of diff1's additions (element change), delete another base atom, add bond.
4. diff3: move an atom added by diff1 (that diff2 didn't touch), add 2 new bonds, set frozen flag.
5. `assert_compose_equivalence(base, [diff1, diff2, diff3], 0.1)`

#### `equivalence_pure_bond_diffs`

All diffs only contain unchanged markers and bond changes (no atom adds/deletes/moves).

1. Base: 5 atoms with bonds A-B, B-C, C-D, D-E.
2. diff1: delete bond A-B, add bond A-C (single), change B-C to double.
3. diff2: change A-C to triple (override diff1), delete D-E, add bond A-E.
4. `assert_compose_equivalence(base, [diff1, diff2], 0.1)`

#### `equivalence_large_structure_sparse_diffs`

Stress test with many base atoms but small diffs.

1. Base: 50 atoms in a grid pattern with bonds.
2. diff1: move 2 atoms, add 1 atom.
3. diff2: delete 1 atom, modify 1 bond.
4. `assert_compose_equivalence(base, [diff1, diff2], 0.1)`
5. Verify that the composed diff is small (most base atoms are passthroughs, not in the diff).

---

### 5.14 Composed Diff Structure Tests

These tests verify properties of the composed diff itself (not just the application result).

#### `composed_diff_is_diff`

The composed result always has `is_diff() == true`.

#### `composed_diff_has_correct_anchors`

After composing diff1 (move A→B) and diff2 (move B→C), the composed diff should have an atom at C with anchor at A, not at B.

#### `composed_diff_no_orphan_bonds`

Bonds in the composed diff should only reference atom IDs that exist in the composed diff.

#### `composed_diff_cancellation_is_clean`

After cancellation (add + delete), the cancelled atom should not appear in the composed diff at all — no delete marker, no atom, no bonds referencing it.

#### `composed_stats_are_accurate`

Verify `DiffCompositionStats` fields: `diff1_passthrough`, `diff2_passthrough`, `composed_pairs`, `cancellations` match the expected counts for each test.

---

### 5.15 Node-Level Tests

Located in `rust/tests/structure_designer/atom_composediff_test.rs`.

#### `atom_composediff_basic_two_diffs`

Build a node network:
```
atom_fill → atom_edit1 (adds 2 atoms)
atom_fill → atom_edit2 (deletes 1 atom)
atom_edit1.diff ──┐
atom_edit2.diff ──┼──▶ atom_composediff → apply_diff(base=atom_fill, diff=composed)
```

Compare the apply_diff result with the chained path:
```
atom_fill → atom_edit1 → atom_edit2
```

Both should produce identical structures.

#### `atom_composediff_equivalence_with_chained_apply_diff`

Full pipeline comparison at node level with 3 atom_edit nodes.
Evaluate both paths, `assert_structures_equal`.

#### `atom_composediff_single_input`

Single diff wired in → output equals the input diff.

#### `atom_composediff_error_non_diff_input`

Wire a non-diff atomic structure (e.g., from atom_fill directly) → expect error message containing "diff".

#### `atom_composediff_empty_input`

No wires connected to `diffs` pin → expect error message.

#### `atom_composediff_text_format_roundtrip`

Serialize network containing `atom_composediff` to text format and parse back.

### 5.16 Snapshot Tests

Add `atom_composediff` to the node snapshot test suite (`node_snapshot_test.rs`).

---

## 6. Implementation Plan

### Phase 1: Core Algorithm

1. Implement `compose_two_diffs` in `rust/src/crystolecule/atomic_structure_diff.rs`
2. Implement `compose_diffs` (N-ary fold wrapper)
3. Write unit tests in `rust/tests/crystolecule/compose_diffs_test.rs`
4. Write equivalence property tests

### Phase 2: Node Implementation

1. Create `rust/src/structure_designer/nodes/atom_composediff.rs`
2. Register in `nodes/mod.rs` and `node_type_registry.rs`
3. Implement `AtomComposeDiffData` with `NodeData` trait
4. Add node-level tests
5. Add snapshot test

### Phase 3: Flutter Integration

1. Run `flutter_rust_bridge_codegen generate` (if any API types changed)
2. Verify the node appears in the Flutter UI node palette
3. Test wiring atom_edit .diff outputs into the compose node

---

## 7. File Manifest

| File | Action | Description |
|------|--------|-------------|
| `rust/src/crystolecule/atomic_structure_diff.rs` | Modify | Add `compose_two_diffs`, `compose_diffs`, stat types |
| `rust/src/structure_designer/nodes/atom_composediff.rs` | Create | Node implementation |
| `rust/src/structure_designer/nodes/mod.rs` | Modify | Add `pub mod atom_composediff;` |
| `rust/src/structure_designer/node_type_registry.rs` | Modify | Register `atom_composediff` |
| `rust/tests/crystolecule/compose_diffs_test.rs` | Create | Crystolecule-level tests |
| `rust/tests/crystolecule.rs` | Modify | Register test module |
| `rust/tests/structure_designer/atom_composediff_test.rs` | Create | Node-level tests |
| `rust/tests/structure_designer.rs` | Modify | Register test module |
