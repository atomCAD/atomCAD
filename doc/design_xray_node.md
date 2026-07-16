# Design: `xray` node — per-region semi-transparent atom display

## Motivation

Structures of interest are frequently buried inside a larger molecule or
crystal. Today the only ways to reveal them are depth culling or cutting the
occluding atoms away — both destructive to the presented picture. The `xray`
node lets the user make a *region* of a structure semi-transparent (or the
whole structure, when no region is given), so internal features show through
ghosted surroundings.

Semi-transparency is implemented **for the impostor atomic rendering method
only** (`AtomicRenderingMethod::Impostors`). In `TriangleMesh` mode x-rayed
atoms render opaque (documented limitation).

## Node specification

| | |
|---|---|
| **Name** | `xray` |
| **Category** | `AtomicStructure` |
| **Pin 0** | `molecule: HasAtoms` — required |
| **Pin 1** | `alpha: Float` — optional; wired value overrides the stored property (extrude `dir`-style precedence) |
| **Pin 2** | `region: Blueprint` — optional, last pin (Part A region-gated op convention) |
| **Output** | `OutputPinDefinition::single_same_as("molecule")` — concrete phase flows through |
| **Property** | `alpha: f64`, default `0.5`, clamped to `[0.0, 1.0]` at eval |

Semantics: for every atom inside the region (all atoms when `region` is
disconnected), record display alpha `alpha` on the structure. `alpha == 1.0`
**removes** the recording (restores opacity) — the `unfreeze` analog for
free, and chaining composes: an `xray` with region A at α 0.3 followed by an
`xray` with region B at α 1.0 re-opaques the atoms in the overlap. When two
chained nodes' regions overlap at α < 1.0, the downstream node's value wins
(last-writer-wins).

The node is a **metadata-only pass-through** exactly like `freeze`/`unfreeze`:
stateless apart from the `alpha` property, no `diff` output pin (consistent
with `freeze`/`unfreeze` being deferred from `doc/design_diff_outputs_for_atom_ops.md`),
`eval` not gated on `decorate` — the alpha is semantic display data that flows
through the network like the frozen flag.

Region membership reuses the Part A machinery verbatim
(`doc/design_blueprint_region_atom_edits.md`):
`evaluator::atom_op::map_atomic_in_region` with membership
`region_geo.implicit_eval_3d(pos) ≤ DEFAULT_REGION_MARGIN`. The geo_tree is
real-space, so atom positions test directly. Multiple regions = chained nodes.

## Storage: `AtomicStructureDecorator.atom_alpha`

```rust
/// Per-atom display alpha in [0,1). Absent = fully opaque. Runtime-only
/// display augmentation, like all decorator state (never serialized).
pub atom_alpha: FxHashMap<u32, f32>,
```

Why this shape (alternatives considered):

- **`Atom.flags` bits** — rejected. Alpha is a value, not a bit, and flag
  bits are scarce (0–6 already assigned).
- **Group table (`Vec<f32>` alphas + per-atom group index)** — rejected. The
  alpha value *is* the group: every atom touched by one `xray` node shares one
  f32, and different alphas from chained nodes coexist naturally in one map.
  A group indirection would only pay off if the renderer drew one batch per
  group with a uniform alpha — but the renderer uses **per-vertex alpha** in
  one merged transparent mesh (§Renderer), and the back-to-front sort must
  interleave quads from different alpha groups in one index buffer, so
  batching by group would actively fight it.
- **Serialized per-atom field** — unnecessary. `AtomicStructure` is never
  serde-serialized; results are recomputed from the network. Zero `.cnnd`
  migration surface.

Invariant: entries store `alpha ∈ [0.0, 1.0)` only. The setter clamps
negative values to `0.0` and treats any value `≥ 1.0` as removal of the
entry. Accessor pair on `AtomicStructure`:

```rust
pub fn set_atom_alpha(&mut self, atom_id: u32, alpha: f32); // <0 clamps to 0; ≥1 removes
pub fn get_atom_alpha(&self, atom_id: u32) -> f32;          // absent → 1.0
```

