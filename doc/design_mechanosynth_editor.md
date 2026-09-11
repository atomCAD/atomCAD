# Design: build scripts as network values and the `mechanosynth_edit` node

Status: **draft 2026-09-11**, discussed and agreed in principle; not
implemented. Extends the `mechanosynth` subsystem
(`rust/crates/atomcad-crystolecule/src/mechanosynth/`,
`rust/crates/atomcad-structure-designer/src/nodes/mechanosynth.rs`,
`lib/structure_designer/node_data/mechanosynth_editor.dart`, reference guide
`doc/reference_guide/nodes/atomic.md` §mechanosynth). Builds on
`doc/design_mechanosynth_step_metadata.md`.

## Motivation

The `mechanosynth` node replays a build script that a generator wrote. That
is the right division of labour for the bulk of a script — the hundreds of
routine placements that grow a terrace — but the interesting steps of a
process are judgement calls: which site, which orientation, which order,
which alternative reaction. Today the only way to change one step is to edit
a generator or a JSON file outside the application, replay, and look.

A hand-authored process in the node network, before the node existed,
showed what that costs: a chain of `atom_edit` diffs with absolute
coordinates, folded through `atom_composediff`, truncated with `collect`, no
way to verify a step, and no way to insert one. The author's own notes asked
for named steps, stepping with the arrow keys, and "easier interspersing of
unpredicted new ones".

This design gives a user three things without a generator:

1. **Steps as a value.** A build script flows through the network as an
   array of records, so generated blocks, hand-authored blocks and loaded
   files concatenate with the array nodes that already exist.
2. **One-click placement.** Choose an operation, click one atom, and the
   step is placed exactly, with its rigid transform solved from the
   operation's own pattern against the workpiece — no coordinates typed,
   no orientation guessed.
3. **A separate editor node.** `mechanosynth` stays a two-pin replayer with
   a slider; `mechanosynth_edit` owns the authoring tools, the stored block
   and the undo commands, the way `atom_edit` owns editing and `apply_diff`
   only applies.

## Scope and non-goals

In scope (called T0 and T1 in the discussion that produced this document):

- an opaque `OpLibrary` data type and an `ops_library` node that loads one;
- a built-in `BuildStep` record and a `build_script` node that loads a JSON
  build file into `[BuildStep]`;
- `mechanosynth` taking `ops` and `steps` on wires, with the file
  properties kept as a deprecated fallback;
- one optional operation-library field (`chiral`), an effect-derived
  `touched` list, and a validated origin convention;
- a placement engine (`mechanosynth/place.rs`) that turns "this op, this
  clicked atom" into a list of candidate steps;
- the `mechanosynth_edit` node, its API, undo commands, text format and
  panel;
- an `export_build_script` node.

Out of scope, each named in §Follow-ups with only its node signature: area
apply and site repeat (T2), a `compare` node and a per-step valence guard
(T3), authoring operations from an interactive edit (T4), inserting into a
wired block, and any change to matching tolerance or replay semantics.

## Decisions and the alternatives they replace

Recorded first because every later section follows from them.

### Operation libraries are wired values, not global state

Every node that interprets a step against a workpiece takes an
`ops: OpLibrary` input pin. One node, `ops_library`, produces such a value
from a file. Nodes that only produce or reshape steps (loaders, array
nodes, the future site-repeat) never see ops.

Rejected:

- **A design-level operation registry** (import once, steps reference ops by
  name, resolved like record schemas). No wires, but a second global
  namespace in the design, and two libraries that both define `dimerize`
  with different geometry — the public diamond library and the internal
  silicon one do — need qualified names or a congruence rule to coexist.
  Judged too intrusive for the benefit.
- **A `BuildScript` product value carrying ops and steps together.** One
  wire, but every `map` over the steps needs an unbind/rebind pair, and
  concatenation needs a library-union rule.
- **Ops embedded in each step record.** Self-contained in memory, but the
  text format and JSON would repeat every pattern per step, and identity
  by name is what the file format already commits to.
- **Ops riding on the workpiece** the way a `Structure` rides on a
  `Crystal`. Matches the consumer set exactly, but puts process vocabulary
  on atoms and makes every structure node responsible for carrying it.

