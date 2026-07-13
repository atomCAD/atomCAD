# Design: Pickable view-up axis for 3D navigation (issue #349)

**Issue:** https://github.com/atomCAD/atomCAD/issues/349 — "add feature:
different axis than global z for on-screen-vertical in 3D navigations
pickable"

**Related:**
- #391 "unified direction xor miller-plane picker mode" — its use case (B)
  is this feature; this design deliberately provides the API seam #391 will
  plug into. The picker UI built here is a placeholder that #391 replaces.
- #97 "Expanding on 3D navigation modes" — the "free 3D navigation"
  (trackball) alternative mentioned in #349 belongs there, not here.
- #355 / #392 / #393 — stereographic picker improvements and symmetry
  analysis; out of scope, consumed later via #391.

## Motivation

Working on non-(100) crystal surfaces — (111), (110) — keeps the object in
the `Crystal` phase with the surface tilted away from the world Z axis
(rotating the atoms themselves would force a phase break to `Molecule`,
losing all `Crystal`-phase processing benefits). Turntable navigation with
world Z as the fixed screen-vertical makes orbiting around such a tilted
surface awkward: the surface never levels out on screen.

The fix is to let the user pick a different axis to act as the turntable's
screen-vertical — typically the normal of the crystal plane they are
working on.

## Current state (analysis)

**Camera per network.** Each `NodeNetwork` stores `camera_settings:
Option<CameraSettings>` (`structure_designer/camera_settings.rs`: eye,
target, up, orthographic, ortho_half_height, pivot_point), serialized in
`.cnnd` (`serialization/node_networks_serialization.rs`,
`SerializableCameraSettings`) and restored on network switch. Every camera
mutation funnels through `sync_camera_to_active_network`
(`api/api_common.rs`), which also sets the dirty flag. Camera state is
deliberately *not* undo-tracked.

**Background grid follows the active node.** `scene.unit_cell` is the unit
cell of the **active node's** evaluated output (`structure_designer.rs`
~line 1193: taken from the displayed node whose id equals
`active_node_id`; `None` if the active node is hidden or its output
carries no lattice). The background tessellator
(`display/coordinate_system_tessellator.rs`) falls back to
`UnitCellStruct::cubic_diamond` when `None`. Drawing-plane outputs
additionally get a grid drawn *in the tilted plane*
(`tessellate_drawing_plane_grid_and_axes`).

**Navigation is the one Z-locked component.** Orbit lives in Flutter —
`lib/common/cad_viewport.dart::rotateCamera()` — a turntable that
hard-codes world +Z in exactly two places: the horizontal-orbit axis
(`vertAxis`, line ~521) and the no-roll up reference (`globalUp`, line
~538). Canonical views (`renderer/camera.rs::set_canonical_view` /
`get_canonical_view`) hard-code the Cartesian frame too.

**DOF accounting.** A camera pose has 6 DOF. Turntable navigation leaves
position free (orbit + pan + zoom = 3 DOF) but constrains orientation to
"camera-right ⊥ up-axis" (zero roll relative to the up axis) — 2 of 3
orientation DOF. The reachable set is therefore the 5-DOF family of
roll-free poses *with respect to the current up axis*. Changing the up
axis swaps this set for a different one; the current orientation is
generically **outside** the new set (it is "rolled" w.r.t. the new axis).
Today's code self-corrects such poses silently on the first drag (up is
re-derived from Z every increment); with a pickable axis that correction
must instead happen explicitly at the moment the axis is set (decision D3)
or the first drag after switching produces a visible roll snap mid-drag.

## Non-goals

- **No unconstrained trackball mode.** #349 mentions it as an alternative;
  it is an independent navigation mode tracked by #97. This design only
  reserves a UI slot for it (D7).
- **No gizmo / stereographic picker work.** That is #391 (+ #392, #393).
  The interim input UI reuses the existing `MillerIndexMap` widget as-is.
- **No symbolic (re-resolving) up axis.** The stored state is a resolved
  world-space vector (D1); it does not track later lattice changes.
- **No automatic axis changes on node activation or network edits.** The
  axis changes only by explicit user action, plus the per-network restore
  on network switch (a network without saved settings restores the
  default `+Z`; D8).
- **No undo integration.** Camera settings are not undo-tracked today;
  the up axis follows the same rule (it is part of `CameraSettings`).
- **No change to pan, zoom, pivot picking, or the background grid.** The
  grid keeps showing the honest world/lattice orientation; the tilted
  drawing-plane grid already provides the "floor" for tilted surfaces.

## Design decisions

