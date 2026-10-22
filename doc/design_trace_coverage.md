# Design: Trace Coverage — Narrower Contributing-Node Traces

Follow-up to `doc/design_output_driven_editing.md` (the right-click
"contributing nodes" menu). Read that document first; this one uses its terms
(probe, link, fallback, lost, origin) without redefining them.

## Problem

Feedback on the menu (mechadense, 2026-10-22):

> Unlike in the mockup you have actually combined at least part of the entire
> upstream dependency tree … not just the singular linear upstream nonbranching
> provenance chain that the picking ray defines. … maybe at least de-emphasize
> the upstream path parts that are not directly on the picking ray but are just
> found via node topology?

The engine does not list the whole upstream tree. But in several common
situations it lists a large part of it, and the menu does not tell those rows
apart from the rows that are on the path. All of it comes from the **fallback**
(`trace/engine.rs`, `Walker::fallback`), which runs for a node without `trace`:
it re-syncs the probe into **every** phase-typed input, and an input where the
probe matches nothing has the probe **lost**, after which every node upstream of
that input is listed as "possibly contributed" without being asked.

Three situations account for nearly all of it:

1. **A side input that does not contain the probe.** Seven region-gated atom
   ops (`passivate`, `remove_hydrogen`, `freeze`/`unfreeze`, `tag`/`untag`,
   `infer_bonds`, `xray`, `atom_replace`) have an optional `region: Blueprint`.
   An atom outside the region loses the probe there, and the whole region cone
   is listed. `apply_diff` does the same with its `diff` input for every base
   atom the diff did not touch, which is most of them. `mechanosynth` /
   `mechanosynth_edit` do it with every tool and feedstock, `patch_latticefill`
   with its `region`.
2. **Branches not taken.** `if` and `switch` get phase-typed pins from their
   instance type (`calculate_custom_node_type`), so the fallback follows every
   branch, the untaken ones included. It even *evaluates* the untaken branches,
   which `eval` never does. `array_append` has the same shape.
3. **Atoms a node created or moved.** When the hit atom was added by an untraced
   node (a `passivate` terminator, an `edit_atom` addition, a `proxy` cap, a
   `mechanosynth` deposition) or moved by it, the probe is lost on the main
   input and the whole main chain is listed.

In each case the extra rows render exactly like the honest "possibly
contributed" row of an untraced node that *is* on the path: both are
`RowContribution::Unknown`, dimmed the same way.

A smaller, opposite defect: the trace **stops early, with a wrong label**, at a
node whose input carries the structure inside a record, an iterator or a
function (`record_get`, `record_destructure`, `collect`, `apply`, …). The
fallback sees no phase-typed input, calls the node an origin, and labels it
`source` — the node did not create anything.

## Goal

- Give `trace` to every node in the three situations above, so the menu lists
  the chain the hit actually went through.
- Make the rows that are listed **only because of wiring** visibly different,
  and collapsed by default, so whatever fallback remains is de-emphasized as
  asked.
- Stop labelling a record/iterator/function reader as an origin.

### Non-goals

- **Tracing through records, iterators and functions.** That needs a field
  path beside the array `element` (see Deferred). Here the trace only stops
  honestly at such a node.
- New contribution semantics for the nodes already traced (`atom_edit`,
  `materialize`, the movers, the booleans). They stay as built.

### Example

`materialize` → `passivate(region = cuboid)` → displayed. The user right-clicks
a surface carbon outside the cuboid. Today:

```
passivate        possibly contributed
cuboid           possibly contributed
materialize      filled
half_space_3     shape
…
```

`cuboid` (and anything feeding it) is listed only because it is wired to
`passivate`. After this work:

```
passivate        passed
materialize      filled
half_space_3     shape
```

Right-clicking a hydrogen `passivate` placed inside the cuboid:

```
passivate        passivated
materialize      filled         ← the carbon it caps
half_space_3     shape
cuboid           shape          ← the region that admitted the carbon
```

## The rule for side inputs

> **A node links to an input only if it used that input to produce or change
> the probed thing.**

The main input of an atom op is always followed: the atom came from there (or,
for an atom the node created, its host did). A gating or selecting input — a
`region`, an `if` condition's other branch, a `diff` that did not touch the
atom, a tool that did not take part — is followed only when it decided what
happened to *this* atom. When it did, it is followed with a precise probe, so
the booleans upstream of it narrow it down as they do for `materialize`.

This is the same pruning the existing traces do (`atom_union` keeps one
element, the booleans keep the deciding children). The new traces apply it to
the nodes that so far relied on the fallback.

## Engine changes

### Two new row kinds

