# Design: tool molecules in `mechanosynth`

Status: **draft 2026-09-14, revised 2026-09-15 (twice); Phase 1 (schema and
engine), Phase 2 (nodes, records, API) and Phase 3 (panel) implemented
2026-09-15, and the document corrected against all three. Phase 4 (guide and
walkthrough) open, and the manual walkthrough in §Phase 3 is still pending.**
Building Phase 1 falsified two things this document asserted, and both are
rewritten where they are stated rather than noted here: **a pattern cannot
assert an atom's absence**, so a tool wired carrying cargo an operation's tool
side does not name is not caught geometrically at all (§Symbolic state is a
label); and **a step has to be all or nothing**, both sides matched before the
first atom moves, or a tool-side failure leaves the target side's rewrite behind
and the partial-result form hands back a half-applied step (§The scene).
Building Phase 2 falsified two more, likewise rewritten in place: **the
editor's two passes need one scene**, which `replay_scene` cannot express, so
the engine exposes `build_scene` and `replay_steps` beside it (§The scene); and
**the last-good-state override is not a new hook** but the per-pin display
override `EvalOutput` already carries, while what the node stores is the whole
`Scene` rather than a structure (§mechanosynth_edit). Building Phase 3 falsified
one more: **there is no armed strip and no armed repeat flow** — that mode was
removed from the editor before this document was written — so a spent tool is
discovered on the *Tools* readout or on a blocked offer row, never on a failing
repeat pick (§mechanosynth_edit, §Tool status).
The first revision made three things structural: operations declare their
method; one operation is one instrument; tools are identified by atom tags
and posed by four tagged atoms, so no step names a tool and nothing is
searched. The second, after review, gave feedstocks their own pin again —
a `feedstocks: [HasAtoms]` input beside `tools` — because the
**workpiece alone** has to be an output, and only the engine knows which
structure a step's added atoms belong to; it also fixed the frame example,
the bulk-order semantics, the phase rule on the array pins, the tool-side
containment check, and made `scene` the pin a placed node displays. **The
build script loses a field and gains none.** Backward compatibility with
the files and libraries that exist today is deliberately not a goal — one
colleague uses the feature, the libraries are generated, and the generator
is regenerated with the format.

Extends the `mechanosynth` subsystem
(`rust/crates/atomcad-crystolecule/src/mechanosynth/`,
`rust/crates/atomcad-structure-designer/src/nodes/mechanosynth.rs`,
`nodes/mechanosynth_edit.rs`,
`lib/structure_designer/node_data/mechanosynth_editor.dart`, reference guide
`doc/reference_guide/nodes/atomic.md` §mechanosynth). Builds on
`doc/design_mechanosynth_step_metadata.md` and
`doc/design_mechanosynth_editor.md`, and supersedes the latter's deferred
"tool-molecule output pin", the former's deferred `axis` field, and the
former's per-step `method` field.

This is **milestone 1** of two. It models *what* a build does to every
molecule it involves — the workpiece, the feedstock it draws on, the tool
molecules — so that scrubbing a build shows atoms leaving a feedstock,
riding on a tool and arriving on the workpiece. **Milestone 2**, the tool
*trajectories* between those states and the animations they make, is a
separate design; this document reserves the one schema hook it will need and
nothing else.

## Motivation

The `mechanosynth` node replays a build script onto one structure. Every
step is written as if atoms came from nowhere and went nowhere: a hydrogen
abstraction deletes an H, a methylation conjures a CH₂ group. In a real
positional process each of those atoms is carried by a **tool molecule** whose
state changes with every reaction, and the tool is charged from and discharged
into **feedstock** structures — the "recharge" reactions of the diamond
toolset (Freitas & Merkle 2008), which every generated library so far has
omitted with the note "they change the tools, not the workpiece".

Three things want the tools in the model:

1. **Seeing the process.** A viewer who scrubs the build should see the tool
   tip gain the hydrogen it just abstracted, lose the group it just placed,
   and the feedstock deplete. That is what a mechanosynthetic process *is*;
   today the demo shows only its shadow on the workpiece.
2. **Checking the process.** A build script that abstracts two hydrogens in a
   row with a tool that can hold one is wrong, and nothing today can say so.
   With the tool's state in the replay, an operation whose tool-side
   precondition fails stops the replay with a message naming the tool, the
   step and the state it found — the same standing a workpiece-side match
   failure has.
3. **Animating the process** (milestone 2). A trajectory needs a start pose
   and an end pose per step, and the end pose needs the tool's structure at
   that step. Milestone 1 provides the structures; milestone 2 moves them.

The governing principle, learned from the editor: **the library is where
things are designed in detail; the build script and the design supply as
little as possible.** An operation is the researched entity. A step says
which operation and where. A design supplies one concrete molecule for each
tool type the library envisions, tagged so the node knows which and where.
Nothing that can be derived from those three is stored anywhere else, and
nothing that the library can state is typed by a user.

## Scope and non-goals

In scope:

- a required **`method`** on every operation, one of three engine-defined
  kinds (`tip`, `bulk`, `spontaneous`), with the semantics each kind carries
  for replay, for the panel and for the animation milestone; the step's
  `method` field is **removed**;
- a **`tools` section** of the operation library, one descriptor per tool
  *type*, and a **tool side** on every `tip` operation: a before/after
  rewrite of the tool in the tool's own local frame, plus an optional
  symbolic state transition;
- two appended input pins on `mechanosynth` and `mechanosynth_edit`:
  `feedstocks: [HasAtoms]`, the reservoirs a build draws on, and
  `tools: [HasAtoms]`, whose elements are **bound to tool types by an atom
  tag** and posed by **four tagged atoms**, one molecule per type;
- the engine replaying one **scene** — the base, the feedstocks and the
  tools merged, every atom mapped to its **participant** — with `result`
  staying **the workpiece alone** and an appended `scene` output carrying
  the merged structure for display;
- a **display default**: both nodes show `scene` when placed, through one
  small policy hook beside the existing "all pins" opt-in, with pin order
  unchanged;
- the tool's pose in the design solved from its own structure, never entered;
- the `step` record reporting the method, the tool type and state, and the
  agent;
- panel readouts; and, in the editor, **tool-aware offers**, a tool status
  readout and a last-good-state view of a failing cursor step, so that a
  tool-aware sequence is authored under guidance rather than by trial.

A step that draws on a reservoir is an ordinary step; nothing in the build
script says which structure a step acts on, the match does.

**The build script gains no field.** A step stays `op`, `t`, `r`, `note`,
`phase`, `layer`, `site`; `method` leaves it. That is a consequence of the
principle above, and a test pins the `BuildStep` field list.

Out of scope, each named in §Milestone 2 and §Follow-ups with what this
design leaves for it: tool trajectories, approach and retract motion, tool
collision checks, animation export; ghosting the tool side in the editor's
preview; a per-participant array output; any change to matching tolerance or
to the workpiece-side replay semantics.

## Decisions and the alternatives they replace

### A tool is a structure that an operation rewrites — the same primitive

The engine has one primitive: a local `before`/`after` rewrite placed on a
structure by a rigid transform and matched by nearest atom within tolerance.
A tool's state change *is* that primitive applied to the tool structure. So
an operation gains a second **side**: the target side is what exists today,
the tool side is the same two patterns expressed in the tool's local frame.
`apply_step` runs on the tool atoms with the tool-side patterns and a fixed
transform, and every property the workpiece side has — exact placement,
effect-derived `touched`, a failure that names the nearest atom — comes with
it.

Rejected: **a second, pattern-free "positioned changes" mechanism for the
tool side** (a list of "add atom X at p / delete the atom at p"). It is what
the patterns already say, minus the check that the atom at `p` is the one the
library expects. The nearest-atom match is not graph matching; it is exactly
the positional sanity check a tool model wants, and it costs nothing to
reuse.

### Feedstocks have their pin, because the workpiece is an output

A feedstock is not different in kind from the workpiece: a step rewrites it
with the same operations, through the same engine, at a position its `t`
names. What *is* different is what the user wants out: the **workpiece
alone** — to export it, relax it, count it, feed it to the next build —
without the hydrogen dump beside it. So the node has a **`feedstocks:
[HasAtoms]`** input beside `tools`, `result` is the base after the step
and nothing else, and `scene` is the base, the feedstocks and the tools
merged.

The engine already builds the thing this needs. A tool is a **participant**
of the scene, every atom is mapped to its participant, and an atom a step
adds is entered under the participant its match landed in. A feedstock is
one more participant kind. That is the whole cost: one enum variant, one
error, one tag, one pin per node, one panel line.

- **A reservoir that is a separate molecule** — a hydrogen dump, a source
  cluster — is placed with the ordinary transform nodes and wired to
  `feedstocks`. A step whose `t` lies on it matches there; a reservoir
  running out is an ordinary match failure, and the error's participant
  label says which reservoir.
- **A reservoir that is a region of the workpiece** — a sacrificial terrace
  the tool picks atoms from — needs nothing at all; it is already in the
  base, and its atoms are workpiece atoms.
- **Telling reservoirs apart in the scene** is the tag `ms_feedstock` the
  scene paints, plus whatever `tag` the user put on the molecule before
  wiring it — tags survive the merge, `apply_style` colours by them, and a
  user's own tag names the reservoir, which no index ever could.

Rejected: **folding the feedstocks into `base` with `atom_union` and
recovering the workpiece with `filter` by tag** — the shape of the previous
revision of this document. It was one node fewer, and it does not work: the
atom a step *adds* to the reservoir carries no tag, so an H dumped onto the
reservoir survives the filter and turns up in the "workpiece". Only the
engine knows, at match time, which structure a step acted on, and a
participant is the record of that. Also rejected: **`base` as `[HasAtoms]`
with the workpiece first** — it loses the `result` type and dies on the
array system's single-phase rule the moment a Crystal workpiece meets a
Molecule reservoir; the answer is the same as for `tools`, convert first.
And **a participant field on the `step` record**: which reservoir a step
drew on is readable from the error, the panel and the scene's tags, and a
derived field on every step for a fact only recharges have is not worth
its slot. Deferred, not rejected: **a `feedstocks` array output** with the
reservoirs after the step (§Follow-ups).

### The library envisions the tools; the design supplies them, tagged

Tools need more from the node than a reservoir does: a pose, a state that
changes with every step, and a refusal to be placed on, besides the
exclusion from `result` the two share. So they have their own pin, and
the library has to describe them. The
library's `tools` section names every tool type the process uses and, per
type, a **frame**: four or more named atoms with positions in the tool's
local coordinate system, the one named `apex` at the origin. A step never
names a tool. A `tip` operation names its tool type, and the wired molecule
bound to that type performs the step.

**Binding is a tag lookup, never a search.** The design marks each wired
molecule with **atom tags**, the mechanism atomCAD already has for "these
atoms play this role":

- the **tool type's name** as a tag, on the molecule's atoms — on the four
  frame atoms at least, on the whole molecule if that is easier. It says
  which type the molecule plays;
- the tag **`apex`** on exactly one atom, the origin of the tool's frame;
- one tag per remaining frame atom, with the names the library's frame
  uses (`a`, `b`, `c` in this document), each on exactly one atom.

That is the whole contract with the design: one type tag and four frame
tags, four atoms. The frame atoms cannot be coplanar, which is what makes
four enough.

