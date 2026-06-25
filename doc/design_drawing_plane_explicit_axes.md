# Drawing Plane — Explicit In-Plane Axes

## Problem

`drawing_plane` is oriented solely by its Miller index `m = (h k l)` (the plane
normal). It then picks the two in-plane basis vectors `(u_axis, v_axis)`
*automatically* — `compute_preferred_plane_axes` enumerates canonical
perpendicular candidates, scores them against the world X/Y axes, and
`DrawingPlane::new` flips `v` for right-handedness and Gram-Schmidt-orthonormalizes
them into `effective_unit_cell`.

Two motivations to give the user explicit control:

- **Explicit control** — a user needs to fix which lattice directions become the
  horizontal/vertical axes of the drawing coordinate system.
- **Undesired fragility** — the auto-pick is an *arbitrary* deterministic choice.
  A future change to that scoring algorithm could silently rotate the drawing
  coordinate system of existing designs. Letting the user pin `u`/`v` explicitly
  is what insulates them from that risk.

## Design philosophy (from the user discussion)

Two kinds of fragility, treated oppositely:

- **Desired fragility** — when the user supplies redundant information, we
  *verify*, never *reconcile*. No input silently overrides another, no fallback.
  Any inconsistency stops with a loud, obvious error. The "free plausibility
  check" the user gets by wiring redundant inputs is the entire point.
- **Undesired fragility** — the arbitrary auto-basis (above), which this feature
  lets the user opt out of.

## Inputs

The two in-plane axes are **direct-space lattice direction indices** `[u v w]` —
integer steps along the unit-cell vectors **a, b, c** (a *direction* in the
crystal). This is a different kind of index from `m`, which is a *plane* index
`(h k l)` in reciprocal space. A direction lies in a plane iff the **Weiss zone
law** holds: `h·u + k·v + l·w = 0`. Confirmed against the code: `u_axis`/`v_axis`
flow through `unit_cell.ivec3_lattice_to_real(..)` (`crystolecule/drawing_plane.rs`),
i.e. they are direct-lattice directions, not Miller indices.

All three orientation inputs become **optional**:

| Pin | Type | Index | Notes |
|-----|------|-------|-------|
| `m_index` | `IVec3` | 1 (existing) | Miller plane index, now optional/unsettable |
| `u`       | `IVec3` | 5 (new)      | first in-plane direction `[u v w]` |
| `v`       | `IVec3` | 6 (new)      | second in-plane direction `[u v w]` |

(Existing pins: `structure` 0, `m_index` 1, `center` 2, `shift` 3,
`subdivision` 4. The new pins append at 5/6 so all existing indices are
preserved.)

Each is resolved with the standard three-state rule — **wired pin > stored field
> unset**:

- pin connected → use the wired value;
- pin disconnected, stored field set → use the stored value;
- pin disconnected, stored field unset → the input is absent.

## The four cases

Resolution operates on `(m, u, v)`, each `Option<IVec3>`. The rule in one line:
**a plane needs either its normal, or two in-plane directions.**

| Case | Inputs present | Behavior |
|------|----------------|----------|
| **A** | `m` only | Exactly today's behavior: auto-generate both axes from `m`. |
| **B** | `m` + `u` | Verify `u` lies in the plane (Weiss). Basis = `u` **plus the first of case-A's two auto axes that is not collinear with `u`**. |
| **C** | `m` + `u` + `v` | Verify *both* `u` and `v` (Weiss) and that they are non-collinear. Use `u`, `v` exactly. |
| **D** | `u` + `v`, no `m` | Derive `m = reduce(u × v)` (Weiss zone law backwards). Use `u`, `v` exactly. |

Everything else is an error (see below). Note case B is **not** a computed
perpendicular `v` — a true perpendicular almost never coincides with a lattice
direction; we reuse one of the already-valid auto axes.

## Resolved decisions