**D1 — Store the resolved world-space unit vector, not the symbolic
index.** `nav_up: DVec3` (default `+Z`) plus a cosmetic provenance label
`nav_up_label: String` (e.g. `"Z"`, `"(111)"`, `"[110]"`) live in
`CameraSettings`, mirrored on `Camera` for runtime use (Phase 1).
Resolving symbolically on every lattice change would require answering
"which node's lattice?" continuously and would re-orient the camera
behind the user's back. Pick-time resolution keeps the camera
model dumb and predictable.

**D2 — Miller plane and lattice direction are strictly separate inputs.**
For non-cubic lattices the (hkl) plane normal is *not* the [hkl] lattice
direction (#391 stresses this; #349's author warns about it too). Two
distinct resolution paths, both already implemented in `UnitCellStruct`:

- plane `(hkl)` → `ivec3_miller_index_to_plane_props(hkl).normal`
  (reciprocal-space normal)
- direction `[uvw]` → `ivec3_lattice_to_real(uvw).normalize()`
  (direct-space direction)

**D3 — Roll re-alignment at set time.** Setting a new axis `n` projects
the camera into the new reachable set immediately, by a pure roll about
the current forward vector `f`:

```
u' = normalize(n − f · (n·f))      // n projected ⊥ f; u'·n > 0 by construction
```

Eye, target, and forward are unchanged — whatever was centered stays
centered; the image rotates on screen until `n` reads as vertical. This is
the feature's visible confirmation, not a workaround.

Note this is *not* always the minimal roll: `u'` always has a positive
dot with `n`, so when the current up points *against* the new axis (the
user is viewing the surface from the underside — exactly the state before
picking "from displayed plane" on a back-facing plane) the snap
approaches 180°. That sign choice is deliberate: it preserves the
turntable invariant `up·axis > 0` that today's Z code re-establishes on
every drag; sign-matching the current up instead would leave horizontal
drags feeling inverted until the user crossed a pole. Accepted cost: a
one-time roll snap, at worst 180°, at the moment of an explicit user
action (see the animation note under Future work). Degenerate case
`f ∥ ±n` (projection ≈ 0, threshold 1e-6): keep the current up unchanged —
any roll is equally valid, mirroring the existing pole guard in
`rotateCamera`. Implemented in Rust (testable, and shared by every entry
point) rather than in Flutter.

**D4 — Canonical views follow the nav frame.** `Top` means "look along
−nav_up", etc. A user who set (111)-up wants Top to face the (111)
surface; keeping canonical views global would make the feature feel
half-installed.

The rotated frame must be constructed explicitly, **not** via
`DQuat::from_rotation_arc(DVec3::Z, nav_up)`: the arc rotation only pins
where Z goes — the azimuth around `nav_up` is whatever the great-circle
path happens to produce, so Front/Back/Left/Right would point in
directions with no relationship to the lattice, the world axes, or
anything the user can predict (and for `nav_up ≈ −Z` the whole frame
flips about an arbitrary axis). Deterministic is not enough; the side
views must stay as world-aligned as possible:

```
Z' = nav_up
Y' = normalize(Y − Z'·(Y·Z'))    // world +Y projected ⊥ nav_up
     (fallback when nav_up ∥ ±Y, threshold 1e-6:
      Y' = normalize(Z − Z'·(Z·Z')), world +Z projected)
X' = Y' × Z'
```

This reduces to the identity for `nav_up = +Z` (today's behavior is
byte-identical), makes `Front` the tilted view closest to today's Front,
and gives `nav_up = −Z` a clean 180° flip about Y instead of an arbitrary
one. `get_canonical_view` compares against the same rotated directions so
the dropdown indicator stays consistent.

**D5 — Miller/direction input resolves against `scene.unit_cell`** (the
active node's lattice), with the same `cubic_diamond` fallback the
background grid uses. Invariant for the user: *the lattice the background
grid is drawn from is the lattice your index is interpreted in.* The
dialog displays which lattice it is resolving against, so the fallback is
never silent.

**D6 — Persistence via serde default, no version bump.** New fields on
`SerializableCameraSettings` default to `+Z` / `"Z"`. Old files were
authored under a Z turntable, so the default is semantically correct for
them; no `.cnnd` migration.

Implementation trap: a plain `#[serde(default)]` on a `DVec3` yields
`(0,0,0)` — a zero `nav_up` NaN-poisons the nav-frame math for **every
old file**. The field must use a custom default fn
(`#[serde(default = "default_nav_up")]` returning `DVec3::Z`). The
`SerializableCameraSettings → CameraSettings` conversion additionally
sanitizes: a non-finite or near-zero (< 1e-6) vector falls back to
`+Z` / `"Z"`, anything else is re-normalized. The setters' zero-vector
check (Phase 2) guards user input only, not this deserialization path.

**D7 — The picking UI is an explicitly replaceable front-end.** All
picking flows call the same Rust setters (Phase 2). The interim dialog
(Phase 3) is what #391's use case (B) later replaces; the camera-row
control is shaped as the "navigation up-axis" entry so #97's free mode and
#391's gizmo slot in without redesign.

**D8 — No implicit axis changes; no leaking across networks.** Activating
another node, editing the network, or loading a file never rewrites
`nav_up` except by restoring the per-network saved value — and a network
with **no** saved `camera_settings` counts as "saved value = default".
Today `apply_camera_settings(None)` leaves the whole camera untouched;
that is benign for eye/target but would silently carry a tilted `nav_up`
into fresh networks, and the first camera drag there would then *persist*
the leaked axis via `sync_camera_to_active_network`. So
`apply_camera_settings` is extended: on `None` it calls
`Camera::reset_nav_up()` (default axis + label, then the D3
re-alignment — so `up` may roll while eye/target stay untouched as
today), followed by `update_camera_buffer()` since the camera may now
have changed on this branch too. This is safe because every current call
site (`structure_designer_api.rs`: network switch, new network,
duplicate, navigate back/forward, file load) is a network-restore path
where `None` means exactly "this network has no saved settings". Navigation itself can never leave the reachable set, so
D3's re-alignment at set time plus this restore rule are the only
correction points needed.

---

## Phase 1 — Camera model (Rust core + persistence)

### `renderer/camera.rs`

- Add `pub nav_up: DVec3` and `pub nav_up_label: String` to `Camera`
  (alongside `pivot_point`, which is the precedent for
  navigation-not-projection state on this struct). The label must live
  here too — `sync_camera_to_active_network` rebuilds `CameraSettings`
  purely from `renderer.camera` fields on every camera move, so a label
  stored only on `CameraSettings` would be wiped by the first drag.
  Initialize to `DVec3::Z` / `"Z"` everywhere `Camera` is constructed.
- Add `pub fn nav_frame(&self) -> DQuat` building the explicit D4 basis
  (`Z' = nav_up`, `Y'` = world +Y projected ⊥ `nav_up` with the ∥±Y
  fallback, `X' = Y' × Z'`) via
  `DQuat::from_mat3(&DMat3::from_cols(x, y, z))`.
- Rework `set_canonical_view`: the per-view `(view_dir, up)` table stays,
  but both vectors are rotated by `nav_frame()` before use.
- Rework `get_canonical_view`: compare `view_dir` against the rotated
  cardinal directions (same epsilon).
- Add `pub fn realign_up_to_nav_axis(&mut self)` implementing D3
  (set `up` to `nav_up` projected onto the plane ⊥ forward, normalized;
  no-op within the 1e-6 degeneracy threshold).
- Add `pub fn reset_nav_up(&mut self)` — set `nav_up = DVec3::Z`,
  `nav_up_label = "Z"`, then call `realign_up_to_nav_axis()`. Used by
  D8's `None`-restore rule and Phase 2's `reset_view_up`.

### `structure_designer/camera_settings.rs`

- Add `nav_up: DVec3` (default `DVec3::Z`) and `nav_up_label: String`
  (default `"Z"`).

### Wiring

- `sync_camera_to_active_network` and `apply_camera_settings`
  (`api/api_common.rs`) copy the new fields both ways;
  `apply_camera_settings(None)` calls `camera.reset_nav_up()` +
  `update_camera_buffer()` per D8 instead of doing nothing (eye/target
  stay untouched as today; `up` may roll-realign).
- `serialization/node_networks_serialization.rs`: add the two fields to
  `SerializableCameraSettings` with custom-fn serde defaults
  (`default_nav_up() -> DVec3::Z` — see the D6 trap) and sanitization in
  the from-serializable conversion; update the to/from conversions.

### Tests

`rust/tests/renderer/camera_test.rs` (`Camera` math is GPU-free — no
renderer test crate exists yet, so also create the crate root
`rust/tests/renderer.rs` containing
`#[path = "renderer/camera_test.rs"] mod camera_test;`, mirroring the
existing test crates):

- `realign_up_to_nav_axis`: generic case (forward preserved, resulting up
  ⊥ forward, coplanar with `{forward, nav_up}`, positive dot with
  `nav_up`), already-aligned case (no-op), underside case (current up
  anti-aligned with `n` → result still satisfies `u'·n > 0`), degenerate
  case (forward ∥ n → up unchanged).
- `nav_frame`: identity for `nav_up = +Z`; 180°-about-Y frame for
  `nav_up = −Z`; the `Y'` fallback kicks in for `nav_up = ±Y`; under a
  tilted axis, `Front`'s view direction has the largest world-`+Y`
  component of the four side views (the D4 "closest to today's Front"
  property).
- `reset_nav_up`: restores `Z`/`"Z"` and re-aligns up.
- Canonical views under a tilted `nav_up`: `Top` looks along `−nav_up`;
  `get_canonical_view(set_canonical_view(v)) == v` round-trip for all six
  views under both `Z` and a tilted axis.

The serde behavior is a serialization test, not camera math, so it goes in
`rust/tests/structure_designer/` alongside the existing `.cnnd` tests
(`cnnd_roundtrip_test.rs`), mirroring the source hierarchy:

- Round-trip of `nav_up`/`nav_up_label`; old-file default (deserialize a
  settings blob **without** the fields → `Z`, not `(0,0,0)`); sanitization
  (blob with a zero or non-finite `nav_up` → `Z`; non-unit vector →
  normalized).

## Phase 2 — API surface (resolution + setters)

All in `rust/src/api/common_api.rs` (camera domain), `#[frb(sync)]`,
followed by `flutter_rust_bridge_codegen generate`. Every setter ends
with: update `camera.nav_up` + `nav_up_label`, call
`camera.realign_up_to_nav_axis()`, `sync_camera_to_active_network`, and a
lightweight refresh so the viewport re-renders.

- `set_view_up_axis(axis: APIVec3, label: String) -> Option<String>` —
  raw normalized vector; the escape hatch every other entry point funnels
  through. Returns an error string for a (near-)zero vector, else `None`.
- `set_view_up_from_miller_plane(hkl: APIIVec3) -> Option<String>` —
  resolves per D2/D5; label `"(h k l)"`. Error for zero index or plane
  props failure.
- `set_view_up_from_lattice_direction(uvw: APIIVec3) -> Option<String>` —
  resolves per D2/D5; label `"[u v w]"`. Error for zero direction.
- `set_view_up_from_active_drawing_plane() -> Option<String>` — if the
  active node's **interactive pin** (lowest-indexed displayed output pin,
  the same rule hit-testing uses — `NodeSceneData::interactive_pin_index()`)
  carries a construction plane, use its plane normal
  (`ivec3_miller_index_to_plane_props(miller_index).normal` on the plane's
  own `unit_cell`); label from its Miller index. Error if that pin does
  not carry a plane. This is the one-click path for the motivating workflow.

  **A "construction plane" is broader than a `DrawingPlane` output**
  (as-built correction — the original spec below assumed the interactive
  output *is* a `DrawingPlane`). A `rect`/`circle`/`polygon`/… node outputs
  `Geometry2D`, whose `GeometrySummary2D` **embeds** the `drawing_plane` it is
  drawn on — the same plane its downstream `extrude` reads for its normal — so
  the action must find it there too, not only on a literal `drawing_plane`
  node. Two things follow: (1) extraction is `NetworkResult::construction_plane()`
  (matches both `DrawingPlane` and `Geometry2D.drawing_plane`); (2) the scene's
  `NodeOutput` is **lossy** (no `Geometry2D` variant — it collapses to a point
  cloud/mesh and drops the plane), so the plane cannot be read back from
  `NodeOutput` at API time. It is instead **derived onto `NodeSceneData`**
  (`construction_plane: Option<DrawingPlane>`, from the interactive pin's
  wire-level `NetworkResult`) during `generate_scene`, mirroring the existing
  `unit_cell` field, and the setter reads that. See
  `rust/src/structure_designer/evaluator/AGENTS.md` ("Scene Output Types").

**Error decisions live in the core, not the wrappers.** So the failure
branches are testable under the `rust/AGENTS.md` "test the core, skip the
API wrapper" rule, the two index setters delegate to plain functions that
return `Result<DVec3, String>` (or `Option`), and the setters only map
that into the returned error string + refresh:

- `resolve_miller_plane_up(cell: &UnitCellStruct, hkl: IVec3) -> Result<DVec3, String>`
- `resolve_lattice_direction_up(cell: &UnitCellStruct, uvw: IVec3) -> Result<DVec3, String>`
- `drawing_plane_up(plane: &DrawingPlane) -> (DVec3, String)` — pure
  extraction from an already-resolved plane; the *selection* of the
  interactive pin and the "is it a `DrawingPlane`" test stay in the
  (skippable) API setter, so this helper takes the plane by value and is
  unit-testable on its own.
- `reset_view_up()` — delegates to `Camera::reset_nav_up()` (back to
  `Z` / `"Z"`, re-aligned per D3).
- `get_view_up() -> APIViewUpInfo { axis: APIVec3, label: String,
  is_default: bool, lattice_source_label: String }` — for the dialog and
  the camera-row indicator. `is_default` = `nav_up` within epsilon of
  `+Z` (drives the highlight in Phase 3). `lattice_source_label` reports what
  `scene.unit_cell` currently is (active node's name, or the
  "cubic diamond (fallback)" case) per D5.
- Extend `APICamera` with `nav_up: APIVec3` (consumed by the Flutter
  turntable math).

### Tests

`rust/tests/renderer/camera_test.rs` (same crate as Phase 1 — the
resolution helpers are pure and GPU-free; the `set_view_up_*` API
wrappers themselves are skipped per `rust/AGENTS.md`):

- `resolve_miller_plane_up` / `resolve_lattice_direction_up` on a
  **non-cubic** (e.g. hexagonal) cell: for the same index triple the two
  results **differ**, and each matches a hand-computed expected vector.
- The same two on a **cubic** cell: plane-normal and direction **coincide**
  for `(111)`/`[111]` — the contrast that makes D2's separation visible.
- Error paths: zero `(hkl)`, zero `[uvw]`, and (for the raw setter) a
  near-zero axis each return `Err`, so the setter emits its error string.
- `drawing_plane_up`: a `DrawingPlane` payload yields its plane normal +
  Miller-index label; construct the plane on a non-cubic cell so the
  normal is confirmed to be the reciprocal-space normal, not the lattice
  direction.

## Phase 3 — Flutter (navigation math + interim UI)

### Navigation math (`lib/common/cad_viewport.dart`)

- `rotateCamera`: replace the two hard-coded `Vector3(0,0,1)` with the
  camera's `nav_up` (available on the extended `APICamera` /
  `CameraTransform`). The existing pole guard (`newRight2.length < 0.001`
  → previous-right fallback) is axis-agnostic by construction, so it
  should carry over unchanged — but that is exactly the claim the
  walkthrough's "orbit through the pole of the tilted axis" step is there
  to confirm (see Verification), since this math is not unit-tested.
