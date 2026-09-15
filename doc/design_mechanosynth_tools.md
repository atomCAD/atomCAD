# Design: tool molecules in `mechanosynth`

Status: **draft 2026-09-14, revised 2026-09-15, not implemented.** The
revision made four things structural: operations declare their method; one
operation is one instrument; tools are identified by atom tags and posed by
four tagged atoms, so no step names a tool and nothing is searched; and
feedstocks are part of the workpiece, so the node has one `base` pin and one
`tools` pin. **The build script loses a field and gains none.** Backward
compatibility with the files and libraries that exist today is deliberately
not a goal — one colleague uses the feature, the libraries are generated,
and the generator is regenerated with the format.

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
- one appended input pin on `mechanosynth` and `mechanosynth_edit`,
  `tools: [HasAtoms]`, whose elements are **bound to tool types by an atom
  tag** and posed by **four tagged atoms**, one molecule per type;
- the engine replaying one **scene** — the base and the tools merged — and a
  `scene` output pin carrying it for display;
- the tool's pose in the design solved from its own structure, never entered;
- the `step` record reporting the method, the tool type and state, and the
  agent;
- panel readouts; and, in the editor, **tool-aware offers**, a tool status
  readout and a last-good-state view of a failing cursor step, so that a
  tool-aware sequence is authored under guidance rather than by trial.

**Feedstocks need nothing from this design.** They are part of `base` —
§Decisions says how — and a step that draws on one is an ordinary step.

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

### Feedstocks are part of the base

A feedstock is not different in kind from the workpiece: a step rewrites it
with the same operations, through the same engine, at a position its `t`
names. Nothing the node could do with a feedstock — count it, tag it, keep
it out of an output — is anything it does not already do, or that the
network cannot do beside it. So the node has **one `base` pin**, and a
feedstock is whatever part of the base the process draws on.

The practice, with the nodes that exist:

- **A reservoir that is a separate molecule** — a hydrogen dump, a source
  cluster — is placed with the ordinary transform nodes and joined to the
  workpiece with **`atom_union`** before `base`. A step whose `t` lies on it
  matches there; a reservoir running out is an ordinary match failure.
- **A reservoir that is a region of the workpiece** — a sacrificial terrace
  the tool picks atoms from — needs nothing at all; it is already in the
  base.
- **Telling feedstock atoms apart** — to colour them, to count what a build
  took from them, to drop them from an export — is a **`tag`** before the
  union. Tags survive `atom_union`, `apply_style` colours by them, and
  `filter` removes by them. This also names the reservoir, which no index
  ever could.

Rejected: **a `feedstocks: [HasAtoms]` pin** beside `tools`. Considered at
length, and it was the shape of two earlier revisions. It would have carried
a participant tag, a derived "which structure did this step act on" field
on the record, panel rows and its own error for a step spanning two
structures — every one of which restates something a tag or a match already
knows, at the price of a pin, a tag slot, a record field and a quarter of
the tests. The one convenience it offered, not having to add the union node,
is one node in a network that already has a dozen. Also rejected: **`base`
as `[HasAtoms]` with the workpiece first** — it loses the `result` type and
dies on the array system's single-phase rule the moment a Crystal workpiece
meets a Molecule reservoir; `atom_union` has the same rule and the same
answer, convert first.

### The library envisions the tools; the design supplies them, tagged

Tools are the one kind of molecule the network *cannot* fold into the base:
they need a pose, a state that changes with every step, exclusion from
`result`, and a refusal to be placed on. So they have their pin. The
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
was computed for. Four non-coplanar correspondences determine a proper
rotation uniquely, so there is no mirror to consider, no candidate list, no
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
tool type, its state and the state required. The wired molecule has to *be*
in the initial state; the frame atoms do not include the apex's cargo, so
binding cannot check that, and the first tool-side match does, with the
nearest-atom message. The geometric rewrite is applied after the symbolic
check and must match too. When the two disagree — the
state says *charged* and the apex has no atom to give — the geometric
failure is reported, because the geometry is what a viewer sees and the
label is the thing that is wrong.

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
| `bulk` | an exposure of the whole workpiece — a gas, a dose, light; named by the operation's `agent` | one site's share of an exposure | a maximal run of consecutive `bulk` steps with the same `agent` is one **event**; their order within the event carries no meaning, and a generator sorts them for stable files | the event plays as one: the agent arrives everywhere at once, every site of the event reacts together |
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
      { "tag": "a",    "pos": [0, 0, -1.21] },
      { "tag": "b",    "pos": [0, 0, -2.66] },
      { "tag": "c",    "pos": [1.02, 0, -3.15] }
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