**The pose is a direct fit on the tagged atoms.** The library frame gives
each named atom a local position; the tags give each a design position; the
rigid fit of the correspondences — Kabsch, the same fit the placement engine
uses — is the tool's transform, and its residual against the library
tolerance is the check that the molecule really has the geometry the library
was computed for. Three non-collinear correspondences already fix a proper
rotation; the fourth, off their plane, is what makes a molecule that is the
mirror image of the library's fail the residual instead of binding
mirrored — a triangle is congruent to its mirror image in space, a
tetrahedron is not. So there is no mirror rule, no candidate list, no
ambiguity and no sweep: a tip of thousands of atoms binds in the time it
takes to look up four tags.

Errors, each naming the molecule and the type: a wired molecule carrying no
type tag; one carrying two type tags; two molecules carrying the same type
tag; a frame tag missing from the molecule or present on two of its atoms;
a residual above tolerance. A type with no molecule is fine until a step
needs it. Every one of these is an error and never a guess, because the
design is supposed to supply exactly the cast the library wrote parts for.

Rejected: **identifying and posing a tool by searching its atoms for the
frame pattern.** The shape of an earlier revision, dropped in review because
the frame would have served two purposes that pull apart. As
identification, nothing makes an apex's first shell unique — the corner
atoms of a crystalline tip have the apex's coordination, and a bare tip's
apex pattern survives inside the functionalized tip built from the same tip
model — so binding would have been a coincidence of what the frame happened
to include. As a pose, the search needed a sweep, a mirror rule and an
ambiguity error. Tags remove all of it. Also rejected: **a `tool` index on
the step** (positional, redundant), **two molecules of one type chosen per
step** (a second instance can be a second type), **a per-tool offset input**
(a number the tagged atoms already contain), and **one pin for base and
tools with tags telling them apart** (a molecule tagged wrongly would
silently become workpiece, and the pin is what excludes tools from
`result`).

### Symbolic state is a label; geometry is the truth

A tool type may declare a list of **state names**, and an operation's tool
side may say `from` and `to`. The node tracks one state per bound tool.
**Every tool starts in its type's initial state**, the first entry of the
list: a tool type is researched with a resting state, every sequence starts
there, and the library is the one that knows it — so no per-instance
configuration exists and the symbolic check applies from the first step. An
operation whose `from` differs from the tracked state is an error naming the
tool type, its state and the state required. The geometric rewrite is applied
after the symbolic check and must match too. When the two disagree — the state
says *charged* and the apex has no atom to give — the geometric failure is
reported, because the geometry is what a viewer sees and the label is the thing
that is wrong.

**What the geometry can and cannot catch, exactly** (corrected during Phase 1,
where the first reading of this paragraph turned out to be false). The wired
molecule is *supposed* to be in the initial state, and binding cannot check
that: the frame atoms are on the handle and say nothing about the apex's cargo.
Neither, in general, can the first tool-side match — **a pattern says what must
be present, never what must be absent.** So the two directions are not
symmetric:

- A tool wired **carrying cargo it should not have** is caught only when an
  operation's tool side *names* the cargo in its `before`. `habst`'s `before` is
  the bare ethynyl — apex C, second C, handle C — and all three are still there
  when an H hangs off the apex, so `habst` matches and the extra H is silently
  tolerated. `hdump`'s `before` *does* name the cargo H, so a tool wired charged
  and asked to recharge fails there with the nearest-atom message.
- A tool wired **missing something it should have** is caught immediately by
  whichever tool side names the missing atom.

The symbolic state is what covers the remaining gap, and it is why it exists:
an initial state the library declares means the *sequence* is checked from step
one even where the geometry cannot see the difference. A library that wants the
cargo checked geometrically as well lists the atoms around the apex that a
charged tool has and a spent one does not — which is what `hdump` does, and what
`habst` deliberately does not need to.

The labels earn their place three ways: the `step` record carries
`tool_state`, so a caption can read "tool: charged"; the panel shows each
tool's state; and the error for a mis-sequenced build names the state rather
than an atom position, which is the message a process author can act on.

Rejected: **state derived from geometry alone** (no labels). Correct but
mute. Also rejected: **labels only**, with no tool-side geometry. Then
nothing changes on the tool in the viewport, which is the whole motivation.
And **an unknown state until first use** (an earlier revision): it avoided
per-instance configuration, but so does a library-declared initial state,
and it left the first step unchecked and every sequence starting from
nowhere.

### Operations declare their method; the user never types it

`note` is free text and empty by default, which is right for a note.
`phase` is a chapter name, subjective, inherited from the previous step and
empty by default, which is tolerable. `method` is neither: it is a fact about
the reaction, it has a small closed vocabulary, and it decides how the step
is sequenced and how it will be animated. So it lives on the **operation**,
stated once by whoever researched the reaction, and leaves the step.

Rejected: **a default method on the operation that a step may override.**
Overriding is exactly the freedom the principle removes, and every use of it
in the existing libraries turns out to be a different reaction wearing the
same name (next subsection). Also rejected: **a library-defined method
vocabulary.** The kinds are engine behaviour — how steps batch, what the
animation does — and a library cannot add a way of moving atoms in bulk
without code. What a library *does* say is which instrument or agent
performs a kind: the tool type for `tip`, the `agent` for `bulk`.

### One operation, one instrument

If the operation carries the method, then "the same reaction performed by two
instruments" is two operations. The existing libraries have exactly one such
case, an abstraction done by lithography in one phase and by a charged tip in
another, and it is not the same reaction: the two have different published
provenance, different tool sides and different animation, and share only the
target-side pattern. Splitting it follows the environment-variant convention
of `doc/design_mechanosynth_editor.md`: a variant is a claim, and the claim
"this reaction, by this instrument, has been computed" is one per instrument.
The naming convention is the library's; `<operation>_<instrument>` beside the
`<operation>_<environment>` suffixes is the obvious one, and the target-side
repetition costs nothing because a generator writes both from one definition.

Consequently an operation has **exactly one tool type when it is `tip`**, and
none otherwise.

### Two ways of using the editor, one node: the tools pin is the mode

The editor serves two different users. One wants a **precise modelling
tool**: a structure that is easiest to bring into existence as a sequence
of researched operations with almost no degrees of freedom, and no interest
in which instrument would perform them. The other authors a **tool-aware
build**: every tip step must find its tool in the right state, so recharge
steps on a feedstock are interleaved with the workpiece steps to keep the
states right.

They differ in exactly one fact of the network — whether tool molecules are
wired — and that fact is already the engine's switch. With `tools` unwired
no tool side is applied and no state exists; the offers are geometry only;
this is the modelling tool. With `tools` wired the same node becomes
tool-aware in three places, none of them a setting: the offers know each
tool's state, the panel shows it, and a failing cursor step shows the state
before it. There is no mode flag, nothing to remember, nothing to get out of
sync with the wiring.

Two facts make this enough. **What makes the tool-aware use strict is
geometry, not a checkbox.** A second abstraction on a spent tip cannot be
authored under any setting: the tool side's `before` expects a bare apex and
the apex holds an atom, so the step does not match. The symbolic state only
makes the message readable. So a "check tool states" toggle would toggle
nothing real, and none is offered. **The cursor is already the validity
frontier.** The editor replays only up to the cursor, so an invalid tail is
not fatal while authoring. That is what makes the natural workflow work:
build the workpiece steps first with tools unwired, wire the tools, then
walk the cursor forward and insert a recharge in front of each step that
complains, front to back, until the block replays to the end. The `steps`
output carries the whole block throughout, valid or not; a replayer
downstream reports the first violation, which is what it should do.

Rejected: **an explicit mode on the node** (a "tool-aware" checkbox or two
node types). It would duplicate a fact the wiring already states and could
disagree with it. Also rejected: **making tool-state violations non-fatal
in the replay**. The geometric side fails anyway when the state is wrong,
and a replay that continued past a failed tool side would show a build the
tool could not have performed.

### Tools stay parked in milestone 1

The tool molecules on the `tools` pin are at their **parked** poses, wherever
the user placed them, and the node rewrites them in place. A tool that is
moved between steps is milestone 2's concern, rendered as motion rather than
stored as state.

## Methods: three kinds and what each means

`method` is a closed, engine-defined vocabulary. Every operation names one
kind; the kind decides how a step is performed, how consecutive steps group,
and what the animation milestone will do with it. The library adds the
*who*: a tool type for a tip step, an agent for a bulk step.

| kind | who performs it | what one step is | sequencing | animation (milestone 2) |
|---|---|---|---|---|
| `tip` | a positional probe — the operation's tool type; a bare probe is a tool type with an empty tool side | one visit of one tool to one site, at the step's `t`/`r` | strictly sequential | the tool travels from its parked pose to the `approach` pose, the rewrite happens, the tool retracts |
| `bulk` | an exposure of the whole workpiece — a gas, a dose, light; named by the operation's `agent` | one site's share of an exposure | a maximal run of consecutive `bulk` steps with the same `agent` is one **event**, the unit the panel and the animation group by; the steps are still replayed one after another in file order | the event plays as one: the agent arrives everywhere at once, every site of the event reacts together |
| `spontaneous` | nothing external — the workpiece rearranges by itself | one rearrangement enabled by the step before it | belongs to the **event of the preceding non-spontaneous step**; a spontaneous step at the very start of a script is its own event | plays as the settling that follows its enabling step, before the next tool visit or exposure |

Three consequences, which are the reason the vocabulary is closed:

- **The tool is implied by the kind.** A `tip` operation has a tool side,
  hence a tool type; a `bulk` or `spontaneous` operation has none, and a
  library that gives one a tool side is invalid. Two instrument labels the
  earlier libraries carried in the step become `tip` with two tool types;
  what distinguished them was always the tool.
- **Bulk needs an agent.** `agent` is a required string on a `bulk`
  operation — the species or energy that does the work, `"Cl2"`,
  `"C2HCl3"`, `"UV"` — and it is what batches steps into events and what a
  caption names. It is the library author's word; the engine reads it only
  for equality.
- **Events are derivable, so the node does not store them.**
  `MechanosynthStep` reports `method` and `agent`; the event boundaries
  follow from the sequence by the two rules in the table, and the panel's
  chapter machinery can show them when milestone 2 wants them. Milestone 1
  defines the rules, exposes the fields, and does no grouping.
- **An event is a display grouping, not a replay semantics.** The replay is
  sequential for every kind: each step's `before` is matched against the
  scene as the steps before it left it, and two bulk steps on neighbouring
  sites are not independent — one can add the atom the other's `*` slot
  then finds, or delete a neighbour the other's frame names. So the order of
  the steps inside an event **is** their meaning, exactly as for tip steps;
  a generator that wants a stable file sorts its sites *before* it replays
  them, and a re-sort of an existing file is an edit that has to replay
  again. The animation shows an event as one arrival because that is what
  an exposure looks like, and it shows the scene after the event, which is
  the sequential result; nothing in milestone 2 may assume the steps
  commute.

What is deliberately *not* in the vocabulary: the instrument labels the
earlier libraries used as free strings (`sam`, `stml`, `gas`, `dose`, `uv`,
`relax`). They were doing two jobs at once, kind and instrument, and the two
now have their own fields — `method` for the kind, the tool type or the
`agent` for the instrument. A style rule that coloured by label colours by
`tool_type` or `agent` instead.

## The operation library

The format string becomes **`atomcad-msops/2`**: `method` is required on
every operation, `tools` is required whenever any operation is `tip`, and a
`/1` file is refused with a message saying to regenerate it. The parser keeps
skipping unknown keys.

### The `tools` section