In practice the ops wire fans out to one node, the replayer, and to the
editor when there is one. The wire-count concern that motivated the
alternatives does not survive the consumer list.

### Steps are plain records

`[BuildStep]` is an ordinary array of a built-in named record. No wrapper
type, so `array`, `array_append`, `sequence`, `collect`, `map` and `switch`
work on it unchanged, and a text-format literal can spell a step.

### A separate editor node

`mechanosynth` keeps its contract — `base`, `ops`, `steps`, `step` in;
`result`, `step` out — and never grows editing state. `mechanosynth_edit`
is a second node with the same engine underneath.

Rejected: **editing inside `mechanosynth`.** The two jobs give the `result`
pin different meanings (state after N steps with replay highlights, versus
state at the cursor with ghost previews and host highlights), which forces a
mode flag that changes what a wire carries. `atom_edit`'s retired
`output_diff` checkbox is the precedent for how that ages. The split also
follows `atom_edit`/`apply_diff`, keeps the replayer safe to embed in shared
demos, confines persistence and undo to the editor, and lets the editor
cache prefix snapshots without the replayer paying for them. The cost is a
shared eval helper and a shared scrubber widget, both small.

### The authored block is appended after the wired prefix

The editor's stored steps come after whatever arrives on its `steps` pin.
The cursor can only sit inside the authored block. To author between two
generated blocks, chain two editors through `result`.

Rejected: **an edit list applied onto the wired steps** (insert at index k,
delete k). Insert-anywhere, but it breaks silently whenever the upstream
block changes length — the same drift problem absolute-coordinate diffs
have.

### Placement is click, translate, rigid fit

One click on a workpiece atom. The clicked atom may play any role in the
operation's `before` pattern that its element allows; the remaining roles
are found by a congruent search in the neighbourhood; the rigid transform
`(r, t)` comes from the fit. Candidates are whatever the environment admits;
the operation hardcodes none of them.

### Orientation comes from frame atoms in the operation

A one-atom `before` pattern carries no orientation. Rather than derive the
direction from the host's bonds in the application, the library states it:
a donation op lists the host's bonded neighbours as `*` atoms that appear
unchanged in `after`. Such *frame atoms* need no flag: a before atom that is
kept with the same position and element and no bond change is recognisable
from the two patterns alone. Kabsch on four non-planar atoms then gives `r`
exactly. Measured on Si(100), the ideal-site host and the
reconstructed dimer atom are not congruent (back-bonds 2.352 versus
2.452 Å, terminator tilt 19.7° versus 24.0°): one pattern would match both
within the 0.3 Å tolerance and place the added atom 0.11 Å wrong on one of
them, which the generator's replay-equals-target check rejects. So a
donation splits into one op per host environment, the way the H library
already splits `h_donate` from `h_donate_dimer` by bond length. The user
picks the variant; the wrong one fails the gate or commits with a visible
residual (§Exactness).

Deriving the direction from bonds survives only as the fallback for ops
without frame atoms (hand-written libraries), and it is labelled as such.

## T0 — types and nodes

### `OpLibrary` data type

A new `DataType::OpLibrary` and `NetworkResult::OpLibrary(Arc<OpLibrary>)`.
Opaque to the network in this design: no field access, no construction from
the network, no `expr` support. It is a value like `Structure` or `Motif`:
produced by one node, accepted by pins typed `OpLibrary`, shown in the text
format only as a wire. `Arc` because the replayer and the editor share one
parsed library and evaluation clones results.

### `BuildStep` record

A built-in named record, registered beside `MechanosynthStep`:

| field | type | meaning |
|---|---|---|
| `op` | `String` | operation name in the wired library |
| `t` | `Vec3` | translation, Å |
| `r` | `Mat3` | rotation (rows as in the JSON); identity when the file has none |
| `note` | `String` | free text, `""` when absent |
| `method` | `String` | `""` when absent |
| `phase` | `String` | `""` when absent |
| `layer` | `Int` | `-1` when absent |
| `site` | `Int` | `-1` when absent |

