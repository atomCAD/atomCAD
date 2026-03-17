# atom_edit: Cached Input Molecule Optimization

## Problem

When dragging atoms in atom_edit's default tool (or dragging the XYZ gadget), the node is
re-evaluated every frame. The evaluator is pull-based with no memoization: `eval()` calls
`evaluate_arg(0)` which recursively evaluates the entire upstream chain. Since only the
atom_edit node's diff changed (not the upstream), this upstream re-evaluation is wasted work.

For a chain like `Diamond → Repeat → atom_edit`, every drag frame re-evaluates Diamond and
Repeat even though their outputs are identical each time.

## Solution

Cache the input molecule on `AtomEditData`. When `eval()` finds a cached input, it reuses it
instead of recursively evaluating upstream nodes.

## Invalidation Strategy

The refresh path is the invalidation signal. The `NodeData` trait has a `clear_input_cache()`
method (default no-op) that the refresh system calls before evaluation when upstream may have
changed:

- `refresh_full()` → always clears caches (full re-evaluation)
- `refresh_partial()` with `skip_downstream = false` → clears caches (upstream may have changed)
- `refresh_partial()` with `skip_downstream = true` → does NOT clear caches (only diff changed)

This is correct because `mark_skip_downstream()` is only called after operations that
exclusively modify the atom_edit node's diff (atom drag, gadget drag). Any operation that
could change upstream (user edits upstream parameter, wire reconnect, full refresh, CLI batch)
uses normal refresh with `skip_downstream = false`.

### Why not a flag on NetworkEvaluationContext?

The original design considered adding a `skip_upstream_eval` flag to `NetworkEvaluationContext`
and threading it through `generate_scene()`. This was rejected because it puts atom_edit-specific
caching business on a completely generic evaluation infrastructure struct. The trait method
approach keeps the invalidation logic generic (any node can override `clear_input_cache()`)
while the actual cache lives where it belongs — on `AtomEditData`.

## Implementation Steps

### Step 1: Add `clear_input_cache()` to `NodeData` trait

**File:** `rust/src/structure_designer/node_data.rs`

Add a default no-op method:

```rust
/// Clears any cached input data used for interactive editing performance.
/// Called by the refresh system before evaluation when upstream may have changed.
/// Default implementation does nothing.
fn clear_input_cache(&self) {}
```

### Step 2: Add `cached_input` field to `AtomEditData`

**File:** `rust/src/structure_designer/nodes/atom_edit/atom_edit_data.rs`

Add a new field to the struct:

```rust
use std::sync::Mutex;

pub struct AtomEditData {
    // ... existing fields ...

    /// Cached input molecule for interactive editing performance.
    /// When present, reused instead of re-evaluating upstream.
    /// Cleared by `clear_input_cache()` when upstream may have changed.
    cached_input: Mutex<Option<AtomicStructure>>,
}
```

**Why Mutex:** `NodeData` requires `Send + Sync`. `RefCell` is not `Sync`. The Mutex is
never contended (all evaluation is single-threaded on the UI thread), so overhead is zero.

Update `new()`, `from_deserialized()`, and `clone_box()` to initialize with
`Mutex::new(None)`.

Override `clear_input_cache()`:

```rust
fn clear_input_cache(&self) {
    if let Ok(mut guard) = self.cached_input.lock() {
        *guard = None;
    }
}
```

### Step 3: Use the cache in `atom_edit::eval()`

**File:** `rust/src/structure_designer/nodes/atom_edit/atom_edit_data.rs`

Replace the input evaluation block with cache-aware logic:

```rust
let input_structure = if let Some(cached) = self.get_cached_input() {
    cached
} else {
    let input_val =
        network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
    if let NetworkResult::Error(_) = input_val {
        return input_val;
    }
    let structure = match input_val {
        NetworkResult::Atomic(s) => s,
        _ => AtomicStructure::new(),
    };
    self.set_cached_input(&structure);
    structure
};
```

### Step 4: Call `clear_input_cache()` from refresh paths

**File:** `rust/src/structure_designer/structure_designer.rs`

In `refresh_full()`, clear caches on all displayed nodes before evaluation:

```rust
for node_entry in &network.displayed_node_ids {
    let node_id = *node_entry.0;
    if let Some(data) = network.get_node_network_data(node_id) {
        data.clear_input_cache();
    }
}
```

In `refresh_partial()`, clear caches on affected nodes when `!skip_downstream`:

```rust
} else {
    let affected = compute_downstream_dependents(network, &changes.data_changed);
    // Clear input caches — upstream may have changed
    for &node_id in &affected {
        if let Some(data) = network.get_node_network_data(node_id) {
            data.clear_input_cache();
        }
    }
    self.last_generated_structure_designer_scene
        .invalidate_cached_nodes(&affected);
    affected
}
```

## Testing

1. **Correctness:** Open a project with `Diamond → atom_edit`. Drag atoms. Verify the
   structure looks identical to before this change.

2. **Cache refresh:** While atom_edit is active, change an upstream parameter. Verify the
   atom_edit output updates correctly (cache is refreshed).

3. **Performance:** With a large upstream chain, verify that drag is noticeably smoother
   (upstream nodes are not re-evaluated during drag).

4. **No regressions:** Run `cargo test` — all ~866 tests should pass. The cache is transient
   and not serialized, so no .cnnd roundtrip tests should be affected.

## Files Modified

| File | Change |
|------|--------|
| `rust/src/structure_designer/node_data.rs` | Add `clear_input_cache()` to `NodeData` trait |
| `rust/src/structure_designer/nodes/atom_edit/atom_edit_data.rs` | Add `cached_input` field, update constructors, override `clear_input_cache()`, use cache in `eval()` |
| `rust/src/structure_designer/structure_designer.rs` | Call `clear_input_cache()` in `refresh_full()` and `refresh_partial()` |

## Future Work

This is a surgical fix for atom_edit. A more general solution would add a per-node evaluation
cache on `NetworkEvaluator` that benefits all node types during partial refresh. That requires
`Clone` on `NetworkResult` and careful invalidation, but would eliminate redundant upstream
evaluation for any interactive editing node, not just atom_edit.