```json
"tools": [
  {
    "name": "habst_tool",
    "note": "Hydrogen abstraction tool: an ethynyl radical on a handle.",
    "states": ["charged", "spent"],
    "frame": [
      { "tag": "apex", "pos": [0, 0, 0] },
      { "tag": "a",    "pos": [1.45, 0, -3.17] },
      { "tag": "b",    "pos": [-0.73, 1.26, -3.17] },
      { "tag": "c",    "pos": [-0.73, -1.26, -3.17] }
    ]
  }
]
```

| key | meaning |
|---|---|
| `name` | unique among tool types; what an operation's `tool.type` names, and the tag a design puts on the molecule that plays it |
| `note` | free text for the panel and the offer list, like an operation's |
| `states` | optional list of state names; the vocabulary `from`/`to` may use, and **the first entry is the type's initial state**, the one every bound tool starts in. Absent means the type carries no symbolic state; an empty list is a parse error |
| `frame` | four or more entries, each a **tag name** and a position in the tool's local frame. No elements, no `*`: the atoms are found by tag, never by pattern. Exactly one entry is `apex` and it sits at the origin; the entries are not coplanar; tag names are unique within the frame. Each is a **parse error** when violated, naming the type — unlike the origin *warning* on operations, this is a rule the fit depends on |

The tag vocabulary is the library's, and it should be **shared across
types** — `apex`, `a`, `b`, `c` for every tool — so a user tags every tool
the same way and only the type tag differs. A type name may not collide
with a frame tag name (parse error), and `apex` is reserved for the origin.

**The frame and every tool side of that type share one local frame.** The
pose solved from the frame is the transform every tool-side pattern of that
type is placed with. So the frame atoms must be atoms that exist and stay put
in every state the tool passes through — the handle, not the apex's cargo —
and the library author chooses them so. A frame atom that a tool side moves
or deletes is a library error the engine cannot see; the residual on the
next binding will.

**The legs come off the axis.** A functionalized tool is usually linear at
the business end — the example is an ethynyl on a handle, apex C at the
origin, the second C at (0, 0, −1.21), the handle carbon at (0, 0, −2.66) —
and the apex plus any two atoms of that axis are collinear, so any fourth
atom leaves the four coplanar and the parse rule rejects the frame. The legs
are therefore taken from the **handle**, not the axis: in the example `a`,
`b`, `c` are the three cage atoms bonded to the handle carbon, at
tetrahedral positions around it, which puts them in a plane 3.17 Å below the
apex and the apex off that plane. The same holds for a bare probe, whose
apex and its nearest neighbour are on the axis: the legs are three
second-shell atoms around the axis, never the axis atom itself.

**What a design does.** Per tool: tag the molecule with the type name (one
`tag` node with no region tags the whole molecule), then tag the four frame
atoms — `apex` and the three legs — with `atom_edit`'s *Tag selected…* on
each, or with `tag` nodes carrying a small region. Hovering an atom shows
its tags, and an `apply_style` rule with `label: "{tag}"` draws them, which
is how a mis-tagged frame is found. The `ops_library` panel lists, per type,
the tags it expects.

| instrument | frame atoms to tag | tool side | wire for a quick check | wire for an animation |
|---|---|---|---|---|
| a functionalized probe | the tooltip's apex and three atoms of its handle | atoms come and go at the apex | the tooltip molecule alone | the tooltip bonded to the tip model, so the whole tip moves |
| a bare probe (lithography, atom manipulation by a bare tip) | the tip's apex atom and three of its neighbours | empty for lithography; an atom at the apex for pick-and-place | the tip model | the same |

The four positions must agree with the library's to within the tolerance,
so a bare probe's frame **is coupled to a tip model**: the library states
the apex geometry of the tip the process is designed for, as it states every
other geometry, and a different tip model is a different type or a
regeneration. Which tip a process uses is a designed fact.

**Tag budget.** Type tags, `apex`, the shared leg names, the user's own
reservoir tags and the replay's own tags (`ms_current`, `ms_added`,
`ms_layer`, `ms_tool`, `ms_feedstock`) all count against a structure's
thirty-two names once the participants are merged into the scene; with a
shared leg vocabulary a process with a handful of tool types uses about a
dozen.

### The tool side of an operation

```json
{
  "name": "habst",
  "method": "tip",
  "before": { "atoms": [ { "id": 1, "el": "H", "pos": [0, 0, 0] } ], "bonds": [] },
  "after":  { "atoms": [], "bonds": [] },
  "tool": {
    "type": "habst_tool",
    "from": "charged",
    "to": "spent",
    "before": {
      "atoms": [
        { "id": 1, "el": "C", "pos": [0, 0, 0] },
        { "id": 2, "el": "C", "pos": [0, 0, -1.21] },
        { "id": 3, "el": "*", "pos": [0, 0, -2.66] }
      ],
      "bonds": [[1, 2, 3], [2, 3]]
    },
    "after": {
      "atoms": [
        { "id": 1, "el": "C", "pos": [0, 0, 0] },
        { "id": 2, "el": "C", "pos": [0, 0, -1.21] },
        { "id": 3, "el": "*", "pos": [0, 0, -2.66] },
        { "id": 4, "el": "H", "pos": [0, 0, 1.06] }
      ],
      "bonds": [[1, 2, 3], [2, 3], [1, 4]]
    }
  }
}
```

| key | meaning |
|---|---|
| `type` | required; must name an entry in `tools`, else a parse error naming the operation |
| `from` | optional; the state the tool must be in. Must be in the type's `states`. Absent: any state |
| `to` | optional; the state after the step. Must be in `states`. Absent: unchanged |
| `before`, `after` | the rewrite of the tool, in the tool's local frame; same pattern rules as the target side, including `*`, bond rules and the origin convention. Frame atoms are recognised the same way (`is_frame_atom`) and drop out of `touched` the same way |

The tool side names the atoms the step reacts with and enough of their
neighbourhood to be a positional sanity check; it does **not** have to
repeat the frame. The pose comes from the binding, so the pattern's job is
only to say "this is what the apex must look like now", and the example
lists the apex, the ethynyl carbon and the handle carbon it hangs from.

Both patterns may be empty, which is a tool side that changes nothing: a
bare probe performing lithography names its type so the step records which
instrument visited, without asserting any change. `from`/`to` still apply.

The tool side is written per operation, not per environment variant of it:
five variants of one donation that differ in their *target* environment all
say the same thing about the tool, so they carry five identical tool sides.
That repetition is the file format's, and a generator writes it once from
one definition.

**One tool side, one tool type per operation.** A `tip` operation without a
tool side is invalid; a `bulk` or `spontaneous` operation with one is
invalid.

### `method` and `agent`

```json
{ "name": "hdon",   "method": "tip",  "tool": { "type": "hdon_tool", … }, … }
{ "name": "cl_add", "method": "bulk", "agent": "Cl2", … }
{ "name": "settle", "method": "spontaneous", … }
```

| key | meaning |
|---|---|
| `method` | required; one of `tip`, `bulk`, `spontaneous`; anything else, or its absence, is a parse error naming the operation |
| `agent` | required on `bulk` (a missing one is a parse error), forbidden elsewhere; free text chosen by the library author |

### Reserved for milestone 2: `approach`

An operation may carry `"approach": { "r": [[…]], "t": […] }` — the tool
frame relative to the operation's target frame at the moment of reaction.
Milestone 1 **parses and keeps** it on `Operation` and uses it for nothing.
It is reserved so that a generator can start writing it and a milestone-2
build can start reading it without a format change. No default.

## The build script

The format string becomes **`atomcad-msbuild/2`**. A step is `op`, `t`,
`r`, `note`, `phase`, `layer`, `site`. **`method` is gone** from the step,
from `BuildStep`, from `build_step.rs`, from the authored block and
`AuthoredStepJson`, from the text-format step literal, and from what
`export_build_script` writes; the loader treats a `method` key as the unknown
key it now is and skips it. `BuildStep` is therefore seven fields, and a
test pins that.

`MechanosynthStep` changes as follows. `method: String` now reports the
**operation's kind name** — `tip`, `bulk` or `spontaneous` — so a `switch`
downstream sees the kind. Three fields are appended: `tool_type: String`,
the operation's tool type when it is `tip`; `tool_state: String`, the
tracked state of that tool *after* the step, `""` when the type has no
states; and `agent: String`, the operation's agent when it is `bulk`. Empty
strings at step 0, as for every other field.

## Engine

### The scene

```rust
/// Which structure an atom of the scene belongs to. The index is the
/// position on the respective pin.
pub enum Participant { Base, Feedstock(usize), Tool(usize) }

/// A wired tool molecule after binding.
pub struct ToolBinding {
    pub instance: usize,          // index on the `tools` pin
    pub tool_type: String,        // the type its frame fitted
    pub pose: ToolPose,           // r, t, residual
    pub state: Option<String>,    // the type's initial state, changed by `to`;
                                  // None only for a type without `states`
}

/// Everything a replay acts on, as one structure.
pub struct Scene {
    pub structure: AtomicStructure,
    pub participants: FxHashMap<u32, Participant>,
    pub bindings: Vec<ToolBinding>,   // one per wired tool, in pin order
}

pub fn replay_scene(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    tags: HighlightTags<'_>,
) -> Result<Scene, MechanosynthError>;
```

`replay` becomes the wrapper with no feedstocks and no tools, returning the
base, and keeps its signature, so the engine tests and every other caller
compile unchanged.

**Phase 2 added one entry point the phase list did not foresee.** The editor's
`replay_prefix_and_block` replays *twice* — the wired prefix with no highlights,
then the authored block with `ms_current` — and neither form above can express
the second pass: `replay_scene` builds a scene from the three inputs, so calling
it again would re-bind every tool into its *initial* state and lose what the
prefix did to it. So `replay_scene_partial` is factored into the two halves it
always was, and both are public:

```rust
pub fn build_scene(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
) -> Result<Scene, MechanosynthError>;

/// Replays `script` into a scene that already exists.
pub fn replay_steps(
    scene: &mut Scene,
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    tags: HighlightTags<'_>,
) -> Result<Option<MechanosynthError>, MechanosynthError>;
```

`replay_scene_partial` is now `build_scene` followed by one `replay_steps`, so
there is still one code path. The obvious alternative — one concatenated
`prefix + block` script — is wrong for the reason the two replays existed in the
first place: it paints `ms_current` on the last *prefix* step whenever the cursor
sits at 0.

**Building the scene.** The base is cloned first, so its atom ids are
unchanged; each feedstock, then each tool, is merged with
`add_atomic_structure`, which remaps their ids and interns their tags, and
every merged atom is entered in `participants`. The merge unions the tag
tables, and a structure has thirty-two tag names: when the base's names
plus the participants' plus the replay's own would exceed that, the merge
fails and the replay is `Err(SceneTags)` naming the molecule whose tags did
not fit and the count — a real error, unlike the cosmetic highlight that
`paint` drops silently, because a tool whose type tag cannot be interned
cannot bind. Then **binding**, per wired tool molecule: the type tags it
carries are collected — none is `Err(ToolUntagged)` naming the molecule and
listing the library's types, two is `Err(ToolMultiType)` naming both; the
type's frame tags are looked up among the molecule's atoms — a tag on no
atom or on two is `Err(ToolFrameTag)` naming the tag; the fit of the frame
positions onto those atoms is the pose, and a residual above tolerance is
`Err(ToolPoseResidual)` naming the type and the residual; the binding's
state is the type's initial state. A type bound twice across molecules is
`Err(ToolDuplicate)` naming both molecules. A
type with **no** molecule is fine until a step needs it. Binding happens
before any step, so a mis-tagged tool is reported once, at the top, rather
than at the first step that uses it.

