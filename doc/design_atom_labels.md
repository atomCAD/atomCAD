# Design: Atom labels — per-atom text drawn on styled atoms

## Motivation

`apply_style` (`doc/design_style_rules.md`, shipped) turns tags and elements
into per-atom **color**, **alpha**, and **render style**. Every one of those
channels answers "which atoms are these?" only indirectly — the user has to
remember that green means surface and red means dopant, and a hover popup is
the only place a name is ever spelled out.

This design adds the channel that says it directly: **text drawn on the atom**
— an element symbol, a tag name, or a fixed string — as a sixth `StyleRule`
field (its fourth *property*, alongside the two selectors). It is the natural
completion of the style-rule family and, like the rest of it, is transient
display state driven by selectors.

It builds on the same two precedents `design_style_rules.md` did — `xray`'s
decorator model and `atom_replace.rules`' shaped rule input — and changes
neither. It does, however, introduce one genuinely new thing to the renderer:
**the first textured pipeline in the application**. §Renderer strategy
justifies that at length, because it is the only part of this feature that is
not a copy of something already working.

## Guiding decisions

1. **A label is a `StyleRule` property, not a node and not a boolean.** It
   follows the family's rules verbatim: absent = leave alone, ordered
   last-writer-wins, selectors do the selecting. No new node, no new pin.

2. **The label is an explicit string with substitution tokens — never an
   implicit heuristic.** A tempting shape was "a `Bool` that means *label
   this*, resolving to the element symbol when the atom has no tags and to
   the tag when it has one". That is rejected: it is undefined for two tags,
   and it makes the rendered text depend on tag *count*, so adding an
   unrelated tag upstream silently changes what is drawn. Every other
   `StyleRule` field is explicit about what it writes; this one is too.
   `label: "{element}"` says what it does.

3. **Text is scene content, not UI overlay.** Labels are drawn by the Rust
   renderer as depth-tested 3D billboards, not as Flutter widgets over the
   viewport. §Why not a Flutter overlay makes the case; the short version is
   that a UI overlay cannot be occluded by the atoms in front of it, and for
   a crystal that is not a cosmetic flaw.

4. **Labels are runtime-only, like every other decorator channel.** Never
   serialized, recomputed each evaluation, cloned/remapped with the
   structure, silently dropped by structure-*rebuilding* nodes. So, like
   `xray` and the rest of `apply_style`, labels belong **late in the chain**,
   and the guide says so.

5. **Both rendering methods.** Unlike `alpha` (impostor-only, an inherited
   `xray` limitation), labels are their own mesh and their own pipeline,
   independent of how atoms are drawn. They therefore work in **both**
   `Impostors` and `TriangleMesh` mode for free. The only method-dependent
   input is the atom's displayed radius, which `effective_visualization` +
   `get_displayed_atom_radius` already resolve in both paths.

## The `label` field on `StyleRule`

A sixth field on the existing built-in def
(`node_type_registry.rs::built_in_record_type_defs`, currently five fields at
`:688-724`):

```text
StyleRule = {
  element:      Optional[Int],     // selector: atomic number
  tag:          Optional[String],  // selector: tag name
  color:        Optional[Vec3],    // 0–1 RGB
  alpha:        Optional[Float],   // 0–1; 1.0 restores opacity
  render_style: Optional[String],  // "ball_and_stick" | "space_filling" | "default"
  label:        Optional[String],  // NEW: text to draw; "" removes the label
}
```

No editor hint — `label` is free text, exactly like `tag` (the only other
hint-less field). Adding a field to a non-serialized built-in def is
non-breaking: pin layouts re-derive through the existing repair pass, so a
pre-existing `record_construct` with schema `StyleRule` simply gains the pin
with its wires intact. This is the same argument Phase 4 of
`design_style_rules.md` made when it added `render_style`.

**`label` has a reset value: the empty string.** Setting `label: ""` removes
the label, mirroring `alpha: 1.0` and `render_style: "default"`. This is worth
stating because it is a *contrast* with the field it sits next to: `color`
famously has no identity value (`design_style_rules.md` §The `StyleRule`
built-in record type def), and needed the "remove or reorder past the rule"
escape hatch. Text does have a natural identity, so `label` gets the clean
semantics for free. Note the asymmetry with `tag`, where empty-after-trim is
an *error* — an empty tag name can never exist, but empty label text is a
meaningful "draw nothing".

### Token expansion

The field value is a template. Expansion happens **per matched atom**, at
apply time, so a single match-all rule yields different text per atom — which
is precisely what makes `label: "{element}"` a one-rule feature rather than a
118-rule one. That is why tokens are load-bearing rather than scope creep:
without them, "show chemical symbols" is unauthorable.

| Token | Expands to |
|---|---|
| `{element}` | The atom's chemical symbol. |
| `{tag}` | The rule's own `tag` selector if it has one; else the atom's first tag; else empty. |
| `{{` / `}}` | Literal `{` / `}`. |

Anything else inside braces is a **localized error naming the rule index and
the offending token** — the same strictness `render_style` applies to unknown
strings, and for the same reason: a silently-ignored typo is worse than a
message.

`{element}` resolves exactly the way the hover popup does
(`structure_designer_api.rs:8732-8767`), and must keep doing so or the two
surfaces will disagree about the same atom:

1. If `decorator.element_name_overrides` contains the atom's (raw)
   `atomic_number`, the symbol is `P{index+1}` via
   `param_atomic_number_to_index(atom.atomic_number)`, falling back to `?`
   (`:8740-8743`) — this is what makes motif_edit parameter elements render
   as `P1` / `P2` rather than a debug atomic number. Note the override map is
   used as a **membership test** here; the mapped `String` is the parameter's
   display *name*, which the popup shows separately and a label does not use.