`MechanosynthStep` (the replayer's output record) is unchanged; it carries
replay provenance (`index`, `count`) that an authored step does not have.
It gains one field, `r: Mat3`, so a downstream network can orient a gadget
at a reaction site and not only place it. The two records stay distinct
types; a `record_destructure` on either works as today.

### `ops_library` node

- Input `file: String` (optional, overrides the stored path). Property
  `file`, stored relative to the project when possible, exactly as
  `import_cif` and the current `mechanosynth` do; the loader/saver hooks
  re-parse on load and relativise on save.
- Output `ops: OpLibrary`.
- A parse failure is an evaluation error naming the file and location (the
  `import_cif` pattern: the node loads, the failure shows when evaluated).
- Panel: the path field with Browse, a Reload button, and the library
  listing (name, note, before/after atom counts) that the editor's palette also
  uses.

### `build_script` node

- Input `file: String` (optional, overrides the stored path). Property
  `file`.
- Output `steps: [BuildStep]`.
- Parses the build JSON with the existing `parse_build_script`, then converts
  each `Step` to a record. The file's `tolerance` is dropped (see below).
- Unknown op names cannot be checked here — there is no library on this
  node — and are reported by the consumer.

### `mechanosynth` node changes

Pins become `base: HasAtoms`, `ops: OpLibrary`, `steps: [BuildStep]`,
`step: Int`. Outputs unchanged. Behaviour:

- **Tolerance.** With `ops` wired the replay tolerance is the library's
  (`library.tolerance`, else the default, now 0.05 Å). The per-file build tolerance
  override goes away: a step array has no header. A wired library's
  tolerance is the one value per replay, as before.
- **Unknown op** in a step array is a validation error on this node naming
  the step index and op; the replay does not run.
- **Legacy properties.** `ops_file` and `build_file` stay as deprecated
  stored properties. When the corresponding pin is unwired and the property
  is set, the node reads the file as today, so every existing project and
  text-format round trip keeps working. When a pin is wired the property is
  ignored and the panel hides its field. The panel shows a **Convert to
  nodes** button when either property is set: one undoable command that
  creates `ops_library`/`build_script` nodes with the paths, wires them in,
  and clears the properties. No automatic graph surgery on load.
- `steps` unwired and no `build_file`: the node replays nothing and emits
  the base (step 0), not an error, so a partially wired node still displays.

### `export_build_script` node

`steps: [BuildStep]`, `file_name: String`, `metadata` as on `export_atoms`.
Writes the current JSON build format from the array; `format` is set,
per-step `r` is written only when it is not the identity, `note`/`method`/
`phase` only when non-empty, `layer`/`site` only when not `-1`. Like
`export_atoms` it is an action, not part of evaluation.

### Operation-library schema extensions

One optional key, ignored by the current engine (unknown keys are already
skipped), so new files load on old builds:

- `"chiral": true` on an operation. The placement engine then drops fits
  with `det r = −1`. Default false: mirrored fits are offered, which the
  existing libraries need (a chemisorption landing is placed with `det −1`).

**`touched` is derived from effect, not from pattern membership.** Today
`apply_step` lists every id present in both patterns as touched, whether
or not the step changed it, so frame atoms would light up the host's
neighbours under `ms_current` on every donation. Instead of a flag to
exclude them, the rule becomes: an atom is touched when the step **added**
it, **moved** it, **changed its element** (compared against the workpiece's
current element, not the pattern's), **added, deleted or re-ordered a bond**
it is an endpoint of, or **deleted an atom it was bonded to**. Frame atoms
fail every clause and drop out by construction; the reacting atoms of every
existing op pass one — a `bridge` endpoint or a donation host through the
bond clause, an abstraction's host through the deletion clause. `added` is
unchanged. The two existing tests that assert "a kept atom is touched" are
rewritten to assert the clause that makes it so.

One convention is validated: `before` atom with id 1 sits at the origin.
A violation is a load-time **warning** attached to the library (the panel
shows it), never an error — foreign libraries may not follow it, and the
placement engine does not depend on it (`t` falls out of the fit). The
convention only names the natural atom to click.

### Engine

`replay` already takes `&OpLibrary` and `&[Step]`; the node-level change is
converting `[BuildStep]` records to `Step`s once per evaluation. The
conversion lives in `nodes/mechanosynth.rs` next to `step_record` (its
inverse) so the two stay in step. `resolve_tolerance` loses its script
argument, and `DEFAULT_TOLERANCE` becomes 0.05 Å (the comment in
`apply.rs` that explains the 0.3 figure, and the two `0.3` examples plus
the "uses 0.3 Å" sentence in the guide's two-files section, follow).

## T1 — placement engine

`rust/crates/atomcad-crystolecule/src/mechanosynth/place.rs`, a pure
function so it is testable without a node:

```rust
pub struct Candidate {
    pub step: Step,          // op, r, t; metadata empty
    pub roles: Vec<(i64, u32)>, // pattern id → workpiece atom id, frame atoms included
    pub residual: f64,       // max per-atom distance of the fit, Å
    pub exact: bool,         // residual < EXACT_FIT_RESIDUAL (1e-4 Å)
    pub mirrored: bool,      // det r < 0
    pub approximate: bool,   // orientation derived from bonds (fallback)
}

pub fn place(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    op: &str,
    clicked: u32,
    tolerance: f64,
) -> Result<Vec<Candidate>, MechanosynthError>;
```

Algorithm:

1. **Roles the click can play.** Every `before` atom whose element matches
   the clicked atom's element (`*` matches all). Frame atoms (kept,
   unchanged, no bond change) are eligible roles too, but ranked after the
   atoms the op changes.
2. **Neighbourhood.** Atoms within `extent + tolerance` of the clicked atom,
   where `extent` is the pattern's largest distance from the role atom.
3. **Assignment search.** For each role, recursively assign the remaining
   `before` atoms to distinct neighbourhood atoms with matching elements,
   pruning on pairwise distance (`|d_workpiece − d_pattern| ≤ 2·tolerance`).
   Patterns are small (at most five atoms plus frames), so this is cheap.
4. **Fit.** Kabsch (closed-form least-squares rigid fit via a 3×3 SVD; a
   small Jacobi or quaternion solver, no new dependency) on the assignment,
   proper and, unless `chiral`, improper — two separate fits, the mirror
   cannot be recovered from the proper one.
   Keep fits whose max per-atom residual is within tolerance. For a
   one-atom pattern the fit is `t = clicked position`, `r = identity`.
5. **Fallback.** If the `before` pattern has exactly one atom and `after`
   adds atoms off the origin, orientation is undetermined.
   Compute the host's free directions from its bonds (the guided-placement
   candidate code in `atom_edit/add_atom_tool.rs`, moved down into
   crystolecule if it is not already), emit one candidate per direction with
   `r` taking `+z` there and the azimuth chosen to keep `r` proper, and mark
   them `approximate`.
6. **Dedupe and rank.** Two candidates are one when they produce the same
   after state — the same set of placed positions and elements within
   1e-6 Å — not when they share `(r, t)`: a symmetric pattern (either end
   of `dimerize`, the permutations of three tetrahedral `*` frame atoms)
   yields several transforms for one reaction. Sort by residual, then proper before mirrored, then
   exact before approximate.

What the engine guarantees, against the two libraries in use: abstractions
commit with one click; donations with frame atoms commit with one click;
`dimer_open` and multi-atom insertions commit with one click (planar before
and after in the same plane make the mirror fit identical); `dimerize` and
`bridge` yield one candidate on a well-ordered build and two when the
partner is genuinely ambiguous; a four-atom chemisorption yields exactly two,
the two sides, one of them mirrored. Nothing in the engine knows any of
this; it is a property of the patterns.

The matching loop (`nearest unclaimed atom within tolerance, optional
element filter`) exists three times already (`match_diff_atoms`,
`apply_step`, `compare_structures`). This design adds a fourth caller, so
the loop is extracted into one primitive on `AtomicStructure` first, with
the element-symbol helper and canonical-bond keying that are duplicated
beside it.

## Exactness: the matching tolerance never reaches a coordinate

**Requirement.** A build authored interactively with a library whose
patterns were derived from the target geometry (the generator's libraries)
must produce the same structure as the generator's replay, to the file
rounding of 1e-6 Å. The 0.3 Å tolerance is unrelated to this figure.

**Why it holds.** `apply_step` never snaps a kept atom, and places every
moved or added atom at exactly `r · after.pos + t`. The tolerance only
decides whether a candidate is accepted; it never enters a coordinate. So
the result is exact if and only if each step's `(r, t)` is exact. The
generator writes them from the target. The editor gets them from the Kabsch
fit of the `before` pattern (frame atoms included) onto workpiece atoms. On
a workpiece whose atoms are where `materialize` and the earlier exact steps
put them, and with patterns derived from that same `materialize` geometry,
pattern and workpiece are congruent to floating-point precision and the fit
recovers `(r, t)` to ~1e-12 Å. The fit residual on such a step is ~1e-12,
and the placed atoms agree with the generator's to the file rounding. The
replayer and the editor share one engine, so the authored steps replay to
the same result wherever they are consumed.

**Where it can leak, and what stops it.**

1. *An inexact fit accepted because it passed the gate.* A wrong
   environment variant, or a foreign pattern, can fit an environment with a residual of
   0.1 Å and place an atom 0.1 Å off. Ranking by residual picks the exact
   variant when one exists; when none exists the fit is still accepted, so
   it must be visible.
2. *The bond-derived fallback.* 0.15–0.17 Å off by construction on a
   reconstructed dimer atom. A later fit whose `before` pattern includes
   such an atom spreads part of that error into its own placement; the
   error decays through re-placing ops but does not vanish.

Therefore:

- **Generated libraries state a tight tolerance.** The tolerance is a
  per-library value in the file. With frame atoms a generated library is
  congruent to its workpiece to ~1e-12 Å, the files round to 1e-6, and the
  smallest environment difference known is the 0.12 Å between an ideal-site
  host and a dimer atom; so the generator writes `tolerance: 0.05` (an order
  of magnitude below that), and a wrong variant no longer fits at all.
  Nothing legitimate lies between 1e-6 and 0.05 on a generated build: relax
  is never inside a replay and every coordinate comes from one
  `materialize`. **The engine default changes to match**:
  `DEFAULT_TOLERANCE` in `mechanosynth/schema.rs` goes from 0.3 to 0.05 Å,
  so a file that states no tolerance gets the tight gate too. A
  hand-written library that needs slack states its own value; the two
  existing libraries state 0.3 explicitly and are unaffected until the
  generator rewrites them.
- Every candidate carries its fit `residual` (§placement engine). The
  editor **stores the residual with each authored step** (node data, not a
  `BuildStep` field) and classifies the step *exact* when the residual is
  below `EXACT_FIT_RESIDUAL = 1e-4 Å`, the generator's own congruence
  threshold; otherwise *inexact*. Fallback candidates are *approximate*.
- Inexact and approximate steps show a warning chip in the steps list with
  the residual, and `export_build_script` reports their count. A design with
  no chips is exact to file rounding.
- The placement engine ranks by residual before anything else, so an exact
  candidate always outranks an inexact one.
- One test pins the claim (§Tests, *round-trip exactness*): re-author a
  generated build by driving `place()` with each step's origin atom in
  order, choose the candidate whose `r` matches the generator's, and require
  every residual below 1e-4 Å and the final structure equal to the
  generator's replay to 1e-6 Å.

Error amplification is of order one for the current libraries: an added
atom sits at about the same distance from the frame as the frame atoms
themselves, so a residual of ε moves it by about ε. A future pattern that
places an atom far from a small frame would amplify; the recorder (T4)
should warn on such patterns.

## T1 — the `mechanosynth_edit` node

### Pins and data

| | |
|---|---|
| inputs | `base: HasAtoms` (required), `ops: OpLibrary` (required), `steps: [BuildStep]` (optional prefix) |
| output 0 | `result` — the workpiece after the prefix and the authored steps up to the cursor; same concrete type as `base` |
| output 1 | `steps: [BuildStep]` — prefix followed by the whole authored block, cursor ignored |
| stored | `authored: Vec<AuthoredStep>` (a `Step` plus its fit `residual` and `approximate` flag), `cursor: i32` (index into the authored block, `0` = none of it, `-1` = all, the replayer's clamp) |
| transient (`#[serde(skip)]`) | placement state (chosen op, pending candidates), the prefix snapshot, last replay error |

`result` is painted with `ms_current` on the cursor step's touched atoms and
with nothing else; `ms_added`/`ms_layer` are the replayer's concern. Ghost
previews and host highlights are display-only decorations, never atoms in
the output.

Evaluation replays `prefix ++ authored[..cursor]` with the shared helper.
The prefix snapshot — the workpiece after the wired prefix — is kept in
transient data keyed on the input's `env_epoch`, so a placement or a cursor
move replays only the authored block. That is the only cache; the rule from
`feedback_avoid_speculative_caching` applies to anything further.

### Cursor

One integer: "the state after authored step k". The scrubber is the cursor.
Placing a step inserts it at `k+1` and moves the cursor to it. Selecting a
row in the steps list moves the cursor there. The cursor is node data but
its changes are **not** undo commands, like the replayer's slider.

### Placement tool

Active only while the node is selected and its `result` pin is the displayed
one (the same rule `atom_edit` uses to own viewport picks). States:

1. **Idle.** The palette lists the wired library's operations, with a
   filter box. Choosing one enters Armed. Typing selects by prefix.
2. **Armed.** The prompt reads "click the *host* atom" (the origin atom's
   element and, when present, the op `note`). A viewport click on an atom
   calls `place`; a click elsewhere or Escape returns to Idle.
3. **Candidates.** Zero: the engine's message is shown in place ("no
   `dimerize` partner within tolerance of the clicked atom; nearest bare Si
   at 4.59 Å") and the tool stays Armed. One: commit immediately. Several:
   every candidate's after state is drawn as ghosts at once (added atoms
   green, deleted red, moved with an arrow, at 40 % alpha, the
   `xray`-style ghosting), the panel lists them ("2 of 2: mirrored, residual
   0.00 Å"), Tab cycles, clicking a ghost atom that belongs to exactly one
   candidate chooses it, Enter takes the highlighted one, Escape abandons.
4. **Commit.** One undo command inserts the step after the cursor with
   metadata copied from the previous authored step (or the last prefix
   step), moves the cursor to it, and returns to Armed with the same op
   so a series of identical placements is one click each.

Approximate candidates (fallback orientation) carry a warning chip on the
step row, "orientation derived from bonds", so a user knows the coordinates
came from the application, not the library.

### Steps list and metadata

The panel shows the prefix as a read-only, collapsed block ("142 steps from
`build_script`") and the authored block as rows: index, op, method colour,
note. Rows drag to reorder, delete, duplicate; each is one undo command.
The chapter list from the replayer's panel appears above the rows and jumps
the cursor. Chips edit `method`, `phase`, `layer`, `site`, `note` per step;
one undo command per edit, coalesced while the same chip has focus.

### Undo commands

`InsertStepCommand`, `DeleteStepCommand`, `MoveStepCommand`,
`SetStepMetadataCommand`, all on the node's `authored` vector through the
`with_node_data` pattern, restoring `cursor` alongside. A placement is one
`InsertStepCommand`. `Convert to nodes` on the replayer is its own command.

### API

All `#[frb(sync)]`, all taking `scope_path` and `node_id`, thin wrappers
over `#[frb(ignore)]` functions on `&StructureDesigner` so the tests need no
`CAD_INSTANCE`:

- `get_mechanosynth_edit_data` → stored block, cursor, prefix length,
  library op names, last error.
- `set_mechanosynth_edit_cursor`, `begin/endNodeDataDrag` around scrubs.
- `mechanosynth_edit_arm(op)`, `mechanosynth_edit_pick(atom_id)` →
  candidate list (indices, residual, mirrored, approximate, ghost atoms for
  rendering), `mechanosynth_edit_choose(index)`, `mechanosynth_edit_cancel`.
- `mechanosynth_edit_insert/delete/move_step`, `set_step_metadata`.
- `mechanosynth_convert_files_to_nodes` on the replayer.

Ghost rendering goes through the same path the guideline tool and guided
placement use for transient viewport overlays; the API returns positions,
elements and kinds, and Flutter never sees the engine's structures.

### Text format

The authored block serialises as a `steps` property of record literals, one
per line, and `cursor` as an int:

```
edit = mechanosynth_edit { base: slab, ops: lib, steps: gen, cursor: 3, authored: [
  { op: "habst", t: (12.71, 9.53, 8.02), method: "probe", phase: "layer1", layer: 1, site: 0 },
  { op: "dimerize", t: (14.27, 9.53, 8.02), r: ((0, 1, 0), (-1, 0, 0), (0, 0, 1)), method: "relax" },
] }
```

Identity `r` and empty/`-1` metadata are omitted on output and defaulted on
input, so a short step stays short. `get_text_properties` must stay total
(an empty `authored` is `[]`), the rule learned on the replayer. The
round-trip corpus test (`project_text_format_roundtrip`) gains a fixture
with an editor node.

### Reference guide

- `doc/reference_guide/nodes/atomic.md`: a new `## mechanosynth_edit`
  section (pins, the placement tool, candidates, the steps list, the text
  format), new `## ops_library`, `## build_script`, `## export_build_script`
  sections, and the `## mechanosynth` section updated for the `ops`/`steps`
  pins, the deprecated file properties and Convert to nodes.
- `doc/reference_guide/nodes/math_programming.md`: the `BuildStep` record
  beside `MechanosynthStep`; `r` added to the latter.
- The two-files section of the `mechanosynth` guide documents `chiral`,
  frame atoms as a pattern-writing convention, the tolerance advice, the effect-derived
  `ms_current` rule, and the origin convention.

## Follow-ups (signatures only)

- **T2 area apply.** On the editor: select N host atoms (marquee, region
  Blueprint or tag) with an op armed → one step per host whose candidate
  list has exactly one entry, in a chosen order (pick order, along a lattice
  direction, nearest-first); hosts with several candidates are listed and
  skipped.
- **T2 site repeat.** `steps_move { steps: [BuildStep], translation: Vec3,
  rotation: Mat3 } → [BuildStep]`: applies one rigid transform to every
  step's `t`/`r`. Pure array node, no ops.
- **T3 compare.** `compare { a: HasAtoms, b: HasAtoms, tolerance: Float }`
  → `a` with a mismatch tag and a count; wraps the existing
  `compare_structures`. Plus the per-step valence guard from the node
  design's wanted list.
- **T4 op authoring.** Record an operation from an `atom_edit`: the picked
  atoms define `before` and the frame (origin atom, +x atom, +z from a third
  atom or the surface normal), the edit defines `after`; written into a
  library file. Needs the op ⇄ diff lowering described in the 2026-09-11
  assessment.

## Tests

Conventions as in the crate `AGENTS.md` files: tests in the owning crate's
`tests/` directory, fixtures under `rust/tests/fixtures/mechanosynth/`,
synthetic and small.

- **Schema/engine** (`crystolecule/tests/crystolecule/mechanosynth_test.rs`):
  `touched` derived from effect — a kept atom with no change and no bond
  change is not touched, a kept atom with an added, deleted or re-ordered
  bond is, an element set to its current value is not a change; the two
  existing kept-atom assertions rewritten; `chiral` parsed and ignored by
  replay; the default tolerance is 0.05 — the engine tests that rely on the
  old default through fixtures without a `tolerance` key (only
  `valid_ops.json`/`valid_build.json` state one) are checked one by one
  and either keep passing at 0.05 or state 0.3 in the fixture where the
  test is about slack, never by loosening the default; origin-convention warning; a file with the new keys
  loads on the old code path unchanged.
- **Placement** (`crystolecule/tests/crystolecule/mechanosynth_place_test.rs`,
  new fixture `place_ops.json`): one-atom abstraction → one exact candidate;
  donation with three frame atoms on a tetrahedral host → one candidate,
  `r` recovered to 1e-9 from a host rotated by a random proper rotation;
  the two environment variants of a donation → the wrong one rejected at
  the 0.05 Å default and accepted-but-inexact when the library states 0.3; two-atom `dimerize` with two bare neighbours → two candidates;
  planar three-atom op → one candidate (mirror deduped); planar four-atom
  op with out-of-plane `after` → two candidates, one mirrored, and only one
  with `chiral`; clicked atom in a non-origin role → same step as clicking
  the origin atom; one-atom op without frame atoms and an off-origin `after`
  → approximate candidates, one per free direction; no fit → empty list and
  a `NoMatch`-style diagnostic naming the nearest atom.
- **Round-trip exactness** (same file, fixture = the public diamond
  library once it carries frame atoms): drive `place()` with each generated
  step's origin atom in order on a workpiece built by the same engine,
  choose the candidate whose `r` matches the generator's, assert every
  residual `< 1e-4 Å`, and assert the final structure equals the
  generator's replay atom for atom to 1e-6 Å. This is the test that makes
  §Exactness a checked requirement rather than a claim.
- **Nodes** (`structure-designer/tests/structure_designer/`): `ops_library`
  and `build_script` load, relativise and reload; `mechanosynth` with wired
  `ops`/`steps` equals the file-driven replay atom for atom; unknown op →
  validation error naming the step; legacy properties still evaluate;
  Convert to nodes produces the same result and is undoable;
  `mechanosynth_edit` eval equals `mechanosynth` on `prefix ++ authored` at
  every cursor; insert/delete/move/metadata commands and their undo restore
  `authored` and `cursor`; text-format round trip of an editor node.
- **API** (`rust/tests/structure_designer_api/`): the pick → candidates →
  choose sequence inserts one step and one undo entry; cancel inserts
  nothing.
- **Manual walkthrough** (human): palette, click-to-place on a diamond
  library build, a two-candidate pick with ghosts, reorder in the list,
  arrow-key scrub, Convert to nodes on the demo project. The Flutter smoke
  test is not run by agents.

## Implementation phases

1. **P1 — values and loaders.** Matcher primitive extraction; schema
   extensions; `OpLibrary` type and result; `BuildStep` record and `r` on
   `MechanosynthStep`; `ops_library`, `build_script`, `export_build_script`
   nodes; `mechanosynth` pins, legacy fallback, Convert to nodes; FRB
   regenerated; guide sections. Baseline: the demo projects replay
   unchanged before and after conversion.
2. **P2 — placement engine.** `place.rs` and its tests, including the
   round-trip exactness test. No UI.
3. **P3 — the editor node.** Node, data, eval with the shared helper and
   prefix snapshot, undo commands, API, text format, round-trip fixture.
4. **P4 — panel and tool.** Scrubber and chapter list extracted from
   `mechanosynth_editor.dart` into a shared widget; palette, prompt,
   candidate list, ghosts, steps list, chips; the placement tool wired into
   viewport picking.
5. **P5 — guide and walkthrough.** Remaining guide pages, screenshots slot,
   the manual checklist.

Outside the repository, a prerequisite for one-click donations on the real
libraries: the generator emits frame atoms and environment variants for
its donation and dimerization ops, and writes `tolerance: 0.05`. The diamond output
stays the regression reference for everything else.

## Considered and rejected

- Global op registry, `BuildScript` product type, ops in steps, ops on the
  workpiece — see §Decisions.
- Editing inside `mechanosynth`; an edit list over the wired prefix — see
  §Decisions.
- A `frame: true` flag on pattern atoms, to keep frame atoms out of
  `ms_current` and the pick prompt. Redundant: whether a step changed an
  atom is decidable from its effect, and an effect-derived `touched` list
  is the more honest rule for every op, flag or no flag.
- Hardcoding candidate orientations in an operation. Candidates are
  environment symmetries, found by the fit; hardcoding them fails on any
  surface the author did not foresee.
- Deriving donation directions from the host's bonds as the primary
  mechanism. 4.3° off on a reconstructed dimer atom, 0.15–0.17 Å at the
  added atom; kept only as the labelled fallback.
- A `family` key grouping environment variants in the palette, with the
  variant chosen by smallest residual. Complicates the schema and the
  engine to hide a distinction the user should learn; the residual chip and
  a tight library tolerance make the wrong variant fail visibly instead.
- Pinning `t` to the clicked atom for multi-atom ops. The fit's `t` is what
  congruence-derived ops effectively produce; kept atoms are never snapped,
  so `t` only decides where added atoms land.
- Automatic conversion of legacy file properties on load. Graph surgery
  without the user asking; a button instead.
- A per-array tolerance for `[BuildStep]`. Arrays have no header; the
  library's tolerance is the one value per replay.

## Open questions

- Whether `MechanosynthStep` should become a hierarchical record wrapping a
  `BuildStep` (`{ step, index, count }`) once hierarchical records reach the
  network; for now the two flat records share field names.
- Whether the editor should offer "commit the mirrored candidate by
  default" as a per-op preference for users who place many landings.
- The neighbourhood radius for the assignment search on libraries with
  large patterns (the T4 recorder may produce them); `extent + tolerance`
  is right for the current libraries.
