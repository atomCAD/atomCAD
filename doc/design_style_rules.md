# Design: Style rules — per-atom visual properties driven by tags

## Motivation

The atom-tags core (`doc/design_atom_tags.md`, shipped) gives users named
per-atom groups, but tags are deliberately inert — today their only visible
surface is the hover popup. This design adds the consumer that motivated
them: **style rules**, which map `element` / `tag` selectors to per-atom
visual properties (color, transparency, render style), applied by a new
`apply_style` node.

The design builds on two shipped precedents and changes neither:

- **`xray`** (`doc/design_xray_node.md`) — the model for per-atom visual
  state: a decorator field written by a metadata-only pass-through node,
  consumed by the tessellators, runtime-only, never serialized.
- **`atom_replace.rules`** (`doc/design_atom_replace_rules_input.md`) — the
  model for shaped rule input: a built-in record type def consumed from an
  `Array[Record]` pin, parsed defensively at eval.

A specific question this document was asked to settle: **is per-atom
ball-and-stick vs. space-filling really the expensive part that should be
deferred**, as `design_atom_tags.md` §Future work suggested? §Assessment
answers with evidence; the short version is that it is roughly as much work
as everything else in this design combined, but it needs **no new renderer
architecture** — so it is specified fully here as Phases 3–4, which can slip
independently, rather than deferred to yet another design doc.

## Guiding decisions

1. **Rules are values on a wire, never node properties.** Record values have
   no property/text-format representation by construction (`TextValue` has no
   `Record` variant; `atom_replace` stores its *primitive* fallback rules as
   IVec2 pairs, and its wired record rules are never stored). More
   importantly, rules-as-values is the point: build a rule set once, wire it
   into several `apply_style` nodes, compute it (`map`, `product`,
   `array_concat`) like any other data. The `apply_style` node therefore has
   **no stored properties at all** — no property editor, no node-data API, no
   text-format properties, no migration surface.

2. **Styling writes the decorator, nothing else.** All three properties land
   in `AtomicStructureDecorator` state (or the existing `atom_alpha` field
   there): runtime-only, recomputed each evaluation, never serialized,
   cloned/remapped with the structure, silently dropped by structure-
   *rebuilding* nodes (`materialize`, lattice fill) — so, like `xray`,
   `apply_style` belongs **late in the chain**, and the reference guide says
   so. Durable semantic state (flags, tags) is untouched; per the tags
   design's guiding principle, no semantic property ever derives from a tag —
   this node reads tags only as selectors.

3. **Matching is ordered, per-property last-writer-wins.** Rules apply in
   array order; a matching rule overrides only the properties it sets. No
   CSS-style specificity — last-writer-wins is the composition idiom the app
   already teaches (`xray` chaining, `materialize.regions` painter's
   algorithm). The same rule extends across chained `apply_style` nodes: the
   downstream node's writes win where they overlap.

4. **Both rendering methods where the plumbing allows.** Color and render
   style work in both `Impostors` and `TriangleMesh` modes (both already
   decide color/radius per atom at tessellation time). Alpha remains
   impostor-only — the existing, documented `xray` limitation; `apply_style`
   inherits it rather than fixing it.

## The `StyleRule` built-in record type def

A fourth built-in record def, registered in
`NodeTypeRegistry::built_in_record_type_defs` next to `ElementMapping`,
`Patch`, and `MaterializeRegion` (which is the working precedent for
`Optional[…]` fields — `doc/design_optional_type.md`):

```text
StyleRule = {
  element:      Optional[Int],     // selector: atomic number
  tag:          Optional[String],  // selector: tag name
  color:        Optional[Vec3],    // 0–1 RGB
  alpha:        Optional[Float],   // 0–1; 1.0 restores opacity
  render_style: Optional[String],  // "ball_and_stick" | "space_filling"
                                   //   | "default"    (added in Phase 4)
}
```

Field semantics, in the order a rule is interpreted:

- **Selectors** (`element`, `tag`): a rule matches an atom iff every
  *present* selector matches — `element` compares against
  `atom.atomic_number`, `tag` tests membership via the structure's own tag
  table. Both present ⇒ AND. **Both absent ⇒ the rule matches every atom**
  (the whole-structure "make everything slightly transparent" case).
  - `element` accepts any `Int` that fits `i16` — including the param-element
    and debug atomic numbers ≥ 1000. A number no displayed atom carries
    simply matches nothing; that is not an error (networks are parametric).
    Values outside `i16` → localized error.
  - `tag` is trimmed; empty-after-trim → localized error (an empty tag name
    can never exist, so it is certainly a mistake — same rule as the `tag`
    node). A name absent from the structure's tag table matches nothing,
    without error, for the same parametric reason as `untag`.
- **Properties** (`color`, `alpha`, `render_style`): a present property is
  written onto every matched atom; an absent property leaves that property
  alone.
  - `color` is 0–1 RGB (the `AtomInfo.color` convention in
    `atomic_constants.rs`); components are clamped to `[0,1]` at eval.
  - `alpha` is clamped to `[0,1]`; writing goes through the existing
    `set_atom_alpha`, so `1.0` **removes** the entry (restores full opacity)
    — identical semantics to `xray`, including composition with a preceding
    `xray` node (same field, last writer wins) and with the global
    `scene_alpha` multiplier.
  - `render_style` must be exactly `"ball_and_stick"`, `"space_filling"`, or
    `"default"` (which removes the override, restoring the global
    preference); anything else → localized error naming the value. A string
    enum because the type system has no enum `DataType`; the `switch` node's
    Int/String selector is the established discrimination idiom. The field
    lands in the def only in Phase 4, when the machinery behind it exists —
    a field the node would silently ignore is worse than an absent field.