2. Else `ATOM_INFO[atom.atomic_number as i32].symbol`, falling back to
   `DEFAULT_ATOM_INFO` (`"X"`) (`:8758-8760`). Note the raw
   `atom.atomic_number` (not `effective_atomic_number` — the popup only uses
   that for a supplementary line), and note `ATOM_INFO` is keyed by `i32`
   while atomic numbers are `i16`, hence the `as i32` cast the existing call
   sites all use.

`{tag}`'s two-step resolution is deliberate. When the rule *has* a tag
selector (`tag: "surface"`, `label: "{tag}"`), the answer is unambiguous by
construction — the rule only matched atoms carrying it. Only the
selector-less case has to fall back to "first tag" (`atom_tags(id)` returns
names in bit order, `atomic_structure/mod.rs:367`), and that case is
documented as first-tag rather than left to look like a bug.

Non-goal: this is a substitution pass, not an expression language. If a label
ever needs computation, the answer is to compute the string upstream and wire
it in — rules are values, which is the whole point of decision 1 in
`design_style_rules.md`.

## Storage: `AtomicStructureDecorator.atom_label`

Mirrors `atom_color` point for point
(`atomic_structure_decorator.rs:78-108`):

```rust
/// Per-atom label text, already token-expanded. Absent = no label.
/// Runtime-only display augmentation, like all decorator state.
pub atom_label: FxHashMap<u32, String>,
```

Accessors on `AtomicStructure`, next to the `atom_color` trio
(`atomic_structure/mod.rs:661-673`):

```rust
pub fn set_atom_label(&mut self, atom_id: u32, text: String); // empty => clears
pub fn clear_atom_label(&mut self, atom_id: u32);
pub fn get_atom_label(&self, atom_id: u32) -> Option<&str>;   // None = no label
```

`set_atom_label` clearing on empty input mirrors `set_atom_alpha`'s
remove-at-`>= 1.0` (`:641-643`), keeping "write the identity value" and "clear" the
same operation across the family.

The three maintenance touchpoints are the same ones `atom_color` and
`atom_alpha` have, at the same code sites:

- **Delete paths** clear the entry (next to the existing `atom_alpha` /
  `atom_color` removals in `delete_atom` / `delete_lone_atom`).
- **Merge** — `add_atomic_structure` in `atomic_structure/mod.rs` (the
  function `atom_union` calls) remaps entries through its `atom_id_map`, next
  to the existing `atom_alpha` / `atom_color` / `atom_render_style` remaps.
- **Clone** is free (plain field).

Storing an expanded `String` per labeled atom is heavier than the `f32` and
`Vec3` channels beside it. That is accepted: §Readability explains why the
labeled-atom count is small by construction in any sane use, and interning is
listed as future work rather than built speculatively (per the project's
standing guidance against speculative caching).

Note the field holds an `AtomicStructure`-level string with no `display`
dependency, so the crystolecule architectural constraint (never depend on
`display` / `renderer`) holds without the mapping dance `AtomRenderStyle`
needed.

## Renderer strategy: SDF glyph atlas + billboarded glyph quads

### What already exists, and why this is cheaper than it sounds

The part that sounds hardest — a camera-facing label pinned in front of a
sphere with correct occlusion — is mostly already written.
`atom_impostor.wgsl:161-190` is a complete, shipping billboard: it takes a
world center plus a `quad_offset: vec2`, expands it into an eye-facing quad,
and already factors out `camera_right()` / `camera_up()` / `camera_backward()`
(`:50-61`) and the orthographic-vs-perspective split (`:66`). The label vertex
stage is that shader with `radius` swapped for a text scale.

What does **not** exist, anywhere in `rust/src`: a `Sampler`, a
`BindingType::Texture`, a `textureSample`, or any font/glyph code. All six
pipelines share one `PipelineLayout` with exactly two bind groups — camera and
model (`renderer.rs:311-315`). This feature is the first textured pipeline in
the codebase.

That is additive, not invasive: a label pipeline gets **its own**
`PipelineLayout` with a third bind group; the shared one and the six existing
pipelines are untouched.

### Why not a Flutter overlay

There is a real precedent for viewport text — `AtomTooltip` is `Positioned`
over the `Texture` widget by projecting 3D→screen in Dart
(`structure_designer_viewport.dart:1240-1265`), and `elementNumberToSymbol`
already exists Dart-side. One `CustomPaint` with cached `Paragraph` objects
could draw hundreds of labels with no GPU work at all. It is by far the
cheapest option.

It is rejected because it has **no depth occlusion**. Labels for buried atoms
would float on top of the atoms in front of them. For a crystal — the case
this feature exists for — that is not a cosmetic compromise, it is
unreadable. Recovering occlusion means a per-label ray-cast per frame through
a synchronous FFI; the depth buffer is not read back (only color is,
`renderer.rs:995`), so there is no cheaper test available. A second reason,
weaker but real: overlay text lives in the UI layer, so it would be absent
from anything that consumes the renderer's output directly, e.g. the
screenshot API.

### Why not stroke text on the existing line pipeline

