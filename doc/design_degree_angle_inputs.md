# Design: Degree angle inputs (issue #384)

**Issue:** https://github.com/atomCAD/atomCAD/issues/384 — "change angle inputs
to degree rather than radians for all nodes"

## Summary of the agreement (issue discussion, 2026-07-08)

- **`free_rot.angle` switches from radians to degrees** — pin, stored
  property, and text format. This is the only node in the system with a
  radian-unit angle input.
- **Backward compatibility via file migration, not a per-node unit flag.**
  A radian/degree UI option on the node was explicitly rejected ("keeps
  entropy low on the UI/UX side"; a persistent flag is "more a source of
  errors than a feature"). Old `.cnnd` files are up-converted at load:
  stored angles are converted in place; **wired** angle inputs get a
  synthesized conversion `expr` node inserted on the wire — *always*, even
  when the source is a plain `float` node (uniform rule, simpler migration).
- **The expr language keeps its radian trig functions unchanged** and gains
  a parallel degree family with a `deg` suffix: `sindeg`, `cosdeg`,
  `tandeg`, `asindeg`, `acosdeg`, `atandeg`, `atan2deg`.
- **Bonus additions:** `degrees(x)` / `radians(x)` conversion functions and
  a `pi` constant in the expr language.
- `SERIALIZATION_VERSION` bumps 5 → 6 with a `migrate_v5_to_v6` pre-pass.

## Motivation

Crystallography is full of round angles (90°, 109.47°, 120°); radians make
none of them round. The Flutter property editor already displays and edits
`free_rot`'s angle in degrees (`free_rot_editor.dart` converts both ways),
so radians leak out only through the two programmatic surfaces: **wired
inputs** (a `float`/`expr` feeding the `angle` pin must produce radians) and
the **text format** (`angle = 1.5707963…`). Both are exactly the surfaces an
AI or power user touches. Every other angle in the system is already
degrees: `lattice_symop.rot_angle` (`rotation_angle_degrees`),
`lattice_vecs` / `lattice_vecs_params` cell angles, the atom_edit
measurement dialogs. `free_rot` is the lone inconsistency, and the
convention going forward is settled by this change:

> **Convention: every angle exposed on a node pin, node property, or the
> text format is in degrees. Stored fields are suffixed `_degrees`.**
> (Radians remain the internal math unit — conversion happens at the eval
> boundary. The expr language's classic trig functions remain radian-based,
> matching standard math notation; degree variants carry a `deg` suffix.)

## Non-goals

- No per-node or per-network radian/degree flag (rejected in discussion).
- No change to the existing radian trig functions (`sin`, `cos`, `tan`,
  `asin`, `acos`, `atan`, `atan2`).
- No first-class `Angle` data type (noted as a possible long-term direction
  if angle pins proliferate; out of scope here).
- No change to `lattice_symop`, `lattice_vecs`, `structure_rot`,
  `geo_trans`, or internal renderer/camera math (all already fine).

---

## Phase 1 — Expr language additions

Independent of the `free_rot` switch and shippable first (the migration in
Phase 3 synthesizes `degrees(x)` calls, so this phase is a hard prerequisite
for Phase 3).

### New functions (all fixed-signature)

Registered in `rust/src/expr/validation.rs`
(`create_standard_function_signatures` + `create_standard_function_implementations`),
following the pattern documented for issues #387/expr-numeric-functions:

| Function | Signature | Semantics |
|---|---|---|
| `degrees(x)` | `Float → Float` | `x.to_degrees()` — radians → degrees |
| `radians(x)` | `Float → Float` | `x.to_radians()` — degrees → radians |
| `sindeg(x)` | `Float → Float` | `x.to_radians().sin()` |
| `cosdeg(x)` | `Float → Float` | `x.to_radians().cos()` |
| `tandeg(x)` | `Float → Float` | `x.to_radians().tan()` |
| `asindeg(x)` | `Float → Float` | `x.asin().to_degrees()`; error for \|x\| > 1 (mirrors `asin`) |
| `acosdeg(x)` | `Float → Float` | `x.acos().to_degrees()`; error for \|x\| > 1 (mirrors `acos`) |
| `atandeg(x)` | `Float → Float` | `x.atan().to_degrees()` |
| `atan2deg(y, x)` | `(Float, Float) → Float` | `y.atan2(x).to_degrees()`; arg order matches `atan2` |

Implementation notes:

- **Use the `as_f64` Int-coercing helper, not the strict
  `NetworkResult::extract_float`** — otherwise `sindeg(90)` with an `Int`
  literal validates but errors at runtime (the known latent bug in
  `sin`/`cos`/`tan`; do not replicate it in the new functions).
- Domain errors return `NetworkResult::Error` (no NaN propagation),
  following the `sqrt`/`asin` precedent.

### `pi` constant

`Expr::Var` resolution gains a built-in constant lookup. Resolution order
for a bare identifier:

1. expr parameter of that name (**parameters shadow `pi`** — an existing
   file with a user parameter named `pi` keeps working unchanged),
2. built-in constant `pi` → `Float`, value `std::f64::consts::PI`,
3. otherwise the existing "Unknown variable" error, unchanged.

(Note: there is **no** bare-ident → `Record(Named)` fallback in expression
position — that fallback lives in the *type-annotation* parser
(`expr/parser.rs`, `parse_data_type`), which is untouched here. A bare
unknown identifier in an expression currently just errors — see
`Expr::Var` in `expr/expr.rs`, both the `validate` and `evaluate` arms. So
`pi` shadows nothing except a same-named parameter, and only those two
arms change.)

Both the validation arm (type = `Float`) and the evaluation arm need the
case; there is no lexer/parser change (identifiers already parse as `Var`).

### Phase 1 tests (land with this phase)

- `rust/tests/expr/expr_validation_test.rs`: every new function validates
  with `Float` and `Int` args and rejects wrong arity/types; `pi` types as
  `Float`; `pi` **shadowed by a user parameter** of the same name.
- `rust/tests/expr/expr_evaluation_test.rs`: exact values at round angles
  (`sindeg(90) == 1`, `cosdeg(180) == -1`, `atan2deg(1, 1) == 45`,
  `degrees(pi) == 180`, `radians(180) == pi`), **Int-arg coercion at
  runtime** (`sindeg(90)` with an `Int` literal — the `as_f64` guard),
  domain errors for `asindeg`/`acosdeg` at \|x\| > 1. (`NetworkResult` has
  no `PartialEq` — use `match`, not `assert_eq!`.)
- If the expr node's description string changes, `node_snapshots` needs a
  `cargo insta review` in this phase, not later.

### Phase 1 documentation (the standard doc spots)

- `doc/reference_guide/nodes/math_programming.md` — "Mathematical
  Functions" table (+ a "Constants" line for `pi`).
- expr node description string in
  `rust/src/structure_designer/nodes/expr.rs`.
- `.claude/skills/atomcad/skill.md` — "Supported in expr" list.

---

## Phase 2 — `free_rot` switches to degrees

All changes land together with Phase 3 in one release; a degree-interpreting
`free_rot` must never ship without the migration.

### Rust node (`rust/src/structure_designer/nodes/free_rot.rs`)

- `FreeRotData.angle: f64` → **`angle_degrees: f64`**. The field rename is
  deliberate: the serialized JSON key and the text-format property change
  name too, so any stale radian-era snippet fails loudly instead of being
  silently reinterpreted. Naming precedent: `lattice_symop`'s
  `rotation_angle_degrees`.
- `eval`: the value flowing through pin 1 (`angle`) is now degrees; convert
  at the single math boundary —
  `DQuat::from_axis_angle(axis, angle_degrees.to_radians())`. The
  `worsen_alignment_with_reason` message drops its `.to_degrees()`.
- The **pin name stays `angle`** (short, like `lattice_symop`'s
  `rot_angle`); the node description gains "in degrees".
- `get_subtitle`: drop the `to_degrees()` conversions.
- Text format: `get_text_properties` emits `("angle_degrees", Float)`.
  `set_text_properties` reads `angle_degrees`, and **explicitly rejects the
  old key `angle`** with a pointed error
  (`"angle was renamed to angle_degrees and is now in degrees"`) rather
  than ignoring it — unknown-key silence would make stale AI-generated
  snippets no-op invisibly. This rejection only affects the *literal* path
  (`angle: 90`): a node-reference statement (`angle: my_float`) is resolved
  by the text editor's wire pass against the **pin** name, which stays
  `angle` — wiring syntax keeps working, by design. Do not "fix" this
  apparent inconsistency; it is the same pin/property name split
  `lattice_symop` (`rot_angle` / `rotation_angle_degrees`) and `extrude`
  (`dir` / `extrude_direction`) already have.
- **`get_parameter_metadata` gains an entry for `angle`.** Today the pin
  name matches the property name, so text-format introspection (Pattern A
  in `text_format/node_type_introspection.rs`) reads the pin's default from
  the property. After the rename they diverge, and the introspection
  fallback marks a parameter with no matching property and no metadata as
  **required** — wrong for this optional pin. Add
  `m.insert("angle".to_string(), (false, Some("0 (degrees; stored as angle_degrees)".to_string())))`
  alongside the existing `input` entry (precedent: `extrude`'s `dir` ↔
  `extrude_direction` mismatch).
- `node_data_creator` default stays `0.0` (unit-independent).

### Gadget (`FreeRotGadget`)

The gadget's internal `angle` stays **radians** (all its rotation and
tessellation math is radian-based, and `ROTATION_SENSITIVITY` keeps its
tuned feel untouched). Conversion happens at the two data boundaries:
`FreeRotGadget::new(data.angle_degrees.to_radians(), …)` in
`provide_gadget`, and `d.angle_degrees = self.angle.to_degrees()` in
`sync_data`.