1. **Unsettable `m` with unchanged default.** `miller_index` becomes
   `Option<IVec3>`; a freshly created node still defaults to `Some((0,0,1))`, so
   existing files and habits are unchanged. The editor exposes an unset toggle
   (UI precedent: the `collect` node's optional `limit`, see below).
2. **"Not equal" = collinear.** "The first auto axis not equal to `u`" means *not
   collinear* — tested by the integer cross product `a × b == (0,0,0)`. (So
   `u = [2,0,0]` correctly counts as collinear with an auto `[1,0,0]`.)
3. **Case B flips the second axis for right-handedness** — same
   `(u × v_second) · n > 0` enforcement as case A.
4. **`u`/`v` magnitudes are preserved, never reduced.** Their length sets the 2D
   cell period (`effective_unit_cell`), so they are used verbatim. Only a
   *derived* `m` (case D) is reduced to lowest terms.
5. **Degenerate case D errors.** Parallel `u`/`v` give a zero cross product →
   error, even though case D is otherwise "no checks."
6. **Case C honors the user's axes verbatim — no handedness flip, no warning.**
   Cases A/B enforce a right-handed `(u, v, n)`; case C does not. A left-handed
   `(u, v)` pair is accepted exactly as given (`enforce_right_handed = false`) —
   orientation is the user's explicit responsibility, consistent with the
   "desired fragility" philosophy.
7. **`m` + `v` only (no `u`) is an error.** `u` is the primary axis in the model,
   so providing only `v` (with or without `m`, but without `u`) is rejected
   ("specify `u`, not only `v`"), matching the user's enumeration of B as
   `m & u`. See the error matrix.

## Error matrix

All errors are surfaced as a localized `NetworkResult::Error` returned from
`eval` (see "Validation" below). Messages must be explicit.

| `(m, u, v)` | Result |
|-------------|--------|
| `(Some, None, None)` | case A |
| `(Some, Some, None)` | case B; error if `u` not in plane |
| `(Some, Some, Some)` | case C; error if `u` or `v` not in plane, or `u‖v` |
| `(None, Some, Some)` | case D; error if `u‖v` (degenerate) |
| `(Some, None, Some)` | error: "specify `u`, not only `v`" |
| `(None, Some, None)` | error: "under-specified plane: give a Miller index or both `u` and `v`" |
| `(None, None, Some)` | error: same under-specified message |
| `(None, None, None)` | error: "plane orientation unspecified" |

## Core geometry (`crystolecule/drawing_plane.rs`)

The case dispatch is pure geometry and belongs in `crystolecule` (lower level,
independent of `structure_designer`, unit-testable there). Refactor:

- **New entry point**
  ```rust
  pub fn from_spec(
      unit_cell: UnitCellStruct,
      miller: Option<IVec3>,
      u: Option<IVec3>,
      v: Option<IVec3>,
      center: IVec3,
      shift: i32,
      subdivision: i32,
  ) -> Result<Self, String>
  ```
  implementing the case matrix above and returning `Err(String)` with the exact
  messages.

- **Keep `DrawingPlane::new` as a thin wrapper** so every existing caller
  (`xy_plane`, tests) is untouched:
  ```rust
  pub fn new(unit_cell, miller_index, center, shift, subdivision) -> Result<Self, String> {
      Self::from_spec(unit_cell, Some(miller_index), None, None, center, shift, subdivision)
  }
  ```

- **Extract a finalizer** from the current `new` body — the part *after* the axes
  are chosen (right-handed flip, Gram-Schmidt, `effective_unit_cell`):
  ```rust
  fn build_from_axes(
      unit_cell, miller_index: IVec3, u_axis: IVec3, v_axis: IVec3,
      center, shift, subdivision, enforce_right_handed: bool,
  ) -> Result<Self, String>
  ```

- **Helpers** (all integer-exact):
  - `compute_auto_axes(unit_cell, m) -> Result<(IVec3, IVec3), String>` — the
    case-A pair (today's `compute_preferred_plane_axes`).
  - `in_plane(m, d) -> bool` = `m.x*d.x + m.y*d.y + m.z*d.z == 0` (Weiss).
  - `collinear(a, b) -> bool` = `a.cross(b) == IVec3::ZERO`.
  - `derive_miller(u, v) -> Result<IVec3, String>` = `reduce_to_primitive(u.cross(v))`,
    error if `u.cross(v) == 0`. (`reduce_to_primitive` already exists.)

- **Per-case wiring:**
  - A: `(ua, va) = compute_auto_axes(m)`; `build_from_axes(.., m, ua, va, .., true)`.
  - B: assert `in_plane(m, u)`; `(ua, va) = compute_auto_axes(m)`; `second =`
    first of `[ua, va]` with `!collinear(u, second)`;
    `build_from_axes(.., m, u, second, .., true)`. (At least one of the two auto
    axes is non-collinear with any single in-plane `u`.)
  - C: assert `in_plane(m, u) && in_plane(m, v) && !collinear(u, v)`;
    `build_from_axes(.., m, u, v, .., false)`.
  - D: assert `!collinear(u, v)`; `m = derive_miller(u, v)`;
    `build_from_axes(.., m, u, v, .., false)`. (By construction `(u × v) · n > 0`,
    so it is already right-handed; `enforce` is moot.)

### `is_compatible` must compare the in-plane axes

`DrawingPlane::is_compatible` gates 2D boolean operations (it is called from
`evaluator/network_result.rs` and `nodes/diff_2d.rs`). It currently compares
`miller_index`, `center`, `shift`, `subdivision` but **not** `u_axis`/`v_axis`,
justified by the comment *"u_axis and v_axis should be deterministically same if
above match."* That invariant is exactly what this feature removes: with
user-pinned axes, two planes can share the same resolved `miller_index` yet have
entirely different in-plane frames — e.g. one is case A (auto axes) and the other
is case C (explicit, rotated `u`/`v`), both with `m = (0,0,1)`. With the axes
omitted from the check, `is_compatible` returns `true` and the boolean op
combines two *different* in-plane coordinate systems as if identical — a
silently-wrong result, precisely the "never silently reconcile" failure this
design's desired-fragility philosophy exists to prevent.

Fix: extend `is_compatible` to also require `self.u_axis == other.u_axis &&
self.v_axis == other.v_axis`, and delete the now-false comment. The axes compared
are the **finalized** ones stored on `DrawingPlane` (post right-handed flip /
Gram-Schmidt), so case A vs. case A still matches as before, while a case-A plane
and a differently-oriented explicit plane are now correctly reported
incompatible and the op goes dark instead of producing wrong geometry.

## Node data, serialization, eval (`nodes/drawing_plane.rs`)

### Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingPlaneData {
    pub max_miller_index: i32,
    #[serde(with = "option_ivec3_serializer", default)]
    pub miller_index: Option<IVec3>,
    #[serde(with = "ivec3_serializer")]
    pub center: IVec3,
    pub shift: i32,
    #[serde(default = "default_subdivision")]
    pub subdivision: i32,
    #[serde(with = "option_ivec3_serializer", default)]
    pub u_axis: Option<IVec3>,
    #[serde(with = "option_ivec3_serializer", default)]
    pub v_axis: Option<IVec3>,
}
```

`node_data_creator` sets `miller_index: Some(IVec3::new(0, 0, 1))`,
`u_axis: None`, `v_axis: None`.

### Serialization — no version bump

Per `doc/cnnd_versioning.md`:

- Old files always carry `miller_index: [h,k,l]`. `option_ivec3_serializer`
  deserializes the bare array → `Some(..)` (the array form is byte-identical to
  the old `ivec3_serializer` output), so every existing file loads with the
  correct Miller index and no migration. `#[serde(default)]` covers an absent
  field → `None`.
- `u_axis`/`v_axis` are new optional properties (`#[serde(default)]`) — the
  doc's "add new property" case.
- New pins are appended to the end of the parameter list — the doc's auto-handled
  "add input pin at end" case.

The only new on-disk state, `miller_index: null` (case D), appears only in files
using this feature; that forward-incompatibility is inherent and out of scope for
migrations (which cover new-code-reads-old-file, fully satisfied here).

### Parameters / pins

Append to `parameters`:
```rust
Parameter { name: "u".into(), data_type: DataType::IVec3 },
Parameter { name: "v".into(), data_type: DataType::IVec3 },
```
`get_parameter_metadata`: mark `u`, `v` (and `m_index`) optional.

### eval

Resolve each of `m`/`u`/`v` to `Option<IVec3>` via a small override helper, then
call `from_spec`:

```rust
// pin connected -> Some(value); disconnected -> stored Option; error -> propagate
fn resolve_optional_ivec3(idx, stored: Option<IVec3>) -> Result<Option<IVec3>, NetworkResult> {
    match evaluate_arg(idx) {
        NetworkResult::None => Ok(stored),
        r if r.is_error()   => Err(r),
        r => r.extract_ivec3().map(Some)
              .ok_or_else(|| NetworkResult::Error("u/v must be an IVec3".into())),
    }
}
```

`center`/`shift`/`subdivision` keep their current `evaluate_or_default` paths.
Then:

```rust
let plane = match DrawingPlane::from_spec(unit_cell, m, u, v, center, shift, subdivision) {
    Ok(p) => p,
    Err(e) => return EvalOutput::single(NetworkResult::Error(e)),
};
```

### Eval cache (for gadget + editor feedback)

`DrawingPlaneEvalCache` currently stores `unit_cell`. Add the **resolved**
orientation so the gadget and editor reflect the effective plane — important for
case D where `m` is derived and case B where the second axis is auto-picked:

```rust
struct DrawingPlaneEvalCache {
    unit_cell: UnitCellStruct,
    resolved_miller: IVec3,   // derived in case D
    resolved_u: IVec3,
    resolved_v: IVec3,
}
```

### Subtitle / text properties

- `get_subtitle`: only show `m: (..)` when `miller_index` is `Some` and the pin
  is disconnected; show `u:`/`v:` when set. When `m` is `None`, show a marker
  (e.g. `m: derived`).
- `get_text_properties` / `set_text_properties`: `m_index`, `u`, `v` become
  optional text properties — **present ⇒ `Some`, absent ⇒ `None`** (replace-mode
  text edits rebuild the node, so absence naturally unsets). This gives the AI
  text format a way to express the unset/derived state without a new TextValue
  variant.

## Validation — no validator change

By the `structure_designer/AGENTS.md` "blocking vs non-blocking" litmus test:
`from_spec` turns every bad combination into a clean, localized
`NetworkResult::Error` (no panic, no hang, no silently-wrong value). So there is
**no** new `validate_network` rule — the error surfaces on the node and its
downstream cone goes dark naturally, exactly the "desired fragility" behavior.

## Gadget (`provide_gadget`)

`DrawingPlaneGadget` reads `self.miller_index`, now `Option`. Drive it from the
eval cache's **resolved** values instead, so it always reflects the concrete
plane (including derived `m`). When the stored `m` is `None` (case D), disable
interactive miller-index dragging — the index is derived from `u`/`v` and not
directly editable.

## API / FFI

Extend the drawing_plane editor API data type (consumed by
`drawing_plane_editor.dart`) so `miller_index`, `u`, `v` are nullable
(`Option<IVec3>` → `APIIVec3?` in Dart), and optionally expose the resolved
miller index for read-only display. Getters/setters take `scope_path` like their
siblings (per `rust/AGENTS.md`). Regenerate bindings:
`flutter_rust_bridge_codegen generate`.

## Flutter editor (`lib/structure_designer/node_data/drawing_plane_editor.dart`)

UI precedent exists; no new widget needed:

- **Optional `m` / `u` / `v` fields** — the checkbox-toggle pattern from
  `collect_editor.dart` (`limit`): a checkbox whose unchecked state sets the
  field to `null`, with `IVec3Input` (`lib/inputs/ivec3_input.dart`) rendered
  only when checked.
- **Disable editor when the pin is wired** — the `atom_replace_editor.dart` /
  `collect_editor.dart` pattern: detect a wire whose `destParamIndex` is the
  pin's index, then `Opacity(0.5)` + `IgnorePointer` over the field with an
  italic "supplied by input" note.
- Optionally surface the **derived Miller index** (read-only) when in case D,
  read from the resolved value exposed via the API.

## Phasing & tests

Tests live under `rust/tests/` mirroring source (never inline `#[cfg(test)]`).

### Phase 1 — core geometry (`crystolecule`)

`from_spec` + helpers + finalizer refactor. `DrawingPlane::new` wrapper keeps all
existing callers green. Tests in `tests/crystolecule/drawing_plane_test.rs`:

- Case A regression (unchanged output vs current).
- Case B: `u` valid → `(u, second)`, `second` non-collinear & right-handed; `u`
  collinear with the *first* auto axis → picks the other; `u` not in plane → err.
- Case C: `u`,`v` honored verbatim; Weiss violation → err; collinear `u`,`v` →
  err; a left-handed `(u, v)` pair is accepted unchanged (decision 6, no flip).
- Case D: `m` derived & reduced (e.g. `u=[1,0,0]`, `v=[0,1,0]` → `m=(0,0,1)`;
  a non-primitive example to exercise `reduce`); parallel `u`,`v` → err.
- Case D handedness: the resulting `(u, v, n)` is right-handed
  (`(u_axis × v_axis) · normal > 0`) **without** a flip — pins the "right-handed
  by construction" assumption that lets case D set `enforce_right_handed = false`
  (depends on `reduce_to_primitive` preserving the sign of `u × v`).
- Under-specified / `v`-only error combos.
- Magnitude preservation: `u=[2,0,0]` yields a 2× `effective_unit_cell` length
  vs `u=[1,0,0]`.
- `is_compatible`: two planes with the same `m` but different explicit axes
  (case A auto vs. case C explicit, both `m=(0,0,1)`) are **incompatible**; two
  case-A planes with the same `m` remain compatible (regression).

### Phase 2 — node data + serialization + eval

Struct change, pins, eval resolution, subtitle, text props. Tests:

- `tests/structure_designer/cnnd_roundtrip_test.rs`: an old fixture with
  `miller_index: [h,k,l]` loads as `Some`; a new node with `m` unset + `u`/`v`
  set round-trips; `u`/`v` absent → `None`.
- Eval-through-network for each case (A–D) and representative errors.
- **Three-state resolution precedence** (`resolve_optional_ivec3`), tested
  per pin independent of the geometry cases above:
  - pin connected → wired value wins, even when the stored field is set to a
    different value;
  - pin disconnected + stored field set → stored value used;
  - pin disconnected + stored field unset → input absent (`None`);
  - a **mixed** combination (e.g. `m` from the stored field, `u` from a wired
    pin) resolves to the expected `(m, u, v)` triple — the wiring most likely to
    be mis-indexed;
  - a wired pin whose value is an error propagates the error (not silently
    treated as absent).
- **Text properties round-trip** (`get_text_properties` / `set_text_properties`):
  `m_index`/`u`/`v` present ⇒ `Some`, absent ⇒ `None` (replace-mode rebuild
  unsets); a node with `m` unset (`m: null`, case D) and one with explicit `u`/`v`
  each survive serialize → parse → serialize unchanged.
- **Eval cache holds resolved values:** after eval, `DrawingPlaneEvalCache`
  carries the *resolved* orientation, not the stored one — case D exposes the
  derived `m` (stored `None`), and case B exposes the auto-picked second axis.

### Phase 3 — API + FFI + Flutter

API data type + scoped getters/setters, codegen, editor (checkbox toggles +
pin-disable), gadget resolved-value wiring + derived-`m` read-only display.
`flutter analyze` clean; manual `flutter run` walkthrough of all four cases.

## Files touched

- `rust/src/crystolecule/drawing_plane.rs` — `from_spec`, helpers, finalizer,
  `is_compatible` (now also compares `u_axis`/`v_axis`).
- `rust/src/structure_designer/nodes/drawing_plane.rs` — struct, pins, eval,
  cache, subtitle, text props.
- `rust/src/api/structure_designer/…` — drawing_plane editor data type +
  getters/setters.
- `lib/src/rust/…` — regenerated bindings (do not hand-edit).
- `lib/structure_designer/node_data/drawing_plane_editor.dart` — UI.
- `rust/tests/crystolecule/drawing_plane_test.rs`,
  `rust/tests/structure_designer/cnnd_roundtrip_test.rs` (+ eval tests).
- `doc/cnnd_versioning.md` — **already updated** alongside this design with a
  general "make an existing required property optional (`T` → `Option<T>`)"
  subsection (no new format version is needed for this feature).