Maintenance touchpoints (both have existing precedent in
`atomic_structure/mod.rs`):

- `remove_atom` / delete paths clear the entry — mirror the existing
  `atom_display_states.remove(&id)` calls (~lines 429–443).
- `merge` (used by `atom_union`) remaps ids into the target map — mirror the
  bond-selection remap (~lines 894–904).

Documented caveat: nodes that **rebuild** structures rather than mutate a
clone in place (e.g. `materialize`, `patch_latticefill`) drop decorator state
**silently** — atoms just render opaque again, with no error or badge. In
practice `xray` sits near the display end of the chain, which is the use
case; the reference-guide page must state this explicitly (§Phase 6) so users
learn to place `xray` after rebuilding nodes.

## Bond rule: min-alpha

A bond's alpha is `min(get_atom_alpha(a1), get_atom_alpha(a2))`. A bond is
routed to the transparent mesh iff that min is `< 1.0`. Cheap (two map
lookups already adjacent to existing per-bond code), order-independent, and
fades bonds that cross the region boundary instead of leaving an opaque stick
poking into the ghost region.

## Renderer strategy: one merged transparent mesh, depth-sorted

All transparent content — ghost atoms **and** ghost bonds — lives in **one
dedicated mesh** with a unified vertex format, drawn by **one pipeline** with
standard alpha blending (depth *test* on, depth *write* off) after all opaque
content, in **back-to-front quad order** re-sorted on the CPU when the camera
or the mesh changes.

Why merged (the two-mesh alternative was rejected): separate transparent
atom/bond meshes are drawn one after the other with depth writes off, so
every ghost bond would composite over every ghost atom regardless of depth —
a systematic, orientation-independent artifact. And no per-mesh index sort
can fix ordering *across* two meshes; correct interleaving would need many
alternating draw calls or a later merge anyway. Merging up front makes
back-to-front ordering a pure index-buffer permutation of one mesh.

Why sorting ships in V1 (not deferred): once the merged mesh exists, sorting
is the small part — a per-quad depth key, an argsort, and an index-buffer
rewrite, all renderer-local (~100–150 lines, §Sorting). Deferring it would
ship the visual defect the merge exists to fix and buy a second
design/review cycle for very little code.

Two codebase facts make the depth handling exact against opaque content:

1. The impostor fragment shaders ray-cast the sphere/cylinder and write
   `@builtin(frag_depth)` from the true hit point — so transparent impostors
   depth-test **exactly** against all opaque geometry (which draws first with
   depth writes on).
2. The render-target clear alpha is `1.0` and `BlendState::ALPHA_BLENDING`'s
   alpha component (`One`/`OneMinusSrcAlpha`) keeps destination alpha at
   `1.0`, so the `Bgra8Unorm` readback to Flutter is unaffected.