```rust
pub enum RowContribution {
    Traced(Contribution),
    /// The node has no `trace`, or cannot answer this probe: "possibly
    /// contributed". It is on the hit path.
    Unknown,
    /// Listed only because it is wired upstream of a node where the probe was
    /// lost. Nothing is known about it: "wired upstream".
    Unreached,
    /// The probe reached the node through an input the trace cannot see into
    /// (a record, an iterator, a function): "trace stops here".
    Opaque,
}
```

- **`Unreached`** replaces `Unknown` on every row the engine emits for a lost
  probe: the `probe: None` branch of `Walker::visit`, and `visit_parameter` when
  its probe is `None`. The node at which the probe was lost keeps `Unknown`: it
  is on the path, it just could not say more. The instance-row strength order
  becomes `Created` > `Modified` > `Passed` > `Unknown` = `Opaque` >
  `Unreached`, so an instance whose children were all reached only through
  wiring is itself `Unreached`. An instance row with no children (an empty
  subnetwork walk) defaults to `Unreached` when it was entered with a lost
  probe, and to `Unknown` otherwise, instead of always `Unknown` as today.
- **A hit that resolves to nothing.** When `start_atom` cannot find the hit atom
  in the displayed output, the displayed node is visited with a lost probe.
  That node is where the probe was lost, so it reads `Unknown`, not
  `Unreached`; only the nodes above it are `Unreached`. `visit` tells the two
  apart by whether the caller is `start` (pass an explicit flag, do not infer
  it from the row count).