### API + Flutter

- `APIFreeRotData.angle` → `angle_degrees` (Dart: `angleDegrees`) in
  `rust/src/api/structure_designer/…`; run
  `flutter_rust_bridge_codegen generate`.
- `lib/structure_designer/node_data/free_rot_editor.dart` gets simpler:
  delete `_radiansToDegrees` / `_degreesToRadians` and pass the value
  straight through. Labels already say "degrees".

### Phase 2 tests (land with this phase)

In `rust/tests/structure_designer/` (mirroring the source hierarchy):

- **Eval semantics** — the load-bearing assertion of the whole change: a
  `free_rot` with `angle_degrees = 90` around Z rotates a known atom
  position by a quarter turn (compare positions, not just "no error"); a
  wired `float(90)` into the `angle` pin produces the same result as the
  stored value.
- **Text format** — `angle_degrees` serializes and parses round-trip
  (`text_format_test.rs`); `set_text_properties` with the **old `angle`
  key returns the pointed error**, not a silent no-op.
- **Subtitle** — `get_subtitle` renders the stored value directly (no
  double conversion; e.g. `angle_degrees = 90` → `90.0°`).
- **Gadget boundary** — `sync_data` writes degrees back
  (`FreeRotGadget { angle: PI/2 }` → `angle_degrees == 90`).
