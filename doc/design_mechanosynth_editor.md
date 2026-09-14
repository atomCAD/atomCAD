# Design: build scripts as network values and the `mechanosynth_edit` node

Status: **draft 2026-09-11, revised 2026-09-14** (clicked-atom role rule,
tests moved into phases; then atom-first placement, the first-shell frame
rule and the environment-variant convention — §Applicability and the last
subsection of §Decisions, all landing in Phases 3–5 because Phase 2 is
closed). **All five phases implemented 2026-09-14**, with Phase 4's UI revised
twice after use — see §Revised for the removal of the armed mode, the move of
the ghost preview into the scene, and the inlining of the orientation variants. Extends the `mechanosynth` subsystem
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

One click on a workpiece atom. The clicked atom is assigned one role in the
operation's `before` pattern (next subsection); the remaining roles are
found by a congruent search in the neighbourhood; the rigid transform
`(r, t)` comes from the fit. Candidates are whatever the environment admits;
the operation hardcodes none of them.

### The clicked atom's role is fixed by rule, never chosen in the UI

An atom can often stand for more than one `before` atom: both ends of
`dimerize` are bare Si, a donation host and its `*` frame neighbours all
admit a Si click, and any atom admits a `*` role. Orientation candidates
are already a choice the user sometimes has to make; a second choice —
"which pattern atom did you mean" — layered on top would turn a one-click
tool into a dialog. So the role is decided by a fixed rule before the
search runs:

1. Among the `before` atoms whose element admits the clicked atom (`*`
   admits all), take the one at the origin of the operation's coordinate
   system (position within 1e-6 Å of `(0, 0, 0)`).
2. If none of the eligible atoms is at the origin, take the eligible atom
   with the smallest id.

The rule is deterministic and learnable. For every operation whose origin
atom is the reacting atom — the convention the loader already warns about —
"click the atom the operation acts on" is the whole instruction, and the
Armed prompt says which element that is. Clicking an atom that cannot be
the origin atom (the H of an abstraction, whose origin atom is the C) still
works, through the fallback, because only one role admits it.

The rule is strict: if the search finds no congruent assignment with the
clicked atom in its assigned role, the result is "no match", not a retry in
another role. A silent role change would place the reaction on a different
atom from the one the user clicked, which is exactly the surprise the rule
exists to remove. The diagnostic names the role it tried.

Rejected: **offering the roles as candidates** beside the orientation
candidates. Every asymmetric op with two same-element atoms would then
always produce at least two candidates, and the common case would need a
second interaction. Also rejected: **falling through to the next eligible
role when the fit fails**, for the reason above.

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
already splits `h_donate` from `h_donate_dimer` by bond length. Which variant
applies is decided by the fit rather than by the user, and the wrong one fails
the gate or commits with a visible residual (§Exactness, and the next
subsection for what the split means for the palette).

A two-atom `before` has the same problem in a second guise: two atoms are
collinear, so the roll about their axis is undetermined, and an `after` that
displaces them off that axis — a reconstructed dimer does, by 0.03 Å — would
have the displacement land at an arbitrary azimuth. Frame atoms fix that too,
which is why dimerization needs them as much as donation does.

**Which atoms become the frame is a library-authoring choice, and the rule is
the first shell: the bonded neighbours of the origin atom, nothing further
out.** A host and its three neighbours are four non-planar atoms, which is
exactly what Kabsch needs, and every distinction such a frame can draw is a
distinction in bond lengths and angles *at the host* — chemistry. A frame
taken from a radius instead would split operations on third-shell differences
that change nothing about the reaction, and the variant count would multiply
for no information gained. The engine imposes none of this: it reads whatever
patterns a library states, and `is_frame_atom` recognises them from the two
patterns either way.

Deriving the direction from bonds survives only as the fallback for ops
without frame atoms (hand-written libraries), and it is labelled as such.

### The library answers "what can be done here"

A library whose patterns fix orientation exactly is a library with more
operations in it. Frame atoms state the host's environment, and an operation
that states an environment applies only to that environment: an ideal-site
host and a reconstructed dimer atom become `si_donate` and `si_donate_dimer`,
and the same split runs through every donation. That is not a cost to be
minimised. It is the library saying how many distinct situations it has
actually been calculated for, and the two ways of keeping the list short were
both measured and both fail §Exactness — one tolerant operation places an atom
0.11 Å wrong on the variant it was not derived from, and one operation
oriented from the host's bonds is 0.15–0.17 Å off.

So the operation count grows, and the interaction inverts to keep it
invisible. **The primary flow is atom-first**: click an atom, and the editor
answers with the operations that fit *it*, ranked (§Applicability, §Placement
tool). The user reaches `si_donate_dimer` by choosing the entry that is
offered, which is the one whose pattern matches what is actually there — the
gate resolves the variant, the way argument types resolve an overload. Under a
library tolerance of 0.05 Å at most one variant of a family can fit, so the
list stays short and correct, and the variant names are read as an answer
rather than as a taxonomy to be learned up front. Op-first survives as the
repeat flow: after a commit the tool stays armed with the same operation, so a
run of identical placements is one click each.

Two consequences, because they are why this works rather than decoration:

- **A miss becomes a coverage report.** When nothing fits, the offer list
  still shows what came close and how close: "`si_donate_dimer` — 0.31 Å off"
  says this host is not an environment the library knows. A bare "no
  placement" says nothing, and a precalculated library's real limit is exactly
  the set of environments its generator enumerated, so that limit should be
  readable at the point of use.
- **The variants must be nameable.** `si_donate_2`, `si_donate_3` would mean
  the split is not understood — the equivalent of calling two types `Type1`
  and `Type2`. The convention is `<operation>_<environment>` with the
  environment vocabulary **shared across operations**, so that a library where
  `cl_donate_dimer`, `si_donate_dimer` and `h_donate_dimer` all name the same
  kind of host is one a user learns once. Nothing in the engine reads the
  name; this is a convention for whoever writes the generator.