- **One row per node: precise beats wired.** The dedup key includes the probe
  kind, so today a node reached both with a probe and through a lost cone gets
  two rows. Under the new labels these would contradict each other
  (`materialize filled` and `materialize wired upstream`). That happens
  whenever one input of a node keeps the probe and another loses it into
  shared upstream, and on the diff chains that stay on the fallback
  (`apply_diff` → a producer's pin 1). So when the tree is built,
  `finish` drops every `Unreached` row whose (frame, network, node) was also
  emitted under any other kind, wherever in the walk that happened. A dropped
  instance row takes its subtree with it; its children's own rows survive only
  where they were emitted elsewhere.
- **`Opaque`** is emitted by the fallback instead of an origin label when the
  node has no phase-typed input but **does** have a wired input whose type
  could carry a phase value (`may_carry_phase`, below). The trace stops there,
  as it does today; only the label changes.

```rust
/// Can a value of this type contain a Blueprint, Crystal or Molecule
/// somewhere inside it? A record (any field, through named defs), an array
/// or iterator of one, a function whose return type can, `AnyFunction`.
fn may_carry_phase(data_type: &DataType, registry: &NodeTypeRegistry) -> bool;
```

Named record defs resolve through `NodeTypeRegistry::lookup_record_type_def`
(built-ins included), and the def graph is acyclic (the cycle check on add),
so the walk terminates without a visited set. An unresolvable name answers
`true`: claiming "stops here" is the honest answer when the type is unknown.

### Shared helpers in `trace/mod.rs`

Every new `trace` is a few lines on top of these:

```rust
/// A node whose output on pin 0 *is* its input `pin`'s value, with every
/// atom id and every point of geometry unchanged (`exit_structure`,
/// `enter_structure`, `with_structure`, `dematerialize`, the selectors):
/// link `pin` with the same probe and element, reported `Passed`.
pub fn trace_passthrough(out_pin: usize, element: Option<usize>, probe: &TraceProbe,
                         in_pin: usize) -> Option<TraceStep>;

/// Point probe on a Blueprint input made from atom `id` of `atoms`: the
/// position of that atom, `on_surface: false`, δ = `atom_point_delta`.
/// What `materialize` hands to its Blueprint, factored out for reuse.
pub fn atom_point_probe(atoms: &AtomicStructure, id: u32) -> Option<TraceProbe>;

/// The region-gated atom op family (see Node implementations).
pub fn trace_region_op(pin: usize, element: Option<usize>, probe: &TraceProbe,
                       ctx: &TraceContext, spec: &RegionOpSpec) -> Option<TraceStep>;
```

`materialize` switches to `atom_point_probe`; its behaviour does not change.

The region membership test moves to one place. Today it is inlined in
`evaluator/atom_op.rs::map_atomic_in_region` (`sdf <= margin` over a batch).
Add beside it

```rust
/// Is `position` inside `region` as the region-gated ops count it?
pub fn region_contains(region: &GeoNode, position: DVec3) -> bool {
    region.implicit_eval_3d(&position) <= DEFAULT_REGION_MARGIN
}
```

and make the batch filter call the same margin through it, so a trace can never
disagree with `eval` about whether an atom was in the region.

## Node implementations

All of them follow the existing conventions (`rust/crates/atomcad-structure-designer/src/AGENTS.md`,
Output tracing): read only persisted node data, get values through
`TraceContext`, and resolve parameter pins with the same helper `eval` uses.
Each answers on pin 0 only, unless stated; the `diff` output pins (pin 1 of
`atom_replace` and others) keep `None` and fall back.

**Atom ids survive in place.** `AtomicStructure::add_atom` always appends
(id = `atoms.len() + 1`), so ids are never reused. A node that mutates a clone
of its input (`map_atomic`, `map_atomic_in_region`, `edit_atom`'s history
replay) therefore keeps every surviving atom's id, and an id the input does not
have is an atom the node created. Several traces below rely on this; the ones
that rebuild their structure (`proxy`, `patch_latticefill`) match by position
instead.

### Selectors and array plumbing

**`if`** (`if_else.rs`). Read the condition with `ctx.input_value(0)`, exactly
as `eval` does; link pin 1 (`true`) or pin 2 (`false`) with the same probe and
element, `Passed`. An unwired, `None` or erroring condition returns `None`.
Factor the branch choice into a helper used by `eval` and `trace`.

**`switch`** (`switch.rs`). Same, with the selector on pin 0 and the taken pin
`1 + case_index` or the default `1 + cases.len()`. Factor eval's step 2
("find the case whose literal equals the selector value") into
`SwitchData::taken_pin(&NetworkResult) -> Option<usize>`, used by both.

Both are type-agnostic: they route whatever probe arrives, so they work for
Blueprint, Crystal and Molecule branches alike, and for arrays (the element
passes through unchanged).

**`array_append`**. Output element *k* < length of `array` → pin 0, element
*k*; *k* = that length → pin 1 (the appended `element`), no element index. Like
`sequence`, a nested array element type returns `None`.

### Pass-through phase nodes

These change the phase tag of a value and nothing inside it, so the probe
passes unchanged (`trace_passthrough`):

| Node | Output | Link | Probe kinds |
|---|---|---|---|
| `exit_structure` | Crystal → Molecule | pin 0 | Atom, Point |
| `enter_structure` | Molecule → Crystal | pin 0 | Atom, Point |
| `with_structure` | Blueprint → Blueprint | pin 0 (`shape`) | Point |
| `dematerialize` | Crystal → Blueprint | pin 0 | Point |

`exit_structure` / `enter_structure` move `atoms` and `geo_tree_root` across
unchanged (checked in `eval`); `dematerialize` returns the crystal's
`geo_tree_root`; `with_structure` re-labels the structure and keeps the
geometry. None of them follows the `structure` pin: it does not place the
probed thing.

A Point probe through `dematerialize` reaches a Crystal. That is usually a
`materialize`, whose `trace` handles Atom probes only today. Extend it: a
`Point` on pin 0 is a point of the crystal's geometry, which is the
Blueprint's geometry, so link pin 0 with the same Point, `Passed`. Without this
the fallback would answer correctly anyway (it keeps a Point the Blueprint
contains), but the row would read "possibly contributed".

**`lattice_symop`** moves its Blueprint rigidly, like `geo_trans`, unless
`transform_only_frame` ("keep_geo") is set, in which case only the reference
frame moves and the geometry stays in place. Factor the transform `eval`
applies to the geometry out of `eval` into a helper taking the `arg` closure
(the movers' pattern), resolving `transform_only_frame` from its wired pin or
stored value as `eval` does; the helper returns the identity in keep-geo mode.
Answer with `trace_moved_point` and the inverse transform, reporting
`Modified("moved")`, or, for the identity, link pin 0 with the same probe,
`Passed`.

### Region-gated atom ops

`passivate`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, `freeze`,
`unfreeze`, `tag`, `untag` and `xray` share one shape: a main `molecule` input,
an optional `region: Blueprint`, and an operation applied to the in-region
atoms of a clone. They share one implementation, `trace_region_op`, configured
per node:

```rust
pub struct RegionOpSpec {
    pub molecule_pin: usize,
    pub region_pin: usize,
    /// Label when the node changed the atom ("frozen", "tagged", …).
    pub modified: &'static str,
    /// Did the node change this atom? Compares the input atom with the output
    /// atom of the same id. `None` = "every in-region atom" (xray: alpha lives
    /// in the decorator, not on the atom).
    pub acted: Option<fn(&Atom, &Atom) -> bool>,
    /// Label for an atom the node created; `None` = the node creates none.
    pub created: Option<&'static str>,
    /// What the region gates. `Atom`: the op touches an atom only when that
    /// atom is in the region (every node but `infer_bonds`). `Bond`: the op
    /// touches a bond when **either** end is in the region, so an
    /// out-of-region atom can still change (`infer_bonds`).
    pub gate: RegionGate,
}

pub enum RegionGate { Atom, Bond }
```

For an `Atom { id: Some(id) }` probe on pin 0:

1. **The id is in the input** (`ctx.input(molecule_pin)`): the atom came from
   there. The node *acted* on it when it is in the region (`region_contains` at
   the **input** position, or the region pin is unwired) **and** `acted`
   holds. Then: `Modified(spec.modified)`, link the molecule with the same id,
   and, if the region is wired, link it with `atom_point_probe` of the input
   atom. Otherwise: `Passed`, molecule link only. The region is not followed:
   it did not decide anything about this atom.
2. **The id is not in the input**: the node created it (only `passivate`, whose
   terminators are appended). Take its one bonded neighbour, the host, which is
   in the input with the same id. `Created(spec.created)`, link the molecule
   with the host's id, and the region (if wired) with `atom_point_probe` of the
   host — the region decided that the host was passivated. This mirrors
   `materialize`, which probes a terminator at its parent.

With `gate: Bond` (`infer_bonds`), step 1 changes: `eval` re-infers every
bond with at least one end in the region and keeps only bonds with **both**
ends outside, so an atom outside the region can still gain or lose a bond to
one inside. The node acted on the atom when `acted` (bonds differ) holds,
whatever the atom's own position. Then `Modified(spec.modified)`, link the
molecule with the same id, and, if the region is wired, link it with
`atom_point_probe` of each **in-region** end among the atom itself and the
other ends of its added or removed bonds (input positions; the deciding
`half_space`s below dedupe across them). An atom whose bonds did not change is
`Passed`, molecule only, as in the `Atom` gate.

Anything else (a Point probe, another pin, an element) returns `None`.

| Node | `modified` | `acted` | `created` | `gate` |
|---|---|---|---|---|
| `passivate` | "passivated" | bonds differ | "passivated" | Atom |
| `remove_hydrogen` | "depassivated" | bonds differ | — | Atom |
| `infer_bonds` | "bonded" | bonds differ | — | Bond |
| `atom_replace` | "replaced" | element differs | — | Atom |
| `freeze` / `unfreeze` | "frozen" / "unfrozen" | frozen flag differs | — | Atom |
| `tag` / `untag` | "tagged" / "untagged" | `tag_bits` differ | — | Atom |
| `xray` | "faded" | `None` (every in-region atom) | — | Atom |

"Bonds differ" compares the two atoms' bond lists as sets of (other id, order).
For `remove_hydrogen` and `passivate` the `Atom` gate is the host: `eval`
strips or adds terminators of in-region hosts, which is the atom whose bonds
change.

The pin indices come from each node type's parameter list (the region is the
last pin everywhere except `xray`, where `fade_depth` follows it, and
`passivate`, where `element` does); name them as constants beside the node
type, not as literals in `trace`.

### Other single-structure atom ops

**`apply_style`**. Same id on pin 0 (ids survive `map_atomic`): `Passed`, link
the molecule. Styling is not provenance, and replaying its rule matching in
`trace` would duplicate `parse_style_rules` logic for a label.

**`edit_atom`**. Its history runs on a clone (`map_atomic`), so ids survive. An
id the input does not have: `Created("added")`, no link. An id the input has:
`Modified("edited")` if the position or element differs, else `Passed`; link
the molecule with the id.

**`proxy`**. `apply_proxy` builds a new structure and hands out fresh ids, but
it moves no atom. Look the probe position up in the input with `atom_at`: found
→ `Passed`, link the molecule with that id. Not found and `is_terminator` →
a severed-bond cap: `Created("capped")`, link the molecule with the atom at its
host's position (its one bonded neighbour). Not found otherwise → `None` (the
fallback, which cannot match it either, will mark the probe lost — correct for
an atom we cannot explain).

