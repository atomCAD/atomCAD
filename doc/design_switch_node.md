# `switch` node — select a value by matching a selector against literal cases

## Motivation

The `if` node (`nodes/if_else.rs`, see `nodes/AGENTS.md`) gave the network a
lazy two-way branch on a `Bool`. Its design deferred the natural follow-up: an
n-way branch keyed by a value. `switch` fills that gap:

- **Selector** — a `selector` input pin of a user-selected *selector type*
  (**Int or String**).
- **Cases** — a user-edited list of literal case values (edited in the
  property panel, not wired). Each case contributes one input pin whose name
  reflects its value (`case_5`, `case_slot_a`), typed by a separate
  user-selected *value type* (any concrete type, like `if.value_type` —
  including structural types such as Crystal, Molecule, Geometry, Function).
- **Default** — a fixed trailing `default` pin of the same value type, always
  present.
- **Output** — a single pin of the value type. At eval the selector is
  compared against the case literals; the matching case's pin (or `default`)
  is the one — and only — branch evaluated.

Like `if`, this is not expressible with `expr`: `expr` cannot carry structural
values and eagerly evaluates every wired input. And like `zip_with`, the
variadic pin list must survive case edits without dropping wires — the design
reuses the hidden-stable-id machinery built there.

## Resolved decisions (from design review)