Tempting: single-stroke (Hershey-style) vector fonts are tiny and public
domain, the `line_pipeline` already exists, and engineering-drawing stroke
text arguably suits a CAD app. Rejected because `line_mesh.wgsl` has no
billboard attribute — `LineVertex` is a raw world position
(`line_mesh.rs:6`). Labels would have to be CPU-retessellated on every camera
rotation, or else lie flat in world space and turn edge-on. Adding
billboarding to the line path means a new vertex format and a new pipeline —
i.e. paying most of the atlas option's cost for worse text that cannot
antialias.

### The font atlas asset — zero new dependencies

`image` is **already a dependency** with the `png` feature
(`Cargo.toml:38`; currently used only for encoding, in
`api/screenshot_api.rs`). The `png` feature covers decoding too. So:

- **Committed asset**: `rust/assets/font_atlas.png`, a single-channel (R8)
  **SDF** atlas covering printable ASCII `0x20..=0x7E` (95 glyphs). Element
  symbols and tag names are ASCII, so this covers the entire feature.
- **Committed metrics**: `rust/src/renderer/font_metrics.rs`, a generated
  `const` table: the atlas's SDF **spread** (the distance range encoded
  around each glyph) plus, per glyph, atlas UV rect, quad size, bearing, and
  advance — all in em units. A `const` table means no runtime parse and no
  serde at startup. **UV rect, quad size, and bearing describe the padded SDF
  cell** — the glyph's tight bounding box inflated by the spread on all four
  sides — never the tight box itself. This is load-bearing, not a
  convention: the outline band and the antialiasing fringe live *outside*
  the tight box, so tight-box quads would clip the outline to a hard
  rectangle at every glyph edge (the classic first bug of SDF text). Only
  `advance` stays purely typographic — pen movement is unaffected by
  padding, and the overlapping padded quads of adjacent glyphs are harmless
  because their SDF texels are empty where the glyphs don't reach.
- **Loaded** via `include_bytes!` + `image::load_from_memory_with_format`.
  `include_bytes!` is new to the codebase but `include_str!` is already the
  established way WGSL is embedded (`renderer.rs:249-266`), so this is the
  same idea for a binary.
- **Generator**: `rust/examples/gen_font_atlas.rs`, run by hand
  (`cargo run --example gen_font_atlas`) when the font or glyph set changes,
  which is approximately never. It takes a **dev-dependency** on a
  rasterizer (`ab_glyph`, Apache-2.0/MIT) — dev-dependencies do not ship in
  the `cdylib`, so the shipping binary gains no dependency. SDF generation is
  a brute-force distance transform over a supersampled glyph bitmap; the
  atlas is small and generated offline, so an O(n²) transform per glyph is
  fine and not worth optimizing.
- **Font**: a permissively-licensed sans compatible with the project's
  MPL-2.0 (e.g. DejaVu Sans Bold, Bitstream Vera license). The font file and
  its license text ship in `rust/assets/`.

**Why SDF rather than a plain bitmap.** Because label size is world-space
(§Label size), zooming in makes a label cover more screen pixels without
bound. A fixed-size bitmap atlas would go blurry exactly when the user leans
in to read it. One small SDF atlas stays crisp at any zoom for roughly one
`smoothstep` in the fragment shader. SDF also makes an **outline** nearly free
(a second distance band on the same sample), which matters more than it first
appears — it is what lets the fixed white fill stay readable against any
background rather than needing a per-atom color (§Label vertex format +
shader). Multi-channel SDF (msdf) gives sharper corners but is substantially
harder to generate; plain single-channel SDF is sufficient at label sizes.

### Label vertex format + shader

New `renderer/label_mesh.rs`. The existing impostor meshes, their WGSL,
pipelines, and every current tessellation call site are **untouched** — the
label path is purely additive.

```rust
pub struct LabelVertex {
    pub anchor_position: [f32; 3], // atom center, world
    pub plane_offset: [f32; 2],    // offset within the billboard plane, world units
                                   //   (glyph layout + centering already baked in, CPU-side)
    pub depth_offset: f32,         // push toward the eye: displayed radius + epsilon
    pub glyph_uv: [f32; 2],        // atlas UV for this corner
}
```

8 f32 = 32 bytes; hand-written offsets, guarded by a layout test the way
`TransparentImpostorVertex` is. One quad (4 vertices, 6 indices) **per
character**. `add_glyph_quad(..)` mirrors the existing meshes' `add_*_quad`
signatures.

**The vertex carries no color.** Fill and outline are both shader constants
(fragment stage, below), so a per-vertex color attribute would put the same
white on every vertex in the scene — 12 dead bytes and plumbing for a feature
that is deliberately future work. When per-rule `label_color` lands it
re-enters as an additive vertex attribute plus a tessellator argument, which is
cheap; guessing at its shape now is not.

New WGSL module `renderer/label.wgsl`. Each shader is its own `include_str!`
string — WGSL has no cross-module sharing — so `label.wgsl` re-declares the
camera/model uniforms and the three camera-basis helpers, copied from
`atom_impostor.wgsl`. Vertex stage:

```wgsl
// Model matrix first, exactly as atom_impostor.wgsl does (:165) — the billboard
// is then built in world space, so the camera basis is not model-transformed.
let anchor = (model.model_matrix * vec4<f32>(anchor_position, 1.0)).xyz;
let world = anchor
          + camera_right()    * plane_offset.x
          + camera_up()       * plane_offset.y
          + camera_backward() * depth_offset;  // camera_backward points toward the eye
output.clip_position = camera.view_proj * vec4<f32>(world, 1.0);
```