- Fix any existing test constructing `FreeRotData { angle: … }` directly;
  `free_rot`'s description change → `cargo insta review` now.

Flutter editor: no automated test — it is a thin pass-through after this
change; manual walkthrough via `flutter run` (drag gadget, type a value,
check the two stay consistent) per the project's thin-editor-UI policy.

---

## Phase 3 — `.cnnd` migration v5 → v6

### Version plumbing

- `SERIALIZATION_VERSION` 5 → 6 in
  `rust/src/structure_designer/serialization/node_networks_serialization.rs`.
- New chained pre-pass `serialization/migrate_v5_to_v6.rs`:

```text
if version < 3 { migrate_v2_to_v3(&mut root_value)?; }
if version < 4 { migrate_v3_to_v4(&mut root_value)?; }
// (no v4→v5 transform)
if version < 6 { migrate_v5_to_v6(&mut root_value)?; }
```

Like its predecessors it is a one-shot JSON transform on
`serde_json::Value` *before* strict deserialization (it synthesizes nodes,
which serde defaults cannot express), **frozen at release** (hardcoded
node-type names, pin indices, and the `degrees(x)` expression string — not
read from the live registry).

### What it does, per `free_rot` node

The pass walks every network **and recurses into every `zone` body at every
depth** — zones shipped without a version bump, so v5 files can contain
`free_rot` nodes inside zone bodies. **Key the recursion on the node's
`zone` field being present, NOT on a hardcoded list of HOF type names.**
Zone bodies live on any zone-bearing node — the four HOFs (`map`, `filter`,
`fold`, `foreach`) but also `closure` and `zip_with`, all shipped pre-v6,
and a name list would silently skip the latter two. A `free_rot` inside a
skipped body keeps its old `data.angle` key and **fails strict
deserialization after the field rename** (missing `angle_degrees`) — the
whole file refuses to load. The "frozen hardcoded names" rule applies only
to the node types the pass *acts on* (`free_rot`, the synthesized `expr`),
not to where it recurses. (This recursion requirement is new relative to
v2→v3/v3→v4, which predate zones. Each body has its own `next_node_id`;
allocate synthesized ids from the body the node lives in.)