Rejected: **merging variants to keep the palette short.** A variant is a claim
— "this reaction, on this site, has this geometry, and it was computed" —
so merging two deletes a claim the library has evidence for and replaces it
with a tolerance wide enough to hide the difference. That is the 0.3 Å gate
this design already moved away from.

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
`step: Int`. Output pins unchanged (the `step` record gains `r`, §`BuildStep`
record). Behaviour:

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
`export_atoms` it is a `Unit`-returning node whose `eval` runs only under the
execute flag (`doc/design_node_execution.md`, the central skip rule), so an
ordinary evaluation never writes a file.

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
placement engine does not need it: `t` falls out of the fit, and the role
rule (§Decisions) uses the origin atom when there is one and the smallest
eligible id when there is not. The convention names the natural atom to
click; following it is what makes "click the atom the operation acts on"
true for every op in a library.

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
    pub role: i64,           // before-pattern id the clicked atom plays (same for every candidate)
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
    tolerance: f64,   // the caller passes resolve_tolerance(library); a parameter so tests can vary it
) -> Result<Vec<Candidate>, MechanosynthError>;
```

`Ok` always holds at least one candidate. Every way of ending with none is
an `Err`, so the diagnostic travels with the failure instead of beside an
empty list. Three new `MechanosynthError` variants carry them:
`UnknownOp { op }` when the library has no such operation;
`NoRole { op, element, accepted }` when no `before` atom admits the clicked
element; and `NoPlacement { op, role, element, nearest }` when the assigned
role finds no congruent assignment — `role` is the pattern id the click was
given, `nearest` the same "nearest bare Si at 4.59 Å" string `NoMatch`
already builds. The existing `NoMatch` stays the replay error; it is about
a step, not a click.

Algorithm:

1. **Role of the clicked atom.** The eligible `before` atoms are those
   whose element matches the clicked atom's element (`*` matches all),
   frame atoms included. Exactly one is chosen by the rule in §Decisions:
   the eligible atom at the origin, else the eligible atom with the
   smallest id. No eligible atom → an error naming the clicked element and
   the elements the op accepts. The role is fixed for the rest of the
   algorithm; it is never revisited.
2. **Neighbourhood.** Atoms within `extent + tolerance` of the clicked atom,
   where `extent` is the pattern's largest distance from the role atom.
3. **Assignment search.** With the clicked atom fixed in its role,
   recursively assign the remaining
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
   1e-6 Å — not when they share `(r, t)`: a symmetric pattern (the
   permutations of three tetrahedral `*` frame atoms around a fixed host)
   yields several transforms for one reaction. Two `dimerize` partners, by
   contrast, are two reactions and stay two candidates. Sort by residual,
   then proper before mirrored, then exact before approximate.

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

## T1 — applicability: what can be done here

Added by the atom-first revision, so it lands in **Phase 3** with the editor
node's API rather than in the closed Phase 2. It is one more pure function
beside `place()` in `place.rs`, and a wrapper over it rather than a second
search:

```rust
pub struct Applicability {
    pub op: String,
    /// Candidates within the library tolerance, ranked; empty for a near miss.
    pub candidates: Vec<Candidate>,
    /// The best fit found *outside* the gate, kept so a near-miss row can be
    /// previewed and measured rather than merely counted. `None` whenever
    /// `candidates` is non-empty — a row is one or the other, never both.
    pub near_miss: Option<Candidate>,
    /// Best residual of `candidates`, or of `near_miss`.
    pub best_residual: f64,
    pub fits: bool,          // best_residual <= tolerance
    pub approximate: bool,   // every candidate came from the fallback
}

/// One entry per library operation that has anything to say about `clicked`,
/// ranked; never an error, because "nothing applies here" is an answer.
pub fn applicable_ops(
    workpiece: &AtomicStructure,
    library: &OpLibrary,
    clicked: u32,
    tolerance: f64,
) -> Vec<Applicability>;
```

For each operation it calls `place()` at `tolerance * NEAR_MISS_FACTOR`
(`NEAR_MISS_FACTOR = 10`, so a 0.05 Å library reports misses out to 0.5 Å) and
partitions the candidates at `tolerance`. An operation with at least one
candidate inside is *applicable* and keeps those candidates; one with only
candidates outside is a *near miss* and keeps the best of them in `near_miss`,
so the editor can preview and measure it; an operation whose `place()` returns
`UnknownOp`, `NoRole` or `NoPlacement` drops out of the list entirely — it has
nothing to say about this atom. The relaxed gate widens the neighbourhood
search along with it (`extent + tolerance`), which is what a near-miss search
wants.

Candidates outside the gate never leave this struct's `near_miss` slot: they
are for showing, not for placing, and the editor refuses to commit one
(§Placement tool). Keeping them in a separate field rather than mixed into
`candidates` is what makes that refusal a type-level fact instead of a filter
every caller has to remember.

The result is sorted: applicable before near miss, then by `best_residual`,
then proper before mirrored, then exact before approximate — `place()`'s own
order with one key in front of it.

**No grouping, and still no `family` key.** Under a library tolerance of
0.05 Å at most one variant of a family fits, so the list does not contain the
near-duplicates that grouping would exist to collapse. The other variants
appear, if at all, in the near-miss section, where the residual is precisely
the information that makes them worth showing.

Cost is one `place()` per library operation. Patterns hold at most a handful
of atoms and the assignment search is pruned, so a twenty-operation library is
one short sweep: cheap **per click or selection**, and to be treated as too
expensive per mouse-move. The editor runs it on a pick, never on hover
(§Placement tool).

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
  the residual, and the editor's panel reports their count above the list.
  The residual is node data; it does not travel on the `steps` wire, so
  `export_build_script` cannot see it and does not try to. A design with no
  chips is exact to file rounding.
- The placement engine ranks by residual before anything else, so an exact
  candidate always outranks an inexact one.
- A host the library has no variant for shows up as a **near miss** in the
  offer list, with its residual (§Applicability), rather than as a silent
  absence — so the gap between the library's environments and the workpiece's
  is readable where it matters.
- One test pins the claim (Phase 2, *round-trip exactness*): re-author a
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
| stored | `authored: Vec<AuthoredStep>` (a `Step` plus its fit `residual` and `approximate` flag), `cursor: i32` (number of authored steps applied, `-1` = all; §Cursor) |
| transient (`#[serde(skip)]`) | placement state (chosen op, pending candidates, the last offer list and the atom it was taken on), the prefix snapshot, last replay error |

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