`camera_right()` / `camera_up()` are unit vectors off the view-matrix rows, so
a world-unit `plane_offset` maps to world-unit displacement directly. Every
mesh currently gets `set_identity_transform` before its draw, so the model
matrix is identity in practice — apply it anyway, both because group 1 is bound
for this pipeline and because silently ignoring it is the kind of latent bug
that only surfaces the day something sets a real transform.

**Labels use the camera-row basis in *both* projection modes** — a deliberate
divergence from `atom_impostor.wgsl`, which switches to a per-atom eye-facing
basis under perspective (`:170-181`). The sphere impostor needs the eye-facing
basis so its ray-cast quad covers the sphere; text needs a screen-aligned
basis so it stays upright and legible regardless of where the atom sits in
frame. A per-atom eye-facing basis would visibly tilt labels toward the edges
of a perspective view. Worth a comment in the shader, since it reads like an
inconsistency otherwise.

Fragment stage samples the SDF, derives fill and outline from two bands off
that single sample, and discards empty texels:

```wgsl
const FILL_COLOR:    vec3<f32> = vec3<f32>(1.0, 1.0, 1.0); // white
const OUTLINE_COLOR: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0); // black
const OUTLINE_WIDTH: f32 = 0.15;  // distance band below the 0.5 edge, in SDF units
const ALPHA_DISCARD: f32 = 0.01;  // below this, write no color and no depth

let d = textureSample(atlas, atlas_sampler, input.glyph_uv).r; // 0.5 = glyph edge
let w = fwidth(d);
let fill    = smoothstep(0.5 - w, 0.5 + w, d);
let outline = smoothstep(0.5 - OUTLINE_WIDTH - w, 0.5 - OUTLINE_WIDTH + w, d);
if outline < ALPHA_DISCARD { discard; }
return vec4<f32>(mix(OUTLINE_COLOR, FILL_COLOR, fill), outline);
```

`OUTLINE_WIDTH` must stay well inside the atlas's SDF spread — the same
spread constant `font_metrics.rs` records and the glyph quads are padded by
(§The font atlas asset) — a band wider than the encoded distance range clips
flat and the outline degrades to a hard edge. Both constants are tuning values, not
load-bearing choices; the starting numbers above are a reasonable first guess
to adjust during Phase 3's manual pass.

**Why white fill + black outline, fixed.** A label sits in front of its atom
but extends past the silhouette onto whatever is behind it, so it needs
contrast against *both* the atom's albedo and the background. Deriving the
fill from the atom's color (luminance → black or white) solves only the first
half and would still vanish against the grid. White-on-black-outline is
readable against anything, for zero fields and no per-atom logic. The SDF makes
the outline nearly free — it is a second `smoothstep` band on a sample the
shader already took — which is a large part of why SDF beats a bitmap atlas
here (§The font atlas asset).

### Depth: how "in front of the atom" works

The impostor fragment shaders write true `@builtin(frag_depth)` from the
ray-cast hit point (`atom_impostor.wgsl:207`, written at `:293`), so each sphere's depth is
geometrically correct rather than the quad's. That is what makes the label
placement trivial:

> Offset the label along `camera_backward()` by the atom's **displayed
> radius** plus a small epsilon, and depth-test normally.

Moving a point by `radius * camera_backward()` reduces its **view-space
depth** by exactly `radius`, in both projection modes — `camera_backward()` is
the view direction, and view-space z depends only on that component. The
sphere spans view-depth `[center_z - radius, center_z + radius]`, so the label
lands just in front of the sphere's nearest extent. Consequences, all of them
the ones we want and none of them special-cased:

- The label draws on top of **its own** atom.
- The label is correctly hidden when a **different** atom is in front of it.
- It works identically in orthographic and perspective.
- It works in `TriangleMesh` mode too, where sphere depth is real geometry.

The radius comes from
`get_displayed_atom_radius(atom, &effective_visualization(structure, id, global))`
(`atomic_tessellator.rs:321`, `:101`), so a `render_style: "space_filling"`
atom pushes its label out to the vdW surface automatically — the two
`StyleRule` fields compose without either knowing about the other.

### Interaction with `alpha` (xray and scene transparency)

`label` shares `StyleRule` with `alpha`, so the combination is defined here
rather than left to fall out of the pipeline:

- **Fully invisible atoms get no label.** The impostor path computes an
  effective alpha of `global_alpha * get_atom_alpha(id)` — where
  `global_alpha` is the scene-transparency preference — and skips the atom
  entirely when it is `<= 0` (`atomic_tessellator.rs:934-935`).
  `tessellate_atom_labels` applies the **same product and the same skip**.
  Without it, `label` + `alpha: 0.0` on one atom (or any labeled atom under
  `scene_alpha = 0`) would draw a floating label anchored to nothing — and,
  worse, the label's depth writes would invisibly occlude ghost atoms behind
  it.
- **Ghosted atoms (`0 < alpha < 1`) keep their label, drawn fully opaque.**
  That is deliberate: a label on a ghosted atom is precisely how a
  deliberately-faded atom stays identifiable. The label writes depth
  (§Pipeline changes), so ghosts behind it are occluded at glyph pixels —
  the same accepted artifact class as the draw-order note there, and rare
  under §Readability's few-labels guidance.

One caveat, accepted rather than special-cased: `alpha` itself is honored
only by the impostor path (guiding decision 5 — an inherited `xray`
limitation), so in `TriangleMesh` mode an `alpha: 0` atom still draws opaque
while its label is skipped anyway. The skip follows the user's stated intent
(this atom should be invisible), not the rendering method's ability to honor
it — and it keeps `tessellate_atom_labels` method-independent, which is what
lets it be called once, outside the `rendering_method` match (§Phase 3).