**`apply_diff`**. Re-run `apply_diff(base, diff, tolerance)` as `atom_edit`'s
`trace` does, with both inputs from `ctx` and the tolerance from the same
wired-pin-else-stored resolution as `eval`, and read `DiffProvenance`:

| `AtomSource` | Contribution | Links |
|---|---|---|
| `BasePassthrough(b)` | `Passed` | base, `Some(b)` |
| `DiffMatchedBase { diff_id, base_id }` | `Modified("edited")` | base `Some(base_id)` **and** diff `Some(diff_id)` |
| `DiffAdded(d)` | `Created("added")` | diff `Some(d)` |

The diff is followed only for atoms it touched. That is the change that stops
`atom_cut`'s or `atom_edit`'s diff chain from being listed for every base atom.
Upstream of the `diff` link sits a producer's diff output (pin 1 of `atom_edit`,
`atom_cut`, `relax`, …), which returns `None` and falls back; that is
acceptable here, since it is reached only for atoms the diff really carries.

### Multi-participant nodes

**`mechanosynth`** and **`mechanosynth_edit`**. A replay merges the base, the
feedstocks and the tools into one `Scene` (`build_scene`) and applies the steps.
The base is cloned first, so base atoms keep their ids; feedstock and tool
atoms get remapped ids; atoms a step adds are appended. `Scene::participants`
records which input every atom belongs to.