Accepted residual artifacts (state these in the reference guide so the manual
walkthrough doesn't file them as bugs):

- **Per-quad center sorting is approximate.** Mutually intersecting ghost
  impostors (a bond shaft entering its atom's sphere, two overlapping ghost
  spheres) cannot be totally ordered by any quad sort — per-pixel blend order
  inside the intersection region can still be wrong. This is inherent to
  sorted alpha blending; only OIT schemes fix it, and with a uniform region
  alpha it is subtle.
- **Ghosts remain pickable.** Viewport ray hit-testing (hover readouts,
  atom_edit-style interaction on a displayed result) is unaware of alpha, so
  a nearly-invisible ghost atom still intercepts rays ahead of the buried
  atoms it reveals. Accepted for V1 and documented; alpha-aware picking is a
  follow-up if it proves annoying in practice.

Alternatives noted for a follow-up only if sorted blending proves
insufficient: hashed alpha dithering (order-independent, stipple aesthetic,
no blending) and weighted-blended OIT (extra render targets + resolve pass).
Not part of this design.

### Unified transparent vertex format + shader

The existing opaque `AtomImpostorVertex` / `BondImpostorVertex`, their WGSL
modules, pipelines, and every current tessellation call site are **untouched**
— the transparent path is purely additive.

New `renderer/transparent_impostor_mesh.rs`:

```rust
pub struct TransparentImpostorVertex {
    pub kind: u32,             // 0 = atom (sphere), 1 = bond (cylinder)
    pub position_a: [f32; 3],  // atom: center; bond: start
    pub position_b: [f32; 3],  // atom: unused;  bond: end
    pub quad_offset: [f32; 2],
    pub radius: f32,
    pub color: [f32; 3],
    pub alpha: f32,
    pub roughness: f32,        // atom branch only; bonds write 0.0
    pub metallic: f32,         // atom branch only; bonds write 0.0
    pub rim_color: [f32; 4],   // atom branch only; bonds write [0.0; 4]
}
```

(20 f32-sized fields = 80 bytes; hand-written offsets guarded by the
Phase 4 layout test.) The mesh offers `add_atom_quad(..)` / `add_bond_quad(..)`
mirroring the opaque meshes' signatures plus `alpha`, and additionally records
one **sort center** per quad (`quad_centers: Vec<Vec3>` — atom center, or bond
midpoint), the input to the depth sort.

One new WGSL module `transparent_impostor.wgsl`: vertex and fragment stages
branch on `kind` — the atom branch is the sphere ray-cast + shading from
`atom_impostor.wgsl`, the bond branch the cylinder ray-cast from
`bond_impostor.wgsl` (shared camera helpers copied along); the fragment
outputs `vec4(color, alpha)` and writes `frag_depth` from the hit point in
both branches. `kind` is constant across a quad, so the branch is uniform per
primitive and costs nothing measurable.

### Pipeline changes

- `Renderer` gains one `transparent_impostor_mesh: GPUMesh` (same pattern as
  the gadget impostor meshes) and one `transparent_impostor_pipeline`:
  `blend: BlendState::ALPHA_BLENDING`, `depth_write_enabled: false`,
  `depth_compare: Less` (matching the opaque impostor pipelines' compare).
- Draw order in the main render pass: existing opaque draws … →
  `background_mesh` → **transparent impostors** (single draw). Transparent
  comes after **everything** opaque, including the background lines. The
  gadget pass (which clears depth and draws on top) is unchanged.
- `update_all_gpu_meshes` and the `tessellate_scene_content` return tuple
  grow by **one** mesh (mechanical plumbing through the API layer). The
  renderer also retains a CPU-side copy of the mesh's `quad_centers` for
  re-sorting between mesh updates.
- Tessellation routing in `tessellate_atomic_structure_impostors` (which
  gains one `transparent_impostor_mesh` output param): per atom,
  `alpha = structure.get_atom_alpha(id)`; `alpha < 1.0` → transparent mesh,
  else opaque mesh as today. Bonds per §Bond rule. Depth culling
  (`should_cull_atom`) applies before routing, unchanged. Appearance
  (`get_atom_impostor_appearance` — element color, rim, ghost desaturation,
  selection) is untouched and composes with alpha. Guide/gadget/anchor-arrow
  tessellation paths are untouched (always opaque). The transparent mesh
  stays empty in lightweight mode and in `TriangleMesh` mode (where atoms
  tessellate opaque as before).

### Sorting

A pure function in `renderer/transparent_sort.rs`:

```rust
/// Back-to-front quad order for alpha blending. Returns a full index buffer
/// (6 indices per quad, 0-1-2 / 0-2-3 winding within each quad), quads
/// ordered by ascending view-space z (farthest first — the same key is
/// correct for orthographic and a solid approximation for perspective).
pub fn sorted_transparent_indices(quad_centers: &[Vec3], view: &Mat4) -> Vec<u32>;
```

`Renderer::render` re-sorts lazily: it caches the view matrix used for the
last sort and recomputes + `queue.write_buffer`s the index buffer only when
the camera has changed since then or the mesh was updated. Sorting permutes
quad order without changing buffer sizes, so between mesh updates it is a
fixed-size `write_buffer` — no reallocation. Tens of thousands of quads sort
well under a millisecond; orbit interaction re-sorts each frame it moves and
goes quiet when the camera rests.

## Phases

Each phase lands green on `cargo fmt && cargo clippy && cargo test` (and
`flutter analyze` where Dart is touched) with the automated tests listed.

---

### Phase 1 — Decorator storage + maintenance

**Implementation**

- Add `atom_alpha: FxHashMap<u32, f32>` to `AtomicStructureDecorator`
  (+ `new()` init).
- Add `AtomicStructure::set_atom_alpha` / `get_atom_alpha` with the
  clamp/remove-at-1.0 invariant.
- Clear entries in `remove_atom` (all deletion paths that already clear
  `atom_display_states`).
- Remap entries in `merge` alongside the bond-selection remap.

**Automated tests** — `rust/tests/crystolecule/atomic_structure_test.rs`
(existing file):

- `set_atom_alpha` stores; `get_atom_alpha` returns 1.0 for absent atoms.
- Boundary semantics: negative values clamp to `0.0`; setting `1.0` (or
  above) removes an existing entry.
- `remove_atom` clears the entry.
- `merge` carries alpha entries across with remapped ids (build two
  structures, alpha on one, merge, assert alphas land on the remapped ids).

**Manual verification** — none possible: pure backend storage with no
user-visible surface until Phase 3. The unit tests are the full coverage.

---

### Phase 2 — `xray` node

**Implementation**

- `rust/src/structure_designer/nodes/xray.rs`: `XrayData { alpha: f64 }`
  (serde, default 0.5), `NodeData` impl modeled on `freeze.rs` +
  `relax.rs`'s float property:
  - `eval`: required `molecule` (pin 0); resolve alpha as wired pin 1
    (`extract_float`) > stored property, clamp to `[0,1]`; optional `region`
    (pin 2) with the same `NetworkResult` match as `freeze`; then
    `map_atomic_in_region(input, region_geo, DEFAULT_REGION_MARGIN, |mut s,
    in_region| { for in-region ids: s.set_atom_alpha(id, alpha) })`.
  - `get_text_properties` / `set_text_properties` for `alpha`
    (`TextValue::Float`, same shape as `relax.diff_min_move`).
  - `get_parameter_metadata`: `molecule` required; `alpha`, `region`
    optional.
  - `get_subtitle`: `α = 0.30`-style readout when pin 1 is unwired.
- Register in `nodes/mod.rs` + `node_type_registry.rs`.

**Automated tests** — new `rust/tests/structure_designer/xray_test.rs`
(registered in `tests/structure_designer.rs`), modeled on `freeze_test.rs`:

- No region → every atom gets the alpha.
- With a region Blueprint → only in-region atoms get it; out-of-region atoms
  read 1.0.
- Wired `alpha` pin overrides the stored property.
- `alpha = 1.0` clears previously set alphas (chain a no-region `xray` at
  0.3 into a region-gated `xray` at 1.0 → in-region atoms read 1.0 again);
  chaining two nodes with different alphas and disjoint regions leaves both
  values on their respective atoms; overlapping regions → the downstream
  value wins.
- Out-of-range property values clamp.
- Concrete phase passes through (`Crystal` in → `Crystal` out, `Molecule` in
  → `Molecule` out); non-atomic input on pin 0 and non-Blueprint on `region`
  → localized `NetworkResult::Error`.
- Node-type snapshot: `cargo test node_snapshots` + `cargo insta review` for
  the new node type.

**Manual verification** — `flutter run`: the node is addable from the
add-node menu (AtomicStructure category) and wires up (`molecule` accepts a
Crystal or Molecule, `region` a Blueprint, the output wires onward as the
input's concrete phase); the subtitle shows the `α = 0.30`-style readout and
hides it when pin 1 is wired; creating/editing the node through the text
format panel round-trips the `alpha` property. Expected at this stage: **no
visual change in the viewport** — everything still renders opaque (rendering
lands in Phases 3–5), and there is no property editor yet (Phase 6).

---

### Phase 3 — Transparent mesh + tessellation routing

**Implementation**

- New `renderer/transparent_impostor_mesh.rs`: `TransparentImpostorVertex`
  (§Unified format), CPU mesh with `add_atom_quad` / `add_bond_quad` and the
  parallel `quad_centers` sort-center array. No change to the opaque vertex
  structs or their call sites.
- `tessellate_atomic_structure_impostors` gains one output param
  (`transparent_impostor_mesh`) and routes atoms/bonds per §Pipeline changes
  and §Bond rule.
- `scene_tessellator`: build + return the new mesh (empty in lightweight
  mode and in `TriangleMesh` mode). So the phase compiles and lands green on
  its own, update the API-layer caller of `tessellate_scene_content` to
  destructure the grown tuple and **drop** the new mesh — the renderer only
  starts consuming it in Phase 4.

**Automated tests** — new `rust/tests/display/atomic_impostor_alpha_test.rs`
(registered in `tests/display.rs`); CPU meshes, no GPU needed:

- Structure with no alphas → transparent mesh empty, opaque meshes carry all
  quads exactly as before.
- Alpha on a subset → exactly those atoms' quads (4 vertices each, `kind` 0,
  alpha value on every vertex) land in the transparent mesh; the rest stay
  opaque; totals conserved.
- Bond routing: both endpoints transparent → transparent (`kind` 1) with min
  alpha; mixed endpoints (one 1.0) → transparent with the lower alpha; both
  opaque → opaque mesh.
- `quad_centers` parallels the quads: atom quads record the atom center,
  bond quads the segment midpoint; `quad_centers.len() * 6 == indices.len()`.
- Delete-marker bonds and space-filling overstretched-bond filtering still
  behave (run one case through the transparent path to guard the routing
  refactor).

**Manual verification** — `flutter run`, impostor mode: expected (and
temporary) behavior is that x-rayed atoms and their boundary bonds
**disappear** from the viewport — they are routed into the transparent mesh,
which nothing draws until Phase 4. Check that exactly the in-region atoms
vanish, that a disconnected `region` pin makes the whole structure vanish,
that `alpha = 1.0` brings atoms back, and that `TriangleMesh` mode still
shows everything opaque and unchanged. This vanishing is the routing made
visible — do not "fix" it in this phase.

---

### Phase 4 — Renderer pipeline + draw

**Implementation**

- `transparent_impostor.wgsl`: unified kind-branching shader (§Unified
  format), fragment outputs `vec4(color, alpha)` and writes `frag_depth`.
- `Renderer`: `transparent_impostor_mesh` GPU mesh + retained CPU
  `quad_centers` copy, `transparent_impostor_pipeline` (alpha blend, no depth
  write, `Less`), draw appended after `background_mesh` in the main pass
  (emission order for now — Phase 5 adds the sort); `update_all_gpu_meshes`
  signature +1; plumb through the API scene-refresh path that calls it.

**Automated tests** — GPU pipeline code is exempt per `rust/AGENTS.md`, so
this phase's automated coverage targets the last testable seam plus
regression guards:

- Extend the Phase 3 display test file: `tessellate_scene_content` on a scene
  containing an alpha-carrying structure (impostor prefs) returns a non-empty
  transparent mesh, and an empty one in `TriangleMesh` mode — locking the
  full scene-level routing the renderer consumes.
- Vertex-layout guard: assert `size_of::<TransparentImpostorVertex>` and the
  `desc()` attribute offsets are mutually consistent (catches the classic
  hand-written-offset slip). Lives in a new
  `rust/tests/renderer/transparent_impostor_mesh_test.rs`, registered in
  `tests/renderer.rs` (GPU-free, like the existing `camera_test.rs`).
- Full suite green (`cargo test -j 4`).

**Manual verification** — `flutter run`, impostor mode: xray a sphere region
inside a diamond block. The ghost region shows the buried atoms; ghost
impostors depth-interact **exactly** with opaque geometry (an opaque atom in
front of a ghost hides it, a ghost in front of an opaque atom blends over
it); boundary bonds fade per the min-alpha rule; ghosts blend over the
background grid lines; gadgets still draw on top (gadget pass unchanged);
check both perspective and orthographic, plus ball-and-stick and
space-filling. Expected at this stage: ghost-vs-ghost blend order is
emission order and **looks wrong** (bonds over atoms, popping while
orbiting) — that is Phase 5's job, not a Phase 4 bug.

---

### Phase 5 — Back-to-front sorting

**Implementation**

- `renderer/transparent_sort.rs`: pure `sorted_transparent_indices`
  (§Sorting).
- `Renderer::render`: lazy re-sort — cache the view matrix used for the last
  sort plus a mesh-update generation counter (bumped whenever
  `update_all_gpu_meshes` uploads a new transparent mesh); recompute and
  `write_buffer` the transparent index buffer only when either changes.

**Automated tests** — new `rust/tests/renderer/transparent_sort_test.rs`
(registered in `tests/renderer.rs`, next to the existing GPU-free
`camera_test.rs`):

- Quads at distinct depths come back farthest-first for a perspective view
  matrix and for an orthographic one (including a rotated camera — key is
  view-space z, not world z).
- Each quad's 6 indices keep the 0-1-2 / 0-2-3 winding relative to its
  4-vertex base; the output is a permutation of the input quads (every index
  appears, count preserved).
- Degenerate inputs: empty mesh, single quad.

**Manual verification** (required, per thin-GPU-layer policy) — `flutter
run`, impostor mode; xray a sphere region inside a diamond block:

- Phase 4's ghost-vs-ghost defects are gone: no systematic bonds-over-atoms
  compositing, no popping/shimmer in deep ghost stacks while orbiting slowly
  (perspective **and** orthographic — the sort key must hold in both).
- The sort tracks the camera: orbit, stop, orbit again — ordering never goes
  stale after the camera rests (exercises the lazy re-sort dirty flag).
- Performance: on a large ghost region (thousands of atoms), orbiting stays
  smooth — the per-frame re-sort + index upload must not be felt.
- Confirm the accepted intersecting-impostor artifact class (§Renderer) is
  the only one visible (bond shafts entering their own atom's sphere,
  heavily overlapping ghost spheres).

---

### Phase 6 — API, Flutter property editor, reference guide

**Implementation**

- `rust/src/api/structure_designer/xray_api.rs`: `get_xray_data` /
  `set_xray_data(scope_path, node_id, data)` — thin, `#[frb(sync)]`,
  **scope_path-taking** (hard rule), mirroring `relax_api.rs` including its
  refresh + undo behavior. Run `flutter_rust_bridge_codegen generate`.
- `lib/structure_designer/node_data/xray_editor.dart`: alpha slider (0–1) +
  numeric field, following `relax_editor`/`free_sphere_editor` conventions;
  register in the property-panel dispatch.
- Reference guide: new `doc/reference_guide/nodes/xray.md` (pins, alpha
  semantics, region gating, impostor-only note + TriangleMesh fallback) +
  link from the node index page. Must include the documented limitations:
  ghost atoms remain pickable/hoverable; downstream rebuilding nodes
  (`materialize`, `patch_latticefill`, …) silently drop the transparency —
  place `xray` after them; intersecting ghost impostors can blend slightly
  wrong (§Renderer accepted artifacts).

**Automated tests**

- Rust API-level test in `xray_test.rs`: set data through the
  `StructureDesigner`-level setter, assert re-eval applies the new alpha and
  that the edit is undoable/redoable (undo restores the previous alpha —
  "persisted mutations must be undoable").
- `flutter analyze` clean (no new warnings over baseline).

**Manual verification** (thin editor layer policy) — `flutter run`:

- Slider and numeric field stay in sync; dragging the slider updates the
  viewport live; the node subtitle tracks the edited value.
- Undo/redo from the UI (Ctrl+Z / Ctrl+Y) restores both the panel value and
  the viewport transparency.
- The editor works on an `xray` node **inside a zone body** (property panel
  passes the scope chain — exercises the scope_path-taking API rule).
- Wiring the `alpha` pin makes the wired value win over the panel value —
  same pin-over-property precedence as `extrude`'s `dir`; the panel keeps
  showing the stored value, and the subtitle hides while the pin is wired.
- The reference-guide page reads correctly and is linked from the node index
  page; it states the three documented limitations (pickable ghosts,
  rebuild nodes dropping alphas, intersecting-impostor blending).

---

## Depth falloff (`opaque_depth`) — follow-up, 2026-07-16

Added after user feedback (mechadense) on the shipped feature: with a uniform
alpha, the interior-atom **depth culling** optimization
(`space_filling_cull_depth`, `display/atomic_tessellator.rs::should_cull_atom`)
becomes *visible* through the ghosted shell — you see a hollow block. Raising
the cull depth fills the void but replaces it with a volumetric fog that is
harder to see into, and forces alpha to an extreme.

**Semantics.** `opaque_depth: Float` (Å), stored property + optional pin
(wired > stored, same precedence as `alpha`). Non-positive = ramp off, `alpha`
applied uniformly = pre-ramp behavior exactly. Positive:

```
alpha_eff = lerp(alpha, 1.0, smoothstep(0, opaque_depth, atom.in_crystal_depth))
```

`alpha` is reinterpreted as the alpha *at the surface* (depth 0); it keeps its
old meaning whenever the ramp is off. Pure helper `xray::depth_ramped_alpha`,
unit-tested independently of the node.

**Why this shape.**

- **`in_crystal_depth` already exists** on every `Atom` (f32, Å, set at
  lattice-fill time as `-sdf`). The user assumed depth wasn't carried per atom
  and that adding it would hit the "32 limit" — that limit is the *tag* system
  (`tag_bits: u32`), which is unrelated. No new metadata, no renderer change,
  no shader change: the node just writes different values into the existing
  `atom_alpha` decorator.
- **The ramp reaches 1.0 at a stated depth** rather than decaying
  asymptotically (the user suggested exponential). This is load-bearing, not
  cosmetic: `set_atom_alpha` *removes* entries at ≥ 1.0, so ramped-to-opaque
  atoms route to the **opaque** mesh, write depth, and occlude the culled
  hollow behind them. That is what makes the artifact disappear at the default
  cull threshold instead of merely making it less obvious. An exponential never
  reaches 1.0 (needs an epsilon cutoff) and its decay constant is a less
  intuitive knob than "opaque at N Å". Smoothstep over linear only to avoid a
  visible kink at the surface.
- **Pin appended after `region`**, bending the "region is the last pin"
  convention. `Node.arguments` is a positional `Vec<Argument>` with no pin
  names in the `.cnnd`, and `repair_network_arguments` only grows/truncates at
  the tail — inserting at index 2 would silently reinterpret every existing
  `region` wire as an `opaque_depth` wire. Appending keeps 0..=2 stable and
  needs no migration; a reorder would have cost a file-version bump purely for
  cosmetics.
- **Serde-default `0.0`** ⇒ pre-ramp `.cnnd` files load and render identically.

**Deliberately not exposed:** a falloff-curve picker or exponent (the user
called a general function overkill; one shape + one length parameter covers the
use case); a per-node cull-depth override (the ramp subsumes it, and culling is
a viewport preference, not node state — the user correctly predicted that
coupling them would be a messy hack); a Heaviside mode (it is just a small
`opaque_depth`).

**Known limitation (documented in the reference guide, not a bug):**
`in_crystal_depth` is only meaningful for lattice-filled atoms — imported and
hand-placed atoms default to `0.0` and stay at the surface alpha. It is frozen
at fill time (relax/move don't update it), and `atom_cut` deletes atoms without
re-deriving it, so atoms exposed on a cut face keep their original deep value
and render opaque at the cut. Verified by reading `atom_cut::cut_atomic_structure`
(delete-only) — this is arguably the desired look (solid face behind a cut).

---

## Explicitly out of scope (follow-ups)

- Alpha-aware viewport picking (ghosts stay pickable in V1; documented).
- Transparency in `TriangleMesh` rendering mode.
- Order-independent transparency schemes (hashed alpha dithering,
  weighted-blended OIT) — only if sorted blending's intersecting-impostor
  artifacts prove unacceptable.
- Per-atom alpha authoring in `atom_edit` (regions/whole-input only for now).