### Pipeline changes

Adding a mesh type is a well-worn 7-step path here (new `*_mesh.rs` → `mod.rs`
→ a `MeshType::Labels` variant + its `new_empty` arm → `new_empty_label_mesh`
/ `update_from_label_mesh` with the usual `assert!(self.mesh_type == …)` guard
→ renderer shader/pipeline/field/init/draw → `update_all_gpu_meshes` →
`tessellate_scene_content`). `transparent_impostor_mesh.rs` is the template for
all of it. Only the texture binding departs from it:

- `Renderer` gains one `label_mesh: GPUMesh` and one `label_pipeline` (same
  pattern as the transparent impostor mesh), plus the atlas texture, its
  `Sampler`, an `atlas_bind_group_layout` (group 2) and its bind group, and a
  **dedicated** `label_pipeline_layout` `[camera, model, atlas]`. The atlas
  texture needs `TEXTURE_BINDING` usage — the existing render-target texture
  deliberately does not have it (`renderer.rs:1167`), so this is a genuinely
  new usage flag in this file.
- **The atlas bind group belongs to the `Renderer`, not to `GPUMesh`.** This
  is the one place the transparent-impostor template does not transfer.
  `GPUMesh` (`gpu_mesh.rs:63-70`) knows only `model_bind_group` (group 1) and
  has no notion of a texture; `Renderer::render_mesh` (`renderer.rs:1045`)
  binds group 1 and nothing else. So the atlas bind group is a `Renderer`
  field, set **once per pass** with `set_bind_group(2, …)` before the label
  draw — the way `camera_bind_group` is already set once at `renderer.rs:900`
  — rather than threaded through `render_mesh` or stored per mesh.
- Pipeline state: `blend: BlendState::ALPHA_BLENDING`,
  **`depth_write_enabled: true`**, `depth_compare: Less`. `cull_mode: None`,
  matching the transparent pipeline's reasoning — a billboard's facing is
  meaningless, so culling could only wrongly drop a quad.
- **Draw order** in the main render pass: … atom impostors → bond impostors →
  `background_mesh` → **labels** → `transparent_impostor_mesh`. Labels go
  after everything opaque (they blend over it) and, critically, **before** the
  transparent pass. The order is forced by the depth-write asymmetry: labels
  write depth, ghosts do not. Labels-then-ghosts gives both cases correctly (a
  ghost behind a label is depth-rejected at the glyph; a ghost in front passes
  `Less` and tints the label). Ghosts-then-labels would be **wrong** — a ghost
  in front of a label wrote no depth, so the label would pass the test and
  paint over a ghost that is actually nearer. The gadget pass, which clears
  depth and draws on top, is unchanged.
- `update_all_gpu_meshes` and the `tessellate_scene_content` return tuple (a
  9-tuple today, destructured and re-passed positionally at
  `api_common.rs:442-474`) grow by **one** mesh — the same mechanical plumbing
  the transparent mesh added. The label mesh param goes after
  `transparent_impostor_mesh` and before the gadget pair, matching the tuple
  order everywhere else. `scene_tessellator.rs`'s lightweight branch returns a
  `LabelMesh::new()`, which is what makes §Text layout's "no labels in
  lightweight mode" true by construction. `dummy_renderer.rs` has no impostor
  surface and needs no change.

Note the label mesh does **not** reuse `transparent_impostor_mesh.rs`'s
`QUAD_OFFSETS` const (`:219`) — that is a square `[-1,1]` billboard, whereas a
glyph quad is a per-character rectangle whose corners come from the layout
pass.

**Why blend + depth-write rather than pure alpha-testing.** The renderer has
no MSAA (`multisample count: 1` on every pipeline), so an alpha-tested label
would have hard, visibly jagged glyph edges — throwing away the antialiasing
that is the main reason to use an SDF. Blending gives smooth edges; keeping
depth writes on preserves correct occlusion of and by other scene content; and
the `discard` on near-zero alpha stops empty texels from polluting the depth
buffer. The residual cost is that **two labels overlapping each other blend in
draw order rather than depth order** — an accepted artifact, in the same class
as the ones `design_xray_node.md` accepts, and one that §Readability's
"label few atoms" guidance makes rare by construction. No sort is needed,
which is the single biggest reason this is a smaller project than `xray` was.

### Text layout

Done on the CPU, so the shader stays trivial. The routine itself lives in
`renderer/label_atlas.rs` (next to the metrics it reads, and unit-testable
without a GPU); `tessellate_atom_labels` calls it and turns the result into
quads. Given a string it returns positioned, UV'd glyph boxes in em units:

1. Look up each `char`'s metrics; chars outside the atlas (i.e. non-ASCII)
   render as the atlas's `?` glyph.
2. Sum advances → total width. Pen starts at `-total_width / 2`.
3. Each glyph emits one quad at `pen + bearing`, sized by its metrics, all in
   em units, then scaled by the label size (§Label size) into world units and
   written into `plane_offset`.
4. Vertically center the cap height on the anchor.

**Non-ASCII is not validated, only degraded.** There is deliberately no
charset check at apply time, and the `?` fallback is the whole story. Checking
the *template* would catch only half the cases: `{tag}` expands to a tag name,
which is arbitrary user text from the `tag` node and may itself be non-ASCII —
so the fallback has to exist regardless. Given it exists, a second
half-covering validation would add an error path that buys nothing. Expansion
is per-atom anyway, and a per-atom error has nowhere good to surface. A user
who types `Ω` gets `?`, which is visible and self-explanatory; widening the
atlas is future work.