`trace` reads nothing the node stored at eval time (`record_last_scene` is
runtime state shared by every call site of a subnetwork). It needs no replay:
a replay only deletes atoms and appends new ids, never re-labels an atom's
participant, so the merge alone (`mechanosynth::merge_cast`, the library-free
first half of `build_scene`, factored out of it) says which input every
pre-existing atom of either output pin came from, and the pin's own value
(`ctx.output`) gives its replayed state. Both nodes share one
implementation (`nodes/mechanosynth.rs::trace_scene_atom`), which needs no
library at all — a library on a legacy file property would otherwise need
the design directory the trace cannot see.

For an `Atom { id: Some(id) }` probe on pin 0 (`result`) or pin 2 (`scene`) —
both carry scene ids:

- `id` not in the built scene → a step added it: `Created("deposited")`, no
  link. (Which participant it came from is not recorded; see Open questions.)
- `Participant::Base` → link `base` with `Some(id)`. `Modified("reacted")` if
  its position, element or bonds differ from the base atom, else `Passed`.
- `Participant::Feedstock(i)` / `Participant::Tool(i)` (scene pin only) → link
  pin `feedstocks` / `tools`, element *i*, with a match-by-position probe at
  the atom's position **in the built scene** (the participant's input
  position; the `scene` pin shows tools posed, the built scene does not).
  `Passed`.

Only the participants an atom belongs to are linked; the other tools and
feedstocks are pruned. The `steps`, `ops` and `step` pins are not followed.

**`patch_latticefill`**. `apply_patch` builds the result (cut, place, weld,
drop, passivate), so match by position. The probe position in `target` (pin 0)
via `atom_at` → `Passed`, link the target with that id. Otherwise the atom came
from the patch: `Created("patched")`, or `Created("passivated")` for a
terminator, and link `region` (pin 1) with a Point at the atom's position (for
a terminator, at its host's), δ from the output structure. `region` is
`HasStructure`, so it may be a Crystal; the engine's Point handling already
covers both. With `region` unwired, `eval` takes the region from the
`target` (a Crystal), so the Point goes to `target` (pin 0) instead. The
`patch` pin is a record and is not followed (Deferred).

**`chemisorb`** (`debug` pin, pin 2). The debug view's atoms carry the plan's
*combined* ids, and `Setup` maps each input atom to its combined id per side
(`adsorbate_ids`, `substrate_ids`). Rebuild the `Setup` from the inputs the way
`plan` does (cheap, no relaxation) and invert the two maps. An id found on one
side → link `adsorbate` (pin 0) or `substrate` (pin 1) with the input id;
`Modified("chemisorbed")` if the position differs (a seated or relaxed view),
else `Passed`. An id on neither side → `Created("passivated")` for a
terminator, else `Created("added")`. Never read the stored search or the stored
debug view.

*Check first:* that every debug form (posed, seated, relaxed) keeps combined
ids. `Candidate::structure` documents it; the debug forms are built from
`setup.combined` in `debug.rs`, but confirm before relying on it, and answer
`None` for any form that does not.

The `candidates` pin (pin 0) is an array of records. A displayed candidate is
reached through `array_at` and `record_get`, so it is not traceable until
records are (Deferred).

## API and Flutter

**API.** `APIContributionKind` gains `Unreached`; `contribution_view` maps
`RowContribution::Unreached` to it with the label "wired upstream", and
`RowContribution::Opaque` to `APIContributionKind::Unknown` with the label
"trace stops here" (it reads and dims like an unknown row; Dart does not need
to tell them apart). Regenerate the bindings (`flutter_rust_bridge_codegen
generate`, after `cargo fmt`).

**Menu** (`lib/structure_designer/trace_menu.dart`).

- `Unreached` rows render fainter than `Passed` / `Unknown` (one more step of
  the existing white54/white38 scale).
- **Custom-node instance rows start collapsed** too (today they start
  expanded). The instance row still shows the label derived from its
  strongest child, so the collapsed menu reads as the chain at the level the
  user is looking at; the chevron opens the subnetwork's rows. `_collapsed`
  is seeded with every instance row when the menu's rows change, instead of
  starting empty.
- A maximal run of consecutive `Unreached` **siblings** — rows at one tree
  level, under one parent (the top level or one instance row) — is shown
  **collapsed** as one summary line, "N wired upstream", at that level's
  indent, with the expand chevron the instance rows already use. N counts the
  run's rows and everything nested under them. A run never crosses a tree
  level: a lost cone that leaves an instance through a `parameter` continues at
  the parent level, after the instance's subtree, and so becomes two runs,
  one closing the instance's children and one among the instance's siblings.
  Each collapses on its own, and a collapsed instance hides its inner run with
  the rest of its children. Expanding shows the rows; each run's collapse
  state is keyed by (parent, index of its first row), lives beside the
  instance-row state (`collapsed`), and is computed by the same visible-rows
  pass.