Restore asymmetry, stated up front: `alpha` and `render_style` both have a
natural "back to default" value (`1.0` / `"default"`); **`color` has none**
in v1 — there is no identity color, and a sentinel (e.g. "any negative
component clears") is the kind of magic value the `Optional[T]` design was
written to avoid. Removing a color override means removing (or reordering
past) the rule that set it. A `reset_color: Optional[Bool]` field is the
documented follow-up if this ever binds in practice.

Built-in-def mechanics are all inherited from the Phase-A infrastructure
(`design_atom_replace_rules_input.md`): registered in `new()`, resolved via
`lookup_record_type_def`, immutable (add/delete/rename/update reject the
name), **reserved name** in the user-type namespace (`name_is_taken`), never
serialized, absent from the user-types panel.

### Authoring rules

The canonical authoring path is one **`record_construct`** node per rule
(schema `StyleRule`): every field is `Optional`, so all pins may stay
unwired and the per-field literal editor — which already has the
tri-state *stored / (unset) / wired* affordance built for
`MaterializeRegion` — expresses "leave this property alone" as a true unset.
A single-rule set wires the `record_construct` straight into `apply_style`
(single-value → Array broadcast); multi-rule sets collect N
`record_construct` outputs through one `sequence` node, or generate rules
programmatically (`map`, `product`) — those emit `Iter[Record]`, and
`Iter[T] → Array[T]` is deliberately not an implicit conversion, so a
`collect` node is required between them and the `rules` pin. The `expr`
node is *not* an authoring path for rules: record width subtyping requires
every `StyleRule` field present in the source, and expr record literals
cannot express an unset `Optional` field.

Eval-side parsing is **defensive** about absent fields, exactly like
materialize's region parsing: `extract_record_field(name)` returning `None`
*or* `Some(NetworkResult::None)` both mean "unset". This keeps the node
robust to structurally-wider records and to any construction path that
omits fields rather than storing explicit `None`s.

A dedicated `style_rules` list-editor node (edit N rules in one panel) is a
UX nicety, not load-bearing, and stays future work.

## The `apply_style` node

| | |
|---|---|
| **Name** | `apply_style` |
| **Category** | `AtomicStructure` |
| **Pin 0** | `molecule: HasAtoms` — required |
| **Pin 1** | `rules: Array[Record(Named("StyleRule"))]` — optional |
| **Output** | `OutputPinDefinition::single_same_as("molecule")` |
| **Properties** | none (rules are wire-only, decision 1) |
| **Subtitle** | none |
| **Diff pin** | none (metadata-only pass-through, the `freeze`/`xray`/`tag` family) |

Eval:

1. `molecule` via `evaluate_arg_required`; `Error` → propagate.
2. `rules` via `evaluate_arg` (optional): `None` → **input passes through
   unchanged** (a disconnected rules pin is a no-op, consistent with an
   empty array — the network stays wireable while rules are under
   construction); `Error` → propagate; `Array(items)` → parse; anything
   else → localized error naming the received type.
3. Parse every rule up front (`parse_style_rules`, modeled on
   `parse_rules_from_records`): per-item name-based field extraction with
   the defensive absent-or-`None` = unset convention, plus the §StyleRule
   validations. Any invalid rule → localized `NetworkResult::Error` naming
   the rule index and problem; nothing is partially applied.
4. `map_atomic` (clone-mutate, phase-preserving — no region machinery; the
   selectors *are* the selection mechanism). For each rule, **precompute**
   the match test once against the styled structure: the element as `i16`,
   and the tag as a bit index via `tag_index(name)` (`None` ⇒ the rule
   matches nothing on the tag axis). Then one pass per rule over the atoms:
   matched atoms get each present property written through the accessors
   (`set_atom_color` / `set_atom_alpha`, plus `set_atom_render_style` from
   Phase 4 on). Rule
   order = array order, so later rules overwrite earlier ones per property.
   Cost is O(rules × atoms) with O(1) per test (an `i16` compare and a bit
   test) — comfortably cheap next to the tessellation that follows.

Because the node has no stored data, the Flutter side needs **no property
editor and no node-data API** — only node registration. The interesting UI
lives in `record_construct`, which already exists.

Not in v1, recorded deliberately: a `region: Blueprint` pin. Every other
node in this family has one, but here the rules are the selection mechanism,
and region-gating a *rule list* raises questions (does the region AND with
every rule?) that no current use case forces. If demand appears, the pin
composes naturally later (AND with every rule's selectors, matching the
family's last-pin convention) with no migration.

## Per-atom color: new decorator plumbing

The one genuinely new visual channel in Phases 1–2. Mirrors `atom_alpha`
point for point:

```rust
// AtomicStructureDecorator gains:
/// Per-atom albedo override, 0–1 RGB. Absent = element-derived color.
/// Runtime-only display augmentation, like all decorator state.
pub atom_color: FxHashMap<u32, Vec3>,   // glam f32 Vec3
```

Accessors on `AtomicStructure` (all tag/alpha-style, no direct map access
outside `atomic_structure/`):

```rust
pub fn set_atom_color(&mut self, atom_id: u32, color: Vec3); // components clamped to [0,1]
pub fn clear_atom_color(&mut self, atom_id: u32);
pub fn get_atom_color(&self, atom_id: u32) -> Option<Vec3>;  // None = element color
```

Maintenance touchpoints — the same three the `xray` design enumerated, with
the same existing code sites:

- **Delete paths** clear the entry (next to the existing
  `atom_display_states` / `atom_alpha` removals in `delete_atom` /
  `delete_lone_atom`).
- **Merge** (`atom_union` path) remaps entries through the id map, next to
  the existing `atom_alpha` remap in `atomic_structure/mod.rs`.
- **Clone** is free (plain field).

Renderer consumption — both methods, no shader or vertex-format change
(color is already a per-atom/per-vertex input everywhere). One seam note:
both appearance helpers below take only `&Atom` and cannot see the decorator,
so the override reaches them as a caller-passed parameter
(`Option<Vec3>`) resolved at the tessellation call sites — the exact seam
`alpha` already uses (`tessellate_atom_impostor`'s `alpha: f32` argument):

- **Impostor path**: `get_atom_impostor_appearance` consults the override
  where it currently reads `ATOM_INFO…color`. Precedence: the
  delete-marker, unchanged-marker, and param-element colors **win over** the
  style color (they are semantic UI, not appearance); the style color
  replaces only the element-derived albedo. Ghost desaturation applies *on
  top of* the override (a ghosted styled atom desaturates like any other),
  and rim colors (selection, frozen, marked) are untouched — selection
  feedback must stay visible on styled atoms. The transparent mesh gets its
  appearance from the same function, so a styled + x-rayed atom is
  automatically a colored ghost.
- **TriangleMesh path**: `get_atom_color_and_material` applies the same
  override with the same marker/param precedence; this path folds selection
  into albedo, and that selection override likewise stays above the style
  color.
- **Bonds are unchanged in v1.** Bond colors come from the bond-type rules
  (`get_bond_color_inline`), not from atom colors; half-bond atom-color
  tinting is future work.

## Per-atom alpha: existing plumbing, new writer

Nothing new below the node: `apply_style`'s `alpha` writes the same
`decorator.atom_alpha` field as `xray`, through the same accessor, so every
downstream behavior is inherited verbatim — transparent-mesh routing,
min-alpha bonds, back-to-front sorting, `scene_alpha` multiplication,
TriangleMesh-renders-opaque, ghosts-stay-pickable. Chaining with `xray`
composes by plain last-writer-wins on the shared field.

## Assessment: per-atom render style — was "defer" right?

`design_atom_tags.md` §Future work called mixed ball-and-stick /
space-filling "the expensive part; its own project… carve out as a separate
phase/doc". Having now enumerated every site that reads the global mode,
here is what the feature actually requires.

**What the global mode drives today.** `AtomicStructureVisualization`
(`display/preferences.rs`) is a single global preference, threaded as a
function parameter (never read from a global) into two families of code:

- *Tessellation*, in `display/atomic_tessellator.rs`, duplicated across the
  TriangleMesh and impostor paths: atom radius
  (`get_displayed_atom_radius` — ball-and-stick `min(vdW·0.25, cov·0.9)`,
  space-filling raw vdW), sphere subdivision counts (mesh path only, 12×6
  vs 36×18), the bond-render gate (space-filling draws only *overstretched*
  bonds, at 4× stick radius), depth-cull threshold selection
  (`should_cull_atom` picks `ball_and_stick_cull_depth` = 8.0-default vs
  `space_filling_cull_depth` = 3.0-default), and the space-filling-only
  occluder-sphere optimization (`calculate_occluder_spheres`, mesh path).
- *Picking*, via `AtomicStructure::hit_test` and
  `get_displayed_atom_radius`: the pick radius per atom, and the rule that
  bonds are hit-testable only in ball-and-stick. Roughly a dozen call sites
  (viewport raytrace/hover/measure plus the atom_edit tools) all just pass
  the preference down.

**What per-atom mode does *not* require — the decisive fact.** Every
renderer data path is already per-atom: `AtomImpostorVertex`,
`BondImpostorVertex`, and `TransparentImpostorVertex` all carry per-instance
`radius`, and the mesh path tessellates each atom independently. Rendering a
mixed structure therefore needs **zero WGSL, pipeline, mesh-format, or
sorting changes** — the impostor shaders neither know nor care which "mode"
a sphere's radius came from. The transparent/xray machinery is fully
orthogonal (routing keys on alpha, not radius). This is what separates the
feature from the xray project, which had to build a new mesh, shader,
pipeline, and sort.

**What it does require.** One new decorator field plus its resolution
threaded through the sites above: a per-atom radius/subdivision choice
(mechanical), the bond decision matrix, the cull-threshold rule, the
occluder set, and per-atom pick radii in `hit_test` (mechanical — `hit_test`
is a method on `AtomicStructure`, so the decorator is already in reach).
Beyond the mechanics there are exactly **three semantic decisions**, which
this document resolves in §Per-atom render style below: what a mixed-endpoint
bond does, which cull depth a styled atom uses, and which atoms count as
occluders. Nothing else in the enumeration involves judgment.

**Cost estimate.** Phases 3–4 touch ~20 sites across the two tessellation
paths, culling, and picking — roughly the same volume as Phases 1–2
combined — but every site is a localized branch on an
`effective_visualization(atom)` lookup, testable at the existing
display-test seam without a GPU.

**Verdict.** The tags doc's instinct was *half* right: keeping render style
out of the tag core was correct (it would have doubled that project), and it
is genuinely the largest single property here. But "its own project / its
own design doc" is not warranted — the hard part turns out to be three
nameable semantic decisions, not architecture, and deferring the *decisions*
to a third document would only buy another design/review cycle. So: **the
semantics are specified fully in this document, and the implementation is
deferred within it** — Phases 3–4 land after color/alpha, depend on nothing
outside this doc, and can slip without touching Phases 1–2 (the `StyleRule`
def gains its `render_style` field only in Phase 4; adding a field to a
non-serialized built-in def later is non-breaking, since pin layouts
re-derive through the existing repair pass).

## Per-atom render style: specification

### Storage

```rust
// crystolecule (decorator file):
/// Per-atom render-style override. Absent = follow the global preference.
pub enum AtomRenderStyle { BallAndStick, SpaceFilling }

// AtomicStructureDecorator gains:
pub atom_render_style: FxHashMap<u32, AtomRenderStyle>,
```

The enum lives in crystolecule (the decorator already holds display-adjacent
state — `AtomDisplayState`, guide visuals — and crystolecule must not depend
on `display`); `display` maps it onto its own `AtomicStructureVisualization`
when resolving. Accessors `set_atom_render_style(id, style)` /
`clear_atom_render_style(id)` / `get_atom_render_style(id) -> Option<…>` on
`AtomicStructure`, with the same three maintenance touchpoints as
`atom_color` (delete-clear, merge-remap, clone-free).

### Resolution

One helper in `display/atomic_tessellator.rs`:

```rust
/// The mode this atom renders in: its decorator override, else the global
/// preference. Every mode-branching site below switches from the global
/// parameter to this per-atom lookup.
fn effective_visualization(structure, atom_id, global) -> AtomicStructureVisualization
```

Atom radius, sphere subdivisions, and the transparent-routing appearance all
key on the atom's effective mode — each an argument swap at an existing
branch. The three real decisions:

### Decision 1 — mixed-endpoint bonds: any ball-and-stick endpoint wins

A bond renders as a **ball-and-stick bond** (stick radius, multi-bond
layout, always drawn) iff **at least one endpoint's effective mode is
ball-and-stick**. A bond whose endpoints are both space-filling keeps
today's space-filling behavior: drawn only when overstretched
(`is_bond_overstretched`), at 4× radius, single cylinder.

Why at-least-one rather than the both-endpoints rule the tags doc sketched:
with both-endpoints, a ball-and-stick atom bonded to a space-filling
neighbor loses that bond stub entirely and appears to float beside the big
sphere. With at-least-one, the stick is drawn and simply **disappears into
the opaque vdW sphere** — the impostor fragment shaders write true
`frag_depth`, so the buried portion is hidden exactly, which is the classic
mixed-representation look (ball-and-stick ligand against a space-filling
pocket). Cost is identical (one boolean over two lookups). Residual
artifact, accepted and documented: if the space-filling endpoint is *also*
transparent (alpha < 1), the swallowed stick segment shows through the
ghost sphere — inherent to sorted alpha blending, same class as the xray
design's accepted intersecting-impostor artifacts.

Bond **alpha** keeps the existing min-endpoint rule unchanged.

### Decision 2 — depth culling: an atom is culled only if it exceeds *both* thresholds

`should_cull_atom` becomes: culled iff `in_crystal_depth` exceeds the
global mode's threshold **and** the atom's effective mode's threshold
(`None` = no culling = infinite). When no overrides exist the two modes
coincide and this is exactly today's behavior.

Why not simply "use the effective mode's threshold": it silently destroys
the feature's headline use case. Style a dopant at depth 5 space-filling
inside a ball-and-stick crystal (global threshold 8): under
effective-threshold-only, the space-filling default of 3 would **cull the
very atom the user just styled to make visible**. The mirrored case (a
ball-and-stick-styled interior atom in a space-filling scene) fails the same
way. Why not "styled atoms are never culled": a match-all
`render_style: "space_filling"` rule over a large crystal would disable
culling wholesale — a performance cliff. The both-thresholds rule handles
all three cases: styled atoms become visible when *either* mode would show
them, and a whole-structure restyle keeps the global mode's culling budget.
Bond culling keeps the existing rule (skipped iff either endpoint is
culled), with each endpoint's culled status computed as above.

### Decision 3 — occluders: only unambiguous full spheres occlude

`calculate_occluder_spheres` (mesh-path optimization) currently runs only
when the global mode is space-filling. It now collects occluders from atoms
whose **effective** mode is space-filling *and* that are opaque
(`get_atom_alpha == 1.0`) and not culled; it runs whenever that set is
non-empty. Ball-and-stick-styled atoms never occlude (their displayed radius
is far smaller than the vdW sphere the optimization assumes). This is purely
a correctness-conservative narrowing of an optimization — worst case some
occludable atoms are tessellated anyway.

### Picking

Each atom's pick radius resolves through `effective_visualization` (the
decorator override, else the global preference). The radius→mode mapping
lives in `display` (`get_displayed_atom_radius`), which crystolecule must
not depend on, so the resolution rides the radius closure that every
`hit_test` caller injects: `effective_displayed_atom_radius(structure, atom,
&global)` in `display/atomic_tessellator.rs`. (This was originally shipped
with the callers still injecting the bare global-mode radius — mixed-style
picking selected the wrong atom; the helper is the fix.) Bonds are pickable iff
**at least one endpoint's effective mode is ball-and-stick**, at stick
radius — Decision 1's first clause, but *not* its overstretched clause.
Overstretched SF–SF bonds stay **rendered-but-unpickable**: that is today's
behavior (`hit_test` returns after the atom tests whenever the mode is not
ball-and-stick, so bonds are never hit-tested in a space-filling scene), and
adopting "pickable iff rendered" verbatim would change zero-override scenes
— breaking the same no-overrides-identical invariant Decision 2 is built to
preserve. If overstretched-bond picking is ever wanted, it is an independent
behavior change to propose on its own, not a side effect of this feature.
Bond pickability needs no call-site change (`hit_test` has `&self`, so the
decorator is in reach); the atom pick radius touches every call site's
injected closure (viewport raytrace/hover plus the atom_edit/edit_atom
tools). atom_edit's own structures carry no overrides today, so tool
behavior is identical there by construction.

## Phases

Each phase lands green on `cargo fmt && cargo clippy && cargo test -j 4`
(and `flutter analyze` where Dart is touched) with the automated tests
listed. Phases 1–2 deliver rules with color + alpha; Phases 3–4 add render
style and may slip independently.

---

### Phase 1 — `atom_color` decorator plumbing + renderer consumption

**Implementation**

- `AtomicStructureDecorator.atom_color` + `new()` init; the three accessors
  (§Per-atom color) with component clamping.
- Delete paths clear; merge remaps (both next to the existing `atom_alpha`
  sites in `atomic_structure/mod.rs`).
- `get_atom_impostor_appearance`: style color replaces element-derived
  albedo; marker/param colors win; ghost desaturation applies on top; rims
  untouched. Same override in the mesh path's
  `get_atom_color_and_material`, below its selection-albedo override.

**Automated tests** — storage in
`rust/tests/crystolecule/atomic_structure_test.rs`; rendering at the
display-test seam (the `atomic_impostor_alpha_test.rs` pattern):

- Set/get/clear round-trip; components clamp to `[0,1]`; `get` returns
  `None` for untouched atoms; delete clears; merge remaps onto the new ids;
  clone preserves.
- Impostor tessellation: overridden atom's quad carries the override
  albedo, neighbors carry element color; delete/unchanged-marker and
  param-element atoms ignore an override; a ghost-state styled atom gets
  the *desaturated* override; a selected styled atom keeps its selection
  rim.
- Mesh tessellation: override lands in the sphere material; a *selected*
  styled atom still shows the selection albedo.
- An atom with both a color override and `atom_alpha < 1` routes to the
  transparent mesh **with** the override color.

**Manual verification** — none possible (no writer yet).

---

### Phase 2 — `StyleRule` def + `apply_style` node (color + alpha) + guide

**Implementation**

- Register `StyleRule` (four fields — `element`, `tag`, `color`, `alpha`)
  in `built_in_record_type_defs`.
- `nodes/apply_style.rs`: empty `ApplyStyleData`; eval per §The
  `apply_style` node (parse → per-rule precompute → ordered application).
  Register in `nodes/mod.rs` + `node_type_registry.rs`.
- Reference guide: new `doc/reference_guide/nodes/apply_style.md` (+ node
  index link) documenting: rule fields and matching (AND, match-all,
  ordered last-writer-wins), authoring via `record_construct` + `sequence`
  (with `collect` when rules come from `map`/`product`),
  the color-has-no-reset asymmetry, alpha's xray-inherited
  semantics and impostor-only limitation, and *place `apply_style` late —
  rebuilding nodes silently drop styling* (the xray caveat, restated).

**Automated tests** — new
`rust/tests/structure_designer/apply_style_test.rs` +
built-in-def guards next to the existing `ElementMapping` ones:

- Def: lookup resolves; add/delete/rename/update of `StyleRule` rejected;
  creating a user type named `StyleRule` rejected.
- Unwired `rules` and wired-empty-array both pass the input through
  unchanged (decorator untouched); concrete phase flows (Crystal→Crystal,
  Molecule→Molecule).
- Matching: element-only, tag-only, AND (only atoms with both), match-all
  (no selectors). Tag matching against tags applied upstream by a `tag`
  node — the end-to-end pipeline this feature exists for.
- Ordering: two overlapping rules where rule 1 sets color+alpha and rule 2
  sets only color ⇒ overlap atoms carry rule 2's color and rule 1's alpha.
- Alpha: values clamp; `alpha: 1.0` removes an entry set by an upstream
  `xray` (shared-field composition).
- Errors: non-array on the pin; element outside `i16`; empty/whitespace
  tag; each names the rule index. Unknown tag name and unmatched element
  match nothing *without* error.
- Node-type snapshots (`cargo test node_snapshots` + `cargo insta review`).

**Manual verification** — `flutter run`: author a rule with
`record_construct` (schema dropdown offers `StyleRule`; unset literals show
the tri-state affordance), wire through a `sequence` node into `apply_style`
on a tagged crystal; matched atoms recolor and ghost in the viewport;
TriangleMesh mode shows the color but stays opaque; hover popup unchanged;
the node has no property editor (empty panel is correct); text-format
round-trip of a network containing `apply_style`.

---

### Phase 3 — per-atom render style: decorator + tessellation + culling + picking

**Implementation**

- `AtomRenderStyle` enum + `atom_render_style` decorator map + accessors +
  the three maintenance touchpoints.
- `effective_visualization` helper; per-atom radius/subdivision in both
  tessellation paths; Decision 1 bond matrix (render gate, radius,
  multi-bond layout) in both paths; Decision 2 in `should_cull_atom`;
  Decision 3 in `calculate_occluder_spheres`; per-atom pick radii + bond
  pickability in `hit_test`.

**Automated tests** — display-seam tests (decorator set directly; no node
needed) + `hit_test` unit tests:

- Mixed structure: overridden atom's quad radius = vdW while neighbors stay
  ball-and-stick radius; mesh path uses per-atom subdivision counts.
- Bond matrix: B&S–B&S drawn (stick radius, multi-bond layout);
  B&S–SF drawn at stick radius; SF–SF absent unless overstretched (then
  4× radius); alpha min-rule unchanged on a mixed bond.
- Culling: dopant case (global B&S depth-8, SF-styled atom at depth 5 stays
  visible); mirrored case (global SF, B&S-styled interior atom visible);
  match-all-SF restyle keeps the global threshold (atom at depth 9 still
  culled); no-overrides behavior byte-identical to today.
- Occluders: only opaque, unculled, effective-SF atoms occlude; a
  transparent SF atom does not.
- `hit_test`: SF-styled atom picked at vdW radius; B&S–SF bond pickable;
  SF–SF bond not pickable even when overstretched (rendered-but-unpickable,
  §Picking); no-overrides picking behavior byte-identical to today.

**Manual verification** — none (no writer until Phase 4; the display tests
are the coverage).

---

### Phase 4 — `render_style` on `StyleRule` + node application + guide

**Implementation**

- `StyleRule` gains `render_style: Optional[String]`; `apply_style`
  validates (`ball_and_stick` / `space_filling` / `default`) and writes /
  clears the override.
- Guide: `apply_style.md` gains the render-style section — the three
  strings, the mixed-bond rule and its transparent-endpoint artifact, the
  culling behavior in plain words ("styled atoms show if either mode would
  show them"), and that the two cull-depth *preferences* stay global.

**Automated tests**

- Rule sets the override; `"default"` clears one set by an earlier rule;
  invalid string → localized error naming it.
- End-to-end dopant scenario: tag an interior atom → `apply_style`
  `{tag, color, render_style: "space_filling"}` → tessellation shows it at
  vdW radius, colored, unculled, with its bonds per the matrix.
- A pre-Phase-4 `record_construct` with schema `StyleRule` gains the new
  field's pin through the existing repair pass (wires preserved).

**Manual verification** — `flutter run`, the headline walkthrough: build a
diamond block, tag a buried region, apply
`{tag, color, render_style: "space_filling", alpha}` variants; check both
global modes × both rendering methods; verify the styled atoms are
hoverable/measurable at their displayed radius; verify the accepted
transparent-mixed-bond artifact is the only visual anomaly; reference-guide
page reads correctly and is linked.

---

## Addendum: `fade_depth` (issue #413)

Added after the four phases shipped. `StyleRule` gained a seventh field,
`fade_depth: Optional[Float]` (appended last so existing `record_construct`
wires stay positionally stable): the depth in Å at which the rule's alpha
write reaches full transparency, reusing `xray::depth_faded_alpha` — the
exact ramp the xray node's own `fade_depth` pin applies (see
`doc/design_xray_node.md` §"Depth falloff").

The one design decision: `alpha` and `fade_depth` combine into **one** alpha
write per matched atom — `depth_faded_alpha(alpha or 1.0, fade_depth or 0.0,
atom.in_crystal_depth)` — and last-writer-wins applies to the pair as a
unit. This mirrors the xray node (which likewise bakes the ramp into the
static per-atom alpha at eval time) and directly serves the issue's use
case: a match-all rule fades a block, and a later `{tag: …, alpha: 1.0}`
rule fully overwrites the faded value on its atoms, exempting them. A rule
setting only `fade_depth` uses a surface alpha of `1.0`. `fade_depth ≤ 0`
or non-finite = ramp off (the helper's existing guard); no eval-side range
validation, matching the xray pin's semantics. No renderer, FRB, Flutter,
or `.cnnd` changes — the built-in-def growth flows through the generic
record machinery.

## Future work

- `region: Blueprint` pin on `apply_style` (AND with every rule) — composes
  later with no migration.
- More properties on `StyleRule`: `visible: Optional[Bool]`,
  `radius_scale: Optional[Float]`, `reset_color: Optional[Bool]` — each an
  independent decorator field + rule plumbing on the rails built here.
- A `style_rules` convenience node (edit a rule list in one panel) — or,
  better, a **generic array-literal node** (`array`: pick an element type,
  add/remove/edit elements inline, record elements editable via the
  `record_construct` field-editor UI). That generalization would serve
  `atom_replace.rules` too (`ElementMapping` is fully literal-capable;
  `MaterializeRegion` is not — its `volume` is a `Blueprint`) and would
  subsume this bullet. Designed in
  `doc/design_array_node_and_field_hints.md`.
- **Field editor hints** on `RecordField` — designed in
  `doc/design_array_node_and_field_hints.md` (Part A). When they land,
  `StyleRule` declares all four hint kinds (`element` → `Element`,
  `color` → `Color`, `alpha` → `Range{0,1}`, `render_style` →
  `Enum(["ball_and_stick", "space_filling", "default"])`), turning the
  generic literal editors into an element dropdown / color editor / slider
  / string dropdown. Hints are purely presentational and cannot cover the
  `tag` field's ideal affordance (a dropdown of the *upstream structure's*
  tag names — runtime context a `record_construct` doesn't have); that
  field stays free text.
- Bond styling: half-bond atom-color tinting; per-bond alpha/color rules.
- Tag pins on the region-gated nodes (`freeze`, `xray`, …) and an expr
  `has_tag(...)` predicate — carried over from the tags doc's future work,
  unchanged.

## Explicitly out of scope

- Serializing any style state (decorator state is transient by contract).
- Styles in export formats (`.xyz`, `.mol`).
- Per-atom alpha in `TriangleMesh` mode (pre-existing xray limitation).
- Order-independent transparency (per the xray doc, only if sorted blending
  proves insufficient).
- Styling in atom_edit / the legacy `edit_atom` node — styles apply to
  displayed results via the node network.
- Any semantic property deriving from tags — permanently ruled out by the
  tags design's guiding principle, restated here because `apply_style` is
  the first tag consumer: it reads tags as selectors and writes only
  transient display state.