**Per step**, in order. **A step is all or nothing**: both sides *match*, and
every check below runs, before the first atom moves. Matching the tool side
against the scene as the target side left it was the first shape of this list
and it is wrong — a tool-side failure would then leave the target side's rewrite
behind, and the partial-result form below would hand the editor a half-applied
step rather than "the scene after the last successful step". Nothing is lost by
matching both up front: the two sides touch different participants by
construction, and the one case where they would not is `ToolSideOffTool`, which
is raised before anything moves.

1. **Target side.** Match `op.before` anywhere in the scene, exactly as
   `apply_step` does today. The matched `before` atoms must
   all belong to **one** participant that is the base or a feedstock — a
   target side matching inside a tool molecule is `Err(StepOnTool)` naming
   the step and the tool, and one whose matched atoms span two
   participants (a workpiece atom and a reservoir atom, or two reservoirs)
   is `Err(StepAcrossParticipants)` naming the step and both. Both are
   raised here, before anything is mutated, which is what makes "and the
   scene is unchanged" true of them.
2. **Tool side**, when the operation is `tip` and the `tools` pin is wired:
   the binding of the operation's tool type is looked up — none is
   `Err(ToolMissing)` naming the step, the operation and the type; the
   symbolic check — if `from` is set and differs from the binding's state,
   `Err(ToolState)`; then a match of the tool-side `before` at
   `Step { r: pose.r, t: pose.t }`.
   This is the same nearest-atom match as the target side, over the whole
   scene, so nothing about it is confined to the tool by construction: a
   tool parked in contact with the workpiece, or later at an approach pose,
   can have base atoms inside tolerance of a pattern position. So the
   matched `before` atoms must **all belong to that binding** — any other
   is `Err(ToolSideOffTool)` naming the step, the tool and the atom it
   found, the mirror of `StepOnTool`.
3. **Apply**, now that nothing can fail: the target side's rewrite, then the
   tool side's. Atoms the target side adds **belong to the participant the
   match landed in** — this is the fact that makes the workpiece separable
   from the reservoirs, and nothing but the engine knows it; a step whose
   `before` is empty (a pure addition) belongs to the base. Atoms the tool
   side adds belong to the tool. Then the state update, `to` if set.
4. **Highlights.** `touched` from both sides is painted `ms_current`, so
   the tip lights up with the site it visited, and a dump lights up with
   the atom it received. `ms_added` and `ms_layer` are painted on **base
   atoms only**: they mean "what the build created on the workpiece, by
   layer", they feed style rules, counts and exports downstream of `result`
   and `scene` alike, and an abstracted H sitting on the tip, or dumped on
   the reservoir, is cargo, not construction. Tool and feedstock atoms are
   told apart by `ms_tool` and `ms_feedstock` instead.

**Workpiece-only replay.** With nothing wired to `tools`, step 2 is skipped
for every step, no binding runs, and no state is tracked. This is not a
compatibility mode but a use: looking at what a build does to the workpiece
without modelling the instruments, which is how a library is developed
before its tools exist. Wiring the pin turns the tool model on for the whole
script. `feedstocks` is independent of it: a build can draw on a reservoir
with no tool modelled, and the dump step is then an ordinary rewrite of
the reservoir.

Failure on either side aborts the replay with the step number in the
message, and the partial state is reachable by asking for one step fewer,
as today. `NoMatch` gains a `participant` label — `base`, `feedstock 1`,
`tool 0 (habst_tool)` — so the sentence says where it looked.

**Outputs from the scene.** `scene` is the structure itself, with every
tool atom tagged `ms_tool` and every feedstock atom `ms_feedstock` from
the participant map. `result` is a clone with every non-base atom deleted;
base ids are unchanged, so for a build that touches no reservoir it is
atom-for-atom what `replay` produces, which is the assertion the tests pin,
and for one that does it is the base's atoms of the scene, added atoms
included, with nothing of the reservoirs.

### The tool pose

```rust
pub struct ToolPose { pub r: DMat3, pub t: DVec3, pub residual: f64 }

/// `correspondences` pairs each frame entry's local position with the
/// design position of the atom carrying its tag.
pub fn tool_pose(
    correspondences: &[(DVec3, DVec3)],
    tolerance: f64,
) -> Result<ToolPose, MechanosynthError>;
```

The rigid fit the placement engine already has (Horn's quaternion Kabsch),
factored out of `place.rs` so both use one function, applied once to four
or more correspondences. No sweep, no candidates, no mirror: three
non-collinear points fix the proper rotation, and with the frame
non-coplanar by parse rule a molecule that is the mirror image of the
library's cannot be superimposed by any proper rotation, so it simply fails
the residual gate.
The residual is the maximum per-atom distance, as everywhere in this
subsystem.

### Applicability with tools

`applicable_ops` gains an optional `bindings: Option<&[ToolBinding]>` and
`Applicability` gains `tool: Option<ToolReadiness>` — the type, the state
found, whether the tool side fits, and the reason when it does not. `None`
when tools are unwired or the operation is not `tip`. The workpiece-side
search is unchanged; the tool check is one match of the tool side's
`before` at the bound pose, run only for operations that already fit the
clicked atom. `applicable_ops` sorts ready rows above tool-blocked rows,
which sort with the near misses.

`replay_scene` gains a partial-result form for the editor's block replay:
on a step failure it returns the scene after the last successful step
together with the error, rather than the error alone. "After the last
successful step" is exact rather than approximate, and it is the
all-or-nothing rule in §The scene that makes it so. The replayer node
keeps calling the all-or-nothing form.

### Errors

New `MechanosynthError` variants: building the scene, `SceneTags`; at
binding, `ToolUntagged`, `ToolMultiType`, `ToolDuplicate`, `ToolFrameTag`,
`ToolPoseResidual`; per step, `ToolMissing`, `ToolState`, `StepOnTool`,
`StepAcrossParticipants`, `ToolSideOffTool`; and a `participant` field on
`NoMatch`. Parse-time: `Invalid` with a location naming the tool type or the
operation, for an unknown `tool.type`, a `from`/`to` outside `states`, a
duplicate tool name, a frame with fewer than four entries, no `apex`, an
`apex` off the origin, coplanar entries, a repeated or reserved tag name, a
type name colliding with a frame tag, a missing or bad `method`, a missing
or misplaced `agent`, a missing or misplaced tool side, a `/1` format
string.

## The `mechanosynth` node

**Pins.** Inputs `base`, `ops`, `steps`, `step` unchanged; **appended**
`feedstocks: [HasAtoms]` (pin 4) and `tools: [HasAtoms]` (pin 5), both
optional and wire-only. Array pins accept several wires and concatenate
them, so `tools: [w_tip, si_tip]` in the text format is two wires, and a
single structure wired to an array pin is broadcast to a one-element array
by the existing rule. **Every wired feedstock and tool must have the phase
of `base`** — all `Crystal` or all `Molecule` — and a mismatch is a
validation error on the node naming the offending pin, wire and fix. The
validator's generic single-phase rule only covers pins whose output type is
taken from the array's elements, which neither of these is, so the node
states the rule itself; it is the same rule `atom_union` enforces, for the
same reason: the `scene` output is a merge of those atoms into `base`'s
variant, and a Molecule's atoms inside a Crystal is exactly what the union
refuses to produce. A tip model that is a Crystal beside a tooltip that is
a Molecule, or a Molecule reservoir under a Crystal workpiece, is therefore
wired through `exit_structure` or `enter_structure` first.

**Outputs.** `result` (pin 0) is the base after the step, same type as
`base`, **the workpiece alone**; `step` (pin 1) is the record above.
**Appended** `scene` (pin 2): the merged scene after the step — base,
feedstocks, tools — keeping `base`'s variant the way `atom_union` does. A
`scene` with nothing wired to either array pin equals `result`. The
replay's own tags take five of the thirty-two slots, counting the three
that exist; the tool and reservoir tags take theirs (§Tag budget).

**Display default: `scene`.** A placed node shows pin 0 only, by the
network's default display state, and the one existing opt-in shows *all*
pins — meant for unpack nodes that draw nothing, and wrong here, because
`result` and the base part of `scene` are the same atoms drawn twice. What
a user scrubbing a build with tools wants to look at is the scene, and
what the editor has to be looking at to author a recharge is the scene
(§mechanosynth_edit). So `NodeData` gains a second, general hook beside
`default_display_all_output_pins`: `default_displayed_output_pins(&self)
-> Option<HashSet<i32>>`, `None` for every node today, `Some({2})` for both
mechanosynth nodes; `add_node` and the display-policy pass consult it where
they set `{0}`. **Pin order does not change.** Making `scene` pin 0 would
have got the same default for free, and it was considered; but wires and
displayed-pin sets are persisted by index, so it would silently rewire
every existing file's `result` consumers to the scene, and in the text
format a bare `build` would become the scene and every downstream consumer
of the workpiece — the common wiring — would have to write `build.result`.
Two things follow the hook: the serializer's "omit the default pin set"
check compares against the node's default, not a hard-coded `{0}`, so a
saved default round-trips and a file that had chosen `{0}` explicitly keeps
it; and an old file in which the node was at its then-default `{0}` (no
entry written) loads showing `scene`, which with nothing wired to the new
pins is the same atoms.

**Evaluation** evaluates the two new pins (`None` → empty), calls
`replay_scene`, splits `result` off the scene, and builds the record from
the same clamp. An evaluation error reaches all three pins.

**Panel.** Below the existing readout, a *Tools* block: one row per wired
tool (index, the type its tag names, pose residual, current state), and a
*Feedstocks* line — one entry per wired reservoir with its atom count at
the step — read from the evaluated scene through `get_mechanosynth_info`.
The method leads the chip row as a **badge** coloured by kind, with the tool
type or the agent beside it; `phase`, `layer` and `site` stay plain chips
behind it. The split is not decoration: the method is the only one of the four
the step does not state, and it decides what the field beside it means.
Nothing is editable — the pins are wire-only and every field is derived.

**Subtitle** unchanged.

## The `mechanosynth_edit` node

The same input pins appended (`feedstocks` pin 3, `tools` pin 4), the same
`scene` output appended (pin 2), the same display default. `result` stays
the base at the cursor, the workpiece alone. Evaluation keeps the
two-replay rule — the prefix without highlights, the block with
`ms_current` — so a cursor at `0` still shows no highlight on any
participant; it runs both passes over **one** scene through `build_scene`
and two `replay_steps` calls rather than through `replay_scene`, for the
reason given in §The scene: a second `replay_scene` would re-bind every
tool into its initial state.

**The placement tool keeps its interaction**, and gains one refusal. The
tool owns picks while the node is active, which the existing rule defines
as selected and displayed on *any* pin; the hit test walks every displayed
output of the node and takes the closest atom, and base ids are the same in
`result` and `scene`, so a click on the workpiece means the same thing
whichever pin is shown. A reservoir atom exists only in `scene`, which is
why `scene` is the display default: a recharge is authored exactly like a
placement — click the dump atom, choose the donation — and with `result`
alone displayed there is nothing to click. The offer preview's ghost atoms
ride on the decorator of the output structure; today that is pin 0 only,
and it becomes **both** structures, so a selected row previews whichever
pin is shown. A click on a tool atom — possible only with `scene`
displayed — is answered with "tools are rewritten by their operations, not
placed on" and no offers. Showing both pins at once draws the workpiece
twice and is the user's choice, not an error.

**Nothing new is stored or asked.** `AuthoredStep` loses `method` and gains
nothing. A committed placement inherits `phase`, `layer` and `site` from the
previous step as today and `note` from nothing; there is no tool to choose,
because the operation names its type and the binding names the molecule.
**The `method` chip goes**: the row shows the operation's kind as a
read-only badge, coloured by kind, with the tool type or agent beside it.

**The badge is on the cursor row, not on every row** (Phase 3). The step list
is fifty rows in a 300 px panel, each already carrying a drag handle, a number,
the operation name, a note, a warning triangle and two icon buttons; a badge
per row would take the width the operation name needs. So every row keeps the
**dot** in its kind's colour — which is what makes a block's shape readable at
a glance — and the badge with its instrument or agent opens with the metadata
fields, on the one row the cursor is on. That is also the row where it replaces
something: the fields beside it are editable and the badge deliberately is not.

**Tool-aware offers.** When tools are wired, `applicable_ops` also answers
"is the tool ready" for every `tip` operation that fits the clicked atom:
the operation's tool type must be bound, its `from` must agree with the
tool's state, and its tool-side `before` must match at the tool's pose —
one nearest-atom match of a few atoms per operation, no search. A row whose
tool is not ready is shown **below the rule with the near misses, dimmed and
not placeable**, with the reason where the residual would be: *habst_tool
is spent*, *no molecule tagged `probe` on the tools pin*. It cannot be
committed for the same reason a near miss cannot: its tool side would not
match.

**"Not placeable" is not "not selectable"** (Phase 3, correcting this
paragraph's word for it). A near-miss row has always been *selectable* in the
popup — seeing the amber ghost is half the answer to "why not here?" — and a
click on one previews it and puts the reason in place of the row instead of
committing. A tool-blocked row behaves identically, and the two reasons differ
in the fix they name: a near miss's fix is an edit to the **library** (add the
variant, or loosen the tolerance), a blocked tool's fix is a **step** (the
recharge). So the rule the list splits on is
`Applicability::offerable()` — fits *and* tool ready — rather than `fits`, and
the header counts what can be placed rather than what fits.

