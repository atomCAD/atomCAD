# `Iter[T]` Display via Explicit `collect`

## Status: Draft

## Motivation

Today, when a node's displayed output pin is typed `Iter[T]`, the evaluator silently performs a bounded `collect` behind the user's back: `drain_iterators_for_display` in `network_evaluator.rs` walks the iterator up to `ITER_DISPLAY_CAP = 256` elements, replaces the `NetworkResult::Iterator(_)` with `NetworkResult::Array(items)`, and stamps a subtitle (`"Iter[T] (showing first 256)"` or `"Iter[T] (N elements)"`) on the node.

This conflicts with the conceptual model `doc/design_iterators.md` establishes: `Iter[T]` is a *lazy, potentially unbounded* stream, and materialization is the consumer's job. The display path is the one place that quietly bypasses the rule that `Iter[T] → Array[T]` requires an explicit `collect` node. Consequences:

- The materialization point is invisible in the graph. A user displaying a `range` or `map` does not see *where* the cap is being applied.
- The cap is a global constant. There is no way to ask for the first 50, or the first 1000, or to tie the bound to a parameter.

This design moves materialization back into the graph: an `Iter[T]`-typed displayed pin renders nothing, and the existing `collect` node grows two ways to bound itself.

The semantics of evaluated networks does not change — only the display path is affected. Project files keep loading; no migration is required.

## Design

### 1. Display behavior

A node whose displayed output pin's resolved type is `Iter[T]` produces `NodeOutput::None` in the viewport. No subtitle is stamped on the producer for the iterator-ness of its output (any subtitle the node already produces via `get_subtitle` is unaffected).

To inspect the elements of a stream, the user wires it into a `collect` node and displays *that*.

### 2. `collect` shape

`CollectData` (in `nodes/collect.rs`) gains an optional limit:

```rust
pub struct CollectData {
    pub element_type: DataType,
    /// `Some(n)` caps the collected array at `n` elements; `None` collects
    /// the full stream (unchanged from today's behavior).
    pub limit: Option<i32>,
}
```

A second input pin is added:

| Direction | Name      | Type      | Required |
|-----------|-----------|-----------|----------|
| Input     | `iter`    | `Iter[T]` | Yes      |
| Input     | **`limit`** | **`Int`** | **No**   |
| Output    | (pin 0)   | `Array[T]`| —        |

**Priority.** When the `limit` pin is wired, its value drives the cap and the stored `limit` field is ignored at eval time (matching the convention documented in `lib/structure_designer/node_data/AGENTS.md` for `imat3_diag`/`mat3_diag`/`supercell`/`atom_replace`). When the pin is disconnected, the stored field drives the cap.

**Limit value semantics.**

- `n > 0` → collect at most `n` elements.
- `n == 0` → collect zero elements; result is `Array[T]([])`. Useful for "evaluate but discard."
- `n < 0` → eval error: `"collect: limit must be non-negative, got {n}"`. Treated like any other mid-stream error.
- Pin connected but its evaluation produces `NetworkResult::None` → fall through to the stored field (treat as if the pin were disconnected). Avoids a footgun where a missing upstream value silently drops the cap.

The unbounded case (no UI checkbox, no pin) is the default and matches today's `collect` semantics exactly — important so existing graphs behave identically after the change.

**Subtitle.** `collect`'s `get_subtitle` reports the materialization outcome:

- Unbounded, walker exhausted: `"(N elements)"`
- Bounded, walker exhausted before cap: `"(N elements)"`
- Bounded, cap hit: `"(stopped at limit N)"`
- Stream error: existing error path (unchanged).

The subtitle is computed from the eval result, not from the configured limit alone, so the user sees the *actual* count in both cases. (Implementation note: `get_subtitle` doesn't have access to eval results today; the count is communicated the same way iterator producers communicate it now — via `context.node_output_strings` from the node's `eval`.)

### 3. UI changes

`collect_editor.dart` gains a row beneath the existing `Element Type` control:

```
[ ] Limit elements    [   100  ] △▽
```

Checkbox unchecked → `limit = None`. Checked → `limit = Some(n)` with a default of `100` on first check (chosen to be small enough that "I just want a peek" is the path of least resistance, large enough to be useful for anything that isn't producing thousands of items per element).

When the `limit` input pin is wired, the row follows the standard "disable on wired input" pattern (`lib/structure_designer/node_data/AGENTS.md`): an italic annotation above the row reading `` Limit supplied by `limit` input. Disconnect to edit inline. ``, and the row wrapped in `Opacity(0.5) + IgnorePointer`.

### 4. Deletions

These all go away:

- `ITER_DISPLAY_CAP` in `common_constants.rs:31`.
- `drain_iterators_for_display` and `drain_walker_for_display` in `network_evaluator.rs`.
- The `DataType::Iterator(_)` arm in `convert_result_to_node_output` (`network_evaluator.rs:657-674`) — it can fall through to `(NodeOutput::None, None)`.
- The display-path subtitle stamping for iterator pins in `generate_scene` (`network_evaluator.rs:259-274`). Note: the same loop is shared with non-iterator subtitles via `context.node_output_strings`; only the iterator-specific drain block is removed.
- References in `evaluator/AGENTS.md` and `doc/design_iterators.md` ("Display" section); update them to point here.

### 5. Text format

`collect`'s `get_text_properties` / `set_text_properties` round-trip the new `limit` field as `TextValue::Int` (with `None` represented by the property being absent — same convention used by other optional fields).

The `limit` input pin is referenced positionally like any other unnamed pin; no syntax change.

### 6. Tests

New unit tests in `rust/tests/structure_designer/`:

- `collect_test.rs` (new file, registered in `structure_designer.rs`):
  - Unbounded `collect` of a finite stream — exhausts.
  - Stored limit, finite stream shorter than limit — exhausts, returns full array.
  - Stored limit, infinite/long stream — caps at limit.
  - Wired `limit` pin overrides stored value.
  - Wired `limit` pin = 0 → empty array.
  - Wired `limit` pin = negative → error.
  - Subtitle wording for cap-hit vs. exhausted.
- `iterator_walker_test.rs`: drop the two tests that assert the cap-via-display behavior (lines around `iterator_walker_test.rs:681, 712` per current tree). Their coverage moves to `collect_test.rs`.

Existing snapshot/roundtrip tests catch the new optional `limit` field — update fixtures as needed.

## Phase plan

The change is small enough for one PR, but split for reviewability:

1. **Backend `collect` extension.** Add the optional `limit` field, the `limit` input pin, and the new eval semantics. Keep `ITER_DISPLAY_CAP` and the display drain working — this phase is purely additive. Tests for the new `collect` shape land here.
2. **Display-path removal.** Delete `ITER_DISPLAY_CAP`, the drain, and the iterator arm in `convert_result_to_node_output`. Update affected tests. After this phase, displaying a bare `range` shows nothing in the viewport.
3. **UI.** Add the limit row to `collect_editor.dart` with the wired-input disable affordance. Regenerate FRB bindings.

Each phase keeps the test suite green.
