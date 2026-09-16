# Design: tool trajectories in `mechanosynth`

Status: **draft 2026-09-16, revised three times after review** — the
approach became a collision-free sweep rather than a surface-normal guess; a
tool was allowed to fly from one site straight to its next instead of
returning to park between them; and the engine was split into a feasibility
layer the sequence generator shares and a presentation layer that cannot
fail (§Architecture). The testing story was then reviewed against the
repository's harnesses and fixtures, which moved two test files to the
harness their imports allow and made the frame's axis sign a property of
the frame rather than a rule that would have flipped every fixture
(§Testing). Unreviewed since. Nothing of it is implemented.

Builds on `doc/design_mechanosynth_tools.md` (milestone 1: what a build does
to every molecule it involves, all four phases implemented 2026-09-15) and is
the **first half of milestone 2** of that design. The second half — a real
clock over a whole build, and animation export — is *not* designed here; §The
clock milestone 2 will need says what this half hands it. This design retires
milestone 1's reserved `approach` field; §The operation library says why.

## What this is

Milestone 1 made a step a **discrete change**: the scene before it and the
scene after it, and nothing in between. The tool molecules sit at their parked
poses and are rewritten in place, so a hydrogen abstracted from the workpiece
vanishes there and appears on a tip nine ångström away.

This design makes a step a **process in time**. Every step owns a unit interval
of *step time*, `time ∈ [0, 1]`, and the scene is defined at every point of
it: for a `tip` step the tool descends onto the site along a direction the
engine has checked for collisions, the reaction happens while it is there, and
the tool lifts off and flies — to its next site if it has one coming, to park
if not; for every other kind the scene switches from before to after partway
through, with a tool that is between two of its own sites hovering over the
next one. A new `time` control on the `mechanosynth` node — a stored property
with a live slider, and an overridable `time: Float` pin — selects the point.
Dragging it moves the tool through the viewport, and the workpiece flips from
its old state to its new one at the moment of reaction.

**That is the whole deliverable**: one scrubbable dimension inside a step. It
is not an animation, and it needs no player, no clock and no export to be a
demonstration: a hand on the slider is the demonstration.

## Scope and non-goals

In scope:

- a **`time`** property and appended `time: Float` pin on `mechanosynth`,
  clamped to `[0, 1]`, default `1.0`, so that every existing project and every
  existing test sees exactly milestone 1's workpiece;
- two library facts: a **reaction point** per `tip` operation, on the target
  side in the operation's frame and on the tool side in the tool's frame, and
  a **collision envelope** per tool type — a cone continuing into a cylinder
  about the tool's axis;
- the engine **visit plan** per `tip` step: the tool placed so that its
  reaction point coincides with the target's, its axis along the
  **approach direction** found by sweeping the envelope over the scene, a
  standoff point above the site, and a path **from wherever the tool is** —
  park, or the standoff it reached at the end of its previous visit — down to
  the reaction pose and on to **wherever it goes next** — its next standoff,
  or park;
- **runs**: a tool that has another visit coming, with only `spontaneous`
  steps in between, flies straight to that visit's standoff and hovers there
  through the steps between;
- the **scene at a step time**: the scene after `step − 1`, with the step's
  rewrite applied iff `time ≥ 0.5`, and the moving or hovering tool's atoms
  at its pose at `time`;
- an **error** when no collision-free approach direction exists, and a
  **clearance scan** of every flight, reported on the panel and the
  record;
- the feasibility half exposed by `atomcad-crystolecule` — the pure sweep,
  the landing, and the step checks without the apply — so that a sequence
  generator refuses an unreachable step before emitting it, and gets that
  refusal for free from the apply it already calls;
- four fields appended to `MechanosynthStep`: `time`, `tool_r`, `tool_t`,
  `clearance`;
- a **wireframe cage of each tool's envelope** in the viewport, following
  the tool, switched on and coloured in the preferences (off by default),
  through a general overlay route from `EvalOutput` to the wireframe pass;
- the panel's time row and readout lines; the reference guide, including the
  layout a demo wants (tools parked behind the workpiece, away from the
  camera).

Out of scope, each with a home in §Follow-ups or §The clock: a real-time clock
over the build and per-operation durations in use; a play button; animation
export; bulk events arriving "everywhere at once"; routing a flight
around obstacles; two tools away from park at once; a drawn path overlay;
`mechanosynth_edit`, which keeps its cursor and gets no time.

## Decisions and the alternatives they replace

### Step time is a fraction, and the node has two knobs

`time` is the fraction of the current step that has elapsed. `step = k,
time = u` means: the first `k − 1` steps applied, step `k` in progress at
`u`. At `u = 1` this is milestone 1's `step = k` for the workpiece, the
reservoirs and the record — atom for atom, positions included — which is the
compatibility assertion the tests pin; the engine's scene is milestone 1's
too, tools included, and only what a viewer is *shown* differs, because
milestone 1 drew the tools parked and this design draws them where they are
(§A tool leaves park once per run, §Architecture). At `u = 0` it is
`step = k − 1` with step `k`'s tool where it starts its visit.

Rejected: **real time** — a `duration` on each step and a pin in seconds.
Nobody knows the number: how long a tip visit takes depends on the instrument,
and a library that wrote `3.0 s` would be inventing it; the stored property
would change meaning whenever the library changed a duration; and the slider
needs none of it, since within one step the interesting quantity is *how far
along* the visit is. A real clock is wanted only to concatenate steps, and
that is the animation half of milestone 2 (§The clock), which will drive these
two pins from its clock rather than replace them. `duration` is nevertheless
**reserved on the operation** now (§The operation library), because when the
clock comes it belongs to the researched entity, not the step.

Rejected: **one global progress float** (`12.35` = step 12 at 35 %) in place
of `step` plus `time`. It folds the integer the record reports, the scrubber
scrubs and every existing file stores into a float, for no gain: a downstream
network that wants one clock builds it from an `expr` in two lines
(`step = floor(t) + 1`, `time = t − floor(t)`), and milestone 2's driver will
do exactly that.

### The reaction is at the middle of the dwell

The visit's timeline, in step time:

| interval | the tool | the scene |
|---|---|---|
| `[0.00, 0.45)` | **inbound**: flies from park to the standoff if it is not already there, then descends | before the step |
| `[0.45, 0.50)` | at the reaction pose | before the step |
| `[0.50, 0.55]` | at the reaction pose | **after the step** |
| `(0.55, 1.00]` | **outbound**: ascends to the standoff, then flies to the next visit's standoff or to park | after the step |

The rewrite is instantaneous and always will be — the engine has ideal
geometry and no transition states, by the module's founding rule. Putting the
instant in the middle of a dwell gives the slider a landed-but-unreacted state
and a reacted-but-not-departed one, and gives the reaction a moment a future
player can pause on. `0.5` is also the switch for `bulk` and `spontaneous`
steps and for a `tip` step whose tool is unwired, so "before or after" means
one thing for every kind.

Rejected: **switching at landing** (`0.45`). It loses the landed-before
frame, which is the one that shows *why* the tool is there.

### A tool leaves park once per run

A tool's **run** is a maximal sequence of its own `tip` steps in which every
step between two consecutive ones is `spontaneous`. A `bulk` step or another
tool's `tip` step ends a run. Within a run the tool never returns to park:
after each reaction it ascends to its standoff and flies — rotating and
translating together — to the standoff of its next visit, waits there through
whatever settles the crystal does in between, and descends when its next step
comes. It leaves park on the first step of the run and returns after the
last. The silicon shuttle's pickup flows into its donation in one arc, the
donation's settles pass under a hovering tool, and the tool crosses to the
reservoir for the next pickup without touching park.

Two facts make this cheap and honest:

- **At most one tool is away from park at any step.** A second tool's run
  cannot begin inside the first's, because its `tip` step would end the
  first run. So the engine never has to ask whether a hovering tool is in
  another tool's way: while a tool hovers, nothing else moves, and the sweep
  that cleared its standoff saw every other tool at park, which is where they
  are.
- **The next standoff is a function of the scene before the next visit**,
  which is the scene after this step plus the settles between — the engine
  has it by applying those steps to a clone. The standoff a tool flies to at
  the end of step `k` and the standoff it descends from at step `j` are the
  same number computed the same way, so the boundary between steps is
  continuous. If planning the next visit fails — its match, its sweep, its
  envelope — the tool returns to park instead, and the failure is reported
  when the replay reaches that step, as it would have been anyway.