A `bulk` or `spontaneous` row is
never affected. With tools unwired no row carries a tool annotation, which
is the modelling use exactly as it is today. A row whose tool *is* ready
carries its annotation in the row's ⓘ beside the library's note rather than on
the row itself (Phase 3): at 300 px a variant row already spends its width on
an indent, a rule, an arrow, an ordinal, a title and a badge, and a ready
instrument is reference — the panel's *Tools* readout is what the user watches
for a recharge. The badge slot is reserved for a *blocked* tool's reason, which
is what the design wanted it for.

**There is no repeat flow to change** (Phase 3, correcting this paragraph's
assumption of one). This document was written against an editor with an
*armed* mode — commit an operation and the next click places it again — and
that mode was removed before this design started: the library names one
operation per host *environment*, so "the same operation again" is usually
wrong at the next site, and a click whose meaning depends on invisible state
costs more than the clicks it saves (`lib/structure_designer/AGENTS.md`
§Mechanosynthesis placement tool). A viewport click is always the atom-first
question, so a spent tool cannot be discovered by a failing repeat pick; it is
discovered on the *Tools* readout, or on the blocked row of the next offer
list. "Click the dump" is still the whole instruction for the recharge.

**Tool status.** The panel's steps list gains a one-line *Tools* readout
above the rows — each bound tool's type and its state at the cursor, *habst_tool
· spent* — refreshed on every cursor move and commit. That is what tells the
user a recharge is due before the offers do, and with no armed strip to carry
it (see above) it is the **only** place that does, which is why it is a line of
its own rather than a fold-out.

**A failing cursor step shows the last good state — in the viewport, not
on the pins.** When the block fails at the cursor step, `result` and
`scene` carry the error exactly as the replayer's would: a downstream
export or style node must never receive a silently truncated build, and
*same result, both nodes* holds for a failing block as for a good one. What
changes is what the editor *displays*. Today the viewport empties, which
is right for the replayer — its slider reaches any step — and wrong for
authoring, where the failing step is the one being worked on and the user
needs to see where it stands and click the dump. So the block replay
returns the last good structure alongside the error instead of discarding
it; the editor keeps that structure as evaluation-time node state (never
persisted), with `ms_current` on the previous step, and the scene
generator renders it in place of the erroring pin. This is **not** the
ghost route: ghosts ride on the decorator of an evaluated structure, and
here the displayed pin evaluated to an error, so there is no structure to
decorate.

**The override channel already existed** (Phase 2, correcting this
paragraph's guess at it): `EvalOutput::set_display_override(pin, value)` is
what `motif_edit` uses to draw something other than what the wire carries,
and `generate_scene` consults it per pin. So there is **no new `NodeData`
hook** — `eval` returns the error on the pins and hands the last good
structures over on the same two pin indices as display overrides.
`hit_test_node_atomic_structure` reads the generated scene, so the
placement tool picks against that structure for free rather than being
taught about the override.

What the node *does* store, and what the earlier text was reaching for, is
the whole `Scene` rather than a structure: the offer sweep needs the tool
**bindings** and a click needs the **participant map**, and neither
survives the trip through a pin. It lives in a `#[serde(skip)]`
`Mutex<Option<Scene>>` on the node data — the `atom_edit` cache pattern —
written on every evaluation that got as far as replaying a step.

**The `steps` output keeps its array** through a block failure, which this
document did not say and its Phase 2 test list assumed ("the `steps` output
carried all steps throughout"). `steps` is not a replay product: the block
is stored data and the prefix arrived intact, so the array is exactly as
valid as it was a moment ago — and it has to be, because the walk in
§Making a sequence tool-aware inserts a recharge *while* the block is
failing and a downstream replayer must see the same steps throughout. An
*input* failure — a bad library, an erroring prefix — still errors on all
three pins, because then there is no prefix to concatenate.

The failing row carries the error chip
with the engine's message, and the panel banner names it. The row is the
**cursor** row — the cursor step is the last one the block replayed, so there is
no other row the failure could belong to — and the chip is keyed on the last
good state being present rather than on there being an error at all, because an
*input* failure has an error and no row (Phase 3, §Phase 3).

The panel also says, in a line of its own, **what the viewport is showing**:
*Showing the last good state — 412 atoms, before step 7* (Phase 3; this document
did not ask for it). Without it the view is a lie by omission. The pins carry an
error, so a user who knows the rules expects an empty viewport, and the
structure in front of them is the state *before* the failing step rather than
after it; nothing else on screen says which.

**Every kind is authored the same way in milestone 1**: click an atom,
choose the row. A bulk step is one site's share of an exposure and is
authored one site at a time, exactly like a tip step; a spontaneous step is
chosen from the offer list on the atom it rearranges. The kinds differ in
how the panel groups them and in how the animation will play them — an
exposure's steps together, a settling right after its enabling step — not
in how they are authored or replayed. Authoring a whole exposure in one action, and
offering a rearrangement right after the step that enables it, are
§Follow-ups, each with a reason it is not here yet.

What the editor does *not* do in milestone 1, listed in §Follow-ups: ghost
the tool side in the preview; adopt a wired prefix with one action; expose
or settle.

### Making a sequence tool-aware, step by step

A sequence authored without tools is not a dead end; turning it into a
tool-aware one is a mechanical walk, and it is worth spelling out because
it is the workflow most users will follow — model first, instrument later.

1. **Wire the tools.** Tag the molecules and wire them to `tools`. Nothing
   else changes: the block, the cursor and the steps are what they were.
2. **Put the cursor at the start and step forward.** Every step whose tool
   is in the right state replays as before. The first step that needs a
   state the tool is not in stops the cursor: the view shows the state
   before it, the row carries the reason — *habst_tool is spent* — and the
   *Tools* readout says the same.
3. **Insert the recharge in front of it.** With the cursor on the step
   before the failing one and `scene` displayed (the default), click the
   reservoir atom; the offer list shows
   the recharge as its applicable row, because that is what the spent tool
   can do there. Commit. The recharge is inserted after the cursor, the
   readout flips to *charged*, and the step that failed now replays.
4. **Repeat to the end.** Each pass is one click on the reservoir; the
   cursor never has to go backwards, because a recharge inserted at the
   frontier cannot invalidate anything before it, and the workpiece after
   the frontier is unchanged by it — a recharge touches the reservoir and
   the tool, never the workpiece. (A tail that *already* contains recharges
   — a half-converted sequence — can be knocked out of step by an inserted
   one, a recharge on a charged tool being a state error; the walk still
   finds it at the frontier, as one more failing step to delete rather than
   insert in front of.)

The result is the same sequence with recharges interleaved, exact to the
same file rounding, authored in as many clicks as there are recharges. The
`steps` output was valid for a replayer at every moment before the first
failing step and is valid everywhere once the walk ends.

**A sequence that arrives on the `steps` pin** — from `build_script` or a
generator — cannot be edited in place, because the authored block comes
after the wired prefix. Two ways, and which one is right depends on where
the sequence came from:

- **regenerate it.** A generated sequence belongs to its generator; the
  recharges are one more rule there, emitted from the same tool-state
  bookkeeping the library encodes, and the file stays reproducible;
- **adopt it.** For a sequence that is nobody's output any more — an old
  file, a hand-written one — the editor's *Adopt prefix* action (§Follow-ups)
  copies the wired steps into the authored block as exact steps, after
  which the `steps` wire is removed and the walk above applies. Until the
  action exists, the same is done through the text format: the steps are
  pasted into `authored` as literals.

## Loader and exporter nodes

`ops_library`'s panel listing gains a *Tools* section (name, note, state
names, and the frame tags a design must apply) above the operations, and
each operation row shows its method and its tool type or agent. `build_script` and
`export_build_script` follow the seven-field step.

## Text format

Nothing new but the two wire pins, and `build` still means the workpiece:

```
dump  = free_move { input: reservoir, translation: (40.0, 0.0, 20.0) }
build = mechanosynth { base: slab, ops: lib, steps: gen, feedstocks: [dump], tools: [tip], step: 40 }
out   = export_atoms { molecule: build, ... }          # the workpiece alone
view  = apply_style { molecule: build.scene, ... }     # everything
```

The bracket form is accepted and is what a multi-wire pin prints, but the
serializer spells a **single** wire on an array pin without them —
`feedstocks: dump` — which is the format's existing convention for every
array pin and not something these two introduce. A `query` → `--replace`
round trip is therefore a no-op on the serializer's own output and
normalizes a hand-written `[dump]` to `dump` once (Phase 2).

The step literal of an editor's `authored` block loses `method` and gains
nothing.

## Milestone 2, and what this design leaves for it

The animation milestone needs, per step, the tool's parked pose (in the
binding), the scene before and after the step (replayed here), which atoms
belong to which tool (the map), and where the tool sits at the moment of
reaction (the reserved `approach` pose, in the operation's target frame,
which `step.r`/`step.t` place into the design). From those a trajectory can
be interpolated park → approach → park, bulk events can play as one, and
spontaneous steps as settling. Whether that is a node over `scene` and
`step` or a mode of the replayer is that design's decision.

## Reference guide

