# Design: build scripts as network values and the `mechanosynth_edit` node

Status: **draft 2026-09-11, revised 2026-09-14** (clicked-atom role rule,
tests moved into phases). **Phase 1 implemented 2026-09-14**; Phases 2–5 not
started. Extends the `mechanosynth` subsystem
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
one (the same rule `atom_edit` uses to own viewport picks). States:

1. **Idle.** The palette lists the wired library's operations, with a
   filter box. Choosing one enters Armed. Typing selects by prefix.
2. **Armed.** The prompt reads "click the *host* atom" (the origin atom's
   element and, when present, the op `note`). A viewport click on an atom
   calls `place`; a click elsewhere or Escape returns to Idle.
3. **Candidates.** An `Err` from `place` (`NoRole`, `NoPlacement`) is
   shown in place ("no `dimerize` partner within tolerance of the clicked
   atom, taken as pattern atom 1; nearest bare Si at 4.59 Å") and the tool
   stays Armed. The message always names the role the clicked atom was
   given, so a click on the wrong atom of an asymmetric op explains itself.
   One candidate: commit immediately. Several: every candidate's after
   state is drawn as ghosts at once (added atoms
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
- `mechanosynth_edit_arm(op)`, `mechanosynth_edit_pick(atom_id)` →
  candidate list (indices, residual, mirrored, approximate, ghost atoms for
  rendering), `mechanosynth_edit_choose(index)`, `mechanosynth_edit_cancel`.
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
  section (pins, the placement tool and its "click the atom the operation
  acts on" rule, candidates, the steps list, the text format), new `## ops_library`, `## build_script`, `## export_build_script`
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
`mechanosynth_edit_test.rs` and `mechanosynth_text_format_test.rs`; API
tests extend `rust/tests/structure_designer_api/mechanosynth_api_test.rs`
and add `mechanosynth_edit_api_test.rs`. The existing helpers in those files
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

### Phase 2 — Placement engine

`place.rs` and its tests. No UI, no node.

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

*Tests — round-trip exactness, real library:* the same test against a copy
of the public diamond library and one of its generated builds, checked into
`rust/tests/fixtures/mechanosynth/` once the generator emits frame atoms
and `tolerance: 0.05`. Those files live outside the repository today, so
this test is not written in Phase 2; adding the fixtures and the test is the
first task of the generator work, and its passing is that work's acceptance
check.

### Phase 3 — The editor node

Node, data, eval with the shared helper and prefix snapshot, undo commands,
API, text format, round-trip fixture.

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

*Tests — placement API:* the arm → pick → choose sequence inserts one step
and one undo entry; pick with a single candidate commits immediately and
reports so; cancel inserts nothing and returns to Idle; pick while Idle is
an error; arm with an op not in the wired library is an error naming the
op; choose with an out-of-range index is an error and inserts nothing; pick
on an atom id not present in `result` is an error; after a commit the tool
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

### Phase 4 — Panel and tool

Scrubber and chapter list extracted from `mechanosynth_editor.dart` into a
shared widget; palette, prompt, candidate list, ghosts, steps list, chips;
the placement tool wired into viewport picking.

*Tests:* a widget test for the extracted scrubber — it builds from the
replayer's info and from the editor's, reports cursor changes through the
callback, and the chapter list jumps; `flutter analyze` clean of new
warnings. Nothing else automated: the panel is thin editor UI and the rule
from `feedback_manual_test_for_editor_ui` applies. The **manual
walkthrough** (human) is: palette filter and type-to-select; click-to-place
on a diamond library build with an exact residual; a two-candidate pick
with ghosts, Tab, click-a-ghost and Enter; an approximate placement and its
chip; a failed pick and its message naming the role; reorder, delete and
duplicate in the list with undo after each; chip edits; arrow-key scrub and
its non-undoability; Convert to nodes on the demo project (outside the
repository). The Flutter smoke test is not run by agents.

### Phase 5 — Guide and walkthrough

Remaining guide pages, screenshot slot, the manual checklist.

*Tests:* none automated beyond the cross-cutting regressions; the guide's
text-format examples are pasted through `edit` once by the human to confirm
they parse.

Outside the repository, a prerequisite for one-click donations on the real
libraries: the generator emits frame atoms and environment variants for
its donation and dimerization ops, and writes `tolerance: 0.05`. The
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