One integer `k`, the number of authored steps applied: `result` is the
state after `authored[..k]`, so `0` shows the prefix alone, `authored.len()`
shows everything, and `-1` means "all" and follows the block as it grows
(the replayer's clamp; any value beyond the length also clamps). Rows in the
steps list are numbered from 1, so row `k` is the step the cursor has just
applied. The scrubber is the cursor. Placing a step inserts it at index `k`
(right after the last applied step) and sets the cursor to `k+1`. Selecting
a row moves the cursor to that row's number. The cursor is node data but its
changes are **not** undo commands, like the replayer's slider.

### Placement tool

Active only while the node is selected and its `result` pin is the displayed
one (the same rule `atom_edit` uses to own viewport picks). The primary flow
is **atom-first** — click an atom and the library answers (the last
subsection of §Decisions); op-first is kept as the repeat flow, entered
by a commit or from the palette.

#### The offer popup

The offer list is a **viewport overlay anchored to the clicked atom**, not a
section of the property panel: the answer appears where the question was
asked, which is the whole point of the inversion, and a 300 px panel on the
far right would put eye and mouse travel on every placement. Concretely: an
`Overlay` entry above the viewport, positioned each frame from the anchor
atom's world position projected to screen, offset up and to the right,
clamped to stay wholly inside the viewport, closed when the anchor atom is no
longer in `result`. It holds keyboard focus while open. About 300 px wide,
eight rows before it scrolls.

**Revised after first use (2026-09-14), twice.** The second round is §Revised
below; this first one is the placement of the list. "Offset up and to the right" from the
anchor is wrong, and the clamp made it worse. A preview is not a point: a
`precursor_chemisorb` row ghosts six added atoms across two dimers, so a 300 px
list pinned 18 px off the host sits on top of the reaction it describes — and
when the clamp bites, the list stops indicating its atom at all. Three changes,
all in Flutter:

- the list is placed clear of an **action box** — the anchor *plus* the
  highlighted row's ghost atoms — taking the first free side, right first;
- the anchor atom is **ringed** (the state-Idle "query anchor" ring, which
  Phase 4 had not implemented) and a dashed **leader line** joins ring to list
  whenever they are apart, so the relation survives the list moving;
- the **header drags**, because the automatic placement knows the anchor and
  the ghosts and not what the user is actually looking at. The drag is stored
  as a delta from the automatic position, so the list still follows the atom;
  a button in the header clears it.

```
┌──────────────────────────────────────────┐
│ 3 operations apply to this Si            │  header: count, clicked element
├──────────────────────────────────────────┤
│ si_donate_dimer            exact · 1  ▸  │  name, badges; highlighted row
│   Si donation onto a reconstructed…      │  the library's own note, elided
│ cl_donate_dimer            exact · 1     │
│   Cl2 dose onto a reconstructed dimer…   │
│ dimerize                   exact · 2     │  2 = candidate count
├──────────────────────────────────────────┤
│ si_donate               0.31 Å off       │  near misses: dimmed, unselectable
│ cl_donate               0.44 Å off       │
└──────────────────────────────────────────┘
```

Badges come from `Applicability`: the candidate count, and one of *exact* /
*inexact, 0.02 Å* / *approximate*.

**Near-miss rows are not selectable.** They are the `fits == false` entries,
below a rule, dimmed. Enter or a click on one replaces the row with the reason
— "0.31 Å off; this host is not an environment `si_donate` was calculated for.
Add the variant, or loosen the library's tolerance." — and nothing is
inserted. There is no cast past the gate, deliberately: an inexact step
*inside* the gate commits with a residual chip (§Exactness), but outside it
the library is stating it has not computed this situation, and the honest
fixes are the two named in the message.

**Highlighting a row previews it.** The highlighted row's first candidate is
ghosted on the workpiece at once — added atoms green, deleted red, moved with
an arrow, 40 % alpha, the `xray`-style ghosting — and a near-miss row previews
its `near_miss` candidate in amber. This is free: the sweep computed and kept
every candidate, so moving the highlight redraws a ghost and never calls
`place` again. It is also what makes a library of variants learnable — the
user sees each reaction happen on the real workpiece before choosing — so it
is not optional polish.

Typing filters the rows by prefix against the operation name. The sweep is not
re-run; the filter only hides rows.

#### Revised after first use (2026-09-14): no armed mode, ghosts in the scene

Two of Phase 4's decisions did not survive contact with the silicon library.
Both are recorded here rather than rewritten above, because the reasoning that
produced them is still the reasoning a reader needs.

**The armed mode is gone.** §States 3 and 5 gave a commit the side effect of
arming the operation it just placed, so that a run of identical placements was
one click each. That assumed the repeated thing is an *operation*; in a library
that splits one reaction into one operation per host environment
(`si_donate_dimer`, `si_donate_site`, `si_donate_core`, …) the repeated thing is
a **family**, and the specific operation just placed is usually the wrong one at
the next site. The cost was a click whose meaning depended on invisible state.
So: a viewport click is always the atom-first question, and a commit returns the
tool to Idle. The palette lists the library and arms nothing. `arm` and `pick`
are gone from the kernel, the API and the UI, along with `ToolState::Armed`,
`PlacementState::armed` and the `NoFit`-falls-back-to-offers path that only an
armed pick could reach.

Fast repetition is still wanted, and is deferred to its own design rather than
approximated here. The two shapes worth considering, both from the T2
area-apply machinery: apply a whole *family* automatically to a chosen set of
hosts, letting the fit pick the variant per host; or arm a family as a tool and
**highlight every atom it fits**, which is the affordance §Placement tool
deliberately declined to build for a single operation.

**The ghosts moved into the scene.** Phase 4 implemented the preview as a
projected 2D overlay in Flutter, on the grounds that the decorator route means a
full evaluation and re-tessellation per redraw and the preview follows the
*highlight*, which moves on every hover and every arrow key. The premise was
right and the conclusion was wrong: what to fix was the highlight, not the
drawing. A 2D overlay is not depth-tested, so a ghost behind an atom draws in
front of it; it is not shaded, so it does not read as an atom; and a
`CustomPainter` paints outside its widget, so ghosts near the viewport edge
landed on the node editor and the property panel.

So the preview is `MechanosynthGhostVisuals` on the decorator, tessellated into
the transparent impostor pass, and **the popup activates a row on a click
instead of a hover**. A row also carries its own **apply** button, so a user who
already knows the operation places it in one click and pays for no preview at
all. Hovering does nothing; opening the list previews nothing; filtering clears
the selection rather than picking a new row. What stays in Flutter is the
annotation about the *question* — the anchor ring and the leader line — clipped
to the viewport.

#### States

1. **Idle.** The panel's prompt reads "Click an atom to see what can be done
   there." The palette still lists the wired library's operations with a
   filter box for the op-first entry, and choosing one enters Armed. A
   viewport click on a workpiece atom runs `applicable_ops`, rings the atom as
   the *query anchor*, and enters Offers. The click does not change the app's
   atom selection: the anchor is the tool's own transient state.
2. **Offers.** The popup above, first row highlighted and previewed. Up/Down
   move the highlight, typing filters, Enter or a click takes the highlighted
   row, Escape or a click on empty space returns to Idle. An empty result is
   the popup with "nothing applies to this Si" and no rows — an answer, not an
   error. Choosing a row goes to Candidates **with the candidates the sweep
   already computed**; `place` is not run a second time.
3. **Armed.** A strip at the top of the viewport reads "`si_donate_dimer`
   armed — click a host · Esc to stop", naming the origin atom's element and,
   when present, the op `note`. A viewport click on an atom calls `place` for
   that one operation; Escape returns to Idle. An `Err` (`NoRole`,
   `NoPlacement`) **opens the offer popup at that atom**, headed by the
   failure — "`si_donate_dimer` doesn't fit this Si — nearest bare Si at
   4.59 Å" — with the operations that do fit beneath it, so a click on the
   wrong atom, or with the wrong variant armed, self-corrects in one more
   click instead of becoming a message to interpret. The failure always names
   the role the clicked atom was given, so a click on the wrong atom of an
   asymmetric op explains itself.
4. **Candidates.** One candidate: commit immediately. Several: the popup is
   replaced **in place** — same anchor, same width — by the candidate rows
   ("2 of 2: mirrored, residual 0.00 Å"), every candidate's after state
   ghosted at once, Tab cycles, clicking a ghost atom that belongs to exactly
   one candidate chooses it, Enter takes the highlighted one, Escape abandons.
5. **Commit.** One undo command inserts the step after the cursor with
   metadata copied from the previous authored step (or the last prefix
   step), moves the cursor to it, closes the popup, and enters Armed with the
   committed op so a series of identical placements is one click each.

The sweep runs on a pick, never on hover: it is one `place()` per library
operation (§Applicability), which is cheap once per click and wasteful once
per mouse move.

**Nothing is shown before the click.** No affordance marks which atoms accept
an operation, because that is a sweep per atom over the whole workpiece. The
user asks by clicking. Highlighting every atom an *armed* operation accepts is
the same machinery T2 area-apply needs and is listed there, not here.

Approximate candidates (fallback orientation) carry a warning chip on the
step row, "orientation derived from bonds", so a user knows the coordinates
came from the application, not the library.

### Steps list and metadata

The panel shows the prefix as a read-only, collapsed block ("142 steps from
`build_script`") and the authored block as rows: index, op, method colour,
note. Rows drag to reorder, delete, duplicate; each is one undo command
(duplicate is an `InsertStepCommand` carrying a copy of the row, residual
and flag included).
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
- `mechanosynth_edit_offers(atom_id)` → the applicability list: the
  atom-first entry point. Per row — op name, the library's `note` for it,
  candidate count, best residual, fits/near-miss, mirrored, approximate, and
  **the ghost atoms of the row's first candidate** (a near-miss row's come
  from `near_miss`), so highlighting a row previews it without a second call.
  Also the anchor atom's world position and element, which the popup needs to
  place itself and to write its header. The full candidates stay in transient
  state, so choosing a row needs no second `place()`.
- `mechanosynth_edit_arm(op)`, `mechanosynth_edit_pick(atom_id)` →
  candidate list (indices, residual, mirrored, approximate, ghost atoms for
  rendering) or, on a failure, the message **and** the same offer list;
  `mechanosynth_edit_choose(op, index)` — the op is named as well as the
  index, because an offer list spans several operations, and an op the last
  offer list reported as a **near miss** is refused here, not silently placed
  — `mechanosynth_edit_cancel`.
- `mechanosynth_edit_insert_step`, `mechanosynth_edit_delete_step`,
  `mechanosynth_edit_move_step`, `set_mechanosynth_edit_step_metadata`.
- `mechanosynth_convert_files_to_nodes` on the replayer.

Ghost rendering goes through the same path the guideline tool and guided
placement use for transient viewport overlays; the API returns positions,
elements and kinds, and Flutter never sees the engine's structures.

### Text format

The authored block serialises as an `authored` property holding one record
literal per line, and `cursor` as an int (`steps: gen` below is the wired
prefix pin, not the block):

```
edit = mechanosynth_edit { base: slab, ops: lib, steps: gen, cursor: 2, authored: [
  { op: "habst", t: (12.71, 9.53, 8.02), method: "probe", phase: "layer1", layer: 1, site: 0 },
  { op: "dimerize", t: (14.27, 9.53, 8.02), r: ((0, 1, 0), (-1, 0, 0), (0, 0, 1)), method: "relax", residual: 0.0213 },
] }
```

Each literal has the `BuildStep` fields plus two that belong to the editor
only: `residual: Float` and `approximate: Bool`. Identity `r`, empty/`-1`
metadata, a residual below `EXACT_FIT_RESIDUAL` and `approximate: false`
are omitted on output and defaulted on input, so a short step stays short
and a step typed by hand counts as exact — the author's assertion, the same
standing a generated file's step has. `r` uses the existing 3×3 literal;
an all-integer matrix lexes as `IMat3` and coerces to `Mat3` through the
existing rule. `get_text_properties` must stay total (an empty `authored`
is `[]`), the rule learned on the replayer. The round-trip corpus test
(`project_text_format_roundtrip`) gains a fixture with an editor node.

### Reference guide

- `doc/reference_guide/nodes/atomic.md`: a new `## mechanosynth_edit`
  section (pins, the placement tool — atom-first, the offer popup and its
  preview, why a near miss is shown but cannot be chosen, and the "click the
  atom the operation acts on" rule behind both — candidates, the steps list,
  the text format), new
  `## ops_library`, `## build_script`, `## export_build_script`
  sections, and the `## mechanosynth` section updated for the `ops`/`steps`
  pins, the deprecated file properties and Convert to nodes.
- `doc/reference_guide/nodes/math_programming.md`: the `BuildStep` record
  beside `MechanosynthStep`; `r` added to the latter.
- The two-files section of the `mechanosynth` guide documents `chiral`,
  frame atoms as a pattern-writing convention — including the first-shell rule
  and the `<operation>_<environment>` naming convention for the environment
  variants they imply — the tolerance advice, the effect-derived
  `ms_current` rule, and the origin convention.

## Follow-ups (signatures only)

- **T2 area apply.** On the editor: select N host atoms (marquee, region
  Blueprint or tag) with an op armed → one step per host whose candidate
  list has exactly one entry, in a chosen order (pick order, along a lattice
  direction, nearest-first); hosts with several candidates are listed and
  skipped. The same sweep-over-many-atoms machinery would let an armed
  operation highlight every atom it accepts, the one affordance §Placement
  tool deliberately leaves out of Phase 4.
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

## Testing

Conventions as in the crate `AGENTS.md` files: tests in the owning crate's
`tests/` directory, never inline; fixtures under
`rust/tests/fixtures/mechanosynth/`, synthetic and small; test names are
sentences (`the_result_pin_carries_the_workpiece_at_every_step`). The
per-phase tests are listed under each phase below; this section names what
they share.

**Files.** Engine and schema tests extend
`crystolecule/tests/crystolecule/mechanosynth_test.rs`; the placement engine
gets `mechanosynth_place_test.rs` beside it. Node tests extend
`structure-designer/tests/structure_designer/mechanosynth_test.rs` and add
`mechanosynth_edit_test.rs` and `mechanosynth_edit_placement_test.rs`; API
tests extend `rust/tests/structure_designer_api/mechanosynth_api_test.rs`.
(Phase 3 put the placement flow on `StructureDesigner` rather than in `api/`,
so its tests sit beside the node's rather than in the api harness — a test's
home is decided by what it imports.) The existing helpers in those files
(`methane()`, `methylate_expected(step)`, `assert_same`, `expect_error`,
`add_value_node`) are reused, not duplicated.

**Fixtures.** `place_ops.json` — a synthetic library written for the
placement tests: a one-atom abstraction, a donation with three `*` frame
atoms on a tetrahedral host, the same donation in a second environment
variant, a two-atom `dimerize`, an asymmetric two-Si op (adds to atom 1
only), a planar three-atom op, a planar four-atom op with an out-of-plane
`after` (once plain, once `chiral`), a one-atom donation without frame
atoms, and an op whose atoms all sit off the origin. `place_workpiece.xyz`
— a small tetrahedral cluster the ops fit exactly, with two bare Si
neighbours around one site and one around another. `gold_build.json` — a
build written by hand against `place_ops.json`/`place_workpiece.xyz` with
known `(r, t)`, the in-repo stand-in for the generator's output.

**Shared assertions.**

- *Every candidate replays.* For any candidate `place()` returns,
  `apply_step` with the candidate's step and the library tolerance succeeds
  on the same workpiece, and the atoms it touches are exactly the workpiece
  atoms in `candidate.roles`. This is the contract between the two halves of
  the engine and every placement test asserts it on every candidate it
  looks at.
- *Same result, both nodes.* A `mechanosynth_edit` at cursor `k` equals a
  `mechanosynth` fed `prefix ++ authored[..k]` on a wire, atom for atom to
  1e-6 Å with the same tags. Every editor eval test is stated as an
  equality against the replayer, never against hand-written coordinates.
- *Undo restores the tuple.* For every editor command, undo restores
  `(authored, cursor)` to the pre-command value and redo re-applies it,
  compared structurally. One helper, used by every command test.

**Cross-cutting regressions,** run at the end of every phase:

- Every `.cnnd` under `rust/tests/fixtures/` that carries a `mechanosynth`
  node evaluates to the same atoms before and after the phase, through the
  `node_snapshots` suite. **No such file exists today** — the demo projects
  live outside the repository, in the maintainer's `mechanosynth/` folder —
  so Phase 1 starts by adding one (`mechanosynth_legacy.cnnd`, a
  `mechanosynth` with `ops_file`/`build_file` pointing at
  `methylate_ops.json`/`methylate_build.json`) and snapshotting it before
  any other change. The demo projects are the human's regression, checked
  at the end of P1 and P5.
- The text-format round-trip corpus (`text_format_roundtrip_corpus_test.rs`)
  stays a no-op on every file, with the new fixtures added as they appear.
- The manual walkthrough at the end of P4 and P5 is the human's; agents run
  the Rust suite and `flutter analyze` and list the walkthrough as pending.

## Phases

### Phase 1 — Values and loaders — **DONE**

Two deviations from the plan below, both recorded where they bite:

- **Unknown op is an evaluation error, not a validation error.** The steps are a
  runtime array on a wire; the validator sees stored node data, so there is
  nothing for a rule to inspect. `replay` calls `validate_script_ops` before
  applying anything, so the message still names the step index and the operation
  and the replay still does not run — it just arrives on the pin rather than as
  a badge.
- **`ops_library` and `build_script` each gained a Reload button**, not only
  `ops_library`. The staleness is identical and the affordance is two lines.

Matcher primitive extraction; schema extensions (`chiral`, effect-derived
`touched`, origin-convention warning, `DEFAULT_TOLERANCE` 0.05); the
`OpLibrary` type and result; the `BuildStep` record and `r` on
`MechanosynthStep`; the `BuildStep` ⇄ `Step` conversion; `ops_library`,
`build_script` and `export_build_script` nodes; `mechanosynth` pins, legacy
fallback and Convert to nodes; FRB regenerated; the first guide sections.
First commit of the phase: the `mechanosynth_legacy.cnnd` fixture and its
snapshot (§Testing), so the legacy path has a regression before it is
touched. Baseline at the end: that snapshot unchanged, and the demo projects
(human) replay unchanged before and after Convert to nodes.

*Tests — matcher primitive:* nearest unclaimed atom within tolerance is
returned; an already-claimed atom is skipped in favour of the next nearest;
the element filter rejects a nearer atom of the wrong element; two atoms at
equal distance resolve by lower id, so results are deterministic; nothing
within tolerance → `None` with the nearest distance available for the
diagnostic. The three existing callers are covered by the existing suite,
which must pass unchanged after the extraction — no test is edited in this
step.

*Tests — schema and engine:* `touched` derived from effect — a kept atom
with no change and no bond change is not touched; a kept atom with an
added, deleted or re-ordered bond is; a kept atom whose bonded partner was
deleted is; an element set to its current value is not a change, an element
set to a different one is; the two existing kept-atom assertions rewritten
to name the clause that makes them true. Frame atoms are recognised from
the two patterns alone (kept, same position, same element, no bond change)
and a kept atom with a bond change is not a frame atom. `chiral` parses,
defaults to false, and replay ignores it. The default tolerance is 0.05:
every engine test that relied on the old default through a fixture without
a `tolerance` key (only `valid_ops.json`/`valid_build.json` state one) is
checked one by one and either keeps passing at 0.05 or states 0.3 in the
fixture where the test is about slack, never by loosening the default. The
build file's `tolerance` no longer overrides the library's — a build stating
`0.3` against a library stating `0.05` replays at `0.05`. Origin convention:
a library whose `before` atom 1 is off the origin loads with a warning
naming the op, and a library with no atom 1 at all loads with a warning,
not an error. A file with every new key loads and replays identically on
the old code path (the key set is additive).

*Tests — types and records:* `OpLibrary` is a distinct `DataType`: an
`ops_library` output wires to an `OpLibrary` pin and is rejected by a
`HasAtoms` pin and by `expr`; `record_destructure` on a `BuildStep` yields
the eight fields with the documented defaults for a step parsed from JSON
that states only `op` and `t`; `BuildStep → Step → BuildStep` is the
identity, including an identity `r`; `MechanosynthStep.r` equals the
replayed step's rotation and is the identity when the file has none (the
existing `step` pin tests gain the assertion rather than a new test).