- `doc/reference_guide/nodes/atomic.md` §mechanosynth: the two new pins,
  the `scene` output, that `result` is the workpiece alone, and that a
  placed node shows `scene`; a new subsection *Tools* — that the library names the tool
  types and the design supplies one molecule each, **how to tag it** (the
  type name, `apex`, the three legs; `atom_edit`'s *Tag selected…*, the
  `tag` node, hover and `label: "{tag}"` to check), that the pose is read
  off the four tagged atoms and never entered, the binding errors,
  workpiece-only replay, the phase rule (`exit_structure` /
  `enter_structure` first), what the panel shows; a new subsection
  *Feedstocks* — that a reservoir is wired to `feedstocks`, appears in
  `scene` tagged `ms_feedstock` and never in `result`, that an atom a step
  adds to it stays with it, and that a recharge is an ordinary step; a new
  subsection *Methods* with the three-kind table, the event rules, that
  events group the display and never the replay, and the statement that a
  step never types its method; *The two files* gains the `/2` formats, the
  `tools` section and the tool side with the example above, the
  invariant-frame rule, the off-axis legs, and the tagging contract;
  *Seeing the build* gains `ms_tool` and says that `ms_added`/`ms_layer`
  stay workpiece-only; *The `step` output pin* gains the three fields and
  the new meaning of `method`.
- §mechanosynth_edit: the pins, that a recharge needs `scene` displayed,
  the tool-atom refusal, the method badge replacing the chip, the text
  format without `method`; a new subsection
  *Two ways to use the editor* — modelling with tools unwired, tool-aware
  authoring with them wired, the tool-blocked rows of the offer list, the
  Tools readout, the last-good-state view, and *Making a sequence
  tool-aware* — the four-step walk along the cursor, and the regenerate-or-
  adopt choice for a wired sequence.
- §ops_library: the Tools listing.
- `doc/reference_guide/nodes/math_programming.md`: `BuildStep` down to seven
  fields; the `MechanosynthStep` field list.

## Testing

Conventions as in the crate `AGENTS.md` files: tests in the owning crate's
`tests/` directory, fixtures under `rust/tests/fixtures/mechanosynth/`
(reached from every crate through `atomcad_test_support::fixture_path`),
synthetic and small, test names as sentences.

**Where each test goes.** Engine tests (parse, pose, binding, replay,
applicability, event rules) in
`crates/atomcad-crystolecule/tests/crystolecule/` — the existing
`mechanosynth_test.rs` and `mechanosynth_place_test.rs` take the edits the
format change forces, and a new `mechanosynth_tools_test.rs` takes the
scene, binding and tool-side tests. Node, evaluator, serialization and
text-format tests in `crates/atomcad-structure-designer/tests/structure_designer/`
— the existing `mechanosynth_test.rs`, `mechanosynth_edit_test.rs` and
`mechanosynth_edit_placement_test.rs` extended, a new
`mechanosynth_tools_node_test.rs` for the pins, the display default and
the participant outputs, and `nodes/node_snapshots_test.rs` and
`text_format_roundtrip_corpus_test.rs` gaining the fixture. API tests in
`rust/tests/structure_designer_api/mechanosynth_api_test.rs`. A test that
asserts something about the viewport — which atom a ray hits, what the
decorator carries — reads `last_generated_structure_designer_scene` after a
refresh, as `error_display_test.rs` does. The node snapshot evaluates pin 0
of every displayed node whatever its pin set, so the display default does
not move a snapshot; a snapshot moves only when `result` does.

**No timing assertions.** "No sweep" is a property of the signature —
`tool_pose` takes the correspondences and nothing else, and binding calls
it once per molecule — not something a test measures with a clock.

**Fixtures.** Every existing library fixture is rewritten to `/2`: `method`
on every operation, and for each `tip` operation a tool side with a type —
the smallest is a `probe` type with a four-entry frame and an empty side,
which is what most existing fixtures get; those fixtures wire no tools, so
the frame is declared and never bound. Every existing build fixture drops
its `method` keys. The `.cnnd` fixtures (`mechanosynth_legacy.cnnd`,
`mechanosynth_wired.cnnd`, `mechanosynth_edit.cnnd`) are re-snapshotted once,
as the first commit of Phase 1, and stay byte-identical afterwards.

New: `tool_ops.json` — a library with two tool types: `habst_tool` (states
`charged`/`spent`) and `probe` (no states), each with a four-entry
**non-coplanar** frame over the shared tags `apex`, `a`, `b`, `c` — the
legs off the axis, as in the example above; operations `habst` (`tip`,
`habst_tool`, gains an H, `charged → spent`), `habst_probe` (`tip`, the same
target side, the `probe` type with an empty tool side), `hdump` (`tip`,
`habst_tool`; the target side adds an H to any bare atom, the tool side
removes it, `spent → charged`), `settle` (`spontaneous`, moves one atom),
`expose` (`bulk`, agent `"X2"`, adds an atom to any bare site), and one
operation with an `approach` pose. `tool_tip.xyz` and `tool_probe.xyz` —
the two tool molecules, each placed rotated and translated away from the
origin; the engine tests tag them programmatically after loading (`.xyz`
carries no tags), and `tool_tip_on_handle.xyz` is the tooltip bonded to a
cluster of a few hundred atoms, tagged the same way. `tool_tip_touching.xyz`
is the tip parked against a workpiece atom of `tool_scene.xyz`, for the off-tool
check. `tool_scene.xyz` — a methane-like workpiece; `tool_dump.xyz` — a small
bare cluster beside it that serves as the dump, a separate structure for the
`feedstocks` pin, and `tool_dump_touching.xyz` the same cluster moved up against
the workpiece, for the across-participants check.

Three things about those three that only came out of building them, and that a
regeneration has to preserve:

- **The off-tool fixture cannot work by parking the apex on a workpiece atom.**
  The apex *is* a frame atom, so the tool's own apex sits exactly at the
  tool-side pattern's first position and always wins the match. The fixture
  instead nudges a **non-frame** atom of the tool (the ethynyl carbon) 0.04 Å
  off its ideal local position and parks the tool so a workpiece carbon is 0.01 Å
  from it: the nearer atom wins, and it is the workpiece's.
- **The across-participants fixture needs a multi-atom `before`.** Every
  operation listed above has a one-atom `before` pattern, and one atom cannot
  span two participants. The library therefore also carries `bridge`
  (`spontaneous`, two `*` atoms 2.5 Å apart, bonding them), and the touching
  cluster sits one bridge length from the workpiece.
- **The reservoir has to be a bonded cluster, and the recharge has to target an
  atom with at least two bonds.** `hdump` adds an atom off the origin from a
  one-atom `before`, so `place` derives its orientation from the host's free
  bonding directions — and an atom with zero or one bond has none to offer, so
  the row is never *offered* (the replay itself is unaffected, since a step
  states its own transform). A loose "bare cluster" or a chain end therefore
  makes every editor test about the recharge vacuous. `tool_build.json` —
`habst` on the workpiece, `hdump` on the cluster, `habst` again,
`habst_probe`, `settle`, two `expose`; **seven keys per step at most**.
`mechanosynth_tools.cnnd` — a slab on `base`, a reservoir tagged `dump` by
a `tag` node on `feedstocks`, the tip and the probe on `tools`, each tagged
with its type by a `tag` node and its four frame atoms by an `atom_edit`,
the node displaying `scene` by default; for `node_snapshots` and
`validation_corpus`.

**Cross-cutting regressions,** run at the end of every phase: the three
re-snapshotted `.cnnd` fixtures evaluate to the same atoms; the text-format
round-trip corpus stays a no-op; the engine, node and API suites pass with
only the edits the format change forces (a `method` assertion removed, a
`/2` format string in a fixture).

## Phases

### Phase 1 — Schema and engine — **DONE**

First commit: the fixture rewrite and re-snapshot above. Then `ToolType`,
`ToolSide`, `Method`, `Operation::{method, agent, tool, approach}`,
`OpLibrary::tools` and the parse rules; `Step` without `method`;
`Participant`, `ToolBinding`, `Scene`, `replay_scene` in both its
all-or-nothing and its partial-result form, `replay` as its wrapper;
`tool_pose` sharing `place()`'s fit; binding; `applicable_ops` over
`bindings` and `ToolReadiness`; the new errors and the `participant`
label; the tag rule across participants; the event rules as a pure
function over a step sequence (for the tests and for milestone 2). Every
engine feature the node layer consumes in Phase 2 is tested here at the
engine level first; Phase 2's tests then cover only the wiring.

*Tests — parse:* a `/2` library with `tools` and tool sides loads and a `/1`
one is refused naming the fix; `method` parses to its kind, and a missing or
unknown kind is `Invalid` naming the operation; a `bulk` operation without
`agent`, an `agent` on a non-bulk operation, a `tip` operation without a
tool side, and a tool side on a `bulk` or `spontaneous` operation are each
`Invalid`; an unknown `tool.type`, a `from` outside `states`, a duplicate
tool name, an empty `states` list and a `*` on an added tool-side atom are
each an `Invalid` naming the location; a tool side with empty patterns loads; a frame with three
entries, without `apex`, with `apex` off the origin, with coplanar entries,
with a repeated tag, or with a tag equal to a type name is each an `Invalid`
naming the type; `approach` parses onto the
operation and a library without it has `None`; a build step with a `method`
key parses with the key skipped; **`BuildStep` has exactly seven fields**
and `tool_build.json` uses no other key.

*Tests — pose and binding:* the frame fitted onto the tagged atoms of
`tool_tip.xyz` recovers the rotation and translation to 1e-9 with
`residual` below `EXACT_FIT_RESIDUAL`, and the same on
`tool_tip_on_handle.xyz` with the same pose; the existing `place()` tests
pass unchanged after the fit is factored out, which is the regression on
the factoring; `tool_pose` with fewer than three correspondences, or with
collinear ones, is an `Err` and never a NaN or an arbitrary rotation — a
mis-tagged molecule whose tagged atoms happen to be collinear is
`ToolPoseResidual`, not a silent bind; a molecule
tagged as the mirror image of the frame fails with `ToolPoseResidual`
rather than binding mirrored; the tip binds to `habst_tool` and the probe to
`probe`, in either wiring order, and with the type tag on the whole
molecule or on the four frame atoms only; a molecule with no type tag is
`ToolUntagged` listing both types; a molecule with both type tags is
`ToolMultiType`; two tips is `ToolDuplicate` naming both instances; a frame
tag missing, or on two atoms, is `ToolFrameTag` naming the tag; a frame atom
displaced beyond tolerance is `ToolPoseResidual` naming the residual;
binding errors are reported before any step is applied, even at `step = 0`;
a type with no molecule is not an error until a step needs it, which is
`ToolMissing` naming the step, the operation and the type; with `tools`
unwired nothing is bound and no tool-side error can occur; the tool tags
survive into `scene` and are absent from `result`; a base carrying
thirty-two tag names with a tool or a reservoir wired is `SceneTags`
naming the molecule, before binding.

*Tests — applicability:* `applicable_ops` with `bindings: None` returns
exactly what it returns today, row for row; with the tip bound and
charged, `habst` on a workpiece H is `ready`; with the tip spent it is
blocked with `ToolReadiness` naming `spent` and `charged`; with the tip
charged, `hdump` on a reservoir atom is blocked by state; with the tip's
apex H removed by hand while the state still says `charged`, `habst` is
blocked by geometry with the nearest-atom reason, which pins that the
geometric check runs even when the label agrees; with no `probe` bound,
`habst_probe` is blocked with a reason naming the tag; `expose` and
`settle` rows carry `tool: None`; ready rows sort above blocked rows and
blocked rows sort with the near misses; the tool check is not run for an
operation that does not fit the clicked atom.