- The summary line is not a node: hover highlights nothing in the editor, and
  clicking it only expands or collapses. It exists only in Dart: the visible-
  rows pass synthesizes it, and neither the Rust report nor `APITraceRow` has
  a row for it. It shares only the chevron and its level's indent with an
  instance row, which *is* a node and whose children sit one level deeper.
- **Expanded**, the summary line stays as a header with the down chevron, so
  the run can be collapsed again, and the run's rows follow it **at their own
  depth's indent**, not one step further in: they are siblings that the header
  groups, not children of it, and indenting them would read as nesting inside
  a node. Their fainter text marks them as the group. Folding a run hides a
  same-depth range (its first row to its last, with everything nested under
  them), which the instance rule in `visibleTraceRows` (hide the *deeper* rows
  after a collapsed row) does not express; the pass handles the two separately.

**Reference guide** (`doc/reference_guide/ui.md`, the contributing-nodes
section). Add "trace stops here" and "wired upstream" to the label table, say
that the wired-upstream rows start collapsed, and extend the paragraph that
lists which nodes answer precisely, phase by phase as the nodes land. Say
that custom-node rows start collapsed, and change any text that says they
start expanded.

## Phased implementation plan

### Phase 1 — Row kinds and the honest stop

- `RowContribution::Unreached` and `Opaque`; the lost-probe sites emit
  `Unreached`; instance strength order and the childless-instance default; the
  unresolved-hit case; dropping `Unreached` rows of nodes emitted under another
  kind; `may_carry_phase` and the fallback's `Opaque` branch.
- API kind, label mapping, codegen; menu fading and run collapsing;
  instance rows collapsed by default; `ui.md` label table.
- `trace_support::label` learns the two kinds ("wired upstream", "trace stops
  here").

Tests (`trace_engine_test.rs`, `trace_api_test.rs`):

- The existing lost-probe test (`fallback_lost_…`) now reads
  `move possibly contributed / s wired upstream / mol wired upstream`: the
  node where the probe was lost stays `Unknown`, everything above it is
  `Unreached`.
- A custom-node instance reached only through a lost probe is itself
  `Unreached`; one with a traced child keeps that child's label.
- One row per node: a node whose one input keeps the probe and whose other
  input loses it into shared upstream (an untraced two-input test node, both
  inputs fed by one `materialize`) lists that `materialize` once, with its
  traced label, and no "wired upstream" row for it — whichever of the two
  visits came first.
- A hit position matching no atom of the displayed output: the displayed node
  reads `possibly contributed`, its inputs `wired upstream`.
- `Opaque`: `record_construct(field = molecule) → record_destructure` displayed
  → `record_destructure trace stops here`, and the trace does not list
  `record_construct` or the molecule. The same through `collect` fed a
  `sequence` of Molecules (the implicit `Array → Iter` conversion). A `value`
  node holding a Molecule, which has no inputs, is still an origin (`source`).
- `may_carry_phase` directly: a named record with a nested named def holding a
  Crystal → true; a record of Floats → false; `Array[Iter[Blueprint]]` → true;
  a function returning Molecule → true; an unresolvable name → true.
- Flattening (API test): `Unreached` rows carry the new kind and label; an
  `Opaque` row carries `Unknown` with "trace stops here".
- Mutation checks: emit `Unknown` instead of `Unreached` at the lost-probe
  site; the lost-probe tests must fail. Skip the `Unreached` drop in `finish`;
  the one-row-per-node test must fail.

Flutter (`test/trace_menu_test.dart`): an instance row with traced children
starts collapsed (its children not shown, `chevron_right`) and its chevron
expands it; existing tests that assume expanded instances expand them first.
A run of `Unreached` top-level rows
collapses to one line with the right N; expanded, the header stays (down
chevron) and the rows keep their own depth's indent; hovering the header
reports no row to `onHover`; a lost cone that leaves an instance
through a `parameter` gives two summary lines, one inside the instance and one
after it at the top level, each expanding on its own; collapsing the instance
hides the inner one.

UI (manual walkthrough, maintainer): a design with an untraced mover upstream
of a large cone shows one collapsed "N wired upstream" line; expanding it shows
the rows, fainter; collapsing restores it.

Phase 1 changes no node, so on its own it already gives mechadense's
de-emphasis for every remaining fallback.

### Phase 2 — Selectors and pass-throughs

`if`, `switch` (with the factored `taken_pin` / branch helper),
`array_append`, `exit_structure`, `enter_structure`, `with_structure`,
`dematerialize`, `materialize`'s Point path, `lattice_symop` (with its
transform helper). `trace_passthrough` and `atom_point_probe` in `trace/mod.rs`.