1. **Stored value:** rename `data.angle` → `data.angle_degrees` and convert
   the value with `f64::to_degrees`. (The converted number may carry float
   dust — e.g. an exact-π/6 radian value becoming `29.999999999999996`;
   accepted, the property editor formats for display anyway.)
2. **Wired `angle` pin** (argument index **1**; guard `arguments.len() > 1`
   and skip when the pin has no incoming wires): synthesize a conversion
   `expr` node and rewire — *unconditionally*, even when the source is a
   plain `float` node (decision: uniform rule beats a prettier special
   case).

The synthesized node:

- `node_type_name: "expr"`, `custom_name: "to_degrees"` (discoverable
  intent in the UI),
- `data`: `{ parameters: [{ id: null, name: "x", data_type: Float,
  data_type_str: "Float" }], expression: "degrees(x)" }` — mirror the exact
  JSON a live-authored expr node saves (write one in-app and copy the
  shape when implementing),
- `id` allocated from the containing network/body's `next_node_id`
  (bumped once at the end),
- `position`: anchored left of the `free_rot` node
  (`free_rot.position − (expr_width + gap, 0)`, snapped to integers,
  matching v3→v4's placement conventions),
- not selected, not in `displayed_nodes`.

Rewiring: the `free_rot.arguments[1]` wire list moves **verbatim** onto the
expr node's `arguments[0]` (its `x` pin), and `free_rot.arguments[1]` is
replaced by a single plain wire from the expr node's pin 0. Moving wires
verbatim preserves capture semantics: `source_scope_depth` and
`SourcePin::ZoneInput` references are relative to the destination's scope,
and the expr node is inserted in the same scope as the `free_rot`. If the
pin somehow carries multiple incoming wires, move all of them.

**Wire-shape caveat:** the migration must read **both** wire storage
shapes — the current `incoming_wires` list and the legacy
`argument_output_pins` map (the custom `Argument` deserializer accepts
both) — and this is not an edge case but the guaranteed input for chained
old files: `migrate_v2_to_v3` and `migrate_v3_to_v4` themselves *emit*
`argument_output_pins` (see `migrate_v3_to_v4.rs`, the synthesized-wire
constructor), and v2→v3 is what synthesizes `free_rot` nodes out of legacy
`atom_rot` in the first place. So every v2/v3 file arrives at this pass
with legacy-shaped wires around exactly the nodes it rewrites. (v4/v5
files saved by the app use `incoming_wires`; only hand-edited files mix
shapes.) Emit `incoming_wires` for anything the pass writes — the
deserializer accepts a per-`Argument` mix.

### Determinism & idempotency

Follow the v3→v4 pattern exactly: a read-only pre-pass collects rewrites,
sorted by `(network name / body path, dst_node_id)` (one angle pin per
`free_rot`, so this key is unique), then a mutation pass allocates ids in
sorted order — byte-identical output across runs. Idempotency is guaranteed
by the version gate, and structurally by keying on the `data.angle` field,
which no longer exists after migration.

### Interaction with the load pipeline

Nothing downstream changes: the synthesized expr node goes through the
normal stage-1 `canonicalize` / `initialize_custom_node_types_for_network` /
`repair_node_network` and stage-2 `validate_network` passes like any
deserialized node. `degrees` exists in the function registry as of Phase 1,
so validation succeeds.

### Phase 3 tests (land with this phase — the migration must not merge without them)

**Golden evaluation values — capture BEFORE implementing Phases 2/3.**
The equivalence target ("migrated file evaluates like the old radian code
did") cannot be computed after the switch, because the radian code is gone.
First implementation step of this phase: author the v5 fixtures, evaluate
them on the **current (pre-change) build**, and record the resulting atom
positions / geometry as constants in the test. The post-migration test
then asserts the loaded-and-evaluated v6 network reproduces those golden
values (within float epsilon).

Fixture matrix (before/after pairs under `rust/tests/fixtures/`, following
the v2→v3/v3→v4 test style):

- unwired `free_rot` (stored-value conversion + field rename),
- `angle` wired from a `float` node (conversion node inserted anyway),
- `angle` wired from an `expr` node,
- a `free_rot` inside a `map` zone body (recursion + per-body id
  allocation), with the angle wire being a **capture**
  (`source_scope_depth ≥ 1`) to lock the verbatim-move rule,
- a `free_rot` inside a **`closure` body** (locks the zone-field-keyed
  recursion — a HOF-name-list recursion misses this one and the file fails
  to load),
- a v5 file using the legacy `argument_output_pins` wire shape,
- a **v2 (or v3) file run through the full chained pipeline**
  (`load_node_networks_from_file`, not the pass function directly) — v2→v3
  synthesizes `free_rot` from legacy `atom_rot` and the earlier passes emit
  legacy-shaped wires, so this exercises the real-world path end to end.

Assertions per fixture:

- migrated JSON matches the checked-in `after.cnnd` byte-for-byte
  (determinism), and running the pass on already-migrated JSON is a no-op
  (idempotency — test the pass function directly, bypassing the version
  gate),
- the loaded network **validates cleanly** (the synthesized expr parses,
  `degrees` resolves) and **evaluates to the golden values** above,
- `next_node_id` (top-level and per-body) is bumped past the synthesized
  ids.

Existing-test fallout owned by this phase:

- `rust/tests/fixtures/rename_wire_loss/before.cnnd` contains a `free_rot`
  and now runs through the new pass — re-check its expectations.
- `cnnd_roundtrip` tests: v6 save → load → save is stable.

---

## Phase 4 — Documentation sweep & final verification

Each phase above owns its own automated tests; this phase is documentation
plus a whole-change verification pass.

### Documentation

- `free_rot` node description (in `free_rot.rs`) — state degrees. (The
  Phase 1 expr doc spots are listed in Phase 1.)
- `doc/reference_guide/` — `free_rot` page (if present).
- `.claude/skills/atomcad/skill.md` — text-format examples mentioning
  `free_rot`'s `angle` property.
- `rust/src/structure_designer/nodes/AGENTS.md` — add the convention line:
  angle pins/properties are degrees, stored fields suffixed `_degrees`.
- Do **not** touch `doc/_atomCAD_reference_guide_pre_phase_0a.md` (frozen
  snapshot).

### Final verification

- Full suite green: `cd rust && cargo test` + `cargo clippy` +
  `flutter analyze`.
- End-to-end manual check: open a real pre-change v5 project containing
  `free_rot` (working-tree examples exist), confirm the model looks
  identical after load, the synthesized `to_degrees` node is visible and
  sensibly placed, and undo/save/reload behaves.

---

## Decisions log (for future archaeology)

| Decision | Choice | Why |
|---|---|---|
| Backward compat mechanism | File migration (v5→v6), not a unit flag | One node affected; flag = permanent two-mode entropy; migration infra + node-synthesis precedent (v3→v4 `collect`) already exists |
| Wired-input migration | Always insert `degrees(x)` expr node, even for exclusive `float` sources | Uniform, simpler, conversion visible & user-deletable |
| Existing trig functions | Unchanged (radians) | Standard math convention; expressions are opaque strings a migration cannot rewrite safely |
| Degree trig naming | `deg` suffix (`sindeg`, …) | Bare `d` suffix deemed confusing, `sin°` untypeable on many keyboards |
| Inverse-trig degree set | `asindeg`, `acosdeg`, `atandeg`, `atan2deg` (return degrees) | Completes the family symmetric to the radian one |
| Stored field name | `angle` → `angle_degrees` | Loud failure for stale radian snippets; `rotation_angle_degrees` precedent |
| Gadget internals | Stay radians; convert in `provide_gadget` / `sync_data` | Rotation math is radian-native; drag feel untouched |