*Tests — partial result:* the partial-result form on a block that fails
at step `k` returns the scene after `k − 1` together with the error, and
that scene equals the all-or-nothing form asked for `k − 1` atom for atom —
**including when step `k` is a `tip` step whose target side would have
matched and whose tool side would not**, which is the case that pins the
all-or-nothing rule and the one a naive two-phase apply gets wrong;
on a block that fails at step 1 it returns the untouched scene; on a block
that succeeds it returns the same scene as the all-or-nothing form and no
error; a binding error yields no scene at all, since nothing was replayed.

*Tests — replay:* the shared assertion **the base is unchanged by the tool
model** — `result` split from `replay_scene` equals `replay(...)` atom for
atom, tags included, for every step of every fixture build that touches no
reservoir, with tools wired and without; for `tool_build.json` with the
dump wired, `result` has the workpiece's atoms and the H `habst` took, and
nothing of the cluster, at every step, and the H `hdump` placed is in
`scene` tagged `ms_feedstock`, in no `result`, and carries no `ms_added`;
a `hdump` step whose `t` lies on the workpiece is an ordinary target-side
rewrite of the workpiece (the base is a reservoir to itself); the dump
wired and `tools` empty replays the whole build with the dump step as a
plain rewrite of the reservoir, which pins that the two pins are
independent; with
`tool_dump_touching.xyz` wired, a step whose matched atoms span the
workpiece and the cluster is `StepAcrossParticipants` naming both, and
the scene is unchanged; the same build with the cluster wired as a
*second* base through `atom_union` and no feedstock replays to the same
scene atom for atom, which pins that a feedstock is nothing but a
participant label; at step 0 every bound tool is in its type's initial state —
`charged` for the tip — and the record's `tool_state` says so after the
first step that uses it; `hdump` as the first step is `ToolState` naming
`charged` and `spent`, because the tip starts charged; after `habst` the tip
has one more H at the apex position transformed by its pose and its state is
`spent`; after `hdump` the dump cluster has one more atom and the tip is
`charged` again; two `habst` in a row is `ToolState` naming the type,
`spent` and `charged`; a tool whose apex region is not what the library
states binds — the frame is on the handle — and fails at the first step whose
tool side *names* the missing atom, with the nearest-atom message, never at
binding; a tip wired carrying cargo that `habst`'s `before` does not name is
**not** caught geometrically at all, which is what the symbolic state is for
(§Symbolic state is a label); a step whose
`before` matches inside a tool molecule is `StepOnTool`; with
`tool_tip_touching.xyz` wired, `habst` is `ToolSideOffTool` naming the
workpiece atom the tool side found, and the workpiece is unchanged;
`habst_probe` leaves the probe unchanged and the record's `tool_type` is
`probe`; a `bulk` and a `spontaneous` step touch no tool; the record's
`method` is `tip` / `bulk` / `spontaneous`, and `tool_type` and `agent` are
the operation's, `""` where absent; tool-side `touched` atoms carry
`ms_current` and the base's previous-step atoms do not; an atom added on
the tool carries **neither** `ms_added` nor `ms_layer`, and the `ms_added`
set of `scene` equals that of `result`; a match failure names `base` or
`tool 0 (habst_tool)`; every atom of the scene has a participant after
every step, added atoms included; a `dump` tag on the reservoir survives
the replay on every atom that survives; asking for one step fewer than a
failing step succeeds; the event rules — two consecutive `expose` with the
same agent are one event, a different agent starts a new one, a `settle`
after `hdump` joins the `hdump` event, a `settle` at index 0 is its own
event; and two `expose` steps **whose first one's product is what the second
one matches** replay to a different outcome in the two orders, pinning that an
event does not commute.

The obvious form of that last test — two exposures on *bonded neighbours* —
does **not** work, and the reason is worth keeping: `expose`'s `before` is one
`*` atom, so the two steps match two different atoms whichever way round they
run and the results are equal. Two steps interact only when one's effect is
inside the other's tolerance, so the fixture chains them: the first exposure's
added atom is the atom the second one matches, and the reverse order finds
nothing there at all.

### Phase 2 — Nodes, records, API — **DONE**

Implemented 2026-09-15. Most of the list below was already true: the
seven-field `BuildStep`, `build_step.rs`, `AuthoredStep` without `method`,
`build_script`, `export_build_script` and the `/2` format string all landed
with Phase 1's commit, because the schema change that removed the field
reached them directly. Four things are worth recording because they are **not**
what the list assumed:

- **The editor's last-good-state hook is the display-override channel that
  already exists.** `EvalOutput::set_display_override(pin, value)` is what
  `motif_edit` uses to draw something other than what the wire carries, and it
  is exactly the shape §mechanosynth_edit asked for — the pins carry the error
  while the viewport shows the last good structure. No new `NodeData` hook was
  needed for the *display* half. What the node does store is the whole `Scene`
  (`#[serde(skip)]`, behind a `Mutex`, the `atom_edit` cache pattern), because
  the placement tool needs the **bindings** and the **participant map**, and
  neither survives the trip through a pin. The replayer parks its scene the same
  way, for the panel's *Tools* and *Feedstocks* readouts — read off the last
  evaluation, never by forcing one, so a panel rebuild costs no replay.
- **A `value` node cannot feed an array pin**, which the node-layer tests ran
  into: `value` declares `DataType::None` and the array merge converts through
  the declared type. The tests wire `import_xyz` nodes with their payload
  written directly instead, which is also the only way to get a *tagged* tool in
  (no `.xyz` carries tags).
- **The phase rule lives in `validate_node_wires`**, keyed on the node type
  name, because it needs the resolved type of two different pins and
  `NodeData::get_data_error` only sees which pins are wired.
- **`steps` survives a block failure**, and the last-good atom count is the
  *editor* API's rather than `get_mechanosynth_info`'s. Both are argued where
  they belong, in §mechanosynth_edit and in the test list below.


The two pins and the `scene` output on both nodes; the
`default_displayed_output_pins` hook, its use in `add_node` and the
display-policy pass, and the serializer's default check against it;
`BuildStep` down to seven fields and `build_step.rs` with it; the record's
three fields and the new meaning of `method`; `AuthoredStep` without
`method`; `build_script` and `export_build_script`; `get_mechanosynth_info`
extended with the tools and the feedstocks; the editor's tool-atom refusal,
tool-aware offers over the bindings, the ghost preview on both output
structures, and the last-good-state block replay with its display hook;
the node-level phase rule on both array pins; the `ops_library` listing
data; FRB regenerated; the `.cnnd` fixture and its snapshot.

*Tests — replayer node:* with `feedstocks` and `tools` wired, `result`
equals the base of `replay_scene`, `scene` has the atom count of base plus
reservoirs plus tools, tool atoms carry `ms_tool`, reservoir atoms
`ms_feedstock`, and no base atom carries either; a single structure wired
to either array pin is one participant; a Molecule wired to `tools` or to
`feedstocks` under a Crystal `base` is a validation error naming the pin,
the wire and `enter_structure`, on the replayer and on the editor alike,
and the same structure through `enter_structure` validates; `scene` has
`base`'s variant; `scene` with nothing wired to either pin equals
`result`; an error reaches all three pins; `step` reports the three
fields with their absent values at step 0; `record_destructure` on a
`BuildStep` yields exactly seven fields; the fixture's `dump` tag reaches
`scene` on the reservoir's atoms and reaches no atom of `result`; a
`.cnnd` whose `mechanosynth_edit` authored steps still carry a `method`
key loads with the key dropped (serde ignores it), which is the file-side
twin of the text format's parse error. *Display default:* a freshly placed
`mechanosynth` and `mechanosynth_edit` node has `{2}` displayed and every
other node type still `{0}`; a network saved with that default writes no
`displayed_output_pins` entry for the node and loads back to `{2}`; a file
with an explicit `{0}` for the node writes the entry and loads to `{0}`;
an old file with no entry for the node loads to `{2}`, and with nothing
wired to the new pins its displayed atoms are the same as before. *(Pin
visibility **is** in the text format — `visible: [scene]` — so the
round-trip corpus prints the new default rather than being unaffected by
it; it round-trips exactly, because both directions name the pin.)*
*API:* `get_mechanosynth_info` reports one tool row
per binding with type, residual and state, and one feedstock entry per
reservoir with its atom count. The last-good structure's atom count is
**`get_mechanosynth_edit_data`'s**, not this one's: `mechanosynth_info`
downcasts to `MechanosynthData` and cannot see an editor node at all.
Both readouts are taken from the scene the node's **last** evaluation
parked, never by forcing one — a panel is rebuilt far more often than a
block changes, and a forced replay per rebuild is the cost this subsystem
least wants.

*Tests — editor node:* the shared assertion *same result, both nodes* holds
with feedstocks and tools wired; a placement on a reservoir atom inserts a
step that replays as a recharge (the tip's state flips back, the H lands on
the reservoir and not in `result`); through
`mechanosynth_edit_anchor_at_ray` on the regenerated scene, a ray at a
reservoir atom returns `None` with only `result` displayed and the atom
with `scene` displayed, and a ray at a workpiece atom returns the same id
under either; offers on a tool atom are refused with the documented
message and nothing is inserted; a base click yields the same step whether
`result` or `scene` is displayed; after `select_preview`, the ghost
visuals are on the decorator of both output structures and the two ghost
lists are equal;
the metadata command has no `method` field; `authored` round-trips without
it. *Tool-aware offers:* with the tip spent, an offer on a workpiece H lists
`habst` as tool-blocked with the reason naming the state, and choosing it is
refused and inserts nothing; the same offer on a dump atom lists `hdump` as
applicable and `habst` blocked; with the tip charged the rows swap; with no
molecule tagged `probe` wired, `habst_probe` is blocked with a reason naming
the tag; `bulk` and `spontaneous` rows carry no tool annotation; with
`tools` unwired no row does, and the offers equal today's. *Last good
state:* with the cursor on a step whose tool side fails, `result` and
`scene` are the same error the replayer node reports on the same block,
the editor's last-good structure (read from the node data's override
hook) equals the state after the previous step atom for atom, the
last-error field carries the message, `mechanosynth_edit_anchor_at_ray`
hits an atom of that structure on the regenerated scene, and moving the
cursor back clears both. *The workflow:* two `habst` authored with
tools unwired evaluate; wiring the tools makes the second fail at cursor 2
and not at cursor 1; inserting `hdump` at cursor 1 makes the whole block
replay; the `steps` output carried all steps throughout.

*Tests — exporter and text format:* `export_build_script` writes `/2` and
seven keys at most per step, and its output re-parses to the same steps; a
`mechanosynth` statement with `feedstocks: [d]` and `tools: [t]`
round-trips through `query` → `--replace`, and `build` in a downstream
statement still resolves to pin 0; an `authored` literal with `method` is a
parse error naming the field; `mechanosynth_tools.cnnd` joins
`node_snapshots`. *(There is no registry snapshot test to grow — the pins
are pinned by the output-pin assertions in `mechanosynth_test.rs`
instead.)*

The fixture is **machine-written**: regenerate it with the `#[ignore]`d
`generate_the_tools_fixture` test beside the others. It tags its one tool
the way a design does — one `tag` node for the type name, four more with a
small `free_sphere` region each for the frame atoms — and stops at step 3,
which covers a tip step, a dump onto the reservoir and the state round
trip. Step 4 is `habst_probe` and would need a second tool molecule for no
extra coverage.

### Phase 3 — Panel — **DONE**

Implemented 2026-09-15. Five things are worth recording because they are
**not** what the list assumed:

- **There is no armed strip**, so the item that put the tool and its state on
  one is void. The editor's armed mode was removed before this design was
  written and this document did not notice; both places that assumed it are
  rewritten above (§mechanosynth_edit, §Tool status). The consequence is that
  the *Tools* readout is the **only** surface that says a recharge is due
  before an offer list does, which is why it is an always-visible line.
- **The method badge needed two fields the API did not have.** `method`
  reached `APIAuthoredStep` in Phase 2 as a lookup of the operation's kind in
  the wired library, but the badge shows the instrument or the agent beside
  the kind, and neither was carried. `APIAuthoredStep` gains `tool_type` and
  `agent`, derived from the same library lookup — three facts about the
  operation, resolved in one place (`OpFacts` in `mechanosynth_edit_api.rs`,
  `frb(ignore)`d: codegen walks every type declared under `api/` and would
  otherwise generate a Dart twin for a local helper).
- **A *ready* tool is not worth a chip on an offer row.** The design asked for
  the tool annotation on the row; at 300 px the row already spends its width on
  a title, a badge and — for a variant — an indent, a rule, an arrow and an
  ordinal. A *blocked* tool takes the badge slot, which is what the design
  wanted it for; a ready one goes into the row's ⓘ beside the library's note,
  because the *Tools* readout is what the user is actually watching for a
  recharge. A blocked row is *dimmed and not placeable*, which is not the same
  as *unselectable* — §Tool-aware offers is rewritten where it said so.
- **The tool-atom refusal message was not Phase 3 work.** It is in the list, but
  the kernel's refusal already reaches the user: `mechanosynth_edit_offers`
  returns it as an error and the viewport shows it on the error snackbar, which
  is the existing surface for a failed placement call. Adding a second surface
  for it in the panel would have said the same thing further from the click.
- **The failing row's error chip keys on `last_good_atom_count`, not on
  `last_error`.** An *input* failure — a bad library, an erroring prefix — sets
  `last_error` too, and it belongs to no row. The discriminator is the one the
  node already draws: a block failure records the scene before the failing step
  (`last_good_atom_count >= 0`), an input failure records `None` because nothing
  ran. The banner at the foot of the panel carries the input failure alone.

The method colours are **literal, not hashed**. The vocabulary is closed and
three words long now, so the hash the editor carried (written when `method` was
a free-text field with a long tail — `probe`, `relax`, …) would have given
`tip` a colour by accident and given one to a library's typo as well. Unknown
kinds get none.

The *Tools* block and the *Feedstocks* line on the replayer, the derived
chips on both panels, the method badge replacing the chip, the tool-atom
refusal message, the Tools listing on `ops_library`; in the editor the
tool-blocked section of the
offer popup with its reasons, the *Tools* readout above the steps list, the
error chip with the last good state in the viewport.

*Tests:* `flutter analyze` clean of new warnings; the rest is thin editor UI
under `feedback_manual_test_for_editor_ui`. One exception earned its tests:
the offer popup already has a harness, and "the popup offered me a step the
kernel then refused" is a correctness bug rather than a cosmetic one, so
`test/mechanosynth_offer_popup_test.dart` gains a *tool-blocked rows* group —
a blocked row sorts below the rule with the near misses, the header counts
what can be **placed** rather than what fits, the reason takes the badge slot,
a click explains instead of placing and the two refusals say different things
(a near miss names the library, a blocked tool names the recharge), a blocked
row still previews in the warning colour, a ready tool is an annotation rather
than a chip, a `bulk` row carries none, and with `tools` unwired every row is
what it always was. The **manual walkthrough**
(human): tag a tool molecule placed by hand — the type on the molecule,
`apex` and three legs in `atom_edit` — and see it bound, with its residual
and state, in the panel; wire it twice and read the duplicate error; move
one leg tag to the wrong atom and read the residual error; scrub across an
abstraction and watch the H appear on the tip; wire a reservoir to
`feedstocks`, scrub across the dump and watch the H leave the tip and land
on the reservoir, then switch the eye from `scene` to `result` and see the
reservoir gone and the workpiece intact; drop a fresh `mechanosynth` node
and see `scene` displayed without touching an eye; move the tool molecule
in the design and see the replay follow it; in the editor, with the tip
spent and `scene` displayed, click a workpiece H and see
the abstraction dimmed with its reason, click the reservoir and see the
recharge offered, commit it and watch the readout flip to *charged*; author
two abstractions with tools unwired, wire the tools, walk the cursor to the
failing step, see the last good state and the chip, insert the recharge in
front of it and watch the block replay; reorder two steps so a tool is used
twice and read the state error. The Flutter smoke test is not run by
agents.

### Phase 4 — Guide and walkthrough

The guide sections above, the screenshot slot, the manual checklist.

## Follow-ups (signatures only)

- **Adopt prefix.** One button on the editor's collapsed prefix block:
  copies the wired steps into the authored block ahead of it, as exact
  steps with their metadata, in one undo command; the user then removes the
  `steps` wire. What turns a generated or loaded sequence into an editable
  one, for the walk in §Making a sequence tool-aware. Deferred because the
  text format already allows it and generated sequences should be
  regenerated rather than adopted.
- **Ghosting the tool side.** The offer preview also ghosts what the step
  does to the tool at its parked pose, so a recharge can be seen before it
  is committed. Small, and separable from the offers themselves.
- **`atom_edit` *Tag as tool…*** (deferred). One action in the Tags section:
  pick a tool type from the wired library (or type its name), then click the
  apex and the three legs in the order the library's frame lists them; the
  action applies the type tag to the whole molecule and the four frame tags
  to the clicked atoms, one undoable edit. Tagging a tool becomes one
  gesture instead of five, and the frame tags cannot be misspelled. Deferred
  because the five-step manual path uses only what exists and the number of
  tools per process is small; worth doing once a second process exists.
- **Event number on the record** (deferred). A derived `event: Int` on
  `MechanosynthStep`, counting events by the §Methods rules over the whole
  script, so a caption can say "dose 2" and a style rule can single out one
  exposure. A readout only; it changes nothing about how a step is applied,
  and it makes event navigation a display change.
- **Event navigation.** The panel's chapter list gains event rows derived by
  the §Methods rules, so a 96-step dose is one row and one scrub stop, and
  the slider's chapter ticks can mark event boundaries.
- **Expose** (deferred, and not implementable as stated). Author a whole
  bulk event in one action: arm a bulk operation, which arms its agent; every
  site the exposure would react at is ghosted with a count; Enter inserts
  the event after the cursor as one undo command, in a canonical order a
  generator would write the same way, while a click on one atom still
  authors one site's share. **Why not yet:** the site list would be "every
  atom where the `before` pattern fits", and a pattern can say what is
  bonded to an atom but not that *nothing else* is — so it cannot tell a
  surface atom from an interior one with the same neighbours, and it cannot
  know that an interior void is not reachable by the gas. An exposure
  authored that way would dose the inside of the crystal. It needs a way to
  state exposure — a "nothing else bonded" clause in patterns, or a surface
  test the engine owns — before it can be trusted; until then bulk steps are
  authored one by one and the animation plays them together.
- **Settle** (deferred). After a commit, offer the spontaneous operations
  that fit at the atoms the step touched — in a strip of its own (there is no
  armed strip to hang it on) with the
  after-state ghosted, applied only on Enter, re-offered after each one so a
  chain is a series of keystrokes, never automatic. The open point is where
  to look: the touched atoms are the obvious neighbourhood, and whether it
  is always the right one is what has to be thought through first.
- **`feedstocks: [HasAtoms]` output** on the replayer — the reservoirs
  after the step, one element per wired reservoir in pin order, split from
  the scene by the participant map exactly as `result` is. Wanted for
  counting what a build took from a reservoir, or chaining a reservoir
  into a second build; deferred because the scene shows it and nothing yet
  consumes it. Same type rule as `result`: each element keeps its input's
  variant.
- **`parts: [HasAtoms]` output** on the replayer — the scene split by
  participant, tools included — when milestone 2's trajectory node wants
  it; the `feedstocks` output above is its reservoir half.
- **Trajectories** (milestone 2): a node or mode interpolating each tool
  from park to `approach` and back, with the scene of the step; animation
  export.

## Considered and rejected

- **A pattern-free tool rewrite; a per-tool offset input; state without
  geometry or geometry without state; `base` as an array; one pin for base
  and tools** — see §Decisions.
- **Finding a tool by searching its atoms for the frame pattern**, with
  elements and `*` in the frame. Retired by *The library envisions the
  tools*: identification was not unique within a crystalline tip nor across
  a bare and a functionalized tip built from one model, and the pose needed
  a sweep, a mirror rule and an ambiguity error that four tagged atoms make
  unnecessary. With it went `chiral` on tool types and the asymmetric-frame
  rule.
- **Feedstocks folded into `base` through `atom_union`, recovered by
  `filter`.** One revision of this document; retired by *Feedstocks have
  their pin*: the atoms a step adds to a reservoir carry no tag, so the
  filter cannot give back the workpiece, and the participant map the tools
  need anyway does.
- **`scene` as pin 0**, to make it the display default for free. Wires and
  displayed-pin sets are persisted by index, so every existing file's
  `result` wires would silently move to the scene, and the text format's
  bare `build` would stop meaning the workpiece. A display-default hook
  costs a few lines and changes no file's meaning.
- **A `tool` index on the step**, in two forms: always present (positional,
  redundant, breaks on rewiring), and "absent means resolve" (nothing left to
  resolve once a type has one molecule). Both retired by *The library
  envisions the tools*.
- **A `target` index on the step**, and its derived twin on the record.
  Positional, and redundant once the match knows where it landed; the
  participant map records it, and the error, the panel and the scene's
  tags report it.
- **Ignoring a second molecule of a type silently.** A mis-wired tip that
  vanishes without a trace is the worst outcome; the error names both.
- **Choosing among several molecules of one type per step.** Not in any
  process this design serves; when it is, a second instance can be a second
  type with its own name.
- **A list of tool sides per operation, resolved by fitting the step's
  tool.** An earlier revision; retired by *One operation, one instrument*.
- **`method` as a step field** (the status quo), **a default method with a
  per-step override**, and **keeping the step field as a deprecated
  duplicate of the operation's kind** (an intermediate revision) — the last
  would have needed a consistency check and a grace period for a file
  format nobody depends on.
- **A library-defined method vocabulary.** The kinds are engine behaviour;
  the library names the instrument through the tool type and the agent.
- **Instrument labels kept as a fourth field** (`sam`, `stml`, …) beside the
  kind. Every use of a label was either the tool type or the agent; a third
  name for the same fact would drift from the other two.
- **Separate structures per participant in the engine** (a `Vec` of
  targets, a `Vec` of tools, an index to choose among them). The merged
  scene matches wherever the step lands, gives the participant map for free,
  and is what the editor and the animation both want to look at.
- **Keeping the `/1` formats loadable.** Not a goal; a `/1` file is refused
  with a one-line fix.
- **Painting one tag per tool index** (`ms_tool_0`, …). Thirty-two slots;
  one class tag plus the participant map serve every use named so far.
- **Storing tool poses per step** for milestone 2's benefit. It puts
  animation state into the build model for a milestone that has not been
  designed.

## Open questions

- Whether `agent` should become a reference to a molecule the animation can
  render rather than a label. A label batches events and captions
  correctly; the animation milestone decides what it needs to draw.
- Whether binding should be validated *before* evaluation as a node-level
  validation error (the node knows its wires, not their contents, so it
  cannot today) — an evaluation error at step 0 is what the design has.