Tests (`trace_atomic_nodes_test.rs` for the atom paths,
`trace_geometry_nodes_test.rs` for the point paths):

- `if` with two Molecule branches that share an atom position: a hit lists only
  the taken branch, both for `true` and `false`, and the untaken branch's
  source is **never evaluated** (a counting leaf on that branch records zero
  evals).
- `switch` with three cases and a default: a hit lists only the matching case;
  an unmatched selector lists the default.
- `array_append` → `atom_union`: an atom from the appended element lists that
  input; one from the array lists the array's element.
- `exit_structure` / `enter_structure`: an atom passes with its id; a surface
  Point on a molecule's geometry passes too.
- `with_structure` and `dematerialize` → `materialize`: a surface hit on the
  Blueprint reaches the deciding `half_space`, and `materialize` reads `passed`
  rather than "possibly contributed".
- `lattice_symop` with a non-identity symmetry rotation and translation: a
  surface point maps back onto the right face of the input shape (an
  off-centre shape, per the δ lesson in the parent design). With
  `transform_only_frame` set, the same hit reads `passed` and reaches the input
  face at the unchanged position.
- Mutation checks: make `if` link both branches; the untaken-branch test must
  fail. Drop the inverse in `lattice_symop`; its test must fail.

### Phase 3 — Atom ops on one structure

The region family through `trace_region_op` and `RegionOpSpec`
(`passivate`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, `freeze`,
`unfreeze`, `tag`, `untag`, `xray`), `region_contains` in `atom_op.rs`,
`apply_style`, `edit_atom`, `proxy`, `apply_diff`.

`tag` is the real "kept" subject of the fallback tests written in
`doc/design_output_driven_editing.md`'s Phase 1 (`fallback_kept_…` and three
more in `trace_engine_test.rs`). Giving it a
`trace` removes that subject, so this phase adds a test-only
`UntracedKeepData` to `trace_support.rs` (a molecule pass-through with the
default `trace`, like `UntracedMoveData`) and moves those tests to it. No real
node is a stable choice any more: every atom op that keeps atoms in place is
on this list.

Tests (`trace_atomic_nodes_test.rs`):

- For each region node, three cases on one network: an atom outside a wired
  region → `passed`, region **not** listed; an atom inside that the node
  changed → its label, region listed down to its deciding `half_space`; an
  atom inside that the node did not change (an already saturated host under
  `passivate`, an element not in `atom_replace`'s rules) → `passed`, region
  not listed. With the region unwired, a changed atom reads its label.
- `infer_bonds` (`gate: Bond`): an atom outside the region that gained a bond
  to an atom inside reads `bonded` and lists the region, probed at the
  in-region partner; an atom outside the region with unchanged bonds reads
  `passed`, region not listed.
- `passivate`: a placed H reads `passivated`, continues to its host in the
  input, and lists the region at the host's position.
- `region_contains` agrees with `map_atomic_in_region` for an atom exactly at
  the margin (the shared constant is what makes this hold).
- `edit_atom`: an added atom stops (`added`); a moved atom is `edited` with
  its id; an untouched one `passed`.
- `proxy`: a kept atom is `passed` with the input id found by position; a cap
  is `capped` and continues to its host.
- `apply_diff` fed by `atom_cut`'s diff and by `atom_edit`'s diff: a base atom
  the diff did not touch lists **only** the base chain; an edited atom lists
  both chains; an added atom lists only the diff chain.
- `apply_style`: `passed`, rules not listed.
- Mutation checks: make `trace_region_op` always link the region; the
  "outside" cases must fail. Make `apply_diff` link the diff for
  `BasePassthrough`; its first case must fail.

### Phase 4 — Multi-participant nodes

`mechanosynth`, `mechanosynth_edit` (with the replay helper factored out of
each `eval`), `patch_latticefill`, `chemisorb`'s debug pin (after the *check
first* above).