The pose at `(k, u)` therefore depends on the scene after `k − 1`, step `k`,
the binding, and the **run structure** around `k` — which is the script and
the library, not a stored state — so it stays computable from the replay the
node already does, undoable through the property it already has, and
**stable under reordering**: a moved step is planned against the scene it
now follows and joins whatever run it now sits in.

Rejected: **returning to park after every visit.** Simpler by the one
look-ahead it saves, and thirty-one round trips across the demo scene for a
tool that never needed to go home; the animation this design exists to make
possible is the one where the tool works.

Rejected: **hovering across a `bulk` step**, or across another tool's visit.
The first is an exposure the instrument would retract from anyway; the second
would put a hovering tool where the other tool's sweep did not look for it.
Both would trade the one-tool-away invariant for a second collision model.

Rejected: **storing the approach in the step.** The direction depends on what
is in the way, and what is in the way depends on every step before this one.
A reordered or edited sequence would carry directions computed against a
scene that no longer exists. The generator and the node both recompute; the
generator's reason to compute at all is to refuse a step that has no
direction (§The sweep).

### The tool is placed by two reaction points

A `tip` operation states **where the reaction happens, twice**: a
`reaction` point on the **target side**, in the operation's local frame, and
one on the **tool side**, in the tool's local frame. The tool is posed so
that the two coincide in design space. For a donation both are the transferred
atom's position — where the workpiece will have it, where the tool holds it;
for an abstraction, where the workpiece has it and where the tool will hold
it; for a bare probe that touches (STM lithography, a push) the target point
is the atom acted on and the tool point is the apex offset by a contact
distance, a number the library author states like every other geometry.

That fixes the tool's position and leaves its orientation. The transferred
atom therefore does not move in space at the reaction: it changes hands.

**The tool axis is the frame's `z` axis, pointing from the business end
toward the legs.** Every tool in the silicon library has this shape — apex at
the origin, the legs at positive `z`, the cargo on the negative axis, the
probe's body opening upward — and so do milestone 1's ethynyl example and
the test fixtures, mirrored: legs at negative `z`, cargo at positive. The
frame rule states the shape and reads the sign from it: **every leg lies on
the same side of the apex's `z = 0` plane**, none on it, a parse error
otherwise because the envelope and the sweep depend on it; the axis sign is
the legs' side. No library has to flip anything.