Culled atoms produce no labels: the label pass reuses `should_cull_atom`
exactly as the impostor path does, so a label can never outlive its atom.
`should_cull_atom` is **private** to `display/atomic_tessellator.rs`
(`:120`), which is why `tessellate_atom_labels` belongs in that file rather
than a new module — no reason to widen its visibility. Effectively invisible
atoms are likewise skipped (§Interaction with `alpha`). Labels are skipped in
lightweight mode, like the transparent mesh.

**The layout pass needs no atlas handle.** Glyph metrics are a `const` table
(`renderer/font_metrics.rs`), so the tessellator reads them directly; the PNG
is decoded exactly once, renderer-side, to upload the texture in
`Renderer::new`. That is why `tessellate_atom_labels` takes no atlas
parameter and the CPU layout path is trivially unit-testable without a GPU.

## Label size: world-space, one global preference

Labels are sized in **world space** — a label scales with zoom, exactly as its
atom does. Screen-space (constant pixel size) was the alternative; world-space
is chosen because a label is an annotation *of an object*, and constant-size
text would drift out of proportion with the atom it names and pile into an
unreadable mass at low zoom while looking absurdly large next to a zoomed-in
atom.

The size is one new global preference, an em height in ångström:

```rust
// AtomicStructureVisualizationPreferences (display/preferences.rs:21) gains:
/// World-space em height of atom labels, in Å. Labels scale with zoom.
pub label_scale: f32,
```

**Default `0.7` Å**, roughly a ball-and-stick carbon's diameter — big enough to
read against its own atom, small enough that two labelled neighbours do not
collide at default zoom. Clamped to `[0.05, 10.0]`: the lower bound keeps a
zero or negative value from collapsing every quad to a degenerate point (which
would look like the feature is broken rather than misconfigured), the upper is
simply past any useful size.

`AtomicStructureVisualizationPreferences` is already where the two cull depths
and `scene_alpha` live, and — usefully — the tessellator receives *exactly*
this sub-struct rather than the whole `DisplayPreferences`
(`scene_tessellator.rs:248`), so the value arrives with **no signature change**
to any existing tessellation function.

It follows `scene_alpha` through the established chain. Note there are **two
parallel structs with the same name**: the FRB API one
(`api/structure_designer/structure_designer_preferences.rs:159`, where the
field is `f64`) and the display one (`display/preferences.rs:21`, where it is
`f32`, like `scene_alpha` — the cull depths there are `Option<f64>`),
converted field-by-field by `to_display_preferences`
(`api_common.rs:113-136`). Both need the field, but **not in the same
phase**: the tessellator cannot lay out a quad without a scale, so the
display-side field lands in Phase 3, with `to_display_preferences` filling it
from a `DEFAULT_LABEL_SCALE` const (the display struct has no `Default` impl
to lean on). Phase 4 adds the API-side field, serde default, and UI, and
switches the conversion to read the user's value.

Two attributes on the API-side field are load-bearing and both fail *silently*
if forgotten:

- `#[frb(non_final)]` — the preferences UI mutates fields in place
  (`prefs.labelScale = value`), which does not compile without it.
- `#[serde(default = "default_label_scale")]` with a named default fn — this
  is what lets existing settings files load without the new field, and is why
  **no migration is needed**.

Plus the `Default` impl entry (the API-side struct is the one with a
`Default` impl), `flutter_rust_bridge_codegen generate`, and a
`preferences_window.dart` input (a `Slider` + `FloatInput` pair, with a test
key in `PreferencesKeys`). Clamping for `scene_alpha` is deliberately
duplicated in the UI *and* at the Rust use-site
(`atomic_tessellator.rs:912` — not on the struct); `label_scale` clamps in
both places the same way.

Global rather than a per-rule `label_scale` field, for the reason
`design_style_rules.md` already settled for the cull depths: the *preference*
stays global, the *rule* only says what to draw. A per-rule size is listed as
future work; nothing in the use cases forces it, and it composes later with no
migration.

## Readability: no display cap, guidance instead

There is deliberately **no cap** on the number of labeled atoms, and no
distance-based fadeout. A match-all `label: "{element}"` rule on a 100k-atom
crystal will happily render 100k labels, store 100k `String`s, and produce an
illegible screen.

That is the user's call to make, not the app's. The precedent for a cap
(`ITER_DISPLAY_CAP = 256`) exists to stop an *unbounded lazy stream* from
hanging the app; labels are bounded by the atom count and merely look bad, and
a silent cap would raise the worse question of *which* labels got dropped. The
`log`-what-you-dropped alternative is noise for a purely visual outcome.

Instead the reference guide states plainly that **labels are for a handful of
atoms** — use selectors to pick out the few atoms worth naming, exactly the
mechanism `apply_style` already gives. The failure mode is visible and
immediate, and the fix (a narrower selector) is the same thing the user was
already doing.

## Phases

Each phase lands green on `cargo fmt && cargo clippy && cargo test -j 4` (and
`flutter analyze` where Dart is touched) with the automated tests listed.

---

### Phase 1 — `atom_label` decorator plumbing

**Implementation**

- `AtomicStructureDecorator.atom_label` + `new()` init; the three accessors
  (§Storage), with `set_atom_label("")` clearing.
- Delete paths clear; merge remaps — both next to the existing `atom_alpha` /
  `atom_color` sites in `atomic_structure/mod.rs`.

**Automated tests** — `rust/tests/crystolecule/atomic_structure_test.rs`,
next to the existing `atom_color` tests:

- Set/get/clear round-trip; `get` returns `None` for untouched atoms;
  `set_atom_label("")` clears an existing entry; delete clears; merge remaps
  onto the new ids; clone preserves.

**Manual verification** — none possible (no writer, no renderer yet).

---

### Phase 2 — font atlas: generator, asset, metrics, CPU layout

**Implementation**

- `rust/examples/gen_font_atlas.rs` + the `ab_glyph` dev-dependency; commit
  `rust/assets/font_atlas.png`, the font, its license, and the generated
  `renderer/font_metrics.rs` const table.
- `renderer/label_atlas.rs`: a decode fn (the PNG via `include_bytes!` +
  `image`; `Renderer::new` will call it in Phase 3 to upload the texture),
  glyph metrics lookup, and the §Text layout routine (string → a list of
  positioned, UV'd glyph boxes in em units, horizontally centered).

**Automated tests** — new `rust/tests/renderer/label_atlas_test.rs` (registered
in `rust/tests/renderer.rs`, next to the existing
`transparent_impostor_mesh_test.rs`). All CPU, no GPU context needed:

- Atlas decodes; dimensions match the metrics table; every ASCII
  `0x20..=0x7E` glyph resolves; UV rects lie within `[0,1]`.
- Padding: the recorded SDF spread is positive, and every glyph's quad size
  is at least `2 × spread` in each dimension — quads are padded SDF cells,
  not tight boxes (§The font atlas asset), which is what keeps the outline
  band from clipping at quad edges.
- Layout: a one-glyph string centers on zero; a three-glyph string's total
  width equals the sum of advances and stays centered; a space advances
  without emitting a box; a non-ASCII char falls back to `?`. Centering is
  **advance-based** (§Text layout step 2), so these assertions measure pen
  positions / advance extents — the padded glyph *boxes* deliberately
  overhang the advance span and are not what "centered" refers to.
- Cap-height vertical centering is stable across strings of different glyphs
  (`"C"` and `"Si"` share a vertical center).

**Manual verification** — eyeball `font_atlas.png` (it is a reviewable PNG on
purpose).

---

### Phase 3 — label mesh, pipeline, tessellation, draw

The GPU phase, and the bulk of the work. Driven by the decorator set directly
— no node needed yet.

**Implementation**

- `renderer/label_mesh.rs` (`LabelVertex`, `desc()`, `add_glyph_quad`) +
  `mod.rs` registration; `renderer/label.wgsl`; a `MeshType::Labels` variant
  and its `new_empty` arm; `GPUMesh::new_empty_label_mesh` /
  `update_from_label_mesh`; atlas texture + sampler + group-2 bind group (on
  the `Renderer`, per §Pipeline changes) + dedicated `label_pipeline_layout` +
  `label_pipeline`; the draw call in its §Pipeline changes slot;
  `update_all_gpu_meshes` / `tessellate_scene_content` grow by one mesh.
- `display/atomic_tessellator.rs` gains
  `tessellate_atom_labels(label_mesh, structure, atomic_viz_prefs)` (that file
  specifically — it needs the private `should_cull_atom`; §Text layout). Per
  atom: skip if culled, unlabeled, or effectively invisible (effective alpha
  `<= 0`; §Interaction with `alpha`), else lay out the string and emit quads
  with
  `depth_offset = get_displayed_atom_radius(atom, &effective_visualization(..)) + eps`.
- `display/preferences.rs` gains `label_scale: f32` — the tessellator's em→Å
  factor (§Label size) — with `to_display_preferences` (`api_common.rs`)
  filling it from a `DEFAULT_LABEL_SCALE` const of `0.7` until Phase 4 wires
  the user preference through. This **breaks the three existing display tests
  at compile time** (`atomic_impostor_alpha_test.rs`, `atomic_color_test.rs`,
  `atomic_render_style_test.rs` — each constructs the struct literally); that
  is the intended tripwire, not a regression, and the new `atom_label_test.rs`
  is written with the field from the start.
- **Call it from outside the `rendering_method` match**, in
  `scene_tessellator.rs`'s per-node loop (`:161`) rather than inside the
  `Impostors` arm (`:242`) where `tessellate_atomic_structure_impostors` is
  called. This placement *is* guiding decision 5 — put the call inside the arm
  and labels silently vanish in `TriangleMesh` mode.

**Automated tests** — a `LabelVertex` layout test in
`rust/tests/renderer/label_mesh_test.rs` (mirroring
`transparent_impostor_mesh_test.rs`), plus display-seam tests in
`rust/tests/display/atom_label_test.rs` (registered in `rust/tests/display.rs`,
next to `atomic_color_test.rs` / `atomic_render_style_test.rs` — the same
pattern, asserting mesh *contents*, not pixels):

- `LabelVertex` field offsets/size match `desc()`. This matters more than it
  looks: `desc()` offsets are hand-written cumulative `size_of::<[f32; N]>()`
  expressions, the same fragile idiom `TransparentImpostorVertex` uses, and
  this test is the only thing standing between a miscount and silently
  garbled glyphs.
- A two-char label emits exactly 2 quads (8 verts, 12 indices); an unlabeled
  atom emits none; an empty-string label emits none.
- All of a label's vertices share the anchor position and the depth offset;
  `plane_offset`s are centered on zero.
- `depth_offset` tracks the atom's displayed radius: a
  `render_style: "space_filling"` atom's label offsets by the vdW radius while
  a ball-and-stick neighbor's offsets by the smaller one.
- A culled atom emits no label.
- An `alpha: 0.0` atom emits no label; a ghosted atom (`alpha: 0.5`) still
  emits one; `scene_transparency_enabled` with `scene_alpha = 0.0` empties
  the label mesh.
- Labels are emitted in `TriangleMesh` mode as well as `Impostors`.
- The label mesh stays empty in lightweight mode.

**Manual verification** — none (no writer until Phase 4; the seam tests are
the coverage).

---

### Phase 4 — `label` on `StyleRule`, token expansion, preference, guide

**Implementation**

- `StyleRule` gains `label: Optional[String]` (no hint) in
  `built_in_record_type_defs`.
- `apply_style`: parse `label` in `parse_style_rules` (defensive
  absent-or-`None` = unset, as with every other field); validate tokens at
  parse time; expand per matched atom in the apply loop and write through
  `set_atom_label` / clear on empty. `StyleRule`'s parsed struct gains
  `label: Option<String>` holding the *unexpanded* template.
- `label_scale` preference, API side (§Label size for the full chain and the
  two silently-failing attributes; the display-side field already landed in
  Phase 3): `structure_designer_preferences.rs` (`f64`, `#[frb(non_final)]` +
  `#[serde(default = "default_label_scale")]` + the `Default` impl entry),
  switch the `api_common.rs` mapping from the Phase-3 `DEFAULT_LABEL_SCALE`
  const to the API value, `flutter_rust_bridge_codegen generate`, and a
  `preferences_window.dart` input.
- Guide: `doc/reference_guide/nodes/atomic.md` §apply_style — add `label` to
  the field table, a **Labels** subsection (the token table, `{tag}`'s
  resolution, `""` as the reset, the both-methods property, that labels ride
  the atom's displayed radius), and the **"label a handful of atoms"**
  guidance from §Readability. The `xray`/`tag` cross-references and the
  existing *place `apply_style` late* caveat already cover the rest.

**Automated tests** — extend
`rust/tests/structure_designer/apply_style_test.rs`:

- A rule sets a literal label; `label: ""` clears one set by an earlier rule
  (ordering, per-property last-writer-wins — the same shape as the existing
  color/alpha ordering test).
- `{element}` expands per atom under a match-all rule (a C and an Si atom get
  different text from **one** rule); honors `element_name_overrides` (a param
  element labels `P1`, matching the hover popup); falls back to `X` for an
  unknown atomic number.
- `{tag}` takes the rule's selector when present; falls back to the atom's
  first tag when the rule is selector-less; empty when the atom has no tags.
- `{{` / `}}` escape; an unknown token errors naming the rule index and the
  token; nothing is partially applied.
- Composition: one rule setting `color` + `label`, a later rule setting only
  `color`, leaves the label intact.
- A pre-Phase-4 `record_construct` with schema `StyleRule` gains the `label`
  pin through the repair pass with wires preserved.
- Preferences round-trip + assertions (`rust/tests/structure_designer/preferences_test.rs`).
- Node-type snapshots (`cargo test node_snapshots` + `cargo insta review`).

The display-test compile breakage from adding the struct field already
happened in Phase 3; this phase touches only the API-side struct, whose serde
default keeps existing settings files loading unchanged.

**Manual verification** — `flutter run`: tag a few atoms on a crystal; wire an
`array` of `StyleRule` into `apply_style` with
`{tag, color, label: "{tag}"}`; the labels appear on the tagged atoms, stay
upright while orbiting, are hidden by atoms in front of them, and scale with
zoom. Check a match-all `label: "{element}"` on a *small* molecule. Check both
rendering methods (labels in both) and both projection modes (upright in
both). Check a `render_style: "space_filling"` atom's label sits on the vdW
surface. Check `alpha` composition: `alpha: 0.0` on a labeled atom removes
the label with the atom; `alpha: 0.5` keeps an opaque label on the ghost.
Check the `label_scale` preference takes effect live. Text-format
round-trip of a network containing `apply_style`.

---

## Future work

- **Per-rule `label_color`** — today fill is white with a black SDF outline,
  which is readable on any background and needs no fields. A luminance-derived
  fill (contrast against the atom's own albedo) or an explicit field composes
  later with no migration.
- **Per-rule `label_scale`** — a rule-level override of the global preference,
  if a use case ever forces per-atom sizing.
- **More tokens** — `{id}`, `{tags}` (all tags, joined). Each is one arm in
  the expansion match.
- **Label interning** — an interned-string table on the decorator, if the
  per-atom `String` ever shows up in a profile. Not built now: labeled-atom
  counts are small by construction, and speculative caching is against
  project guidance.
- **Non-ASCII glyphs** — a wider atlas or on-demand rasterization, if tag
  names ever need it. The metrics/UV plumbing is codepoint-keyed already.
- **Bond labels** — bond order / length text on bonds, reusing this entire
  pipeline with a midpoint anchor.
- **msdf** — sharper glyph corners than single-channel SDF, at a
  substantially more complex generator.

## Explicitly out of scope

- Serializing label state (decorator state is transient by contract).
- Labels in export formats (`.xyz`, `.mol`) or in the hover popup (which
  already shows element and tags, from the same sources).
- A cap or distance fadeout on label count (§Readability — guidance instead).
- Screen-space label sizing, and any label collision/declutter layout.
- Labels in atom_edit / the legacy `edit_atom` node — styles apply to
  displayed results via the node network, per `design_style_rules.md`.
- Any semantic property deriving from a label. A label is text drawn on a
  screen; it is the terminal consumer of tags, never a source of truth.