- No changes to pan, zoom, pivot adjustment, or gadget interaction.

### Interim picker UI

- **Camera row** (`lib/structure_designer/camera_control_widget.dart`):
  an "Up: ⟨label⟩" button after the ortho toggle. Non-default axis renders
  highlighted (primary color) so a rotated turntable is never a mystery.
  Clicking opens the dialog.
- **Dialog** (`DraggableDialog` per lib conventions; new
  `lib/structure_designer/view_up_axis_dialog.dart`):
  - mode toggle: **Plane (hkl)** / **Direction [uvw]** (D2 — two labeled
    modes, never mixed),
  - the existing `MillerIndexMap` (`lib/inputs/miller_index_map.dart`) +
    three int fields — same idiom as the half_space editor, zero new
    picker technology (replaced wholesale by #391 later, D7),
  - lattice-source line from `get_view_up().lattice_source_label` (D5),
  - **From displayed plane** button (enabled when applicable) →
    `set_view_up_from_active_drawing_plane`,
  - **Apply**, **Reset (Z)**, **Close**. Errors surface inline in the
    dialog (they are validation feedback, not snackbar events).
- **Model** (`structure_designer_model.dart`): thin wrappers forwarding to
  the API + `refreshFromKernel()`; no scope_path (camera is global to the
  active network, like the existing camera methods).

### Verification

Manual walkthrough (per the project's thin-editor-UI testing stance):
build a (111) `drawing_plane` scene, set up from displayed plane → the
plane's grid levels out and orbit keeps it level; orbit vertically all the
way through the tilted axis' pole → no roll flip or jitter (confirms the
pole guard carried over, per Navigation math); canonical Top faces the
plane; switch networks back and forth → axis restored per network; switch
to a freshly created network (no saved camera settings) → axis resets to
Z instead of leaking (D8); save + reload → axis persists; reset → world Z
behavior identical to today; non-cubic lattice → plane vs direction
visibly differ.

## Future work (explicitly out of scope)

- **#391 use case (B):** replace the dialog body with the unified
  direction/Miller-plane gizmo; it calls the same Phase 2 setters.
- **#97:** "free 3D navigation" (trackball) as a third entry in the same
  camera-row control.
- **Animated re-alignment:** D3's roll snap (up to 180° in the underside
  case) could interpolate over ~150–250 ms for polish. The Flutter side
  can add this later by animating `move_camera` calls; the Rust model
  needs no change.
- **Symmetry-aware suggestions** (#393-dependent): offering "the ⟨111⟩
  family" instead of a single index.