*Tests — loader nodes:* `ops_library` and `build_script` load a fixture,
store the path relative to the project on save, and re-parse on load (the
`import_cif` pattern's tests, applied to both); the `file` pin overrides the
property and is not cached into it (the existing "wired file names override
the stored ones without being cached" test, ported); a parse failure is an
evaluation error naming the file and the location; a missing file is an
error naming the file; `build_script` passes an unknown op through
unchanged and the consumer reports it. `export_build_script`: the JSON it
writes re-parses with `parse_build_script` to the same steps; an identity
`r` is omitted and a non-identity one written; empty `note`/`method`/`phase`
and `-1` `layer`/`site` are omitted; `format` is set; an evaluation with
`execute == false` writes nothing and an execute run writes once (the
`execute_node_test.rs` pattern).

*Tests — `mechanosynth` with wires:* wired `ops`/`steps` equals the
file-driven replay atom for atom, tags included, at every step; an unknown
op in the array is a validation error naming the step index and op, and
the replay does not run; `steps` unwired with no `build_file` emits the
base and no error; a wired pin wins over the legacy property and the panel
info reports the wired source; every existing `.cnnd` with `ops_file`/
`build_file` loads and evaluates identically (the existing tests are the
regression). Convert to nodes: creates one `ops_library` and one
`build_script`, wires them, clears both properties, and the result is
atom-identical before and after; with only one property set it creates one
node; undo restores the properties and removes the nodes, redo re-applies;
the command is a single undo entry.

*Tests — text format and snapshots:* `ops_library`, `build_script` and
`export_build_script` statements round-trip through `query` → `--replace`;
a `mechanosynth` with legacy properties still round-trips; the node
registry snapshot gains the three node types; a fixture `.cnnd` with the
wired form joins the `node_snapshots` set.

### Phase 2 — Placement engine — **DONE**

`place.rs` and its tests. No UI, no node. Seven deviations from the plan below,
each recorded where it bites:

- **A fourth error variant, `NoSuchAtom { atom_id }`.** A stale atom id has to
  be an error rather than a panic, and none of the three planned variants says
  that. `UnknownOp` additionally carries the library's file label, so the
  message names the library that lacks the operation.
- **The shared *every candidate replays* assertion is stated against the
  replay's match map, not `StepEffect::touched`.** The plan said "the atoms it
  touches are exactly the workpiece atoms in `roles`"; since P1 made `touched`
  effect-derived that is simply false — a frame atom is in `roles` and is not
  touched, and a deletion's bonded neighbour is touched and is not in `roles`.
  The invariant that was meant is that `roles` is what the replay will match,
  and that is what the test asserts.
- **Ranking compares residuals in 1e-6 Å buckets** (`RESIDUAL_RANK_EPSILON`).
  With a raw comparison, two exact fits at 1e-16 and 3e-16 order by whichever
  the arithmetic happened to favour and "proper before mirrored" never gets a
  say — so `land4`'s mirrored candidate could rank first. A residual difference
  below the file rounding is not a ranking signal.
- **Rank-deficient fits are resolved explicitly, and offer no mirrored
  candidate.** A one-atom pattern fits with the identity, a collinear pair with
  the shortest arc between the two axes. Handing either to the eigen solver
  would answer an undetermined question with whichever vector its sweeps
  produced, and an improper fit of a point or a line differs from the proper one
  only in the part that was undetermined anyway.
- **The bond-derived fallback replaces the one-atom fit rather than joining
  it.** The plan's step 4 would have produced a residual-zero candidate with
  `r = identity` — an orientation the library never stated — which would then
  rank first and be committed by the one-click rule.
- **The fallback declines a continuum.** A bare atom (free sphere) and a single
  bond with no dihedral reference (free ring) offer no *set* of directions, so
  they yield `Err(NoPlacement)` saying no orientation can be derived, rather
  than an arbitrary sample of a ring dressed up as candidates. It dispatches on
  the detected hybridization through `guided_placement`, which was already in
  `crystolecule`, so nothing had to move down.
- **The real-library round-trip test is not written**, as planned: its fixtures
  do not exist in the repository yet. See the last test heading below.

*Tests — role rule:* clicking the H of an abstraction (origin atom is C, so
H admits only role 2) yields the same step as clicking the C; on the
asymmetric two-Si op, clicking the partner makes the clicked atom the
origin role, so the added atom lands on the clicked atom and not on its
neighbour; a Si click on the donation with `*` frame roles is always role
1; the op with no atom at the origin falls to the smallest eligible id; a
fit that fails in the assigned role is `Err(NoPlacement)` naming the role
rather than a retry in another role; `candidate.role` is the same on every
candidate of one call.

*Tests — fit:* one-atom abstraction → one exact candidate with `r =
identity`, `t =` the clicked position; donation with three frame atoms on a
tetrahedral host → one candidate, `r` recovered to 1e-9 from a host rotated
by a random proper rotation and translated, `mirrored = false`; the same
under a random improper transform → one candidate, `mirrored = true`,
dropped when the op is `chiral`; the two environment variants of the
donation → the wrong one rejected at the 0.05 Å default and
accepted-but-inexact (residual reported, `exact = false`) when the library
states 0.3; the gate is the **max** per-atom residual, not the RMS — a fit
with one atom off by 1.5·tolerance and the rest exact is rejected; two-atom
`dimerize` with two bare neighbours → two candidates, two different after
states; with one bare neighbour → one; planar three-atom op → one candidate
(the mirror fit yields the same after state and is deduped); planar
four-atom op with out-of-plane `after` → two candidates, one mirrored, one
with `chiral`.

*Tests — search bounds:* a partner at `extent + tolerance − ε` is found and
one at `extent + tolerance + ε` is not; no two roles are assigned the same
workpiece atom; `place_with_stats`, a test-facing variant that also
returns the number of partial assignments visited, stays under a fixed
bound on the largest fixture op against a 200-atom neighbourhood, so a
pruning regression shows up as a number, not a slow test.

*Tests — fallback:* the one-atom donation without frame atoms and an
off-origin `after` → one candidate per free direction, each `approximate`,
each `r` proper with `+z` along the direction; the same op with frame atoms
added produces no approximate candidate; a one-atom op whose `after` adds
nothing off the origin produces no fallback (nothing to orient).

*Tests — dedupe and rank:* two transforms that produce the same after state
collapse to one candidate and the survivor is the proper one; candidates
sort by residual, then proper before mirrored, then exact before
approximate; the order is stable across two calls on the same input.

*Tests — diagnostics:* `Ok` never holds an empty list; no fit →
`Err(NoPlacement)` naming the op, the role, and the nearest atom of the
missing element with its distance (the "nearest bare Si at 4.59 Å" message
in §Placement tool is built from this); an op not in the library →
`Err(UnknownOp)`; an inadmissible element → `Err(NoRole)` listing the
accepted elements.

*Tests — every candidate replays:* the shared assertion, run over every op
in `place_ops.json` against `place_workpiece.xyz` at every atom that admits
a click.

*Tests — round-trip exactness, in-repo:* drive `place()` with each step of
`gold_build.json` in order on a workpiece built by the same engine, choose
the candidate whose `r` matches the gold `r`, assert every residual
`< 1e-4 Å`, and assert the final structure equals the gold replay atom for
atom to 1e-6 Å. This is the test that makes §Exactness a checked
requirement, and it does not wait on the generator.

*Tests — round-trip exactness, real library (not written; P2's one deferred
item):* the same test against a copy
of the public diamond library and one of its generated builds, checked into
`rust/tests/fixtures/mechanosynth/` once the generator emits frame atoms
and `tolerance: 0.05`. Those files live outside the repository today, so
this test is not written in Phase 2; adding the fixtures and the test is the
first task of the generator work, and its passing is that work's acceptance
check.

### Phase 3 — The editor node — **DONE**

Node, data, eval, undo commands, API, text format, round-trip fixture, plus
`applicable_ops` / `NEAR_MISS_FACTOR` in `place.rs` and the placement flow over
them. Seven deviations from the plan below, each recorded where it bites:

- **There is no prefix snapshot, and the design's cache key does not exist.**
  The plan said the snapshot is "kept in transient data keyed on the input's
  `env_epoch`"; `env_epoch` numbers *HOF body invocations*, not input changes, so
  it cannot answer "has the base changed". A correct key would be a structural
  fingerprint of the base — an O(n) walk of the very structure the cache exists
  to avoid touching — and what it would save is the prefix's per-step matching,
  which for a few hundred steps is far below the `base.clone()` `replay` does
  anyway. `feedback_avoid_speculative_caching` applies: `eval` replays prefix
  then block, every time. The eval-profiler test that was to measure the cache is
  not written for the same reason.
- **`eval` runs two replays rather than one concatenated script**, and that is
  load-bearing rather than incidental: `replay` paints `current` on the *last
  applied step*, so one call would light up the last **prefix** step whenever the
  cursor sits at 0 — exactly the state that must show no highlight at all.
- **An unknown op in the authored block is an evaluation error, not a validation
  error** — the same deviation Phase 1 recorded for the wired case, for the
  narrower reason that the validator cannot see the wired *library*. `replay`
  still calls `validate_script_ops` first, so the message names the step index
  and the operation and nothing is applied.
- **One undo command with four constructors**, not four command types
  (`undo/commands/mechanosynth_edit_block.rs`). All four edits change the same
  `(authored, cursor)` pair and all four restore it wholesale; four structs with
  identical fields and identical `undo` bodies would say nothing the
  `description` does not. Coalescing needed two small additions to `UndoStack`
  — `push_count()` and `pop_last()` — documented in `undo/AGENTS.md`.
- **`MechanosynthEditData::Default` is hand-written.** `#[serde(default = …)]`
  covers deserialization only, so a derived `Default` gave a fresh node
  `cursor: 0` — a node that shows its prefix and none of its block, for no reason
  a user could see. Caught by the placement tests, which is what they are for.
- **A fourth ghost kind, `Changed`**, for a kept atom whose element the step
  rewrites. The plan named added / deleted / moved; an element-swap operation
  touches none of those, so its preview would have been empty, which reads as
  "this would do nothing".
- **`Operation` gained a `note`**, parsed and otherwise inert. The offer popup is
  specified to show "the library's own note", and the parser was discarding the
  key. It also feeds `ops_library`'s panel listing.

Two smaller shape choices worth knowing. The placement flow lives on
`StructureDesigner` (`mechanosynth_edit_ops.rs`), not in `api/`, for the reason
`ai_text_edit` does — so its tests need no `CAD_INSTANCE`; the api file is
wrappers plus the transport shapes, and the *ghosts* are built down in the ops
module where the workpiece is already in hand, so no `AtomicStructure` crosses
the boundary. And the API's insert entry point is
`mechanosynth_edit_duplicate_step`, not a general `insert_step`: a step that was
never fitted against a workpiece has no `(r, t)` to write, so every other
insertion comes from a placement.

The corpus fixture is a **pinned case in
`text_format_roundtrip_corpus_test.rs`** rather than a node added to
`demolib/baselib_with_demos.cnnd` — the shared demo library is a user-facing
file and this design has no demo to put in it yet. `mechanosynth_edit.cnnd`
joins the `node_snapshots` set instead, and its atoms match the legacy and wired
mechanosynth fixtures' exactly.


*Tests — eval:* the shared assertion *same result, both nodes* at cursor
`0`, `1`, `authored.len()` and `-1`, with and without a wired prefix; a
cursor beyond the authored length clamps as the replayer's slider does; the
`steps` output is `prefix ++ authored` whatever the cursor; `result` keeps
the input's variant (`Molecule` in, `Molecule` out; the existing replayer
test, ported); `ms_current` marks exactly the cursor step's touched atoms,
nothing at cursor `0`, and no `ms_added`/`ms_layer`; an authored step whose
`before` no longer matches (a step reordered ahead of its prerequisite) is
an evaluation error naming the step index and the engine's message, and
the transient last-error field carries it; an unknown op in the authored
block is a validation error naming the step.

*Tests — prefix snapshot:* changing the `base` upstream (a new `env_epoch`)
changes `result` on the next eval — the snapshot is never stale; changing
the `steps` prefix likewise; a cursor move or an authored insert does not
replay the prefix, asserted through the eval profiler's per-node counters
on the upstream `build_script` node (one eval of the prefix across ten
cursor moves); the snapshot is `#[serde(skip)]` and a saved file carries
none of it.

*Tests — commands:* `InsertStepCommand`, `DeleteStepCommand`,
`MoveStepCommand`, `SetStepMetadataCommand` each pass the shared *undo
restores the tuple* assertion; insert after the cursor moves the cursor to
the new step and undo restores the old cursor; delete of the cursor step
moves the cursor to the previous step; move preserves the step's own
residual and `approximate` flag; consecutive metadata edits to the same
step and field coalesce into one undo entry while edits to a different
field do not; cursor changes are **not** undo entries (setting the cursor
ten times adds nothing to the stack); every API mutator except the cursor
setter adds exactly one undo entry (the `feedback_persisted_mutations_must_be_undoable`
rule, checked mechanically).

*Tests — applicability* (in `mechanosynth_place_test.rs`, beside
`place()`'s, since the function lives in `place.rs`): on the donation with
three frame atoms, a click on
its host lists that operation as fitting and the second environment variant as
a near miss with its residual; at the 0.05 Å default no two variants of one
family are ever both applicable on the same atom, which is what makes the
offer list an answer rather than a menu; a click on an atom no operation
accepts returns an empty list, not an error; each entry's candidates equal
what `place()` returns for that operation and atom at the library tolerance,
so the sweep and the single call cannot disagree; an operation whose only
candidates are over the gate is a near miss, never applicable, and carries its
best rejected fit in `near_miss` while `candidates` stays empty — and the
converse for an applicable one, so no entry is ever both; the result
holds at most one entry per library operation and none for an operation that
errored; the near-miss gate is `tolerance * NEAR_MISS_FACTOR` — a fit at
`10·tolerance − ε` is reported and one at `10·tolerance + ε` is not; the list
sorts applicable before near miss, then by residual, and the order is stable
across two calls on the same input.

*Tests — placement API:* the arm → pick → choose sequence inserts one step
and one undo entry; the atom-first offers → choose sequence inserts the same
step and one undo entry; `mechanosynth_edit_choose` names the operation as
well as the index, so an offer list spanning several operations cannot be
chosen from ambiguously, and a `(op, index)` pair the last offer list does not
contain is an error that inserts nothing; **choosing an op the last offer list
reported as a near miss is an error naming the residual and inserts nothing**,
so an over-gate fit cannot reach the authored block through the API any more
than through the popup; every offer row carries the ghost atoms of its first
candidate, a near-miss row's taken from `near_miss`, so the panel can preview
a row without a second call; a pick that fails while Armed
returns the failure message *and* the offers for that atom; pick with a single
candidate commits immediately and
reports so; cancel inserts nothing and returns to Idle; pick while Idle is
an error; arm with an op not in the wired library is an error naming the
op; choose with an out-of-range index is an error and inserts nothing; pick
on an atom id not present in `result` is an error; offers on an atom id not
present in `result` is an error; after a commit the tool
is Armed with the same op; the inserted step copies `method`/`phase`/
`layer`/`site` from the previous authored step, or from the last prefix
step when the block is empty, and `note` from neither; the candidate list
returned by pick carries residual, mirrored, approximate and the ghost
atoms (positions, elements, added/deleted/moved kinds) for each candidate,
and an approximate candidate's inserted step is flagged `approximate` in
the stored block.

*Tests — persistence and copy:* `authored` with residuals and flags
round-trips through `.cnnd` and evaluates identically after reload;
copy/paste and duplicate-network carry the authored block and cursor and
none of the transient state; a body-scoped editor (inside a `map`) is
reachable by `scope_path` and a colliding root id is not confused (the
existing replayer API test, ported).

*Tests — text format:* an editor node round-trips through `query` →
`--replace` with byte-identical output; identity `r`, empty/`-1`
metadata, an exact residual and `approximate: false` are omitted on output
and defaulted on input, while an inexact residual and `approximate: true`
survive the round trip; `authored: []`
round-trips (the totality rule on `get_text_properties`); a text edit that
sets `authored` validates, refreshes, marks dirty and is one undo entry; a
step literal with an unknown field is a parse error naming the field; the
corpus test gains a fixture with an editor node and a wired prefix.

### Phase 4 — Panel and tool — **DONE**

Six deviations from the plan below, each recorded where it bites:

- **Ghost previews are a projected 2D overlay, not a decorator visual.** The
  plan said "the same path the guideline tool and guided placement use for
  transient viewport overlays"; that path puts visuals on
  `AtomicStructureDecorator` and tessellates them during `eval(decorate: true)`,
  which means a full evaluation and re-tessellation of the workpiece **per
  redraw** — here, on every arrow key, to move a dozen spheres. The API shape
  this design already specified hands Flutter the ghost atoms (`APIGhostAtom`:
  position, element, kind), which is exactly what a projected overlay needs and
  nothing the renderer needs, so they are drawn with a `CustomPainter`, the
  `AddBondPreviewPainter` shape. The cost is that a ghost is a flat disc rather
  than a lit sphere and is not depth-tested against the workpiece; the benefit
  is that §Placement tool's "this is free" is true.
- **The popup is mounted in the viewport's own `Stack`, not a global
  `Overlay` entry.** A global entry positions in screen coordinates and would
  have to re-derive the viewport rect to clamp against it; the viewport's own
  `LayoutBuilder` hands over both the origin and the constraints. Same result,
  less machinery.
- **`renderingNeeded()` had to be overridden to re-anchor.** The plan budgeted
  the re-anchor work without naming the trap: `renderingNeeded()` schedules a
  *frame*, which repaints the 3D texture but does **not** rebuild the widget
  tree — and an overlay laid out from a projected world position only moves when
  `build` runs again. The override schedules one guarded post-frame `setState`
  while a popup or a ghost is on screen.
- **Three kernel entry points were missing and were added here.**
  `mechanosynth_edit_anchor_at_ray` turns a viewport ray into an atom (scoped to
  the editor's *own* displayed outputs, so an atom of an unrelated structure in
  front is not reported), and carries the atom's position and element because
  the popup is anchored to it and the candidate-list path has no sweep to take
  them from. `mechanosynth_edit_open_offer` opens one offer row into its
  candidate list — the plan says "`place` is not run a second time", and Phase
  3's API had no entry point for that: `choose` commits and `pick` re-fits, and
  re-picking would additionally *arm* the operation, which only a commit may do.
  And `mechanosynth_edit_tool_status` reads the tool's state and armed operation
  with **no evaluation**: the viewport is rebuilt on every pointer move, and
  `get_mechanosynth_edit_data` evaluates two input pins to report the prefix
  length and the library's operation names. The viewport reads it on every frame
  for a second reason too — the kernel is the authority on whether a query is
  still live, so a cursor move or an undo from the panel drops the popup instead
  of leaving it offering candidates fitted against a workpiece the node no
  longer shows.
- **Offers and pick are throwing calls on the Dart side.** They return
  `Result<_, String>`, which flutter_rust_bridge turns into a thrown
  `AnyhowException`; a viewport click is not a place to let one escape, so the
  model wraps them in a `MechanosynthToolResult` the caller branches on. Note
  what is *not* on that channel: an armed operation that does not fit is a
  successful pick carrying the failure and the offers, which is what makes the
  self-correcting second click work.
- **The extracted scrubber owns the drag preview, so it reports it back.** Both
  panels suppress a readout they cannot recompute mid-drag (the dragged step's
  op name lives in the kernel's parsed script); with the preview moved inside
  the shared widget they would have shown the step the slider had just left.
  One `onPreview` callback restores it.

One smaller shape choice worth knowing: the cursor scrubber's travel is the
**authored block alone**, not prefix + block. The cursor can only sit inside the
block, so including a 142-step prefix would make most of the slider dead.

Scrubber and chapter list extracted from `mechanosynth_editor.dart` into a
shared widget; palette, prompt, steps list, chips; the **offer popup** — a
viewport `Overlay` anchored to the clicked atom, with its dimmed unselectable
near-miss section, preview-on-highlight, prefix filtering and keyboard model
(§Placement tool) — the candidate list in the same popup, ghosts, the armed
strip; the placement tool wired into viewport picking, with the applicability
sweep on a pick and never on hover.

The popup is the one genuinely new widget: atomCAD has transient viewport
overlays, but none that is interactive and pinned to a projected 3D point, so
budget the re-anchor-on-camera-change and viewport-clamping work rather than
assuming an existing affordance covers it.

*Tests:* two widget tests (`test/mechanosynth_scrubber_test.dart`,
`test/mechanosynth_offer_popup_test.dart`, both written). The extracted
scrubber — it builds from the
replayer's info and from the editor's, reports cursor changes through the
callback, and the chapter list jumps. The offer popup, from a canned
applicability list, because its rules are decidable without a viewport:
applicable rows above the rule and near-miss rows below it; a near-miss row
does not report a choice through the callback and shows its reason instead;
Up/Down move the highlight and each move reports the previewed row exactly
once; typing filters by prefix without re-querying; Escape reports a cancel;
an empty list renders the "nothing applies" header and no rows. Plus
`flutter analyze` clean of new warnings.

Three Rust tests joined `mechanosynth_edit_placement_test.rs` for
`mechanosynth_edit_open_offer`, the entry point this phase added: opening an
applicable row hands over the candidates the sweep already found, leaves the
tool in Candidates **without arming**, inserts nothing and records no undo
entry; opening a near miss is refused with its residual; opening an operation
the offer list does not hold is an error. Nothing else automated: the rest is
thin editor UI and the rule
from `feedback_manual_test_for_editor_ui` applies. The **manual
walkthrough** (human) is: palette filter and type-to-select; a click on an
atom with nothing armed, and choosing a variant from the offer list it
produces; arrowing that list and watching each ghost preview follow the
highlight; the popup staying anchored to its atom while the camera orbits, and
clamping at the viewport edge; a click on a host the library has no variant
for, the near-miss residual it reports, and the refusal when it is chosen; the
wrong variant armed, and the offers that follow the
failure; click-to-place
on a diamond library build with an exact residual; a two-candidate pick
with ghosts, Tab, click-a-ghost and Enter; an approximate placement and its
chip; a failed pick and its message naming the role; reorder, delete and
duplicate in the list with undo after each; chip edits; arrow-key scrub and
its non-undoability; Convert to nodes on the demo project (outside the
repository). The Flutter smoke test is not run by agents.

### Phase 5 — Guide and walkthrough — **DONE**

Remaining guide pages, screenshot slot, the manual checklist.

*Tests:* none automated beyond the cross-cutting regressions; the guide's
text-format examples are pasted through `edit` once by the human to confirm
they parse.

**Delivered.** `doc/reference_guide/nodes/atomic.md` carries the
`mechanosynth_edit`, `ops_library`, `build_script` and `export_build_script`
sections, the updated `mechanosynth` one, and the two-files section now states
the frame-atom conventions the placement tool depends on — the **first-shell
rule** and the `<operation>_<environment>` naming with the silicon library's
five-name vocabulary. `math_programming.md` carries `BuildStep` beside
`MechanosynthStep`. Two screenshot slots are marked `TODO(image)`, in the
node's opening and in §The offer popup.

#### The manual walkthrough

The human's, not an agent's (`rust/AGENTS.md`: agents must not run
`flutter test integration_test/`). The bench is the `edit_sandbox` network in
the silicon demo project — slab → `ops_library` / `build_script` →
`mechanosynth` at `start_step` → `mechanosynth_edit` — which is deliberately
small enough that a failure is attributable.

1. **Click a bare support Si of a stripped footprint.** The popup opens beside
   the atom, the atom is ringed in orange, and the header counts the operations
   that fit. `si_donate_dimer` is among them; `si_donate_site` and the other
   environment variants are below the rule with how far off they are.
2. **Rest on a row.** After a beat its ghosts appear *in the scene* — depth
   tested, hidden behind the atoms in front of them. Crossing several rows
   quickly previews none of them.
3. **Hover `bridge`.** Its preview is a single green stick and no ghost atoms:
   a bond-only operation. Nothing about that reads as a failure.
4. **Click a near miss.** It previews in amber and the row is replaced by the
   reason; nothing is placed.
5. **Click an applicable row.** The step lands at the cursor, the popup closes,
   and the tool returns to Idle — the *next* click is another question, not a
   repeat.
6. **An operation with two placements** shows one header and two indented rows
   with direction arrows that re-aim as the camera orbits. Each places its own
   orientation.
7. **Drag the popup by its header**, orbit, and confirm it still tracks its
   atom from where it was parked; the ⌖ button snaps it back.
8. **Scrub the cursor**, undo, redo: the block and the cursor come back
   together, and the tool drops any open list.
9. **Wire the `steps` output into `export_build_script`** and execute it; the
   file replays through `mechanosynth` to the same structure.
10. **Round-trip the project**: save, reload, and confirm the authored block,
    its residual chips and the cursor survive — then paste the guide's
    text-format example through `atomcad-cli edit` to confirm it parses.

Then the P1/P5 regression the design asks for: the demo projects in the
maintainer's `mechanosynth/` folder still evaluate to the same structures.

Outside the repository, a prerequisite for one-click donations on the real
libraries: the generator emits frame atoms and environment variants for
its donation and dimerization ops — first shell only, named
`<operation>_<environment>` from a vocabulary shared across operations
(§Decisions) — and writes `tolerance: 0.05`. The
diamond output stays the regression reference for everything else, and the
real-library exactness test (P2) is the acceptance check for that work.

## Considered and rejected

- Global op registry, `BuildScript` product type, ops in steps, ops on the
  workpiece — see §Decisions.
- Editing inside `mechanosynth`; an edit list over the wired prefix — see
  §Decisions.
- Role candidates in the UI when the clicked atom fits several `before`
  atoms, and falling through to another role on a failed fit — see
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
  a tight library tolerance make the wrong variant fail visibly instead. The
  atom-first offer list (§Applicability) reaches the same end without either:
  it shows the variant that *fits* rather than the variant that fits *best*,
  so the gate does the resolving, no key is added, and the user learns the
  distinction by seeing which variant is offered where — a better teacher than
  a grouped palette would have been.
- Merging environment variants back into one tolerant operation to keep the
  palette short — see the last subsection of §Decisions.
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
- Whether `NEAR_MISS_FACTOR` should become a per-library value. A constant 10
  is right for a generated library at 0.05 Å; a hand-written library that
  states 0.3 Å would report misses out to 3 Å, which is more noise than
  report. Left a constant until a library complains.
- Whether the offer list should also show operations that fit an atom in a
  role the user plainly did not mean — the `*` frame role of some unrelated
  operation. The role rule picks one role per operation and the fit usually
  rejects it, so this may never arise; if it does, the fix is a rank, not a
  filter.