**Tag budget.** Type tags, `apex`, the shared leg names and the replay's
own tags all count against a structure's thirty-two names once the tools
are merged into the scene; with a shared leg vocabulary a process with a
handful of tool types uses about ten.

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
        { "id": 3, "el": "*", "pos": [0, 0, -2.66] },
        { "id": 4, "el": "*", "pos": [1.02, 0, -3.15] }
      ],
      "bonds": []
    },
    "after": {
      "atoms": [
        { "id": 1, "el": "C", "pos": [0, 0, 0] },
        { "id": 2, "el": "C", "pos": [0, 0, -1.21] },
        { "id": 3, "el": "*", "pos": [0, 0, -2.66] },
        { "id": 4, "el": "*", "pos": [1.02, 0, -3.15] },
        { "id": 5, "el": "H", "pos": [0, 0, 1.06] }
      ],
      "bonds": [[1, 5]]
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
/// Which structure an atom of the scene belongs to.
pub enum Participant { Base, Tool(usize) }

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
    tools: &[AtomicStructure],
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    tags: HighlightTags<'_>,
) -> Result<Scene, MechanosynthError>;
```

`replay` becomes the no-tool wrapper returning the base, and keeps its
signature, so the editor's `replay_prefix_and_block`, the engine tests and
every other caller compile unchanged.

**Building the scene.** The base is cloned first, so its atom ids are
unchanged; each tool is merged with `add_atomic_structure`, which remaps
their ids and interns their tags, and every merged atom is entered in
`participants`. Then **binding**, per wired molecule: the type tags it
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

**Per step**, in order:

1. **Target side.** `apply_step(&mut scene.structure, op, step)` exactly as
   today, matching anywhere in the scene. The matched `before` atoms must
   all belong to the base — a target side matching inside a tool molecule
   is `Err(StepOnTool)` naming the step and the tool. Atoms the step adds
   belong to the base.
2. **Tool side**, when the operation is `tip` and the `tools` pin is wired:
   the binding of the operation's tool type is looked up — none is
   `Err(ToolMissing)` naming the step, the operation and the type; the
   symbolic check — if `from` is set and differs from the binding's state,
   `Err(ToolState)`; then `apply_step` on the scene with the
   tool-side patterns as an `Operation` and `Step { r: pose.r, t: pose.t }`,
   which by construction matches inside that tool molecule; atoms it adds
   belong to the tool; then the state update, `to` if set.
3. **Highlights.** `touched` from both sides is painted `ms_current`;
   `added` from either side joins `ms_added` and, by the step's `layer`,
   `ms_layer`. An H arriving on the tool is "added" in the same sense as an
   H arriving on the workpiece.

**Workpiece-only replay.** With nothing wired to `tools`, step 2 is skipped
for every step, no binding runs, and no state is tracked. This is not a
compatibility mode but a use: looking at what a build does to the workpiece
without modelling the instruments, which is how a library is developed
before its tools exist. Wiring the pin turns the tool model on for the whole
script.

Failure on either side aborts the replay with the step number in the
message, and the partial state is reachable by asking for one step fewer,
as today. `NoMatch` gains a `participant` label — `base`, `tool 0
(habst_tool)` — so the sentence says where it looked.

**Outputs from the scene.** `scene` is the structure itself, with every
tool atom tagged `ms_tool` from the participant map. `result` is a clone
with every tool atom deleted; base ids are unchanged, so it is atom-for-atom
what `replay` produces, which is the assertion the tests pin.

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
or more correspondences. No sweep, no candidates, no mirror: with the frame
non-coplanar by parse rule the proper rotation is unique, and a molecule
that is the mirror image of the library's simply fails the residual gate.
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
together with the error, rather than the error alone. The replayer node
keeps calling the all-or-nothing form.

### Errors

New `MechanosynthError` variants: at binding, `ToolUntagged`,
`ToolMultiType`, `ToolDuplicate`, `ToolFrameTag`, `ToolPoseResidual`; per
step, `ToolMissing`, `ToolState`, `StepOnTool`; and a `participant` field on
`NoMatch`. Parse-time: `Invalid` with a location naming the tool type or the
operation, for an unknown `tool.type`, a `from`/`to` outside `states`, a
duplicate tool name, a frame with fewer than four entries, no `apex`, an
`apex` off the origin, coplanar entries, a repeated or reserved tag name, a
type name colliding with a frame tag, a missing or bad `method`, a missing
or misplaced `agent`, a missing or misplaced tool side, a `/1` format
string.

## The `mechanosynth` node

**Pins.** Inputs `base`, `ops`, `steps`, `step` unchanged; **appended**
`tools: [HasAtoms]` (pin 4), optional, wire-only. Array pins accept several
wires and concatenate them, so `tools: [w_tip, si_tip]` in the text format
is two wires, and a single structure wired to an array pin is broadcast to a
one-element array by the existing rule. The wires may carry different
phases — a tip model that is a Crystal beside a tooltip that is a Molecule —
because the node's outputs do not take their type from the array's
elements, which is the only case the validator's single-phase rule covers;
a Phase 2 test pins this, since it is what every animation setup needs.

**Outputs.** `result` (pin 0) is the base after the step, same type as
`base`; `step` (pin 1) is the record above. **Appended** `scene` (pin 2):
the merged scene after the step, keeping `base`'s variant the way
`atom_union` does. A `scene` with nothing wired to `tools` equals `result`.
The replay's own tags take four of the thirty-two slots, counting the three
that exist; the tool tags take theirs (§Tag budget).

**Evaluation** evaluates the new pin (`None` → empty), calls `replay_scene`,
splits `result` off the scene, and builds the record from the same clamp. An
evaluation error reaches all three pins.

**Panel.** Below the existing readout, a *Tools* block: one row per wired
tool (index, the type its tag names, pose residual, current state), read
from the evaluated scene through `get_mechanosynth_info`. The current step's
chips show the method kind and the tool type or agent. Nothing is editable —
the pin is wire-only and every field is derived.

**Subtitle** unchanged.

## The `mechanosynth_edit` node

The same input pin appended (pin 3), the same `scene` output appended
(pin 2). `result` stays the base at the cursor. Evaluation replays prefix
and block through `replay_scene` with the same two-replay rule (the prefix
without highlights, the block with `ms_current`), so a cursor at `0` still
shows no highlight on any participant.

**The placement tool keeps its interaction**, and gains one refusal.
It is active when `result` *or* `scene` is the displayed pin; base ids are
the same in both, so a click on the base means the same thing whichever pin
is shown. A recharge is authored exactly like a placement, because the
reservoir is part of the base: click the dump atom, choose the donation. A
click on a tool atom — possible only with `scene` displayed — is answered
with "tools are rewritten by their operations, not placed on" and no offers.

**Nothing new is stored or asked.** `AuthoredStep` loses `method` and gains
nothing. A committed placement inherits `phase`, `layer` and `site` from the
previous step as today and `note` from nothing; there is no tool to choose,
because the operation names its type and the binding names the molecule.
**The `method` chip goes**: the row shows the operation's kind as a
read-only badge, coloured by kind, with the tool type or agent beside it.

**Tool-aware offers.** When tools are wired, `applicable_ops` also answers
"is the tool ready" for every `tip` operation that fits the clicked atom:
the operation's tool type must be bound, its `from` must agree with the
tool's state, and its tool-side `before` must match at the tool's pose —
one nearest-atom match of a few atoms per operation, no search. A row whose
tool is not ready is shown **below the rule with the near misses, dimmed and
unselectable**, with the reason where the residual would be: *habst_tool
is spent*, *no molecule tagged `probe` on the tools pin*. It is not
selectable for the same reason a near miss is not: it cannot be committed,
because its tool side would not match. A `bulk` or `spontaneous` row is
never affected. With tools unwired no row carries a tool annotation, which
is the modelling use exactly as it is today.

The repeat flow needs no change: after a commit the same operation stays
armed, and when its tool is now spent the next pick fails with the state in
the message and opens the offer popup at that atom, where the recharge
operation is the applicable row if the atom is a feedstock atom — so "click
the dump" is the whole instruction.

**Tool status.** The panel's steps list gains a one-line *Tools* readout
above the rows — each bound tool's type and its state at the cursor, *habst_tool
· spent* — refreshed on every cursor move and commit, and the armed strip in
the viewport carries the armed operation's tool and state beside its name.
That is what tells the user a recharge is due before the offers do.

**A failing cursor step shows the last good state.** When the block fails at
the cursor step, `result` is the state after the step *before* it, the
failing row carries the error chip with the engine's message, and the panel
banner names it. Today the whole `result` becomes an error and the viewport
empties, which is right for the replayer — its slider reaches any step — and
wrong for authoring, where the failing step is the one being worked on and
the user needs to see where it stands and click the dump. The replay loop
already knows the structure after each step, so the block replay returns
the last good structure alongside the error instead of discarding it; the
replayer keeps its all-or-nothing policy.

**Every kind is authored the same way in milestone 1**: click an atom,
choose the row. A bulk step is one site's share of an exposure and is
authored one site at a time, exactly like a tip step; a spontaneous step is
chosen from the offer list on the atom it rearranges. The kinds differ in
how the replay groups them and in how the animation will play them — an
exposure's steps together, a settling right after its enabling step — not
in how they are authored. Authoring a whole exposure in one action, and
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
   before the failing one, click the reservoir atom; the offer list shows
   the recharge as its applicable row, because that is what the spent tool
   can do there. Commit. The recharge is inserted after the cursor, the
   readout flips to *charged*, and the step that failed now replays.
4. **Repeat to the end.** Each pass is one click on the reservoir; the
   cursor never has to go backwards, because a recharge inserted at the
   frontier cannot invalidate anything before it, and the geometry after
   the frontier is unchanged by it — a recharge touches the reservoir and
   the tool, never the workpiece.

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

Nothing new but the wire pin:

```
dump  = atom_trans { molecule: reservoir, translation: (40.0, 0.0, 20.0) }
work  = atom_union { structures: [slab, dump] }
build = mechanosynth { base: work, ops: lib, steps: gen, tools: [tip], step: 40 }
```

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

- `doc/reference_guide/nodes/atomic.md` §mechanosynth: the new pin and the
  `scene` output; a new subsection *Tools* — that the library names the tool
  types and the design supplies one molecule each, **how to tag it** (the
  type name, `apex`, the three legs; `atom_edit`'s *Tag selected…*, the
  `tag` node, hover and `label: "{tag}"` to check), that the pose is read
  off the four tagged atoms and never entered, the binding errors,
  workpiece-only replay, what the panel shows; a new subsection *Feedstocks*
  — that a reservoir is joined to the base with `atom_union`, told apart with
  `tag`, and that a recharge is an ordinary step; a new subsection *Methods*
  with the three-kind table, the event rules, and the statement that a step
  never types its method; *The two files* gains the `/2` formats, the `tools`
  section and the tool side with the example above, the invariant-frame rule
  and the tagging contract; *Seeing the build* gains `ms_tool`; *The `step`
  output pin* gains the three fields and the new meaning of `method`.
- §mechanosynth_edit: the pin, the tool-atom refusal, the method badge
  replacing the chip, the text format without `method`; a new subsection
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
`tests/` directory, fixtures under `rust/tests/fixtures/mechanosynth/`,
synthetic and small, test names as sentences.

**Fixtures.** Every existing library fixture is rewritten to `/2`: `method`
on every operation, and for each `tip` operation a tool side with a type —
the smallest is a `probe` type with a four-entry frame and an empty side,
which is what most existing fixtures get; those fixtures wire no tools, so
the frame is declared and never bound. Every existing build fixture drops
its `method` keys. The `.cnnd` fixtures (`mechanosynth_legacy.cnnd`,
`mechanosynth_wired.cnnd`, `mechanosynth_edit.cnnd`) are re-snapshotted once,
as the first commit of Phase 1, and stay byte-identical afterwards.

New: `tool_ops.json` — a library with two tool types: `habst_tool` (states
`charged`/`spent`) and `probe` (no states), each with a four-entry frame
over the shared tags `apex`, `a`, `b`, `c`; operations `habst` (`tip`,
`habst_tool`, gains an H, `charged → spent`), `habst_probe` (`tip`, the same
target side, the `probe` type with an empty tool side), `hdump` (`tip`,
`habst_tool`; the target side adds an H to any bare atom, the tool side
removes it, `spent → charged`), `settle` (`spontaneous`, moves one atom),
`expose` (`bulk`, agent `"X2"`, adds an atom to any bare site), and one
operation with an `approach` pose. `tool_tip.xyz` and `tool_probe.xyz` —
the two tool molecules, each placed rotated and translated away from the
origin; the engine tests tag them programmatically after loading (`.xyz`
carries no tags), and `tool_tip_on_handle.xyz` is the tooltip bonded to a
cluster of a few hundred atoms, tagged the same way. `tool_scene.xyz` — a
methane-like workpiece beside a small bare cluster that serves as the dump,
**one structure**, the dump's atoms tagged `dump` in the `.cnnd` that uses
it. `tool_build.json` — `habst` on the workpiece, `hdump` on the cluster,
`habst` again, `habst_probe`, `settle`, two `expose`; **seven keys per step
at most**. `mechanosynth_tools.cnnd` — `atom_union` of a slab and a tagged
reservoir into `base`; the tip and the probe on `tools`, each tagged with
its type by a `tag` node and its four frame atoms by an `atom_edit`; for
`node_snapshots` and `validation_corpus`.

**Cross-cutting regressions,** run at the end of every phase: the three
re-snapshotted `.cnnd` fixtures evaluate to the same atoms; the text-format
round-trip corpus stays a no-op; the engine, node and API suites pass with
only the edits the format change forces (a `method` assertion removed, a
`/2` format string in a fixture).

## Phases

### Phase 1 — Schema and engine

First commit: the fixture rewrite and re-snapshot above. Then `ToolType`,
`ToolSide`, `Method`, `Operation::{method, agent, tool, approach}`,
`OpLibrary::tools` and the parse rules; `Step` without `method`;
`Participant`, `ToolBinding`, `Scene`, `replay_scene`, `replay` as its
wrapper; `tool_pose` sharing `place()`'s fit; binding; the new errors and the
`participant` label; the tag rule across participants; the event rules as a
pure function over a step sequence (for the tests and for milestone 2).

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
`tool_tip_on_handle.xyz` with the same pose and in the same time (no sweep); a molecule
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
survive into `scene` and are absent from `result`.

*Tests — replay:* the shared assertion **the base is unchanged by the tool
model** — `result` split from `replay_scene` equals `replay(...)` atom for
atom, tags included, for every step of every fixture build, with tools wired
and without; at step 0 every bound tool is in its type's initial state —
`charged` for the tip — and the record's `tool_state` says so after the
first step that uses it; `hdump` as the first step is `ToolState` naming
`charged` and `spent`, because the tip starts charged; after `habst` the tip
has one more H at the apex position transformed by its pose and its state is
`spent`; after `hdump` the dump cluster has one more atom and the tip is
`charged` again; two `habst` in a row is `ToolState` naming the type,
`spent` and `charged`; a tip wired with an H already on its apex binds and
fails at its first `habst` with the nearest-atom message, not at binding; a step whose
`before` matches inside a tool molecule is `StepOnTool`; `habst_probe`
leaves the probe unchanged and the record's `tool_type` is `probe`; a
`bulk` and a `spontaneous` step touch no tool; the record's `method` is
`tip` / `bulk` / `spontaneous`, and `tool_type` and `agent` are the
operation's, `""` where absent; tool-side `touched` atoms carry
`ms_current` and the base's previous-step atoms do not; an atom added on the
tool carries `ms_added` and joins `ms_layer` by the step's layer; a match
failure names `base` or `tool 0 (habst_tool)`; every atom of the scene has a
participant after every step, added atoms included; a `dump` tag on the
reservoir survives the replay on every atom that survives; asking for one
step fewer than a failing step succeeds; the event rules — two consecutive
`expose` with the same agent are one event, a different agent starts a new
one, a `settle` after `hdump` joins the `hdump` event, a `settle` at index 0
is its own event.

### Phase 2 — Nodes, records, API

The pin and the `scene` output on both nodes; `BuildStep` down to seven
fields and `build_step.rs` with it; the record's three fields and the new
meaning of `method`; `AuthoredStep` without `method`; `build_script` and
`export_build_script`; `get_mechanosynth_info` extended with the tools; the
editor's tool-atom refusal, tool-aware offers over the bindings, and the
last-good-state block replay; the `ops_library` listing data; FRB regenerated;
the `.cnnd` fixture and its snapshot.

*Tests — replayer node:* with `tools` wired, `result` equals the base of
`replay_scene`, `scene` has the atom count of base plus tools, tool atoms
carry `ms_tool` and no base atom does; a single structure wired to `tools`
is one tool; a Crystal and a Molecule wired to `tools` together validate
and bind, one type each; `scene` with nothing wired equals `result`; an
error reaches all three pins; `step` reports the three fields with their absent values at
step 0; `record_destructure` on a `BuildStep` yields exactly seven fields;
the fixture's `dump` tag reaches `result` and `scene` on the reservoir's
atoms.

*Tests — editor node:* the shared assertion *same result, both nodes* holds
with tools wired; a placement on a reservoir atom of the base inserts a step
that replays as a recharge (the tip's state flips back); offers on a tool
atom are refused with the documented message and nothing is inserted; a
base click yields the same step whether `result` or `scene` is displayed;
the metadata command has no `method` field; `authored` round-trips without
it. *Tool-aware offers:* with the tip spent, an offer on a workpiece H lists
`habst` as tool-blocked with the reason naming the state, and choosing it is
refused and inserts nothing; the same offer on a dump atom lists `hdump` as
applicable and `habst` blocked; with the tip charged the rows swap; with no
molecule tagged `probe` wired, `habst_probe` is blocked with a reason naming
the tag; `bulk` and `spontaneous` rows carry no tool annotation; with
`tools` unwired no row does, and the offers equal today's. *Last good
state:* with the cursor on a step whose tool side fails, `result` equals the
state after the previous step, the last-error field carries the message,
and moving the cursor back clears it; the replayer node on the same block
reports the error on all its pins. *The workflow:* two `habst` authored with
tools unwired evaluate; wiring the tools makes the second fail at cursor 2
and not at cursor 1; inserting `hdump` at cursor 1 makes the whole block
replay; the `steps` output carried all steps throughout.

*Tests — exporter and text format:* `export_build_script` writes `/2` and
seven keys at most per step, and its output re-parses to the same steps; a
`mechanosynth` statement with `tools: [t]` round-trips through `query` →
`--replace`; an `authored` literal with `method` is a parse error naming the
field; the registry snapshot gains the pin; `mechanosynth_tools.cnnd` joins
`node_snapshots`.

### Phase 3 — Panel

The *Tools* block on the replayer, the derived chips on both panels, the
method badge replacing the chip, the tool-atom refusal message, the Tools
listing on `ops_library`; in the editor the tool-blocked section of the
offer popup with its reasons, the *Tools* readout above the steps list, the
tool and state on the armed strip, and the error chip with the last good
state in the viewport.

*Tests:* `flutter analyze` clean of new warnings; the rest is thin editor UI
under `feedback_manual_test_for_editor_ui`. The **manual walkthrough**
(human): tag a tool molecule placed by hand — the type on the molecule,
`apex` and three legs in `atom_edit` — and see it bound, with its residual
and state, in the panel; wire it twice and read the duplicate error; move
one leg tag to the wrong atom and read the residual error; scrub across an
abstraction and watch the H appear on the tip; union a tagged reservoir into
the base, scrub across the dump and watch the H
leave the tip; move the tool molecule in the design and see the replay
follow it; in the editor, with the tip spent, click a workpiece H and see
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
  that fit at the atoms the step touched — on the armed strip with the
  after-state ghosted, applied only on Enter, re-offered after each one so a
  chain is a series of keystrokes, never automatic. The open point is where
  to look: the touched atoms are the obvious neighbourhood, and whether it
  is always the right one is what has to be thought through first.
- **`parts: [HasAtoms]` output** on the replayer — the scene split by
  participant — when milestone 2's trajectory node wants it.
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
- **A `feedstocks: [HasAtoms]` pin.** The shape of two earlier revisions;
  retired by *Feedstocks are part of the base*: `atom_union` and `tag` do
  everything it did, with names instead of indices.
- **A `tool` index on the step**, in two forms: always present (positional,
  redundant, breaks on rewiring), and "absent means resolve" (nothing left to
  resolve once a type has one molecule). Both retired by *The library
  envisions the tools*.
- **A `target` index on the step**, and its derived twin on the record.
  Positional, and redundant once the match knows where it landed; with
  feedstocks in the base there is nothing for it to name.
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