Rejected: **deriving the reaction point from the handoff atoms** (the
centroid of what the target side adds or deletes, the centroid of the tool
side's cargo). It is exact for every loaded tool and undefined for a bare
probe, whose contact distance is a fact the library has to state anyway; two
rules where one field serves. The library is where things are designed in
detail.

Rejected: **deriving the axis direction from the frame legs** (leg centroid
to apex). Exact for the cage tools, whose three legs are symmetric about the
axis, and wrong by twenty-five degrees for the W probe, whose three legs are
three of the four neighbours of a bcc apex. Only the *sign* is read from the
legs, which every frame gets right; the direction is the frame's `z`.

### The envelope: a cone into a cylinder, so the roll is free

Every tool type declares a **collision envelope** in its own frame: a cone
with its apex at the reaction point, half-angle `α`, opening along the tool
axis toward the legs,
continuing as a cylinder of radius `R` once the cone has grown that wide.
The envelope contains the whole tool — and, when the library says so, more
than the wired molecule: a tooltip's `R` can be the radius of the tip shaft
the design does not model, so that an approach avoids what the real
instrument would hit.

Because the envelope is a solid of revolution about the tool axis, the tool's
**roll** about that axis cannot matter to collisions, and the orientation
search is over **directions**: the unit vector `d` the axis points along
once the reaction points coincide. The tool arrives along `−d` and leaves
along `+d`; the roll is chosen to move the tool as little as possible from
the orientation it flies in with, so it never twirls.

Numbers, for orientation: the TLM cage tools fit `α = 30°`, `R = 4 Å` with a
loaded silicon tool's reaction point at the cargo; the bcc tungsten pyramid is
`α = 55°` (its faces) with `R` the tip model's half-width. Both are the
library's to state.

**Containment is checked, not assumed.** At plan time every atom of the bound
molecule is tested against the envelope at the step's reaction point; an atom
outside is an error naming the tool type, the atom and the operation — a
library that claims a smaller envelope than its molecule is wrong the way a
frame whose residual fails is wrong. The check costs one pass over the tool's
atoms.

### The sweep: the approach direction is found, and its absence is an error

The **obstacles** are every scene atom that is not the visiting tool's and not
one of the target side's matched `before` atoms — the site is the reaction, not
an obstacle. Each obstacle is a sphere: its covalent radius plus the largest
covalent radius among the tool's atoms, times the library's clash factor
(`CLASH_BLOCK`, 0.9), so "inside the envelope" means what "clash" means
everywhere else in the module.

For a direction `d`, an obstacle at `p` has axial coordinate
`s = (p − p_r) · d` and radial distance `ρ` from the axis; the envelope's
radius at `s` is `0` for `s < 0` and `min(s · tan α, R)` after. The
**clearance** of `d` is the minimum over obstacles of `ρ − radius(s) −
margin`, obstacles with `s < −margin` skipped (they are behind the apex).
Positive clearance is a free direction.

The **preferred direction** is global `+z`. A scanning probe is vertical
unless something forces a tilt, the workpiece's top face is the `xy` plane
by every convention of this application, and a tool that has to tilt should
tilt as little as possible from vertical, whatever way it happened to be
parked.

The search walks candidates **in order of tilt from `+z`** and stops at the
first that is free, so the common case costs one test and a slightly
blocked site costs a few:

1. the candidates are the `SWEEP_DIRECTIONS` (256) points of a Fibonacci
   sphere **generated from the north pole down**, so that index order is
   tilt order: candidate 0 is `+z` itself, the next few are within a few
   degrees of it, and the south pole is last. The sequence is fixed, so the
   same scene gives the same direction every time;
2. walk the candidates; the first whose clearance is at least
   `CLEAR_MARGIN` (0.5 Å) is taken. Along the way the direction with the
   largest clearance so far is remembered;
3. refine by bisecting the great-circle arc from the chosen direction toward
   `+z`, eight steps, keeping the last direction whose clearance held — so
   the tilt is the smallest the obstacles allow, not the coarsest sample of
   it. When candidate 0 was taken there is nothing to refine;
4. if no candidate reaches `CLEAR_MARGIN`, take the remembered best if its
   clearance is positive; otherwise **`NoApproach`**, an error naming the
   step, the operation, the tool type and the best clearance found. The
   replay stops there as for any other failing step, and the partial state
   is reachable by asking for one step fewer.

The cost is one dot product and one subtraction per obstacle per direction,
and the walk stops at the first free candidate: an unobstructed site costs
one pass over the obstacles, and even the worst case — two thousand atoms and
every one of the 256 directions — is under a million operations,
milliseconds. The infinite cylinder means no obstacle can be skipped by
distance in general; a grid-backed cylinder query is the optimisation to
reach for if a scene ever makes this the slow part, and it is not designed
here.

**The generator gets the same answer, from the same code.** The silicon
generator applies every step to a scene as it emits it, and the sweep runs
inside that apply (§Architecture), so a site that has become unreachable —
under an overhang, beside a parked tool — fails at emission with the step
number, exactly as a pattern that does not match does. For a generator that
wants to ask before committing, the sweep is also public on its own:
`approach_direction` takes an envelope, a reaction point and an obstacle
list and returns the direction and its clearance, or `None`. That is the
design's answer to "what if no direction exists": the generator does not
emit the step, and sequences differently.

Rejected: **an approach direction from the local surface normal** (the
centroid of the neighbourhood around the site). It is a guess about free
space; the sweep measures it. Also rejected: **an explicit full pose in the
library** (milestone 1's reserved `approach`). It bypassed the sweep, and a
pose computed for one scene is wrong in the next; the two reaction points
carry everything a computed reaction geometry has to say about position, and
the orientation is the scene's. Also rejected: **a physically simulated
approach** (relaxing along the way). Every step would cost a relaxation per
sample for a curve nobody has asked to see.

### The path: fly to the standoff, descend along the approach

A visit has a **from** pose and a **to** pose. `from` is park on the first
step of a run and the visit's own standoff otherwise; `to` is the next
visit's standoff inside a run and park at its end. Between them:

- the **standoff point** `S = p_r + d · H`, with
  `H = max((t_park − p_r) · d, MIN_STANDOFF)`, `MIN_STANDOFF = 6 Å`: the
  point on the approach axis at the height the tool was **parked at**,
  measured along `d`, never closer than six ångström. The park height is
  the height every flight happens at, so parking the tools at one height
  keeps every leg level;
- **inbound flight**, when `from` is park: a straight line from `P` to `S`,
  the orientation slerping from the parked one to the reaction one on the
  way. Absent when the tool already hovers at `S`;
- **descend**: from `S` to the reaction pose `A` along `−d`, orientation
  fixed;
- **dwell**: at `A` for `[0.45, 0.55]`, reaction at `0.5`;
- **ascend**: back to `S` along `+d`;
- **outbound flight**: a straight line from `S` to `to` — the next standoff,
  with the orientation slerping to the next reaction one, or park, slerping
  back to the parked one.

**Every descent is collision-free by construction**: the envelope translated
along its own axis by `H` lies inside the envelope at the reaction point, and
the sweep cleared that. The flights are not, and are not routed: each is
**scanned** — sampled every `PATH_SAMPLE` (0.5 Å) of translation and five
degrees of rotation, each tool atom checked against the scene atoms within
`CLASH_SEARCH_RADIUS` that are not the tool's, with the module's
covalent-radius ratio. The worst ratio over the visit's flights, its time and
its pair are the visit's **clearance**, reported on the panel and the record;
below the clash factor a flight *collides*. It is a report because a
collision on a flight is the layout — a tool parked on another's line, a park
too low — and the fix is to re-park, which the user sees at once; the
approach, which the user cannot fix by hand, is what the engine solves.

Time is allotted by **path length within each half**: the inbound legs share
`[0, 0.45)` in proportion to their lengths and the outbound legs `(0.55, 1]`
likewise, so within a half the tool moves at one speed; each leg is eased
with a smoothstep so the corner at `S` does not snap. A chained visit, whose
inbound is one descent, spends its whole first half descending, slower than
a first visit's descent; the halves are fixed so that `0.5` stays the
reaction for every step, and how long a step lasts on a real clock is the
clock's business (§The clock).

Rejected: **the straight line from park to site.** A tool parked thirty
ångström away and nine above descends at seventeen degrees and sweeps its body
through the passivation layer beside the site. Also rejected: **a fixed
standoff height** independent of the park. Six ångström above the site is
below the parked apexes in the silicon demo, so a tool flying at that height
passes *under* its neighbours with its cage through their apex; the park
height puts it beside them. Also rejected: **routing a flight around
obstacles.** It is where a path planner begins, and a re-park solves every
case in a one-camera scene; §Follow-ups names the shape it would take.

### Only `tip` steps move; everything else gates — and a run may hover

A `bulk` step, and a `tip` step with `tools` unwired, has no visit: the scene
is *before* for `time < 0.5` and *after* from `0.5`, every tool at park. A
`spontaneous` step gates the same way, and a tool whose run spans it
**hovers** at its next standoff throughout — it arrived there at the end of
the previous visit and leaves at the start of the next. The animation half of
milestone 2 owns what an exposure looks like; this half only promises that
"before or after" means the same thing for every kind. With `tools` unwired
no sweep runs either — the workpiece-only replay stays the way a library is
developed before its instruments exist.

### `mechanosynth_edit` keeps its cursor

The editor authors steps; its cursor is a step boundary and its picks land on
the scene at that boundary. A time on the editor would have to teach the hit
test that a tool's atoms are somewhere other than where the scene says, for a
view the author has no reason to want while placing. The replayer is where a
build is watched; the editor's steps flow into it by wire — and get their
approach there, recomputed, with nothing written into the authored block.

## The operation library

The format string becomes **`atomcad-msops/4`**: `envelope` is required on
every tool type, `reaction` on every `tip` operation, and a `/3` file is
refused with a message saying to regenerate it, as every bump before it was.
The parser keeps skipping unknown keys, which is how `approach` — parsed by
`/3`, written by nobody — goes: it is removed from `Operation` and the
schema, and a file that still carries it loses nothing.

### The tool type

```json
{
  "name": "si_tool",
  "note": "O3Si radical on a TLM cage: silicon pickup, delivery and the insertion push.",
  "states": ["bare", "si_loaded"],
  "frame": [
    { "tag": "apex", "pos": [0, 0, 0] },
    { "tag": "a",    "pos": [ 1.03447,  1.02551, 0.515] },
    { "tag": "b",    "pos": [ 0.37089, -1.40863, 0.515] },
    { "tag": "c",    "pos": [-1.40535,  0.38312, 0.515] }
  ],
  "envelope": { "half_angle": 30.0, "radius": 4.0 }
}
```

| key | meaning |
|---|---|
| `frame` | as before, with one new rule: every entry but `apex` has `z` of one sign, none zero. The tool axis is `z` with that sign, the business end the other way. Parse error otherwise, naming the type and the tag |
| `envelope` | required. `half_angle` in degrees, `(0, 90)`; `radius` in ångström, positive. The cone's apex is at the operation's tool-side reaction point, its axis the tool axis; it becomes a cylinder of `radius` where `s · tan(half_angle)` reaches it. Parse error when missing or out of range, naming the type |

The envelope is the library's claim about the **instrument**, not the
molecule: a tooltip bonded in reality to a tip the design does not wire
declares the shaft's radius, and the sweep keeps the shaft out of the
workpiece.

### The `tip` operation

```json
{
  "name": "si_donate_dimer",
  "method": "tip",
  "before": { … }, "after": { … },
  "reaction": { "target": [0, 0, 0], "tool": [0, 0, -2.352] },
  "tool": { "type": "si_tool", "from": "si_loaded", "to": "bare", "before": { … }, "after": { … } }
}
```

| key | meaning |
|---|---|
| `reaction.target` | required on `tip`; a point in the operation's local frame, placed into the design by `step.r` / `step.t` like a pattern atom. Where the reaction happens on the workpiece: the transferred atom's position, or the atom a bare probe touches |
| `reaction.tool` | required on `tip`; a point in the tool's local frame, on or near the axis on the business-end side: the cargo's position, or the apex moved `contact` along the axis away from the legs, for a probe that touches. The envelope's cone apex sits here for this operation |
| `duration` | optional, any operation; positive, relative units, default `1.0`. Parsed and kept, **read by nothing** in this design; reserved for the clock half of milestone 2 so a generator can start writing it. Zero or negative is a parse error naming the operation |

`reaction` on a `bulk` or `spontaneous` operation is a parse error, as a tool
side is. Both points are written per operation, not per environment variant,
so five `si_donate` variants carry five identical `reaction` blocks — the
same repetition the tool side has, written once by a generator.

Nothing else. No per-type approach direction (the sweep finds it), no
standoff height and no speed: engine constants until a second process shows
they need to be a library's.

## The build script

Unchanged: **`atomcad-msbuild/2`**, a step stays `op`, `t`, `r`, `note`,
`phase`, `layer`, `site`. A step carries no direction, no time, no pose and
no run membership, for the reasons in §A tool leaves park once per run.

## Architecture

Two layers, with one rule between them: **what can fail a build lives with
the replay; what only moves pixels lives beside it and cannot fail.**

| layer | contains | who calls it | can it fail a step? |
|---|---|---|---|
| **feasibility** | reaction points, the envelope, obstacle collection, the sweep, the landing (direction, reaction point, standoff height), containment | `apply_step_in_scene` — so the replayer, the editor's block replay and the **sequence generator** all get it by applying a step, and the generator can also ask without applying | yes: `NoApproach`, `ToolOutsideEnvelope` |
| **presentation** | runs, the reaction and standoff poses with their roll, the flights, the hover, the scan, the pose at a step time | `replay_scene_at` for the node; a future exporter | never — the scan is a report |

Three consequences shape the code:

**Feasibility is part of applying a step.** `apply_step_in_scene` plans the
landing of every `tip` step whose tool is bound, between the checks and the
mutation, and returns it on `SceneEffect`. The generator therefore learns
that a site is unreachable the way it learns a pattern does not match — its
emit-and-apply loop fails with the step number — and changes nothing to get
the check. For a generator that wants to *try* a site before committing to
it, the same two halves are public separately: `match_step_in_scene` (the
checks, returning the plan the apply consumes) and `plan_landing`. One code
path, three entry points.

**The engine's `Scene` is never moved.** A `Scene` always has its tools at
their bound poses, because that is what the tool-side match and the sweep
assume, and the type keeps it so: nothing in the engine writes a flown pose
into a `Scene`. `replay_scene_at` returns the scene *and* a `ToolMotion`,
and whoever draws the scene — the node, an exporter — applies the pose to
its own copy with `apply_tool_pose`. There is no "do not replay into a moved
scene" rule to remember because there is no moved scene to replay into.

**The pure sweep has no scene.** `approach_direction` takes an envelope, a
point and a list of obstacle spheres. It is what a generator calls when the
scene it holds is not the engine's, or when it searches over candidate sites
and wants the cheapest possible question. `obstacles_for` is the one
function that decides what an obstacle is — which atoms, which margin — so
that the generator and the node never disagree about it.

What the generator imports, all re-exported from `mechanosynth`:
`Envelope`, `Approach`, `approach_direction`, `sweep_directions`,
`obstacles_for`, `Landing`, `plan_landing`, `match_step_in_scene`,
`apply_step_in_scene` (unchanged signature, `SceneEffect` gains `landing`),
`runs` / `Runs`, and the constants. It imports nothing from the
presentation layer, and the presentation layer imports nothing from the
generator.

## Engine

New module `rust/crates/atomcad-crystolecule/src/mechanosynth/trajectory/`
with `envelope.rs` (pure geometry: `Envelope`, the sweep), `landing.rs`
(the scene-side feasibility: obstacles, containment, `plan_landing`),
`runs.rs` (script-only run structure) and `path.rs` (presentation: poses,
flights, hover, scan, `replay_scene_at`). `scene.rs` gains only the split of
`apply_step_in_scene` into `match_step_in_scene` and the apply half.

### Feasibility

```rust
/// A tool type's collision envelope: a cone of `half_angle` (radians here)
/// about the tool axis with its apex at the tool-side reaction point, continuing as a
/// cylinder of `radius`. On `ToolType`.
pub struct Envelope { pub half_angle: f64, pub radius: f64 }

impl Envelope {
    /// The envelope's radius at axial coordinate `s` from its apex: `0`
    /// behind the apex, `min(s · tan α, radius)` ahead of it.
    pub fn radius_at(&self, s: f64) -> f64;
    /// The clearance of direction `d` at `at` against `obstacles`: the
    /// smallest `ρ − radius(s) − margin`, obstacles behind the apex skipped.
    pub fn clearance(&self, at: DVec3, d: DVec3, obstacles: &[(DVec3, f64)]) -> f64;
}

/// What the sweep found for one direction.
pub struct Approach {
    pub direction: DVec3,
    /// The smallest margin by which an obstacle clears the envelope, Å.
    pub clearance: f64,
    /// Angle from global `+z`, radians.
    pub tilt: f64,
}

/// The `SWEEP_DIRECTIONS` unit vectors of the sweep, from `+z` down to `−z`,
/// in order of increasing tilt. Computed once (`LazyLock`).
pub fn sweep_directions() -> &'static [DVec3];

/// The collision-free direction of least tilt from `+z` for a tool whose
/// reaction point sits at `at`. `obstacles` are design-space positions with
/// their margins. `None` when no sampled direction is free. **Pure
/// geometry** — no scene, so a generator can call it on whatever it holds.
pub fn approach_direction(
    envelope: &Envelope,
    at: DVec3,
    obstacles: &[(DVec3, f64)],
) -> Option<Approach>;

/// The one definition of an obstacle: every scene atom that is not the
/// tool's and not in `exclude`, with margin `clash · (r_cov(atom) +
/// r_tool)`, `r_tool` the largest covalent radius among the tool's atoms.
pub fn obstacles_for(
    scene: &Scene,
    tool: usize,
    exclude: &[u32],
    clash: f64,
) -> Vec<(DVec3, f64)>;

/// Where one `tip` step's tool reacts and from which direction — the
/// feasibility half of a visit, with no roll and no path in it.
pub struct Landing {
    pub tool: usize,                 // index into `scene.bindings`
    pub reaction_point: DVec3,       // `p_r = step.r · reaction.target + step.t`
    pub approach: Approach,
    /// `max((t_park − p_r) · d, MIN_STANDOFF)`, Å.
    pub standoff_height: f64,
}

/// Plans the landing of `step` on `scene` — the scene *before* the step —
/// from the plan `match_step_in_scene` produced: the reaction point placed,
/// containment checked, the sweep run, the standoff height fixed. `Err` for
/// `NoApproach` and `ToolOutsideEnvelope`. Called by `apply_step_in_scene`
/// for every `tip` step whose tool is bound; public for a generator that
/// asks without applying.
pub fn plan_landing(
    scene: &Scene,
    plan: &StepPlan,
    library: &OpLibrary,
    step_number: usize,
) -> Result<Landing, MechanosynthError>;

/// The checks of a step, all of them, and nothing applied: the target-side
/// and tool-side matches, the participant rule, the pattern and steric
/// checks. What `apply_step_in_scene` did before its first mutation, split
/// out so a caller can plan a landing — or refuse a step — without moving an
/// atom.
pub struct StepPlan { /* the two matches, the target participant, the op */ }
pub fn match_step_in_scene(
    scene: &Scene,
    op: &Operation,
    step: &Step,
    step_number: usize,
    tolerance: f64,
    clash: f64,
    tools_wired: bool,
) -> Result<StepPlan, MechanosynthError>;

/// `match_step_in_scene`, then `plan_landing` when the step is `tip` and its
/// tool is bound, then the apply. Signature unchanged from milestone 1.
pub fn apply_step_in_scene(…) -> Result<SceneEffect, MechanosynthError>;

pub struct SceneEffect {
    pub touched: Vec<u32>,
    pub added: Vec<u32>,
    /// Appended: the landing of a `tip` step whose tool was bound.
    pub landing: Option<Landing>,
}

pub const MIN_STANDOFF: f64 = 6.0;           // Å
pub const CLEAR_MARGIN: f64 = 0.5;           // Å
pub const SWEEP_DIRECTIONS: usize = 256;
pub const SWEEP_REFINEMENTS: usize = 8;
```

`replay_steps` keeps its arguments and gains a return: beside the failure it
already reports, the `landing` of every step it applied, in step order
(`None` for a step that is not `tip` or whose tool is unbound), because the
presentation layer needs them to carry a tool's orientation along a run.
Since `apply_step_in_scene` now lands every `tip` step, a build whose
twelfth step has no approach fails at step twelve whatever `step` asks for
beyond it — at `time = 1.0`, in the editor's block replay, and in the
generator alike. That is the one
deliberate change to milestone 1's output, and it changes only a file that
was already showing an impossible build.

### Presentation

```rust
/// A rigid pose of a tool's local frame in design space, `p = r · p_tool + t`.
/// `ToolPose` without the residual.
pub struct Pose { pub r: DMat3, pub t: DVec3 }

/// The run structure of a script: for each `tip` step, the previous and
/// next `tip` step of the same tool with only `spontaneous` steps between;
/// for each `spontaneous` step, the two `tip` steps it lies between when
/// they share a tool. Script and library only; no scene.
pub struct Runs { … }
pub fn runs(script: &BuildScript, library: &OpLibrary) -> Runs;

/// The full reaction pose of a landing: `R = ΔR · R_from` with `ΔR` the
/// smallest rotation taking `R_from · z` onto the approach direction, and
/// `t = p_r − R · reaction.tool`. `from` is the orientation the tool arrives
/// with — parked on a first visit, the previous reaction pose inside a run.
pub fn reaction_pose(landing: &Landing, reaction_tool: DVec3, from: &Pose) -> Pose;
/// The reaction pose lifted by `standoff_height` along the direction.
pub fn standoff_pose(landing: &Landing, reaction: &Pose) -> Pose;

/// The orientation a tool arrives at step `k` with: its parked pose on the
/// first visit of a run, else `reaction_pose` folded over the run's earlier
/// landings from the parked pose — each visit turns the tool as little as
/// the previous one left it. Pure; needs only the landings `replay_steps`
/// returned.
pub fn arriving_pose(runs: &Runs, landings: &[Option<Landing>], park: &Pose, k: usize, library: &OpLibrary) -> Pose;

/// One tool's visit for one `tip` step.
pub struct Visit {
    pub landing: Landing,
    pub park: Pose,                  // the binding's pose
    pub reaction: Pose,
    pub standoff: Pose,
    /// `false` when the tool already hovers at its standoff (chained visit).
    pub from_park: bool,
    /// Where the outbound flight ends: the next visit's standoff, or park.
    pub to: Pose,
    pub to_park: bool,
    /// The scan over the visit's flights.
    pub clearance: Clearance,
}

/// What a tool does during a step, if anything.
pub enum ToolMotion {
    Visit(Visit),
    /// A tool between two visits of one run, waiting over its next site
    /// through a `spontaneous` step.
    Hover { tool: usize, pose: Pose },
}

impl ToolMotion {
    pub fn tool(&self) -> usize;
    /// The tool's pose at step time `u`, clamped to `[0, 1]`; the reaction
    /// pose throughout a visit's dwell, the hover pose at every `u` of a
    /// hover. Continuous in `u`, and continuous across the step boundary
    /// inside a run.
    pub fn pose_at(&self, u: f64) -> Pose;
}

pub struct Clearance {
    /// The worst contact along the flights, or `None` when no non-tool atom
    /// came within `CLASH_SEARCH_RADIUS` of any tool atom at any sample.
    pub worst: Option<Contact>,
    /// Step time of `worst`, in `[0, 1]`.
    pub at: f64,
    /// The tool atom of `worst`, for the panel.
    pub tool_atom: Option<u32>,
}

/// Writes the tool's atoms into `structure` — a copy of `scene.structure`
/// the caller owns — at `pose` instead of at the bound pose:
/// `p' = R · R_parkᵀ · (p − t_park) + t`, through `set_atom_position` so the
/// grid follows. Bonds, ids and tags are untouched. The `Scene` itself is
/// never moved.
pub fn apply_tool_pose(structure: &mut AtomicStructure, scene: &Scene, tool: usize, pose: &Pose);

/// The scene at `(step, time)` — tools at their bound poses, as always —
/// and what its tool is doing. `None` motion when nothing is away from park:
/// a `bulk` step, a `spontaneous` step outside any run, tools unwired, or
/// `step` 0 or past the end.
pub fn replay_scene_at(
    base: &AtomicStructure,
    feedstocks: &[AtomicStructure],
    tools: &[AtomicStructure],
    library: &OpLibrary,
    script: &BuildScript,
    step: i32,
    time: f64,
    tags: HighlightTags<'_>,
) -> Result<(Scene, Option<ToolMotion>), MechanosynthError>;

pub const REACTION_LANDING: f64 = 0.45;
pub const REACTION: f64 = 0.5;
pub const REACTION_DEPARTURE: f64 = 0.55;
pub const PATH_SAMPLE: f64 = 0.5;            // Å
pub const PATH_SAMPLE_ANGLE: f64 = 5.0;      // degrees
```

`replay_scene(…, step, tags)` becomes `replay_scene_at(…, step, 1.0, tags)`
with the motion dropped, so every caller and every test compiles unchanged,
and — since the scene is never moved — its output is milestone 1's exactly,
tools included, for every build that has an approach. The tool positions a
*viewer* sees at `time = 1.0` differ inside a run, because the node applies
the motion; the engine's scene does not.

**Per evaluation**, for `(k, u)`, `1 ≤ k ≤ n`:

1. `runs(script, library)` — once, cheap, script-only. Then `build_scene`
   and `replay_steps` to `k − 1`, every `tip` step of it landed and the
   landings kept. A failure there is an error as today.
2. `match_step_in_scene` for step `k`, then `plan_landing` if it is `tip`
   with a bound tool. Step `k` failing — match, containment or sweep — is an
   error at `(k, u)` for every `u`: the step is selected, and its checks are
   what selecting it means. The partial state is reachable by asking for one
   step fewer.
3. If `u ≥ REACTION`: the apply half of step `k`, both sides, at the
   **bound** pose — the tool side matches at the binding's pose, the cargo
   enters the scene at the bound apex, and nothing in the matching or the
   steric check changes.
4. **Look-ahead**, for a `tip` step `k` whose run continues at step `j`, or
   a `spontaneous` step `k` inside a run whose next visit is `j`: on a
   **clone** of the scene after `k` (the step applied to the clone if `u`
   has not applied it), apply the `spontaneous` steps up to `j − 1`, then
   `match_step_in_scene` and `plan_landing` step `j`; its standoff is `to`,
   or the hover pose. Any failure along the way makes `to` park instead —
   silently here, because the replay reports it at the step that fails. The
   clone is discarded. This is the only place the engine looks past the
   selected step, and it costs one structure copy plus the settles.
5. The `ToolMotion` is assembled: `arriving_pose` from the run structure
   and the landings of step 1, `reaction_pose` from it, the standoff,
   `from_park` / `to_park` from the run structure, the look-ahead's `to`,
   the flights scanned. A hover is the look-ahead's standoff with the same
   arriving orientation.
6. Highlights. Before the reaction, `ms_current` is painted on the **matched
   `before` atoms** of step `k`, both sides: the site lights up as the tool
   approaches, and the apex that will react lights with it. From the reaction
   on, it is the step's `touched`, as today. `ms_added` and `ms_layer` follow
   the applied set and change meaning not at all.

The caller then draws: `let mut shown = scene.structure.clone();
apply_tool_pose(&mut shown, &scene, motion.tool(), &motion.pose_at(u))`.

**Errors.** Two new `MechanosynthError` variants: `NoApproach { step, op,
tool_type, best_clearance }` and `ToolOutsideEnvelope { step, op, tool_type,
atom, excess }`. Parse-time `Invalid` locations: legs on both sides of the apex's plane or a leg on it, a
missing or out-of-range `envelope`, a missing `reaction` on a `tip`
operation or a present one elsewhere, a non-positive `duration`, a `/3`
format string.

## The `mechanosynth` node

**Pins.** Inputs `base`, `ops`, `steps`, `step`, `feedstocks`, `tools`
unchanged; **appended** `time: Float` (pin 6), optional, overriding the
property, clamped to `[0, 1]`.

**Property.** `time: Float`, default `1.0`; serialised with a serde default so
every saved project loads at `1.0`; in the text format
`build = mechanosynth { step: 12, time: 0.3 }`, omitted at the default like
every other default. The round-trip corpus (`query` → `--replace` a no-op) is
re-run.

**Outputs.** `result` is the workpiece after `k − 1` steps for `time < 0.5`
and after `k` from `0.5`. `scene` is the same rule with the moving or
hovering tool at its pose. `step` gains four appended fields:

| field | type | meaning |
|---|---|---|
| `time` | Float | the clamped step time the outputs were computed at |
| `tool_r` | Mat3 | the moving or hovering tool's frame at `time`, `R`; identity when every tool is parked |
| `tool_t` | Vec3 | its frame origin (the apex), `t`; zero when every tool is parked |
| `clearance` | Float | the visit's worst flight contact ratio, capped at `2.0`; `2.0` when nothing came near or nothing flies |

`tool_r` / `tool_t` are what a camera follow or a gadget downstream needs and
cannot derive; `clearance` is what a style rule needs to paint a tool whose
flight collides. The cap keeps the record finite and honest: the scan cannot
see past `CLASH_SEARCH_RADIUS`, and two is beyond it for every element pair.

**Evaluation** reads `time` as `free_rot` reads its angle
(`evaluate_or_default` with `extract_float`), calls `replay_scene_at`, and
builds the `scene` pin from a clone of the scene's structure with
`apply_tool_pose` at `motion.pose_at(time)` — the engine's scene stays
unmoved, and `result` is split off it as today. The motion is kept beside
`last_scene` for the panel. The record is built from the same clamp.
Everything else in `eval` is unchanged.

**Cost.** One slider tick is one evaluation: a replay to `k − 1` (each `tip`
step of it landed), one match, one landing, a clone with a few settles and
one more landing for the look-ahead, the scans, one copy with the pose
applied. The silicon demo's 171 steps over two thousand atoms replay in
milliseconds today, and
ninety-two sweeps that mostly stop at their first candidate add a few
milliseconds; the clone is one structure copy. The memoised evaluator
re-runs only this node and its consumers. Nothing is cached until a
measurement says it must be; if one does, the scene after `k − 1` is the
thing to keep, keyed on the inputs' fingerprint, and it belongs beside
`last_scene`.

**Undo.** `time` is node data; a slider drag is bracketed by the existing
`begin_node_data_drag` / `end_node_data_drag` so Ctrl-Z undoes the drag, not
its ticks (`isosurface`'s precedent). Unlike `isosurface`, each tick **does**
write the kernel — the viewport moving under the hand is the feature — which is
`xray`'s precedent.

## API and panel

`APIMechanosynthData` gains `time: f64`. `APIMechanosynthInfo` gains `time`,
`leg: String` — one of `flying from park`, `descending`, `at site (before)`,
`at site (reacted)`, `ascending`, `flying to next site`, `returning to park`,
`hovering over next site`, or empty when every tool is parked —
`tilt_degrees: f64` and `approach_clearance: f64` from the sweep,
`flight_clearance: f64`, `flight_clearance_at: f64` and `collision: String`
from the scan (the panel sentence, empty when clear), all read off the last
evaluation the way the tool rows are.

**Panel.** Under the step scrubber, a **time** row: a `Slider` over `[0, 1]`
with a tick at `0.5`, live during the drag and bracketed for undo, a float box
beside it, both disabled when the `time` pin is wired (the rule the step row
uses). Under it two readout lines. The approach line:
`approach: vertical, clear by 1.8 Å` or `approach: tilted 23°, clear by
0.5 Å`. The flight line: `flights clear (worst 1.32)`, or, in the warning
colour, `flight collides at 41 %: O of si_tool against Si 481, ratio 0.62`.
A `NoApproach` error reaches the user on the result pin like every replay
error, with the best clearance in the sentence. The tools block is unchanged;
the row of a tool that is away from park is prefixed with a marker.

## The envelope cage: a viewport overlay switched in the preferences

The envelope is the one thing in this design a user cannot otherwise see,
and it is the thing a wrong tilt or a `NoApproach` is explained by. So the
viewport can draw it: a **wireframe cage** of each bound tool's envelope —
the cone from its apex to the rim where the cylinder begins, and a length of
the cylinder beyond — in the tool's frame, following the tool wherever the
motion puts it. It is off by default and lives in the preferences, because
it is a way of looking at every build rather than a property of one.

**Preferences.** Two fields on `AtomicStructureVisualizationPreferences`,
both with serde defaults so an existing `preferences.json` loads unchanged:

| field | type | default | meaning |
|---|---|---|---|
| `show_tool_envelopes` | `bool` | `false` | draw the cage of every bound tool of every displayed `mechanosynth` node |
| `tool_envelope_color` | `PrefColor` | `(255, 160, 0)` | the cage's line colour, RGB |

The API twin `APIAtomicStructureVisualizationPreferences` gains the same two,
converted in `api_common.rs` beside `unit_cell_wireframe_color`; the
preferences window gains a checkbox and an `IVec3Input` in the *Atomic
Structure Visualization* section, with `PreferencesKeys` entries for the
tests, wired through `_applyPreferences` like every other field.

**What is drawn.** For each bound tool, with the cone's apex at the
tool-side reaction point of the tool's **nearest visit** — the current step
if it is this tool's, else its next `tip` step in the script, else its last,
else the local origin for a tool no step uses — the cage is
`CAGE_MERIDIANS` (16) lines from the apex to the cone rim continuing up the
cylinder to its top, and three rings: the rim, the cylinder's midpoint and
its top. The cylinder is drawn `CAGE_CYLINDER_LENGTH` (12 Å) long; the
envelope is infinite and the cage is a picture of it. The segments are
computed in the tool's local frame once and posed with the same `Pose` the
atoms get — the binding's for a parked tool, `motion.pose_at(time)` for the
one that flies — so the cage descends with the tool and sits on the site at
the reaction.

**How it reaches the screen.** `EvalOutput` gains a general
`overlays: Vec<Overlay>`, an `Overlay` being a `kind` and a list of line
segments in design space. The evaluator copies it onto `NodeSceneData`
beside `unit_cell`, and the scene tessellator draws each overlay into the
existing `wireframe_mesh` — the pass the unit-cell wireframe and the
drawing-plane grid already use — **iff the preference for its kind is on**,
in that kind's preference colour. `OverlayKind::ToolEnvelope` is the only
kind this design adds; §Follow-ups' drawn path is the second.

Two consequences, both deliberate:

- **Preferences affect tessellation, never evaluation.** The node emits the
  segments whether or not the box is ticked (a few hundred floats per tool),
  so toggling the preference re-tessellates the scene and re-evaluates
  nothing, and the memoised evaluator's outputs stay independent of the
  preferences — the invariant every other preference keeps.
- **The cage is an overlay, not a gadget.** A gadget is the selected node's
  interactive handle set; the cage is a fact about every displayed build's
  tools, wants no hit test and no drag, and should show for a node that is
  merely displayed. The unit-cell wireframe route is the precedent, made
  general so that the next overlay is a `kind` rather than a third field.

Rejected: **the cage as part of the tool's atoms** (extra "atoms" or bonds
in the scene). It would reach `result`-adjacent consumers, count against
tags, and be picked by the editor. Also rejected: **computing the cage only
when the preference is on**, which would make an evaluation depend on a
preference and put a preferences read into `eval`.

## Reference guide

- `doc/reference_guide/nodes/atomic.md` §mechanosynth: the `time` pin and
  property, what the slider shows (the visit's shape, the switch at the
  middle, runs and hovering), the two readout lines, the no-approach error,
  and **how to lay out a scene**: park the tools in a row **behind the
  workpiece and the reservoir, away from the camera**, at one height, that
  height being where every flight happens; a tool then leaves park once per
  run, works across the scene in front of the camera, and goes home when its
  run ends.
- `doc/reference_guide/nodes/math_programming.md`, the `MechanosynthStep`
  record: the four appended fields.
- `doc/reference_guide/ui.md`, the preferences dialog: the two envelope
  fields in *Atomic Structure Visualization*, and what the cage shows.
- `doc/reference_guide/op_libraries.md`: the `/4` format — the frame's
  axis rule, `envelope`, `reaction`, `duration` as reserved, `approach`
  gone.

## Testing

Conventions as in `doc/testing.md` and the crate `AGENTS.md` files: a test
goes where its **imports** allow — a member crate cannot see `api`, so the
domain harnesses hold everything that needs no transport type and
`rust/tests/structure_designer_api/` holds only what does; fixtures stay
under `rust/tests/fixtures/mechanosynth/`, reached through
`atomcad_test_support::fixture_path`; test names are sentences; **no timing
assertions** — that the sweep stops at its first free candidate is a
property of the walk, not something a test measures with a clock. The
layers of §Architecture are also the layers of the tests: the feasibility
layer is tested as pure geometry and then through `apply_step_in_scene`,
which is the call the generator makes; the presentation layer is tested
against the feasibility layer's output, never against hand-typed
coordinates.

### Fixtures first

The `/4` bump touches **every** library fixture, and that migration is the
first commit of Phase 1, before any new code:

- the twelve `*_ops.json` fixtures get the `/4` format string; every `tip`
  operation gets a `reaction` block (its `probe` tool side from the `/2`
  migration is empty, so `reaction.tool` is `apex + (0, 0, −contact)` along
  the type's axis and `reaction.target` the anchor's position); every tool
  type gets an `envelope`. Most of these fixtures wire no tools, so nothing
  in them is ever swept — the fields are the format's, not the tests';
- `tool_ops.json` gets real values: `habst_tool` with its cargo at
  `(0, 0, 1.06)` as `reaction.tool` on `habst` and `hdump`, `probe` with a
  contact-distance point on `habst_probe`, and an envelope each that the
  six-atom skeleton and `tool_tip_on_handle.xyz` fit inside — the
  containment check is what says whether the numbers are right;
- the four `.cnnd` fixtures (`mechanosynth_legacy`, `mechanosynth_wired`,
  `mechanosynth_edit`, `mechanosynth_tools`) are re-snapshotted **once** in
  that commit and must evaluate to the same atoms afterwards, tools
  included, since the engine's scene is never moved (§Architecture). That
  is a stronger regression than milestone 1's, and it is the one that would
  catch a landing leaking into the replay.

**The frame's axis sign is the reason the tool fixtures need no geometry
change.** The fixture tools and milestone 1's ethynyl example have their legs
at `z = −3.17` and their cargo at `+z`; the silicon tools have legs at
`+0.515` and cargo at `−z`. The frame rule is therefore *all legs on one side
of the apex's plane*, and the axis sign is read from them — not "legs at
`z > 0`", which would have flipped seven `.xyz` files and the tools `.cnnd`
for no gain. Where this document says "the tool axis" it means `±z` with
that sign.

New fixtures: `trajectory_build.json` — `habst`, `settle`, `hdump` on the
dump, `habst`, `habst_probe`, `expose`: one run of `habst_tool` with a
settle *inside* it, then the probe's visit ending it, then a `bulk` step;
and `mechanosynth_trajectory.cnnd` — `mechanosynth_tools.cnnd` with that
script, `time: 0.3` stored, and a second copy of the probe parked directly
over the workpiece site, for the node snapshot and the round-trip corpus.
An obstacle over a site is otherwise built in code by translating
`tool_probe.xyz`; no fixture is added for it.

### Feasibility layer — `crates/atomcad-crystolecule/tests/crystolecule/mechanosynth_trajectory_test.rs`

Pure geometry, no scene:

- `Envelope::radius_at`: zero behind the apex, linear in the cone, flat in
  the cylinder; `Envelope::clearance` on one obstacle inside, on the
  surface, and outside the envelope has the sign and magnitude the formula
  says;
- `sweep_directions()`: the first is `+z`, the last `−z`, tilt is
  non-decreasing along the sequence, consecutive tilts never jump by more
  than the sphere's spacing, and every entry is a unit vector;
- `approach_direction` with no obstacles returns exactly `+z` with tilt
  `0`; with a single obstacle on the `+z` axis returns a direction tilted
  just past it (clearance at least `CLEAR_MARGIN`, tilt within a
  refinement step of the minimum); with obstacles enclosing the point
  returns `None`; with every candidate blocked but one below the margin
  returns that one; the same inputs give the same output (determinism).

Through the scene, on `tool_scene.xyz` and the tagged fixture tools:

- `obstacles_for` excludes the tool's atoms and the excluded set, includes
  every other participant's atoms, and its margins are
  `clash · (r_cov + r_tool)`;
- `plan_landing` puts `reaction_point` at `step.r · reaction.target +
  step.t`; the standoff height is the park height along the direction, and
  `MIN_STANDOFF` when the tool is parked lower; a molecule with an atom
  outside its type's envelope is `ToolOutsideEnvelope` naming the atom;
- a parked tool standing over the site tilts the approach; removing it
  makes the approach vertical again;
- **the generator's path**: `apply_step_in_scene` on a `tip` step with a
  bound tool returns a `landing` equal to what `match_step_in_scene`
  followed by `plan_landing` returns, and the latter leaves the scene
  untouched; on an enclosed site the apply fails with `NoApproach`
  naming the step and the tool type, and the scene is unchanged (the
  all-or-nothing rule now covers the landing);
- `replay_steps` returns one landing per applied `tip` step with a bound
  tool and `None` elsewhere; a script whose step `j` has no approach fails
  at `j` for every `step ≥ j`, and `replay_scene` fails the same way —
  the one deliberate change to milestone 1's output;
- with `tools` unwired no landing is planned and nothing fails.

Parse, in the existing `mechanosynth_tools_test.rs` beside the `/2` and
`/3` parse tests: legs on both sides of the apex's plane, or a leg at
`z = 0`, a missing or out-of-range `envelope`, a missing `reaction` on
`tip`, a `reaction` on `bulk`, a zero `duration`, and a `/3` file are each
refused naming the location; `approach` in a file is ignored; a `/4`
library without `duration` reads `1.0`.

### Presentation layer — the same file

Every assertion here compares against a `Landing` the feasibility layer
produced, never against a typed coordinate:

- `runs`: two `tip` steps of one tool with a `spontaneous` between are one
  run; with a `bulk` between, two runs; with another tool's `tip` between,
  two runs; the `spontaneous` step inside a run reports its two neighbours
  and one outside a run reports none;
- `reaction_pose`: the tool-side reaction point lands on the target's to
  `1e-9`, the tool's axis is the approach direction, and the rotation from
  the arriving pose has the angle between the two axes and no more (minimal
  roll); `arriving_pose` on the third visit of a run is the parked
  orientation turned by the first two visits' minimal rotations, in order;
- `ToolMotion::pose_at`: at `u = 0` the park on a first visit and the
  standoff on a chained one; at `u = 1` the park at a run's end and the
  next standoff otherwise; the reaction pose throughout `[0.45, 0.55]`;
  continuous (a `1e-3` step in `u` moves the apex less than `PATH_SAMPLE`)
  at a thousand sampled `u`;
- **continuity across a run**, on `trajectory_build.json`: the pose at
  `(k, 1.0)` equals the pose at `(j, 0.0)` for the two `habst_tool` visits,
  and equals the hover pose at the `settle` between them, to `1e-9`; the
  probe's visit starts and ends at park; the `expose` step has no motion;
- a failing look-ahead (step `j` made unmatchable) sends the tool to park
  at `(k, 1.0)`, and the replay fails at `j`;
- flight scan: a slab placed across a flight reports a ratio below
  `CLASH_BLOCK` at a time inside that flight; the same scene with the slab
  removed reports none; a chained visit reports only its outbound flight;
- `replay_scene_at(k, 1.0)` returns a scene equal to `replay_scene(k)`
  atom for atom, tools included, for every `k` of both build fixtures —
  the compatibility assertion and the proof that the engine never moves a
  scene; `(k, u < 0.5)` has the workpiece of `replay_scene(k − 1)`,
  `(k, u ≥ 0.5)` that of `replay_scene(k)`; `apply_tool_pose` on a clone
  leaves every non-tool atom, every bond and every tag untouched and keeps
  the tool's atoms pairwise-distance-preserved from their bound positions;
- `ms_current` before the reaction is the matched `before` set of both
  sides; after it, the step's `touched`.

### Node layer — `crates/atomcad-structure-designer/tests/structure_designer/mechanosynth_trajectory_node_test.rs`

Wiring only, as `mechanosynth_tools_node_test.rs` does — every structural
assertion is an equality against the engine:

- the `time` pin is index 6, optional, and overrides the property; an
  out-of-range value on either is clamped; `result`, `scene` and the record
  at `(k, u)` equal what `replay_scene_at` and `apply_tool_pose` produce;
- the four record fields are appended after `agent`, `tool_r` / `tool_t`
  are the motion's pose and the identity / zero with every tool parked,
  `clearance` is the scan's ratio capped at `2.0`;
- the `scene` pin carries the tool at its flown pose while `last_scene`
  keeps it bound — the two are compared directly;
- a `mechanosynth` node with a bound tool emits one `ToolEnvelope` overlay
  per tool, apex at the nearest visit's reaction point, whose segments move
  with the flying tool and stand still for a parked one;
- persistence: `time` round-trips through a `.cnnd` and is absent at the
  default; a node saved without `time` loads at `1.0`; the text format
  writes `time: 0.3` and omits `1.0`;
- `nodes/node_snapshots_test.rs` gains `mechanosynth_trajectory_evaluation`
  on the new `.cnnd` — with `time: 0.3` stored, pin 0 is the workpiece after
  `k − 1`, which pins the gating through the real loader;
- `text_format_roundtrip_corpus_test.rs` gains the new `.cnnd`.

Scene and preferences, in the same harness:

- a scene test in the manner of `error_display_test.rs` — refresh, read
  `last_generated_structure_designer_scene` — asserts the overlay reaches
  `wireframe_mesh` in the preference colour with `show_tool_envelopes` on,
  contributes no segment with it off, and that toggling it changes the
  scene without re-evaluating the node (the node's evaluation count, read
  from the profiler hook, is unchanged);
- `preferences_test.rs`: the two fields join
  `test_default_values_match_documentation`, `test_non_default_values_roundtrip`
  and `test_preferences_missing_fields_use_defaults`.

### API layer — `rust/tests/structure_designer_api/mechanosynth_api_test.rs`

- the getter reads `time` and the setter writes it, the caches surviving a
  no-op write as for the other properties;
- `mechanosynth_info` reports `time`, `leg`, `tilt_degrees`,
  `approach_clearance`, the flight clearance fields and `collision` off the
  last evaluation: `leg` names each of the seven legs at a `u` inside it
  and is empty at a `bulk` step; `collision` is the panel sentence for the
  blocked fixture and empty otherwise;
- the preferences API twin round-trips the two new fields.

### Dart — `test/`

`mechanosynth_time_row_test.dart`, on a widget extracted the way
`MechanosynthScrubber` was — plain numbers and callbacks, no kernel: the
slider reports every tick through `onChanged` (unlike the step scrubber,
which reports once), `onDragStart` / `onDragEnd` fire once each around a
drag, the row is disabled when the pin is wired, the float box round-trips a
typed value, and the tick sits at `0.5`. The preferences window's two new
controls get `PreferencesKeys` entries so the human smoke test can find
them.

### Cross-cutting regressions, at the end of every phase

The four re-snapshotted `.cnnd` fixtures evaluate to the same atoms; the
text-format round-trip corpus stays a no-op; the engine, node and API
suites pass with only the edits the format change forces. The harnesses are
run explicitly (`--test structure_designer_api`, `integration`,
`renderer_api`), because `cargo test --workspace` stops at the first failing
harness. The Phase 1 kickoff check against the regenerated silicon v3 file
set is the generator's test, run in the generator, not here.

### Manual only

The feel of the drag, the look of the cage, the demo walkthrough of Phase 4,
and the Flutter smoke test, which stays the human's.

## Phases

### Phase 1 — Schema and engine

The `/4` parser (`envelope`, `reaction`, `duration`, the leg rule, `approach`
removed); `trajectory/envelope.rs` and `landing.rs` first — the feasibility
layer, with `match_step_in_scene` split out of `apply_step_in_scene` and the
apply landing every `tip` step — then `runs.rs` and `path.rs` with
`replay_scene_at`, the look-ahead and `replay_scene` as its `1.0` wrapper;
the engine tests above. The external generator is rebuilt against the new
`SceneEffect` (one added field) and gains nothing else it has to call. The
fixture migration of §Testing is the first commit. Kickoff check: the silicon v3 file set, regenerated as
`/4` by the external generator with envelopes and reaction points, replays
at `(k, 1.0)` with the workpiece identical to milestone 1 for all 171 steps;
every `tip` step has a vertical approach (the demo's sites are all on the top
face with the tools parked above); at `(k, 0.5)` every cargo sits on its
target's reaction point; and the cover phase is **one run per tool**: the
shuttle leaves park once, alternates reservoir and workpiece for sixty-two
visits with the settles passing under it, and returns once.

### Phase 2 — Node, record, API, text format

The `time` property and pin, the four record fields, `APIMechanosynthData`
and `APIMechanosynthInfo`, the loader default, the text-format property,
FRB regeneration, node tests, the round-trip corpus.

### Phase 3 — Panel and preferences

The time row with drag bracketing, the two readout lines, the tool-row
marker; the overlay route (`EvalOutput::overlays` → `NodeSceneData` → the
wireframe pass), the cage segments on the node, the two preference fields
end to end (struct, defaults, API twin, conversion, window, keys).

### Phase 4 — Guide and walkthrough

The four guide pages, the layout advice included; the manual checklist —
with the envelope cage switched on in the preferences, so every item below
is watched with the cone visible —
on the silicon v3 demo re-parked with the tools behind the workpiece: drag
the slider on a pickup and watch the shuttle leave park, descend on the
reservoir, take its atom, lift and fly across to hover over the workpiece
site; scrub the settles and see it wait; scrub the donation and see it
descend, hand over, lift and fly back toward the reservoir; find the run's
last donation and see it go home; park a tool directly over a site and watch
the next visit tilt; set `time` by wire from a `float` node; park a tool on
another's flight line and read the collision; undo one drag with one Ctrl-Z;
open a milestone-1 project with a `/3` library and read the regenerate
message. The Flutter smoke test stays the human's.

## The clock milestone 2 will need

With this half done, the animation half is a **driver**, not a change to the
replay: a clock `T` in the library's relative units, the cumulative
`duration`s of the script mapping `T` to `(k, u)`, and a node — or a mode of
the panel — that feeds the two pins from `T` and steps `T` on a ticker. The
grouping rules for `bulk` events and `spontaneous` settling
(`event_indices`) decide how that driver spends an event's time, and the
run structure (`runs`) is what lets it give a flight between two sites the
time its length deserves; nothing in this design assumes anything about it.
Export is a loop over `T` with the existing image export. Whether the driver
is a node over `scene` and `step` or a mode of the replayer is that design's
decision, as milestone 1 already said.

## Follow-ups (signatures only)

- **Routed flights.** Replace a straight flight by a two-segment path
  through a via point found by the same envelope test on the lateral leg,
  when a re-park is not an option — a workpiece so large that no park is off
  every line. `Visit` gains a `via: Option<DVec3>`.
- **Two tools away at once.** Let a tool hover through another tool's visit
  by entering the hovering tool into the other's obstacle set at its hover
  pose — a second place the engine would have to know a tool is not at park.
  Wanted only if a process interleaves two tools tightly enough for the
  return trips to show.
- **A grid-backed cylinder query** for the sweep, when a scene makes the
  obstacle pass the slow part. `approach_in_scene` is the only caller.
- **A drawn path.** A second `OverlayKind` drawing the flights, the
  standoffs and the colliding sample in red, through the route the envelope
  cage opens — the same `Visit` rendered rather than scrubbed, with its own
  preference switch and colour.
- **Library-stated standoff and speed.** `"trajectory": { "standoff": 8.0 }`
  on the library, overriding `MIN_STANDOFF`, when a second process shows six
  ångström is not one number for all.
- **Time on `mechanosynth_edit`**, if authors want to watch a path before
  committing a step; needs the hit test to read the moved scene.
- **Offers that know the approach.** `applicable_ops` running the sweep for
  each ready row, so the editor greys out a placement the tool could not
  reach before it is committed — the generator's check, in the editor.
- **A `parts: [HasAtoms]` output** (milestone 1's follow-up) — the moved tool
  as its own structure, for a downstream that wants only it.

## Considered and rejected

- **Real time and per-step durations**, **a single global progress float**,
  **the switch at landing**, **returning to park after every visit**,
  **hovering across a `bulk` step or another tool's visit**, **storing the
  approach in the step**, **deriving the reaction point**, **deriving the
  axis from the legs**, **a surface-normal approach**, **an explicit pose in
  the library**, **a simulated approach**, **a straight-line path**, **a fixed
  standoff**, **routed flights**, **time on the editor** — each in
  §Decisions.
- **A separate `trajectory` node** over `scene` and `step` instead of a mode
  of the replayer. It would have to redo the match to find the site's atoms
  and the tool side to find the cargo, on a scene whose participant map it
  does not have — the replayer has all three for free. A node is the right
  shape for the *driver* (§The clock), not for the pose.
- **Storing run membership or the next standoff on the step.** Both are
  functions of the script and the scene; stored, they would go stale on the
  first reorder, which is the whole reason nothing about a trajectory is
  stored.
- **Splitting a step's time by flight length across the halves** (a chained
  visit spending less of its step descending). It would move the reaction
  off `0.5`, which every kind of step and every downstream `expr` relies on;
  the clock half gives a flight the time it deserves instead.
- **A per-atom collision shape for the tool** (sweeping the molecule itself
  rather than an envelope). Exact for the wired molecule and blind to the
  shaft the design does not wire; the roll would matter and the search would
  be three-dimensional. The envelope is what makes the search a sweep over
  directions.
- **Making a flight collision an error too.** A collision on a flight is the
  user's layout and the fix is visible and immediate; an error there would
  stop a demo for a re-park. The approach is different in kind: the user
  cannot fix it by hand, so the engine either solves it or refuses.
- **Interpolating the atoms themselves** (cargo sliding from apex to site).
  There is no path for an atom without a reaction coordinate; the handoff is
  instantaneous by the engine's founding rule and the coincident reaction
  points make it invisible.

## Open questions

- Whether `MIN_STANDOFF = 6 Å`, `CLEAR_MARGIN = 0.5 Å` and the `0.45 / 0.55`
  dwell are right for the silicon demo's camera. Numbers to look at, not to
  argue about.
- Whether a `bulk` step should end a run after all. It does here because an
  exposure is a regime the instrument retracts from; a process that doses
  while a tool waits nearby would want the run to continue, and the change
  is one line in `runs`.
- Whether a side-face process ever wants a preferred direction other than
  `+z`. The sweep finds the face's free directions either way; only the tie
  between two free ones would change. A per-library preference is the
  alternative, and nothing asks for it yet.
- Whether the record's `clearance` should be a Bool `collides` beside the
  ratio, for a `switch` downstream. An `expr` on the ratio does it today.