Tests (`trace_atomic_nodes_test.rs`; the mechanosynth fixtures from
`mechanosynth_tools_test.rs`'s style, built in code):

- `mechanosynth` with one tool and one feedstock: an untouched base atom →
  `passed` to `base`, tools and feedstocks not listed; a base atom a step
  moved → `reacted`; a deposited atom → `deposited`, no upstream; on the
  `scene` pin, a tool atom lists only the `tools` element it belongs to, even
  while the tool is posed away from its input position (`time` mid-flight).
- `mechanosynth_edit`: an atom deposited by the authored block and one by the
  `steps` prefix both read `deposited`; a base atom passes.
- `patch_latticefill`: a bulk atom outside the region → `passed` to `target`,
  region not listed; a tile atom → `patched`, region listed.
- `chemisorb` debug pin, root (posed) view: an adsorbate atom lists only the
  adsorbate chain, a substrate atom only the substrate chain.
- Mutation check: link every tool element in `mechanosynth`; the tool test must
  fail. It fails only if another tool has an atom at the same input position:
  a by-position link into a tool with nothing there is dropped by the engine,
  so the test shifts the probe onto the tip's leg.

Manual walkthrough (maintainer): mechadense's original network, before and
after, for the hit that prompted the feedback.

## Deferred: tracing through records, iterators and functions

`TraceLink` and the engine address one level of structure: an array
`element`. Tracing into a record needs a **field path** as well — "field
`structure` of element 3" — carried from `record_get` / `record_destructure`
up through `array_at`, `record_construct`, `record_set`, and into a node that
outputs records (`chemisorb`'s `candidates`, `patch_build`'s `Patch`). That is
a change to `TraceLink`, to the engine's wire resolution and to every existing
`trace` that passes an element through, so it is not "very easy" and is left
for a later design. Iterators and functions are further still (lazy walkers,
closure bodies; the parent design already treats HOFs as opaque).

Until then, Phase 1's `Opaque` row says honestly that the trace stops at the
reader. The nodes that become reachable when it lands — `chemisorb`'s
`candidates`, `patch_build`, `patch_latticefill`'s `patch` pin — are listed
here so that design picks them up.

## Not in this work

- **`atom_cut`.** A surviving atom is inside its cutters, so the fallback keeps
  the probe on every input and the booleans narrow the cutters down; it only
  reads "possibly contributed" where "passed" would do. Cheap to add later with
  the same id rule.
- **`atom_composediff`, `compare_atoms`, `export_atoms`, `get_structure`.** No
  traced path reaches them: their outputs are diffs, reports, `Unit` or a
  `Structure`.
- **The diff output pins** (pin 1 of `atom_edit`, `atom_cut`, `relax`, the
  movers, `atom_replace`). They keep the fallback.

## Open questions

1. **Where a deposited atom came from.** `mechanosynth` records the participant
   a step's added atom lands in, not the participant it was taken from, so its
   trace stops at `deposited`. Linking it to its source feedstock or tool would
   need the scene to record the source; decide after the Phase 4 walkthrough
   whether that is worth it.
2. **Collapse default.** "N wired upstream" starts collapsed. If the
   walkthrough shows people expanding it every time, start it expanded and
   keep only the fading.

## Testing

Method as in `doc/design_output_driven_editing.md` (Testing): small networks
built in code, `trace_hit` with constructed hits, reports rendered with
`trace_support::render` and compared to literals, no wall-clock assertions.
Run the harnesses explicitly
(`cargo test -p atomcad-structure-designer --test structure_designer trace_ -j 4`,
`cargo test --test structure_designer_api trace_ -j 4`). Every new `trace` gets
an **inside / outside** pair: the case where a side input is followed and the
case where it must not be, because the second is the one this design exists
for. Each phase lists its own tests and mutation checks.

## Files to modify

| Area | Files |
|---|---|
| Engine | `trace/engine.rs` (row kinds, `may_carry_phase`, `Opaque` branch, strength order), `trace/mod.rs` (`trace_passthrough`, `atom_point_probe`, `trace_region_op`, `RegionOpSpec`) |
| Region seam | `evaluator/atom_op.rs` (`region_contains`) |
| Nodes, Phase 2 | `nodes/{if_else,switch,array_append,exit_structure,enter_structure,with_structure,dematerialize,materialize,lattice_symop}.rs` |
| Nodes, Phase 3 | `nodes/{passivate,remove_hydrogen,infer_bonds,atom_replace,freeze,tag,xray,apply_style,proxy,apply_diff}.rs`, `nodes/edit_atom/edit_atom.rs` |
| Nodes, Phase 4 | `nodes/{mechanosynth,mechanosynth_edit,patch_latticefill,chemisorb}.rs` |
| API | `rust/src/api/structure_designer/structure_designer_api_types.rs` (`APIContributionKind::Unreached`), `view_builders.rs` (`contribution_view`), generated bindings |
| Flutter | `lib/structure_designer/trace_menu.dart`, `test/trace_menu_test.dart` |
| Tests | `tests/structure_designer/{trace_support,trace_engine_test,trace_atomic_nodes_test,trace_geometry_nodes_test}.rs`, `rust/tests/structure_designer_api/trace_api_test.rs` |
| Docs | `doc/reference_guide/ui.md`; `atomcad-structure-designer/src/AGENTS.md` (Output tracing: the side-input rule, `Unreached` vs `Unknown`, `region_contains`); `doc/design_output_driven_editing.md` ("Not in v1": point here) |