| Question | Decision |
|---|---|
| Branch/output type | **Separate `value_type`** property (any concrete type, mirroring `if.value_type`), independent of the Int/String `selector_type`. |
| Case pin naming | **`case_<value>`, sanitized** — names derived from the literal values; wires keyed by a hidden stable id per case, so the name is cosmetic. No backtick-quoting (sanitized names always lex as bare identifiers). |
| Duplicate case values | **Rejected at edit time** — the property setter and `set_text_properties` refuse a duplicate value with an error; the node can never hold duplicates through supported edit paths. |
| Default / laziness | **Mirror `if`** — all pins optional; unwired `selector` → inert `None`; no match + unwired `default` → `None`; strictly lazy (selector first, then only the matched branch's upstream cone). No warning for an unwired `default`. |

## Node shape

Pin order (`calculate_custom_node_type` builds the list from scratch — the
count varies, same as `zip_with`/`expr`, not the `if` idiom of indexing base
parameters):

| Pin | Index | Type | Notes |
|---|---|---|---|
| `selector` | 0 | `selector_type` (Int \| String) | the selector |
| `case_<v1>` … `case_<vN>` | 1 … N | `value_type` | one per case, `Parameter.id = case.id` |
| `default` | N+1 | `value_type` | fixed name, `id: None` |
| output | 0 | `value_type` | `OutputPinDefinition::single_fixed` |

The fixed names `selector` / `default` cannot collide with case pins — every case
pin carries the `case_` prefix. The prefix also keeps case pins from colliding
with the text-format property keys (`selector_type`, `value_type`, `cases`) in
the node's `{ … }` block.

All pins are optional (no `get_parameter_metadata` needed — `if` sets the
precedent; the pin names never diverge from properties the way
`free_rot.angle`/`angle_degrees` do).

## Data model

New file `rust/src/structure_designer/nodes/switch.rs` (module name `switch`
is a legal Rust identifier, and `switch` is not a text-format lexer keyword —
verified against `parser.rs`'s keyword match):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SwitchCaseValue {
    Int(i32),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchCase {
    /// Hidden stable identity; wires survive case-value edits and removals.
    #[serde(default)]
    pub id: Option<u64>,
    pub value: SwitchCaseValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchData {
    /// Type of the `selector` pin. Restricted to Int | String —
    /// validated by every setter; a hand-authored other type is healed at
    /// load (see "Healing" below).
    pub selector_type: DataType,
    /// Type of the case pins, the `default` pin, and the output pin.
    pub value_type: DataType,
    pub cases: Vec<SwitchCase>,
    /// Monotonic id source for new cases. Persisted — max(existing)+1 would
    /// recycle the id of a just-removed highest case, the `next_param_id`
    /// wire-stability hazard (`doc/design_parameter_wire_stability.md`).
    /// `#[serde(default)]`; healed on load (see below).
    #[serde(default)]
    pub next_case_id: u64,
}
```

Default: `selector_type: Int`, `value_type: Float`, cases `[0, 1]` (ids 1, 2),
`next_case_id: 3`. **Zero cases is legal** in principle (the node degenerates
to a `default` passthrough) but useless; to stay consistent with `zip_with`'s
"the editor disables deleting the last lane" rule, the **minimum is one case**
— setters reject an empty case list.

`SwitchCaseValue` always matches `selector_type` (Int selector ⇒ all `Int`
variants); the setters enforce this invariant, and the loader heals violations
(below).

`value_type` (and `selector_type`) must be canonicalized in `canonicalize.rs`
exactly like `if.value_type` — `value_type` can be a `Function`.

## Case pin name derivation

One helper, `SwitchData::derived_case_pin_names(&self) -> Vec<String>`, is the
single source of truth, used by `calculate_custom_node_type` (and therefore by
the text-format serializer/parser, which resolve pin names through the custom
node type — existing machinery, same as `zip_with`'s `xs{i}` pins).

- **Int**: `case_5`; negative values render the sign as `neg` → `case_neg3`
  (`-` is not an identifier character; `case__3` would be the sanitizer
  fallback and reads worse). Int values are unique (duplicates rejected), so
  no dedup pass is needed.
- **String**: `case_` + sanitized value: keep alphanumeric (unicode, matching
  the lexer's `is_alphanumeric`) and `_`, map every other char to `_`,
  truncate to 24 chars. A value that sanitizes to nothing yields the bare
  name `case_` (the dedup pass below disambiguates repeats).
- **Dedup**: distinct string values can sanitize to the same name
  (`"a b"` / `"a_b"`) or collide after truncation. Within the case list, the
  first occurrence keeps the bare name and later collisions append `__2`,
  `__3`, … in list order. Deterministic given the case list, so serialize →
  parse round-trips agree on names.

Because the sanitizer output starts with `case_` and contains only
alphanumerics/underscores, derived names always lex as bare identifiers — no
backtick-quoting in the text format, ever.

Wire identity never depends on these names: external wires are rebuilt by
`node_network.rs::set_custom_node_type`, which matches parameters **by id**
when both sides carry one (the mechanism `expr` / `zip_with` rely on). Renames
of the derived name are therefore free.

## Runtime semantics (`eval`, mirrors `if`)

1. `evaluate_arg(pin 0)` — the selector. `None` → node is inert, output
   `EvalOutput::single(None)`. `Error` → propagate. Extract per
   `selector_type` (`NetworkResult::Int` / `NetworkResult::String`); any other
   variant → `switch.selector: expected Int, got …` error. (A `Float` wired to an
   Int selector already truncated at the wire — the standard conversion.)
2. Find the case whose literal equals the selector value — `i32` equality /
   exact case-sensitive string equality. Values are unique through supported
   edit paths; if a hand-authored file smuggled duplicates in, the **first
   match wins** (documented, not policed — the behavior is well-defined, the
   litmus test in `structure_designer/AGENTS.md` says no blocking rule, and a
   validation warning isn't worth a rule for a state the editors can't
   produce).
3. Lazily `evaluate_arg` **only** the taken pin: `1 + case_index` on a match,
   the `default` pin (`1 + cases.len()`) otherwise. An unwired taken pin
   yields `None`, which flows through unchanged — exactly `if`'s contract.

No new walker, closure, or zone machinery — `switch` is bodyless.

## Case-list editing: value-keyed id merge

`zip_with` needed a separate id-accurate `remove_zip_with_lane` operation
because its lanes have no natural key (type only, non-unique) — a positional
whole-list merge misattributes ids on a middle-lane removal. **`switch` cases
do have a unique key: the case value.** That makes a single whole-list setter
id-accurate:

`SwitchData::merge_cases(new_values: Vec<SwitchCaseValue>) -> Result<(), String>`
(shared by `set_text_properties` and the `StructureDesigner`-level setter):

1. Reject duplicates in `new_values` (error, node unchanged). Reject an empty
   list.
2. **Value-match pass**: every new value that equals an old case's value
   (plain same-type equality — a selector-type flip converts the stored
   values *before* any merge, see below, so cross-type comparison never
   happens) keeps that old case's id. Values are unique on both sides, so
   these matches are unambiguous. Handles removal and reorder.
3. **Positional-fallback pass**: every still-unmatched new value inherits the
   id of the old case at its index, if that case exists and its id was not
   consumed by the value-match pass. Handles editing a case's value in
   place — the wire follows (the name-then-position merge of
   `ExprData::set_text_properties`, with the value as the name).
4. **Mint pass**: any new value still without an id takes `next_case_id` and
   increments it. Never max+1 (the `next_param_id` regression shape).

The pass separation is load-bearing, not a style choice. `expr`'s merge
resolves each element in a single left-to-right pass, so an early positional
fallback can *steal* the id a later value match needs: on `[1, 2] → [3, 1]`
(insert a case in front, drop case 2) a single pass hands case 1's id to the
new case 3 positionally, then shunts case 1 onto case 2's id — both wires
silently rerouted onto wrong branches, which for `switch` means wrong values,
not just mislabeled pins. Resolving *all* value matches before any positional
fallback makes the steal impossible: an unchanged value always keeps its wire.

Worked examples: remove middle of `[1,2,3]` → `[1,3]`: both survivors keep
their ids by value; case 2's pin (and its wire) drop. Edit `[1,2,3]` →
`[1,5,3]`: 1 and 3 match by value, 5 inherits 2's id positionally — the wire
survives the value edit and the pin renames from `case_2` to `case_5`. Insert
`[1,2]` → `[3,1]`: 1 keeps its id (and wire) by value; 3 finds its positional
slot's id already consumed and mints; case 2's wire drops.

There is **no reorder UI** (case order only affects pin order cosmetically and
first-match-wins for impossible duplicate states), and no separate remove API
— the delete button sends the list minus the removed case and the value merge
resolves it exactly.

### Selector-type change

Flipping `selector_type` converts the **stored** case values in place, ids
untouched — one helper, `SwitchData::convert_selector_type(new_type) ->
Result<(), String>`. Conversion happens only here and in load-time healing;
`merge_cases` never compares across types (its value match is plain same-type
equality):

- Int → String: stringify (`5` → `"5"`), always succeeds.
- String → Int: parse each; **any failure rejects the whole edit** with an
  error naming the offending value. Parse success alone is not enough:
  distinct strings can parse to the same int (`"5"` / `"05"`, `"5"` / `"+5"`),
  so the converted list is **re-checked for duplicates, and a collision also
  rejects the whole edit** (naming the colliding values) — otherwise the flip
  would smuggle in the duplicate state every other edit path rejects. No
  silent dropping or merging of cases.

When one edit both flips the type and supplies a case list — a
`set_switch_data` call, or a `set_text_properties` block changing
`selector_type` alongside `cases` — **both setters run
`convert_selector_type` first, then `merge_cases`** on the now-same-type
values. Skipping the conversion in the text path would silently defeat every
value match on a flip (String `"1"` ≠ Int `1` under same-type equality),
degrading all ids to the positional fallback.

### Undo

Case-list edits are **not** pure node-data edits: removing (or failing to
value-match) a case drops that pin's external wire, and shrinking `value_type`
compatibility (a retype) can drop wires via revalidation. A
`SetNodeDataCommand` snapshot (node-data blob only — no `arguments`) cannot
restore those wires; this is exactly why `zip_with` grew
`ZipWithLaneEditCommand` (`undo/commands/zip_with_lane_edit.rs`), which is
nothing but `{ network_name, before/after SerializableNodeNetwork }`.

**Generalize that command instead of adding a third copy**: rename it to
`NodeStructureEditCommand` (same file pattern) with a `description: String`
field; `zip_with`'s two call sites pass "Edit zip_with lanes", `switch` passes
"Edit switch cases". Undo/redo/refresh (`UndoRefreshMode::Full`) are already
generic. (If the rename churn is unwanted at implementation time, a sibling
`SwitchCaseEditCommand` is the acceptable fallback — but the preferred shape
is one shared command.)

The `StructureDesigner`-level op — `set_switch_data(scope_path, node_id,
selector_type, value_type, case_values)` — follows `set_zip_with_data`
verbatim: read-only pre-check (node exists, is a switch, types valid,
duplicates/empty rejected) so an error leaves the designer untouched; no-op
check (don't push an empty command); `snapshot_network` before; mutate via
`merge_cases` + field writes; validate/refresh; snapshot after; push the
command only if the snapshots differ. The text path does **not** push it —
text edits are covered by `TextEditNetworkCommand` (no double push).

## Text format

```
s = switch { selector_type: Int, value_type: Crystal, cases: [1, 2, 5],
             selector: sel, case_1: a, case_2: b, case_5: c, default: d }
```

- `get_text_properties`: `selector_type` / `value_type` as
  `TextValue::DataType`, `cases` as `TextValue::Array` of `TextValue::Int` or
  `TextValue::String` (matching the selector type).
- `set_text_properties`: validates `selector_type` ∈ {Int, String}, coerces
  each array element to the selector domain (a whole-number parse for Int —
  reject `TextValue::Float` fractions), then runs `merge_cases` — so
  incremental text edits preserve ids/wires the same way the panel does.
  Duplicates and an empty `cases` array are errors.
- Pin names are derived, so the serializer emits case connections by the
  derived names and the parser resolves them through the custom node type —
  the same dynamic-pin path `expr` / `zip_with` already exercise. Ids never
  appear in the text format.

## Serialization, healing

`generic_node_data_saver` / `loader`; new node type ⇒ **no `.cnnd`
migration**. Loader healing (mirrors `zip_with`'s):

- missing/zero `next_case_id` → max(case ids)+1;
- a case loaded with `id: None` gets an id minted from the healed counter
  (an id-less case silently degrades to name/positional wire matching —
  the fragility the ids exist to prevent);
- a `SwitchCaseValue` variant disagreeing with `selector_type`, or a
  `selector_type` outside {Int, String}, is healed by converting values via
  the canonical string form where possible and dropping cases that can't
  convert. Healing must restore the same invariants the setters enforce:
  after conversion, a later duplicate of an earlier value is dropped too
  (distinct strings can parse to the same int), and if no case survives, the
  case list resets to the default `[0, 1]` with freshly minted ids — never
  duplicates, never empty (hand-authored corruption only; supported paths
  can't produce it).

## Misc node behaviors

- **`get_subtitle`**: `Int → Crystal (3 cases)`.
- **`adapt_for_drag_source`**: mirror `if` — set `value_type` to the source
  type in both drag directions; reject abstract types and `Iter[T]`. (A
  dragged Int also matches the static `selector` pin of the default node, so
  `switch` still surfaces for an integer drag without adaptation; a String
  drag adapts the *value* side — users flip `selector_type` manually when the
  string is meant as the selector.)
- **Display**: single output pin, default pin-0-only display policy; nothing
  special.
- **Category**: `MathAndProgramming`. Description/summary mention "select",
  "match", "case", "multiplex" so registry search finds it.

## Out of scope (deferred)

- **Bool selector** — `if` covers it.
- **Select-by-index** — `array_at` covers indexed selection over arrays; an
  index-keyed switch is just Int cases `0..N`.
- **Ranges / multiple values per case, fallthrough** — flat literal equality
  only in this drop.
- **Expression-language `switch(...)`** — node-graph only, like `if`.

---

## Phase 1 — Core node

**Deliverables**

- `nodes/switch.rs`: `SwitchData` / `SwitchCase` / `SwitchCaseValue`,
  `NodeData` impl (`calculate_custom_node_type` with
  `derived_case_pin_names`, lazy `eval`, `get_subtitle`,
  `adapt_for_drag_source`, text properties incl. `merge_cases`), registration
  in `nodes/mod.rs` + `node_type_registry.rs::create_built_in_node_types()`.
- `canonicalize.rs` entries for `selector_type` / `value_type`.
- Update registry-wide count/list assertions and re-bless node-type snapshots
  (`cargo insta review`).

**Automated tests** — new `rust/tests/structure_designer/switch_test.rs`
(registered in `tests/structure_designer.rs`):

1. Int selector matches a case → that pin's value; no match → `default`;
   no match + unwired `default` → `None`; unwired selector → `None`.
2. String selector: exact, case-sensitive matching.
3. Laziness: error (or a counting `print`) upstream of an *untaken* case pin
   does not poison the output (reuse the `if` test pattern).
4. Structural `value_type` (Crystal): the matched structural value flows
   through intact.
5. Selector error propagates; wrong-typed selector yields a localized error.
6. Derived pin names: negative Int → `case_neg3`; string sanitization,
   truncation, and dedup suffixes (`"a b"` vs `"a_b"`).
7. Hand-built duplicate values (bypassing setters): first match wins, no
   panic.

**Gate:** `cd rust && cargo test -j 4 && cargo clippy && cargo fmt`.

## Phase 2 — Case editing + undo

**Deliverables**

- `SwitchData::merge_cases` (two-pass value-then-position id merge,
  duplicate/empty rejection, `next_case_id` minting) and
  `SwitchData::convert_selector_type` (in-place conversion, parse-failure +
  post-conversion-duplicate rejection).
- `StructureDesigner::set_switch_data(scope_path, node_id, …)` with the
  snapshot-diff undo push.
- `ZipWithLaneEditCommand` → generalized `NodeStructureEditCommand`
  (description field; zip_with call sites updated) — or the sibling-command
  fallback.

**Automated tests** — `switch_test.rs` + `undo_test.rs` additions:

1. Remove middle case of three (whole-list set): survivors' wires follow
   their ids onto renamed/renumbered pins; removed case's wire drops;
   evaluation result for surviving branches unchanged.
2. Edit a case value in place: pin renames, wire survives (positional
   fallback).
3. Reorder via whole-list set: ids follow values, wires intact.
4. No positional steal: `[1,2]` → `[3,1]` keeps case 1's wire on its
   (renumbered) pin and mints a fresh id for 3 (the two-pass order — a
   single-pass merge fails this).
5. Remove the highest-id case, add a new one: the new id is **not** the
   recycled one (`next_case_id`, not max+1).
6. Selector-type flip Int→String keeps ids/wires (in-place conversion);
   String→Int with an unparseable value rejects atomically; String→Int where
   two distinct strings parse to the same int (`"5"` / `"05"`) also rejects
   atomically.
7. Duplicate and empty case lists rejected; node unchanged.
8. Undo/redo of a case removal with a wire attached restores the wire and
   `next_case_id` exactly (whole-network JSON compare via `normalize_json`);
   works for a **body-internal** switch (top-level snapshot carries bodies).
9. `value_type` retype dropping a now-incompatible wire is captured by the
   same command.

**Gate:** full `cargo test -j 4`.

## Phase 3 — Text format + serialization round-trips

**Deliverables**

- `get_text_properties` / `set_text_properties` finalized (`cases` array,
  merge through `merge_cases`).
- Serializer/parser round-trip through derived pin names (existing dynamic-pin
  machinery; verify only).
- Loader healing (`next_case_id`, id-less cases, selector/value-variant
  mismatch).

**Automated tests** — `text_format_test.rs`, `cnnd_roundtrip_test.rs`,
`node_snapshot_test.rs`:

1. Parse the canonical example above; wires land on the right case pins.
2. Serialize → edit-replace → serialize: byte-equal, including a dedup-suffix
   name and a negative-Int name.
3. Incremental text edit of only `cases` preserves ids/wires; a text edit
   flipping `selector_type` (with the `cases` array rewritten in the new
   domain) also preserves ids/wires — the conversion-before-merge rule holds
   on the text path too.
4. `.cnnd` round-trip with wired cases and a structural `value_type`;
   `normalize_json` exact; body-internal switch survives.
5. Healing: hand-authored file with missing ids / zero counter / Int values
   under a String selector loads sanely; a subsequent case edit preserves
   wires (healed ids actually participate). Post-conversion duplicates are
   dropped and an all-dropped list resets to the default cases — the loaded
   node always satisfies the unique/non-empty invariants.
6. Node-type insta snapshot blessed.

**Gate:** full `cargo test -j 4` incl. `cargo test cnnd_roundtrip` and
`cargo test node_snapshots`.

## Phase 4 — API + Flutter UI

**Deliverables**

- API (`rust/src/api/structure_designer/`): `APISwitchData
  { selector_type: APIDataType, value_type: APIDataType, case_values:
  Vec<String> }` — case values cross the API as strings (the editor edits
  text fields; Rust parses per selector type and returns an `APIResult` error
  on a bad Int). `get_switch_data` / `set_switch_data`, both `#[frb(sync)]`,
  both taking **`scope_path`** (hard rule in `rust/AGENTS.md`). Ids never
  cross the API. The setter wraps the Phase 2 `StructureDesigner` op — the
  API layer adds no undo logic.
- `flutter_rust_bridge_codegen generate`.
- Model methods forwarding `propertyEditorScopeChain`, then
  `refreshFromKernel()` + `notifyListeners()`.
- `lib/structure_designer/node_data/switch_editor.dart`, registered in
  `node_data_widget.dart`: selector-type dropdown restricted to Int/String,
  `DataTypeInput` for the value type, one row per case (literal text/int
  field + delete button, delete disabled at one case), an "Add Case" button
  (new case gets the first unused small integer / empty-string-avoiding
  value), inline error display from `APIResult` (duplicates, bad parses).
- Docs: `nodes/AGENTS.md` Math/Programming bullet for `switch`; atomcad skill
  node list if it enumerates types.

**Checks**

1. One Rust-side test driving the Phase 2 setter against a **body-internal**
   switch via `scope_path` (the property-panel-wrong-node bug class).
2. `flutter analyze` (no new issues beyond baseline), `dart format lib/`,
   integration smoke.
3. Manual walkthrough (recorded per house convention): add `switch`; edit a
   case value and watch the pin rename with its wire intact; delete a wired
   case and Ctrl+Z it back; flip selector type Int↔String; wire a Crystal
   through case/default with a structural `value_type`; confirm only the
   matched branch evaluates (e.g. an error upstream of an untaken case
   doesn't redden the output).

**Gate:** `cd rust && cargo fmt && cargo clippy && cargo test -j 4`,
`flutter analyze`, `dart format lib/`, integration smoke.

---

## Open questions

1. **Multi-value cases** (`case 1, 2:`) — would change the pin-name scheme
   and the uniqueness key; revisit only on user demand.
2. **How far to generalize the snapshot undo command** — the rename to
   `NodeStructureEditCommand` is preferred; whether `TextEditNetworkCommand`
   should also fold into it is out of scope here.
