# Design: tool trajectories in `mechanosynth`

Status: **draft 2026-09-16, revised six times after review** — the
approach became a collision-free sweep rather than a surface-normal guess; a
tool was allowed to fly from one site straight to its next instead of
returning to park between them; and the engine was split into a feasibility
layer the sequence generator shares and a presentation layer that cannot
fail (§Architecture). The testing story was then reviewed against the
repository's harnesses and fixtures, which moved two test files to the
harness their imports allow and made the frame's axis sign a property of
the frame rather than a rule that would have flipped every fixture
(§Testing). A fourth review then corrected the sweep's clearance from a
radial gap to a true distance, took the blocked site out of the replay's
failure set — the engine measures, the generator refuses, the node reports —
moved the containment check to binding so that `apply_step_in_scene` really
does keep its signature, put the standoff in the park plane, and made the
generator's re-park part of Phase 1 (§Decisions, §Architecture, §Phases). A fifth review, during Phase 1, made the
envelope a **solid** rather than a locus of atom centres: it now contains the
tool's atoms *with their radii*, and an obstacle is charged its own radius
alone. The old pairing — centres in the envelope, the tool's widest radius added
to every obstacle — charged the tool's extent twice, which grew the keep-out
sphere around a site's own host atom past the bond length and made every
abstraction unreachable from every direction (§The sweep). The same review then
**cut the trajectory loose from the tool layout entirely**: the standoff is a
constant height above the reaction point rather than the park plane, and *no*
tool's atoms are obstacles — not the visiting one's, not anyone else's. A visit
is now a property of its site alone, so a sequence can be generated before
anyone decides where the tools go, and the tools can be moved afterwards without
invalidating it. Keeping them out of each other's way became the designer's job:
park them on different sides of the workpiece and its reservoirs. A sixth
review, during Phase 2, corrected three things the document had wrong about the
repository it describes: the text format has **no** omit-at-the-default rule and
`get_text_properties` must be *total* (§The `mechanosynth` node); the panel's
`leg` is an engine value, not a string the API invents, so `Leg` and
`ToolMotion::leg_at` join the presentation layer (§Presentation); and the
round-trip corpus entry Phase 2 asks for could not be added until `import_xyz`
was made total, which is where that same rule had already been broken
(§Testing). A seventh review, during Phase 3, corrected the panel's write rule:
the time slider writes during the drag as this document says, but **at most once
per frame** rather than on each tick, which is the difference between a drag that
follows the hand and one that crawls seconds behind it (§The `mechanosynth`
node). An eighth review, measuring that panel, replaced the **folded roll with a
park-relative one**: a visit's orientation now reads its own approach direction
rather than the path taken to reach it, which takes a step from one sweep per
visit of its run to two in total (§The envelope, §Presentation). A ninth
review, after using Phase 3's panel, added **Phase 5, the play control**: a
scrubbable build still could not be *shown*, because advancing the step leaves
the time where the last drag left it and the motion breaks at every boundary
(§Playing the build). A tenth review, after playing one, added **Phase 6**: at
one second a step a build that is mostly settles plays mostly stillness, and the
run the trajectory engine exists to show is chopped up by the steps that have no
motion — so the driver walks a list of *playable* steps the kernel computes
(§What playing skips). Playing one then reversed a decision of §Presentation:
`ms_current` no longer lights step `k`'s site during the approach, because a
highlight that arrives before its own reaction makes the reaction — the instant
this whole timeline is built around — read as nothing at all (§Presentation
step 6). Phases 1, 2, 3, 5 and 6 are implemented; Phase 4 — the guide pages and
the manual walkthrough — is not.

Builds on `doc/design_mechanosynth_tools.md` (milestone 1: what a build does
to every molecule it involves, all four phases implemented 2026-09-15) and is
the **first half of milestone 2** of that design. The second half — a real
clock over a whole build, and animation export — is *not* designed here; §The
clock milestone 2 will need says what this half hands it, and Phase 5's play
button is the transport that clock will inherit rather than an instalment of
it. This design retires
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
- a **report** when no collision-free approach direction exists — the tool
  visits along the least-blocked direction and the panel and the record say
  the site is blocked — and a **contact scan** of every leg of the visit,
  the descent included, reported the same way; the replay never fails for
  either;
- the feasibility half exposed by `atomcad-crystolecule` — the pure sweep,
  the landing, and the step checks without the apply — so that a sequence
  generator refuses a blocked step before emitting it, reading the verdict
  off the apply it already calls;
- five fields appended to `MechanosynthStep`: `time`, `tool_r`, `tool_t`,
  `approach`, `contact`;
- a **wireframe cage of each tool's envelope** in the viewport, following
  the tool, switched on and coloured in the preferences (off by default),
  through a general overlay route from `EvalOutput` to the wireframe pass;
- the panel's time row and readout lines; the reference guide, including the
  layout a demo wants (each tool on its own side of the work).

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
  first run. Since the sweep ignores every tool this is no longer what makes
  the collision model sound — it is what keeps the *motion* model simple:
  one moving thing at a time, so `ToolMotion` describes one tool and a viewer
  has one thing to follow.
- **The next standoff is a function of the scene before the next visit**,
  which is the scene after this step plus the settles between — the engine
  has it by applying those steps to a clone. The standoff a tool flies to at
  the end of step `k` and the standoff it descends from at step `j` are the
  same number computed the same way, so the boundary between steps is
  continuous. If the next visit's match fails — a sweep cannot fail, it only
  reports — the tool returns to park instead, and the failure is reported
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
generator's reason to compute at all is to refuse a step whose site is
blocked (§The sweep).

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
The envelope is the **solid the tool occupies**: it contains every atom of the
tool *with its covalent radius*, not merely the atom centres. That is what lets
the sweep charge an obstacle its own radius and nothing more (§The sweep) — the
tool's extent is already in the cone, and adding it again would charge for it
twice. The library may also claim **more** than the wired molecule: a tooltip's
`R` can be the radius of the tip shaft the design does not model, so that an
approach avoids what the real instrument would hit.

One atom is necessarily outside: the **cargo**, whose own sphere swallows the
apex because the apex *is* where it sits. A donation's transferred atom is at
the reaction point by construction, and no cone anchored there could contain it,
so containment exempts an atom whose sphere contains the apex — and only that
atom.

Because the envelope is a solid of revolution about the tool axis, the tool's
**roll** about that axis cannot matter to collisions, and the orientation
search is over **directions**: the unit vector `d` the axis points along
once the reaction points coincide. The tool arrives along `−d` and leaves
along `+d`; the roll is **the parked tool turned by the single smallest
rotation that points its axis down `d`**, so it never twirls and a vertical
approach from an upright park leaves the tool exactly as it was posed.

That rule is **memoryless**, and the first implementation's was not: it folded
each visit's minimal rotation onto the orientation the tool *arrived* with,
chained from park through every earlier visit of the run. The fold reads well
on paper and made a visit's orientation a function of the whole path taken to
reach it — so drawing step `k` meant sweeping every earlier visit of its run,
and the sweep is the most expensive thing in the engine. Measured on the
silicon demo at step 49: **forty sweeps planned, eleven read, 14.6 ms of a
25 ms evaluation**. Reading the orientation off `d` alone costs **two** sweeps
per step — this visit's and the next one's — however long the run is, and buys
two things beyond the speed: continuity across the step boundary becomes
structural rather than a pair of folds that have to agree, and the roll cannot
drift, since composed minimal rotations are not the minimal rotation of the
composition. `reaction_pose`'s doc comment carries the argument.

Numbers, for orientation: the TLM cage tools fit `α = 30°`, `R = 4 Å` with a
loaded silicon tool's reaction point at the cargo; the bcc tungsten pyramid is
`α = 55°` (its faces) with `R` the tip model's half-width. Both are the
library's to state.

**Containment is checked, not assumed — at binding.** `build_scene` tests
every atom of the bound molecule — its **sphere**, `gap ≤ −r_cov`, the cargo
excepted — against the envelope placed at the tool-side
reaction point of **every** `tip` operation of that type, and an atom reaching
outside the envelope is `ToolOutsideEnvelope`, naming the tool type, the atom and the operation —
a library that claims a smaller envelope than its molecule is wrong the way a
frame whose residual fails is wrong, and it is reported where the residual
is: once, before any step. The check costs one pass over the tool's atoms per
operation, and it is what lets the binding carry the envelope, so that
applying a step needs nothing the scene does not already hold
(§Architecture).

### The sweep: the approach direction is found, and its absence is reported

The sweep is run about the **placed reaction point**
`p_r = step.r · reaction.target + step.t` — the target-side reaction point
carried into the design by the step's placement. The **obstacles** are every
scene atom that is not the visiting tool's and not one of the target side's
matched `before` atoms — the site is the reaction, not an obstacle. Each
obstacle is a sphere of **its own covalent radius** times the library's clash
factor (`CLASH_BLOCK`, 0.9). The tool's radius is deliberately *not* in there:
the envelope is the solid the tool occupies (§The envelope), so the tool's
extent has already been counted, and counting it twice is what an earlier draft
did. The cost of that double charge was not marginal — the keep-out sphere
around a site's own host atom became `0.9·(r_host + r_tool_max)`, which for an
Si–Cl site and a tungsten probe is 2.46 Å against a 2.02 Å bond. Since the
cone's apex is itself a point of the envelope's surface, the gap to an obstacle
can never exceed its distance to the apex, so a host that close blocked the site
from **every** direction, and no tilt could help. Twenty-eight of the silicon
demo's 171 steps failed that way, all of them abstractions or probe visits.

For a direction `d`, an obstacle at `p` has axial coordinate
`s = (p − p_r) · d` and radial distance `ρ` from the axis. Its **gap** is
its signed **distance to the envelope's surface** — not its radial excess
over the envelope's radius, which an earlier draft used and which overstates
the room by `1/cos α` (fifteen per cent at 30°, seventy-four at the probe's
55°). With `ℓ = s · cos α + ρ · sin α` the foot of the perpendicular along the
slant, `s_R = R / tan α` the rim's axial position and `ℓ_R = R / sin α` its
position along the slant, for a centre outside the envelope:

- `ℓ ≤ 0`: the nearest surface point is the apex, gap `√(s² + ρ²)`;
- `0 < ℓ < ℓ_R`: the nearest is on the slant, gap `ρ · cos α − s · sin α`;
- `ℓ ≥ ℓ_R`: the nearest is the rim circle or the cylinder wall, gap the
  smaller of `√((s − s_R)² + (ρ − R)²)` and, when `s ≥ s_R`, `ρ − R`.

A centre is **inside** when `s > 0` and `ρ ≤ min(s · tan α, R)`; its gap is
then negative, minus its distance to the nearer of the slant and the wall.
The slant distance of the infinite cone is never used past the rim, where it
would call a point beside the cylinder inside.

The **clearance** of `d` is the minimum over obstacles of `gap − margin`.
Positive clearance means every obstacle sphere is outside the envelope. An
obstacle behind the apex by more than its margin can be skipped without
looking, because its gap is at least that; that is an optimisation, not
part of the definition. Positive clearance is a free direction.

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
4. if no candidate reaches `CLEAR_MARGIN`, take the remembered best. Its
   clearance may be negative: the sweep **always answers**, and the answer
   carries its own verdict. A landing whose clearance is positive is
   **reachable**; one whose clearance is not is **blocked**, and what
   happens then depends on who asked (§Architecture): the generator refuses
   to emit the step; the node performs the visit along that least-blocked
   direction and reports it — the panel's approach line in the warning
   colour, the record's `approach` negative — and the replay does not stop,
   because a view of a build is not the place to decide that the build is
   impossible.

The cost is one dot product and a few multiplications per obstacle per direction,
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
under an overhang, beside a parked tool — comes back on the `SceneEffect`
as a blocked landing, and the generator's loop, which already turns every
apply failure into "step `n` could not be emitted", treats a blocked landing
the same way: one check on the effect, the step number in the message. For
a generator that wants to ask before committing, the sweep is also public
on its own: `approach_direction` takes an envelope, a reaction point and an
obstacle list and returns the best direction with its clearance. That is the
design's answer to "what if no direction exists": the generator does not
emit the step, and sequences differently. The answer is the same in both
places, and now unconditionally so: **no tool is an obstacle**, so neither the
generator nor the node has to know where any tool sits, and the two cannot
disagree about it.

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

- the **standoff point** `S = p_r + d · STANDOFF_HEIGHT`: a constant 6 Å up
  the approach direction from the reaction point. Nothing about the tools
  enters it. Two earlier drafts derived it from the park — first by projecting
  the park offset onto `d`, then by meeting the park plane — and both tied a
  visit's shape to a layout the sequence has no business knowing. A visit is a
  property of its site;
- **inbound flight**, when `from` is park: a straight line from the parked
  apex `t_park` to `S`,
  the orientation slerping from the parked one to the reaction one on the
  way. Absent when the tool already hovers at `S`;
- **descend**: from `S` to the reaction pose `A` along `−d`, orientation
  fixed;
- **dwell**: at `A` for `[0.45, 0.55]`, reaction at `0.5`;
- **ascend**: back to `S` along `+d`;
- **outbound flight**: a straight line from `S` to `to` — the next standoff,
  with the orientation slerping to the next reaction one, or park, slerping
  back to the parked one.

**A reachable descent is collision-free by the envelope's account**: the
envelope translated along its own axis by `H` lies inside the envelope at
the reaction point, and the sweep cleared that. The envelope is the
library's claim, though, and the molecule is the fact, so the descent is
**scanned** like everything else that moves. Nothing is routed; every leg —
inbound flight, descent, ascent, outbound flight — is sampled every
`PATH_SAMPLE` (0.5 Å) of translation and five degrees of rotation, each tool
atom checked against the scene atoms within `CLASH_SEARCH_RADIUS` that are
not the tool's and not the target side's matched `before` atoms, with the
module's covalent-radius ratio. The worst ratio over the visit's legs, its
time and its pair are the visit's **contact**, reported on the panel and
the record; below the clash factor the visit *collides*. Two words, kept
apart throughout: *clearance* is the sweep's number and is in ångström;
*contact* is the scan's and is a ratio. It is a report,
never an error: a collision on a flight is the layout — a tool parked on
another's line, a park too low — and the fix is to re-park, which the user
sees at once; a collision on a descent, or a blocked approach, is the
library's envelope or the site's crowding, and a viewer of the build is
told, while the generator that emitted the step is the one that refuses
(§Architecture).

Time is allotted by **path length within each half**: the inbound legs share
`[0, 0.45)` in proportion to their lengths and the outbound legs `(0.55, 1]`
likewise, so within a half the tool moves at one speed, and a leg of zero
length — a tool parked exactly at its standoff — gets no time; each leg is eased
with a smoothstep so the corner at `S` does not snap. A chained visit, whose
inbound is one descent, spends its whole first half descending, slower than
a first visit's descent; the halves are fixed so that `0.5` stays the
reaction for every step, and how long a step lasts on a real clock is the
clock's business (§The clock).

Rejected: **the straight line from park to site.** A tool parked thirty
ångström away and nine above descends at seventeen degrees and sweeps its body
through the passivation layer beside the site; the standoff is what makes the
last leg vertical. Also rejected: **routing a flight around
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

Two layers, with one rule between them: **the engine measures, the
generator refuses, the node reports.** The feasibility layer computes the
landing and its verdict and hands both to whoever applied the step; nothing
in it fails a step, because the same call serves a generator deciding what
to emit, an editor replaying a half-written block and a viewer scrubbing a
finished build, and only the first of those wants the verdict to stop it.
The presentation layer moves pixels and cannot fail either.

| layer | contains | who calls it | can it fail a step? |
|---|---|---|---|
| **feasibility** | reaction points, the envelope, obstacle collection, the sweep, the landing (direction, clearance, reaction point, standoff height) and its `reachable` verdict; containment, at binding | `apply_step_in_scene` — so the replayer, the editor's block replay and the **sequence generator** all get it by applying a step, and the generator can also ask without applying | a step, never; a **binding**, yes: `ToolOutsideEnvelope` from `build_scene` |
| **presentation** | runs, the reaction and standoff poses with their roll, the flights, the hover, the scan, the pose at a step time | `replay_scene_at` for the node; a future exporter | never — the scan and the blocked approach are reports |

Three consequences shape the code:

**Feasibility is part of applying a step, and its verdict is the caller's.**
`apply_step_in_scene` plans the landing of every `tip` step whose tool is
bound, between the checks and the mutation, and returns it on `SceneEffect`
with its verdict; the apply itself succeeds either way, and its signature is
milestone 1's — the envelope it needs was copied onto the `ToolBinding` by
`build_scene`, and the reaction points are the operation's. The generator
learns that a site is blocked from the effect it already reads for `added`
and `touched`, and its one new line turns `!landing.reachable()` into the
"step `n` could not be emitted" it already produces for a failed match — so
an emitted script is reachable by construction, and the node never sees a
blocked step from a generator. The node and the editor keep going, because
for them the sweep is a description of the build, not a gate on it. For a
generator that wants to *try* a site before committing to it, the same two
halves are public separately: `match_step_in_scene` (the checks, returning
the plan the apply consumes) and `plan_landing`. One code path, three entry
points.

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
`obstacles_for`, `Landing`, `plan_landing`, `StepPlan`, `match_step_in_scene`,
`apply_step_in_scene` (unchanged signature, `SceneEffect` gains `landing`),
`runs` / `Runs`, and the constants. It imports nothing from the
presentation layer, and the presentation layer imports nothing from the
generator.

## Engine

New module `rust/crates/atomcad-crystolecule/src/mechanosynth/trajectory/`
with `envelope.rs` (pure geometry: `Envelope`, the sweep), `landing.rs`
(the scene-side feasibility: obstacles, the containment check `build_scene`
calls, `plan_landing`), `runs.rs` (script-only run structure) and `path.rs`
(presentation: poses, flights, hover, scan, `replay_scene_at`). `scene.rs`
gains the split of `apply_step_in_scene` into `match_step_in_scene` and the
apply half, and, in `build_scene`, the envelope copy onto the binding and
the containment call.

### Feasibility

```rust
/// A tool type's collision envelope: a cone of `half_angle` (radians here)
/// about the tool axis with its apex at the tool-side reaction point, continuing as a
/// cylinder of `radius`. On `ToolType`, and copied onto every `ToolBinding`
/// by `build_scene`, which is also where containment is checked.
pub struct Envelope { pub half_angle: f64, pub radius: f64 }

impl Envelope {
    /// The signed distance from a point at axial coordinate `s` and radial
    /// distance `rho` to the envelope's surface — to the apex point, the
    /// cone's slant, the rim circle or the cylinder wall, whichever is
    /// nearest; negative inside.
    pub fn gap(&self, s: f64, rho: f64) -> f64;
    /// The clearance of direction `d` at `at` against `obstacles`: the
    /// smallest `gap − margin` over them.
    pub fn clearance(&self, at: DVec3, d: DVec3, obstacles: &[(DVec3, f64)]) -> f64;
}

/// What the sweep found for one direction.
pub struct Approach {
    pub direction: DVec3,
    /// The smallest margin by which an obstacle clears the envelope, Å;
    /// negative when one is inside it.
    pub clearance: f64,
    /// Angle from global `+z`, radians.
    pub tilt: f64,
}

/// The `SWEEP_DIRECTIONS` unit vectors of the sweep, from `+z` down to `−z`,
/// in order of increasing tilt. Computed once (`LazyLock`).
pub fn sweep_directions() -> &'static [DVec3];

/// The free direction of least tilt from `+z` for a tool whose reaction
/// point sits at `at`, or, when no sampled direction is free, the least
/// blocked one — always an answer, the clearance carrying the verdict.
/// `obstacles` are design-space positions with their margins. **Pure
/// geometry** — no scene, so a generator can call it on whatever it holds.
pub fn approach_direction(
    envelope: &Envelope,
    at: DVec3,
    obstacles: &[(DVec3, f64)],
) -> Approach;

/// The one definition of an obstacle: every scene atom belonging to **no
/// tool** and not in `exclude`, with margin `clash · r_cov(atom)`. The
/// tool's own extent is in the envelope, not here; the other tools are
/// nowhere, so that a sequence never depends on the layout.
pub fn obstacles_for(scene: &Scene, exclude: &[u32], clash: f64) -> Vec<(DVec3, f64)>;

/// Where one `tip` step's tool reacts and from which direction — the
/// feasibility half of a visit, with no roll and no path in it.
pub struct Landing {
    pub tool: usize,                 // index into `scene.bindings`
    pub reaction_point: DVec3,       // `p_r = step.r · reaction.target + step.t`
    pub reaction_tool: DVec3,        // `reaction.tool`, in the tool's frame
    pub approach: Approach,
    /// How far up the approach the visit begins and ends, Å. Always
    /// `STANDOFF_HEIGHT`; a field so a library could one day state its own.
    pub standoff_height: f64,
}

impl Landing {
    /// `approach.clearance > 0`: every obstacle sphere is outside the
    /// envelope. A generator refuses a step whose landing is not.
    pub fn reachable(&self) -> bool;
}

/// Plans the landing of `step` on `scene` — the scene *before* the step —
/// from the plan `match_step_in_scene` produced: the reaction point placed,
/// the sweep run, the standoff height fixed. Infallible: the envelope is on
/// the binding, the reaction points are on the operation the plan holds, and
/// the sweep always answers. Called by `apply_step_in_scene` for every `tip`
/// step whose tool is bound; public for a generator that asks without
/// applying.
pub fn plan_landing(scene: &Scene, plan: &StepPlan) -> Landing;

/// The checks of a step, all of them, and nothing applied: the target-side
/// and tool-side matches, the participant rule, the pattern and steric
/// checks. What `apply_step_in_scene` did before its first mutation, split
/// out so a caller can plan a landing — or refuse a step — without moving an
/// atom.
pub struct StepPlan { /* the two matches, the target participant, the op, the clash factor */ }
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
    /// Appended: the landing of a `tip` step whose tool was bound, verdict
    /// included — a generator checks `reachable()` here.
    pub landing: Option<Landing>,
}

pub const STANDOFF_HEIGHT: f64 = 6.0;        // Å
pub const CLEAR_MARGIN: f64 = 0.5;           // Å
pub const SWEEP_DIRECTIONS: usize = 256;
pub const SWEEP_REFINEMENTS: usize = 8;
```

`replay_steps` keeps its arguments and gains a return: beside the failure it
already reports, the `landing` of every step it applied, in step order
(`None` for a step that is not `tip` or whose tool is unbound), because the
presentation layer needs them to carry a tool's orientation along a run.
Landing a step cannot fail it, so **milestone 1's output is unchanged for
every build that loads**: `replay_scene(k)` returns the scene it returned
before, in the node, in the editor's block replay and in the generator
alike. The one new way for a build to fail is at binding,
`ToolOutsideEnvelope`, which names a library whose envelope is smaller than
the molecule playing it — a library error, reported where the frame residual
is.

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
pub fn reaction_pose(landing: &Landing, from: &Pose) -> Pose;
/// The reaction pose lifted by `standoff_height` along the direction.
pub fn standoff_pose(landing: &Landing, reaction: &Pose) -> Pose;

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
    /// The scan over the visit's legs, descent and ascent included.
    pub scan: PathScan,
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
    /// Which leg `u` falls on. The panel's readout — and the engine's, not the
    /// API's: it reads the **same length split** `pose_at` interpolates along,
    /// so the word and the pose cannot disagree. An API that decided the word
    /// for itself would have had to re-derive that split from `Visit`'s fields
    /// and would drift from it at the first change.
    pub fn leg_at(&self, u: f64) -> Leg;
}

/// Which part of its visit a tool is on at a step time, in the panel's own
/// words. `Hovering` is not a leg of a visit at all — it is what a tool waiting
/// over its next site through a `spontaneous` step is doing.
pub enum Leg {
    FlyingFromPark,
    Descending,
    AtSiteBefore,
    AtSiteReacted,
    Ascending,
    FlyingToNextSite,
    ReturningToPark,
    Hovering,
}

impl Leg {
    /// `flying from park`, `descending`, `at site (before)`,
    /// `at site (reacted)`, `ascending`, `flying to next site`,
    /// `returning to park`, `hovering over next site`.
    pub fn as_str(&self) -> &'static str;
}

/// The scan of a visit's legs. Its own type, not `apply::Contact`, whose
/// fields describe a placed pattern atom against a host.
pub struct PathScan {
    /// The worst pair along the legs, or `None` when no non-tool atom came
    /// within `CLASH_SEARCH_RADIUS` of any tool atom at any sample.
    pub worst: Option<PathContact>,
}

pub struct PathContact {
    /// Centre-to-centre distance over the sum of the two covalent radii.
    pub ratio: f64,
    pub distance: f64,
    /// Step time of the sample, in `[0, 1]`.
    pub at: f64,
    pub tool_atom: u32,
    pub other: u32,
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
tools included, for every build that binds. The tool positions a
*viewer* sees at `time = 1.0` differ inside a run, because the node applies
the motion; the engine's scene does not.

**Per evaluation**, for `(k, u)`, `1 ≤ k ≤ n`:

1. `runs(script, library)` — once, cheap, script-only. Then `build_scene`
   and `replay_steps` to `k − 1`, every `tip` step of it landed and the
   landings kept. A failure there is an error as today.
2. `match_step_in_scene` for step `k`, then `plan_landing` if it is `tip`
   with a bound tool. Step `k` failing its match is an error at `(k, u)` for
   every `u`: the step is selected, and its checks are what selecting it
   means. The partial state is reachable by asking for one step fewer. A
   blocked landing is not a failure: the visit is built along the sweep's
   best direction and reported.
3. If `u ≥ REACTION`: the apply half of step `k`, both sides, at the
   **bound** pose — the tool side matches at the binding's pose, the cargo
   enters the scene at the bound apex, and nothing in the matching or the
   steric check changes.
4. **Look-ahead**, for a `tip` step `k` whose run continues at step `j`, or
   a `spontaneous` step `k` inside a run whose next visit is `j`: on a
   **clone** of the scene after `k` (the step applied to the clone if `u`
   has not applied it), apply the `spontaneous` steps up to `j − 1`, then
   `match_step_in_scene` and `plan_landing` step `j`; its standoff is `to`,
   or the hover pose. A match failure along the way makes `to` park instead
   — silently here, because the replay reports it at the step that fails; a
   blocked landing at `j` is still a standoff, and is reported when the
   replay reaches `j`. The clone is discarded. This is the only place the engine looks past the
   selected step, and it costs one structure copy plus the settles.
5. The `ToolMotion` is assembled: `reaction_pose` from the step's own landing
   and the binding's park, the standoff, `from_park` / `to_park` from the run
   structure, the look-ahead's `to`, every leg scanned. A hover is the
   look-ahead's standoff in that visit's own reaction orientation — the same
   pose the previous step's outbound flight aimed at, because both read the
   same landing. **No earlier visit's landing is read**, which is why
   `replay_steps` is asked for `LandingPlan::Last` rather than one per step.
6. Highlights. **Nothing to do**: `replay_steps` has already painted the last
   *applied* step's `touched` set, which before the reaction is step
   `k − 1`'s and from the reaction on is step `k`'s. The tag therefore moves
   in the same instant the workpiece does, at `REACTION`. `ms_added` and
   `ms_layer` follow the applied set and change meaning not at all.

   This is a **reversal**. The design originally lit step `k`'s matched
   `before` atoms during the approach, on the theory that a site the tool is
   descending on wants marking. Animating a build showed that for what it is:
   with the site already lit, the reaction has *visually* happened when the
   tool sets off, and the one instant the whole timeline is built around
   (§The reaction is at the middle of the dwell) reads as nothing at all. A
   highlight that anticipates its own cause is worse than no highlight, and
   before the reaction the honest thing to mark is the last reaction that
   *did* happen — which is what the applied set already says.

The caller then draws: `let mut shown = scene.structure.clone();
apply_tool_pose(&mut shown, &scene, motion.tool(), &motion.pose_at(u))`.

**Errors.** One new `MechanosynthError` variant, raised by `build_scene`:
`ToolOutsideEnvelope { tool, tool_type, op, atom, excess }`. No step-time
variant: a blocked approach is a report (§Architecture). Parse-time `Invalid` locations: legs on both sides of the apex's plane or a leg on it, a
missing or out-of-range `envelope`, a missing `reaction` on a `tip`
operation or a present one elsewhere, a non-positive `duration`, a `/3`
format string.

## The `mechanosynth` node

**Pins.** Inputs `base`, `ops`, `steps`, `step`, `feedstocks`, `tools`
unchanged; **appended** `time: Float` (pin 6), optional, overriding the
property, clamped to `[0, 1]`.

**Property.** `time: Float`, default `1.0`; serialised with a serde default so
every saved project loads at `1.0`; in the text format
`build = mechanosynth { step: 12, time: 0.3 }`, and **written at its default
too**, `time: 1`, like `step: -1` beside it.

An earlier draft of this document said "omitted at the default like every other
default". There is no such rule, and the opposite one is load-bearing:
`get_text_properties` **must be total**
(`text_format/AGENTS.md`). The editor decides whether a pin takes a literal by
asking a *fresh* node which properties it has, so a property emitted only when
set reads back as **wire-only** and its literal is dropped with a warning — the
bug `a_structure_rot_axis_survives_a_replace` pins for `structure_rot`'s
`axis_index`. A `time` written only when it was not `1.0` could therefore never
be *set* from the text on a node that still had the default.

The clamp is applied at **evaluation**, to the pin and the property alike, and
not in the setter, so the stored number round-trips as written and only the
outputs and the record see the clamped one. The round-trip corpus
(`query` → `--replace` a no-op) is re-run.

**Outputs.** `result` is the workpiece after `k − 1` steps for `time < 0.5`
and after `k` from `0.5`. `scene` is the same rule with the moving or
hovering tool at its pose. `step` gains five appended fields:

| field | type | meaning |
|---|---|---|
| `time` | Float | the clamped step time the outputs were computed at |
| `tool_r` | Mat3 | the moving or hovering tool's frame at `time`, `R`; identity when every tool is parked |
| `tool_t` | Vec3 | its frame origin (the apex), `t`; zero when every tool is parked |
| `approach` | Float | the sweep's clearance for the visit, Å, capped at `10.0`: positive means the site is reachable, negative that it is blocked and the tool visits along the least-blocked direction; `10.0` when nothing visits |
| `contact` | Float | the visit's worst contact ratio over its legs, capped at `2.0`; `2.0` when nothing came near or nothing moves |

`tool_r` / `tool_t` are what a camera follow or a gadget downstream needs and
cannot derive; `approach` and `contact` are what a style rule needs to
paint a tool whose site is blocked or whose flight collides. The caps keep
the record finite — an empty scan and an empty sweep are both infinities —
and cut nothing a rule would read: a contact at twice the covalent sum is no
contact, and ten ångström of clearance is open sky.

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
its ticks (`isosurface`'s precedent). Unlike `isosurface`, the drag **does**
write the kernel before it ends — the viewport moving under the hand is the
feature — but **at most once per frame**, which the first draft of this section
got wrong by saying "each tick". A write is `frb(sync)`: an evaluation, a
tessellation of the whole scene and a GPU upload, on the UI thread. `Slider`
already drops a repeat of the same discretised value, but a full traversal still
crosses a hundred divisions, and writing each one as the pointer handler delivers
it queues a hundred blocking refreshes for a gesture that can afford a handful —
measured at 26 ms of evaluation alone on the silicon demo, plus the tessellation,
which made the drag crawl seconds behind the pointer. The panel therefore holds
the pointer's value in the widget's own state and renders the slider from it, so
the thumb tracks the hand at frame rate whatever the kernel costs, and queues one
write for the end of the frame carrying the latest value, with a trailing write
on release. `xray`'s per-tick alpha slider is not a precedent for this; it is a
precedent for a *cheap* node.

## API and panel

`APIMechanosynthData` gains `time: f64`. `APIMechanosynthInfo` gains `time`,
`leg: String` — `motion.leg_at(time).as_str()`, so the eight words are the
engine's (§Presentation) and empty when every tool is parked —
`tilt_degrees: f64` and `approach_clearance: f64` from the sweep,
`contact_ratio: f64`, `contact_at: f64` and `collision: String` from the
scan (the panel sentence, empty when clear), all read off the last
evaluation the way the tool rows are — which means the node parks the
`ToolMotion` beside `last_scene`, for the same reason it parks the scene: a
panel rebuild must not cost a replay.

`time` itself is read the way `applied` is: the **wired** `time` pin when one is
connected, else the stored property, clamped — so the readout names the point
the outputs were computed at rather than the one the node happens to store.
`approach_clearance` and `contact_ratio` take the **same caps** as the record's
`approach` and `contact`, and read at those caps when nothing visits, so the two
surfaces never report a different number for the same absence.

**Panel.** Under the step scrubber, a **time** row: a `Slider` over `[0, 1]`
with a tick at `0.5`, live during the drag and bracketed for undo, a float box
beside it, both disabled when the `time` pin is wired (the rule the step row
uses). Under it two readout lines. The approach line:
`approach: vertical, clear by 1.8 Å` or `approach: tilted 23°, clear by
0.5 Å`, or, in the warning colour, `approach: blocked — best is tilted 41°,
0.3 Å short`. The path line: `path clear (worst 1.32)`, or, in the warning
colour, `path collides at 41 %: O of si_tool against Si 481, ratio 0.62`.
Nothing about the approach reaches the result pin as an error: a blocked
site is this line and a negative `approach` on the record. The tools block is unchanged;
the row of a tool that is away from park is prefixed with a marker.

## Playing the build: the transport row

Phase 3 made a build **scrubbable**, and scrubbing is not showing. A
colleague watching over a shoulder wants the build to *run*, and the only way
to run it today is to drag the step slider — which advances `step` and leaves
`time` exactly where the last drag left it, so every step after the first is
entered part-way through its own visit and the motion breaks at every
boundary. What is missing is a **driver**: something that advances `time` by
itself and, when a step is used up, advances `step` and puts `time` back to
zero.

The driver is small, and it is small because the two phases before it built
everything it needs. `setMechanosynthData` carries `step` and `time` in **one**
write, so a boundary is one call and never a frame with the new step at the old
time; `beginNodeDataDrag` / `endNodeDataDrag` already coalesce a gesture into a
single undo entry; `APIMechanosynthInfo.count` already says where the script
ends and `applied` already resolves the stored `-1`. **This phase is Dart
only** — no engine change, no node change, no API change, no FRB
regeneration.

**Hold to play.** The control is a button that plays *while it is held* and
stops the moment it is released. It is the scrub the user already knows,
performed by a clock instead of by a hand, and inheriting that shape is worth
more than it looks: there is no transport state to fall out of sync with the
node, no stop button to hunt for, no way to leave the panel playing, and
letting go is always the way out. It also happens to be the gesture a
presenter wants — press to advance, release to talk, press again — which a
fire-and-forget toggle would not give without a second control. The cost is
real and accepted: a 171-step build is about three minutes of held button.
A latch and a speed control are in §Follow-ups, not here.

**The transport row goes directly under the panel header, above the step
scrubber.** Vertical space in the node properties panel is the scarcest thing
in this UI — the panel and the viewport share one column, and the mechanosynth
panel is already the tallest in the application — so the one control a demo
cannot do without must be the one that is on screen before anything is
scrolled. The row is a single line: **rewind**, **play**, and to their right a
compact position readout (`step 12 / 171 · time 0.43`). The readout duplicates
what the two sliders below say, deliberately: it makes the top row *sufficient*
for a demo, so everything under it can be scrolled away without losing the
thread. The general remedies for the panel's height — a density pass, and the
option to dock the properties panel to the right — are a separate piece of
work and no part of this phase.

**The driver advances by the wall clock, and never skips a step.** A `Ticker`
reports elapsed time; each tick takes `dt` since the previous one and adds
`dt / playStepSeconds(speed)` to the time. When the accumulated time reaches 1.0
the write becomes the next step at time 0, and **the excess is dropped rather
than carried**. On a machine where one step costs more than one frame's budget,
carrying the remainder would silently skip steps and show a build that never
happened; falling behind the wall clock is the acceptable failure, showing the
wrong build is not. So a tick advances at most one step, and a slow scene plays
slow.

**The speed is an integer multiplier, and the default is the second stop.**
`PLAY_BASE_STEP_SECONDS` is 2.0 and the row's `×n` field divides it, so ×1 is two
seconds a step, ×2 one, ×4 a half. The default is **×2** — the only rate the
driver had before it was adjustable — and that choice is the whole point of the
base being 2: making the default the *second* stop rather than the first buys a
**slower** gear as well as faster ones. A step worth narrating while it happens
is worth two seconds, and a base-as-default would have had nowhere to go but up.
The range stops at ×8: below a quarter-second a step the kernel is the limit
rather than the setting, and a larger number would buy nothing but a misleading
readout.

The value is read **on each tick**, never cached into the accumulator, so
turning the speed up mid-press takes effect on the next frame with nothing
recomputed and nothing jumping — the next tick is simply longer or shorter.

It is a bare `IntSpinField` with a `×` prefix rather than a labelled `IntInput`:
a caption over the box would double the row's height, which is the one thing
this row may not spend. The spin field brings the − / + buttons, the wheel and
the arrow keys with it (`lib/AGENTS.md`), so the increment logic is not written
a second time.

**The speed is session state on `StructureDesignerModel`, not node data.** It
changes no atom, so it has no business in the `.cnnd` — a project file that
differed over how fast someone once played it would be a project file that
differed over nothing — and no business on the undo stack either. It lives on
the model rather than in the panel so that clicking to another node and back
does not reset a speed the presenter just set. It does not survive a restart; if
it ever needs to, the envelope cage's route (a `preferences.json` field, api
twin, preferences window) is the precedent, and the project file still is not.

**Every tick writes, so there is no preview state.** This is where the driver
is *simpler* than the drag it resembles, and the reason is worth stating
because the two look alike. A pointer delivers ticks faster than frames, which
is why `MechanosynthTimeRow` holds the dragged value and queues one write for
the end of the frame; a `Ticker` fires exactly once per frame, so every tick is
already the only write that frame and the coalescing machinery has nothing to
do. The write goes through `refresh_structure_designer_auto` synchronously, so
the frame's cost *is* the refresh's cost and the next tick's `dt` has it
already measured. Nothing is held for rendering: the sliders and the readouts
show the kernel's own numbers, as they do at rest. The driver keeps only the
`(step, time)` it is advancing, seeded at press from `info.applied` and
`info.time`, because the kernel's clamp is not a place to accumulate a
fraction.

**One press, one undo entry.** The press brackets the whole run with
`beginNodeDataDrag` / `endNodeDataDrag`, the way a slider drag does, so a
three-minute play costs one Ctrl-Z rather than a hundred and seventy-one. The
bracket must be closed on every exit, not just on release: a pointer cancel,
the end of the script, and `dispose` of a panel whose node was deselected
mid-play all stop the ticker and end the session — the `_dragging` field in
`MechanosynthTimeRow` is the precedent, and it exists for exactly this.

**The end of the script stops it; the next press starts over.** Reaching the
last step writes `(count, 1.0)`, stops the ticker and returns the button to its
idle look while the pointer is still down; the build does not loop, because a
finished workpiece flickering back to a bare slab is not something anyone wants
to watch twice. A press made when the scene is already at the end restarts from
`(0, 0.0)` — the only thing such a press can mean. **Rewind** is the same jump
on its own button, as one undo entry, for the presenter who wants to reset
without playing.

**A wired pin disables the row.** The driver must write both `step` and `time`,
so it is enabled only when **neither** pin is wired; otherwise it greys out
beside the same wired hint the two rows below it already show. Playing half the
pair would advance a number the node ignores.

**What this deliberately is not.** It is not the clock of §*The clock milestone
2 will need*. It does not read the library's reserved `duration`, does not give
a flight more time than a settle, does not group a `bulk` event's steps or run
two tools at once, and does not export frames. Every step gets the same second
and a gating step is a second of a still scene. When the real clock arrives it
takes this button as its transport rather than replacing it — the press, the
bracket, the one-write-per-frame rule and the placement all survive the change;
only the function from elapsed time to `(k, u)` gets richer.

Rejected: **seconds-per-step as the field** rather than a multiplier — the
quantity a presenter thinks in is speed, and ×4 says "four times as fast" where
`0.5 s` needs dividing first. **Persisting the speed in the project** — see
above. **A play/pause toggle** — it is a mode, and a mode in a panel that
rebuilds from the kernel on every write is a thing to keep in sync for a
benefit the hold already delivers. **A driver node** over `scene` and `step`,
feeding the pins from a network-level clock — that is milestone 2's question
and answering it here would settle it on a demo's evidence. **Animating in the
kernel**, i.e. a replay that returns a sequence — the scene is re-evaluated per
frame anyway and a sequence would have to be held somewhere; the driver's whole
claim is that the existing per-frame write is enough.

## What playing skips: the playable steps

Phase 5 gives every step the same second, and a real build does not deserve
one. The silicon demo's 171 steps are mostly `spontaneous` settles, and a
settle has **no motion at all** — the tool holds its hover pose for the whole
step while the crystal relaxes under it. Played at one second each they cost
minutes and, worse, they *break the run*: a shuttle descends, reacts, lifts,
flies — and then stands perfectly still for three seconds before descending
again. The motion the trajectory engine exists to show is chopped up by the
steps that have none.

So the driver does not walk the script one step at a time. It walks a list of
**playable steps**, and the rule that builds it is one rule with two
consequences:

> A `tip` step is playable. A maximal block of consecutive non-`tip` steps is
> **one** playable step — its last — if the block contains a `bulk` step, and
> **no** playable step at all if the block is nothing but settles.

A settle between two visits therefore disappears from the playback, and a
chlorination phase of `bulk` exposures with their settles between them plays as
a single second landing on the phase's last step. Everything in between is
still *applied* — jumping to step `m` means the first `m − 1` steps have run —
so the scene the next second starts from is exactly the scene the skipped steps
produced. Nothing is dropped; only the waiting is.

**Skipping a settle is pose-continuous, which is why this is a pacing change
and not an animation change.** `path.rs` poses a `spontaneous` step that a run
spans as a static `ToolMotion::Hover` at the **next** visit's standoff — the
very pose the previous visit's flight ends on at `time 1.0`, and the one the
next visit begins from at `time 0.0` (§Presentation). So the jump from
`(k, 1.0)` to `(m, 0.0)` moves the tool **not at all**: only the workpiece
changes, instantly, which is what a relaxation looks like when you are not
watching it. No interpolation, no new geometry, nothing in the engine to
change. The same is true of a bulk block for the trivial reason that a `bulk`
step ends a run and parks every tool.

**Only what can be proved skippable is skipped.** The method is the
*operation's*, so a step whose operation the wired library does not define — or
any step at all when nothing is wired to `ops` — has no known method, and those
steps stay playable and end whatever block is open. Playback then degrades to
Phase 5's every-step walk rather than silently dropping steps a user asked to
see. `runs()` already ends a run on an unknown operation for the same reason,
and this rule is deliberately the same shape.

**The list is the kernel's, because the panel cannot see a method.** The parsed
script never crosses the bridge and `APIMechanosynthInfo` carries
`current_method` for the selected step alone, so a Dart-side rule is not
available at any price. `APIMechanosynthInfo` gains:

| field | type | meaning |
|---|---|---|
| `playable` | `Vec<i32>` | the 1-based step numbers the transport stops on, ascending. Empty when no script is loaded |

filled in `mechanosynth_info`, which already holds both the script and the
library. The rule itself is a pure function of the two —
`playable_steps(script, library) -> Vec<usize>` in
`trajectory/runs.rs`, beside `runs()`, which is the other statement about a
script's structure that needs the library and no scene. It lives there rather
than in `api` so it is tested in the crystolecule harness against `Method`
directly, the way §Testing requires; the api layer only maps it to 1-based
`i32`s. `chapters()` is the precedent for a list the panel navigates by and
could not compute.

**What changes in the transport.** One line of the driver: at a boundary, the
next step is the first entry of `playable` greater than the current one rather
than `current + 1`. Everything else in Phase 5 is untouched — the hold, the
bracket, the one-write-per-frame rule, rewind, and the end-of-script stop.
Three details fall out of it and are worth stating because each is a test:

- **A press from a step that is not playable still plays that step**, to its
  end, and then jumps. The user scrubbed there deliberately; the transport is
  not entitled to skip the thing they are looking at.
- **The end lands on `count`, playable or not.** When no entry is greater than
  the current step the driver writes `(count, 1.0)` and stops, as it already
  does — so a script ending in settles finishes on the settled workpiece rather
  than stopping short of it.
- **Step 0 is still a beat on the bare base**, and its boundary goes to the
  *first* playable step rather than to step 1.

**It is not a switch.** A checkbox would put a second control in a row whose
whole argument was that it stays one line, and it would be offering a choice
between watching the build and watching a still frame of it. The step scrubber
is still there, unchanged, for anyone who wants to look at a settle: scrubbing
is inspection, playing is presentation, and this is the difference between
them.

Rejected: **a per-step method array in the API**, which would put the policy in
Dart and ship 171 strings a panel would have to re-derive a block structure
from on every rebuild. **Playing a settle faster** rather than not at all — a
compressed second of a still scene is still a still scene, and it would need a
per-step rate the driver does not otherwise have. **Reading the library's
`duration`** to pace the skipping: `duration` is milestone 2's, it is reserved
and unpopulated today, and a rule that depends on it would not work on any
library that exists.

## The envelope cage: a viewport overlay switched in the preferences

The envelope is the one thing in this design a user cannot otherwise see,
and it is the thing a wrong tilt or a blocked site is explained by. So the
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
  middle, runs and hovering), the two readout lines, what a blocked site
  looks like,
  and **how to lay out a scene**: put each tool on a **different side** of the
  workpiece and its reservoirs — one left, one right, one in front — clear of
  the work itself. Nothing checks this, because the sweep does not look at
  tools; what it buys is that no flight crosses another tool and no park sits
  where the build will grow. A tool then leaves park once per run, works across
  the scene, and goes home when its run ends.
- `doc/reference_guide/nodes/atomic.md` §mechanosynth again, a
  §*Playing the build*: the transport row at the top of the panel, that the
  play button plays while held and stops on release, that a run is one undo
  entry, that the end of the script stops it and the next press starts over,
  and that a wired `step` or `time` pin greys the row out. Phase 5's, not
  Phase 4's.
- `doc/reference_guide/nodes/atomic.md` §*Playing the build* again, a
  paragraph on **what playing skips**: that settles are stepped over and a run
  of bulk exposures plays as one beat on its last step, that nothing is
  dropped — every skipped step is still applied — that the scrubber is where a
  settle is looked at, and that a step whose operation the library does not
  define is never skipped. Phase 6's.
- `doc/reference_guide/nodes/math_programming.md`, the `MechanosynthStep`
  record: the five appended fields.
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
  that commit and must evaluate to the same atoms afterwards: `result`
  always, because the engine's scene is never moved (§Architecture), and
  the `scene` pin too, because none of the four stores a step inside a run
  — `mechanosynth_tools.cnnd` stores step 3, the last visit of its
  `habst_tool` run, after which the tool is home. A fixture that stored
  step 1 or 2 would show the tool hovering on `scene` and would be
  re-snapshotted with it; that is the design, not a leak. The `result`
  equality is the stronger regression, and the one that would catch a
  landing leaking into the replay.

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
script, `time: 0.3` stored, and an **obstacle over the workpiece site**: a
copy of `tool_probe.xyz` with its tags stripped, wired as a second
feedstock — not a second molecule of type `probe`, which `build_scene`
refuses as `ToolDuplicate` — for the node snapshot and the round-trip
corpus. In the engine tests the same obstacle is built in code, an untagged
`tool_probe.xyz` translated over the site and passed on `feedstocks`; no
`.xyz` fixture is added for it.

### Feasibility layer — `crates/atomcad-crystolecule/tests/crystolecule/mechanosynth_trajectory_test.rs`

Pure geometry, no scene:

- `Envelope::gap`: negative inside; zero on the slant, on the rim and on
  the cylinder; for a point beside the slant at radial excess `x` it is
  `x · cos α`, **not** `x` — the assertion that pins the distance against
  the radial gap an earlier draft used; the apex-point case for a point
  behind the apex; `Envelope::clearance` on one obstacle inside, on the
  surface, and outside the envelope has the sign and magnitude the formula
  says;
- `sweep_directions()`: the first is `+z`, the last `−z`, tilt is
  non-decreasing along the sequence, consecutive tilts never jump by more
  than the sphere's spacing, and every entry is a unit vector;
- `approach_direction` with no obstacles returns exactly `+z` with tilt
  `0`; with a single obstacle on the `+z` axis returns a direction tilted
  just past it (clearance at least `CLEAR_MARGIN`, tilt within a
  refinement step of the minimum); with obstacles enclosing the point
  returns a negative clearance and the least blocked of the sampled
  directions; with every candidate blocked but one below the margin
  returns that one; the same inputs give the same output (determinism).

Through the scene, on `tool_scene.xyz` and the tagged fixture tools:

- `obstacles_for` excludes the tool's atoms and the excluded set, includes
  every other participant's atoms, and its margins are
  `clash · (r_cov + r_tool)`;
- `plan_landing` puts `reaction_point` at `step.r · reaction.target +
  step.t`; the standoff is `STANDOFF_HEIGHT` up the approach direction, the
  same for a vertical approach and a tilted one, and a tool parked anywhere at
  all gives the same landing;
- `build_scene` copies the type's envelope onto the binding, and on a
  molecule with an atom outside that envelope at some `tip` operation's
  tool-side reaction point fails with `ToolOutsideEnvelope` naming the atom
  and the operation;
- the probe parked over `habst_tool`'s site tilts that tool's approach;
  moving it away makes the approach vertical again;
- **the generator's path**: `apply_step_in_scene` on a `tip` step with a
  bound tool returns a `landing` equal to what `match_step_in_scene`
  followed by `plan_landing` returns, and the latter leaves the scene
  untouched; on an enclosed site the apply **succeeds**, applies the step,
  and returns a landing whose `reachable()` is false with a negative
  clearance — the verdict the generator turns into a refusal;
- `replay_steps` returns one landing per applied `tip` step with a bound
  tool and `None` elsewhere; a script whose step `j` is blocked replays to
  its end with the landing at `j` blocked, and `replay_scene(k)` is
  milestone 1's for every `k` — nothing about a landing changes the
  replay's output;
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
  `1e-9`, the tool's axis is the approach direction, and the rotation from the
  **parked** pose has the angle between the two axes and no more (minimal
  roll); it is memoryless — two landings with the same approach give the same
  orientation whatever sat between them in the run, and each tool reads its own
  park; an approach exactly antiparallel to the parked axis still turns
  deterministically;
- `replay_steps` plans **only the landings the caller asked for**:
  `LandingPlan::None` plans none, `Last` plans the last applied step's when it
  lands, `All` one per `tip` step with a bound tool. The budget test, because a
  landing is a sweep and nothing else in the replay comes close to its cost;
- `ToolMotion::leg_at`: a first visit whose run continues names its six legs
  in order (`flying from park`, `descending`, the two dwell words, `ascending`,
  `flying to next site`); a lone visit ends `returning to park`; a chained
  visit's inbound half is one `descending` and never a flight; a hover is
  `hovering over next site` at every `u`; and the leg agrees with the pose —
  where the word first becomes `descending` the tool is at its standoff;
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
- the scan: a slab placed across a flight reports a ratio below
  `CLASH_BLOCK` at a time inside that flight; the same scene with the slab
  removed reports none; a chained visit's inbound scan covers only its
  descent; an obstacle just outside the envelope's slant — clear by the
  sweep's account — is reported by the descent's scan when the molecule
  reaches it, which is why the descent is scanned;
- a blocked site (the site boxed in by obstacles built in code): the visit
  is built along the sweep's best direction, `pose_at` is as continuous as
  for any other visit, the motion carries the negative clearance, and the
  replay does not fail;
- `replay_scene_at(k, 1.0)` returns a scene equal to `replay_scene(k)`
  atom for atom, tools included, for every `k` of both build fixtures —
  the compatibility assertion and the proof that the engine never moves a
  scene; `(k, u < 0.5)` has the workpiece of `replay_scene(k − 1)`,
  `(k, u ≥ 0.5)` that of `replay_scene(k)`; `apply_tool_pose` on a clone
  leaves every non-tool atom, every bond and every tag untouched and keeps
  the tool's atoms pairwise-distance-preserved from their bound positions;
- `ms_current` moves **at** the reaction and not before: nothing is lit while
  step 1's tool descends, the step's `touched` set appears at `u = 0.5` and
  holds to `u = 1`, and through step 2's approach it is still step 1's — the
  tag agrees with the workpiece at every `u`.

### Node layer — `crates/atomcad-structure-designer/tests/structure_designer/mechanosynth_trajectory_node_test.rs`

Wiring only, as `mechanosynth_tools_node_test.rs` does — every structural
assertion is an equality against the engine:

- the `time` pin is index 6, optional, and overrides the property; an
  out-of-range value on either is clamped; `result`, `scene` and the record
  at `(k, u)` equal what `replay_scene_at` and `apply_tool_pose` produce;
- the five record fields are appended after `agent`, `tool_r` / `tool_t`
  are the motion's pose and the identity / zero with every tool parked,
  `approach` is the sweep's clearance capped at `10.0` and negative on the
  blocked scene, `contact` is the scan's ratio capped at `2.0`;
- the `scene` pin carries the tool at its flown pose while `last_scene`
  keeps it bound — the two are compared directly;
- a `mechanosynth` node with a bound tool emits one `ToolEnvelope` overlay
  per tool, apex at the nearest visit's reaction point, whose segments move
  with the flying tool and stand still for a parked one;
- persistence: `time` round-trips through a `.cnnd`; a node saved **without**
  `time` — a project from before the property existed — loads at `1.0`; the
  text format writes `time: 0.3`, and writes `time: 1` at the default too,
  because `get_text_properties` is total (§The `mechanosynth` node);
- `nodes/node_snapshots_test.rs` gains `mechanosynth_trajectory_evaluation`
  on the new `.cnnd` — with `time: 0.3` stored, pin 0 is the workpiece after
  `k − 1`, which pins the gating through the real loader;
- `text_format_roundtrip_corpus_test.rs` gains the new `.cnnd`. **This is what
  found the `import_xyz` bug**: the node emitted `file_name` only when set, so
  by the totality rule above a `query` → `--replace` dropped the file name of
  every imported structure — the `structure_rot` bug again, in a node the corpus
  had never covered because `demolib` imports nothing. Making it total (and
  reading `""` back as `None`, as `mechanosynth`'s own two file names do) is part
  of this phase. `import_cif` and `import_cube` have the same shape and were not
  audited.

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
  `approach_clearance`, `contact_ratio`, `contact_at` and `collision` off
  the last evaluation: `leg` takes each of its eight values at a `u` inside
  that leg and is empty at a `bulk` step; `collision` is the panel sentence for the
  slab scene and empty otherwise; `approach_clearance` is negative on the
  blocked scene and the info is still returned — no error;
- the preferences API twin round-trips the two new fields;
- (Phase 6) `mechanosynth_info` carries `playable` as **1-based** step numbers
  and nothing else — the rule is tested in the crystolecule harness, so what
  the api test owns is the index base, that the list is empty with no script,
  and that it is every step when no library is wired.

### Playable steps — `crates/atomcad-crystolecule/tests/crystolecule/mechanosynth_trajectory_test.rs`

`playable_steps` is a pure function of a script and a library, so it is tested
as one, beside `runs()`: a script of nothing but `tip` steps is every step; a
settle between two visits is absent while both visits are present; a block of
`bulk` steps with settles between them yields its **last** step and nothing
else; a block of settles alone yields nothing; a settle trailing the last visit
yields nothing (the driver's end-of-script write is what covers it); a step
naming an operation the library does not define is playable **and** ends the
block it would otherwise have joined; and an empty library makes every step
playable. `trajectory_build.json` is the combined case — a real script,
written for something else, that happens to hold one settle between two visits
and one trailing exposure. The 171-step silicon build is **not** a fixture here
(it lives outside the repository), so its shortening is a manual check, not a
test.

### Dart — `test/`

`mechanosynth_time_row_test.dart`, on a widget extracted the way
`MechanosynthScrubber` was — plain numbers and callbacks, no kernel: the
slider reports every tick through `onChanged` (unlike the step scrubber,
which reports once), `onDragStart` / `onDragEnd` fire once each around a
drag, the row is disabled when the pin is wired, the float box round-trips a
typed value, and the tick sits at `0.5`. The preferences window's two new
controls get `PreferencesKeys` entries so the human smoke test can find
them.

`mechanosynth_transport_test.dart`, on the same kind of widget — a driver over
plain numbers and callbacks, pumped with `tester.pump(Duration(...))` so the
ticker is the test's clock: a press then a pump of half `PLAY_STEP_SECONDS`
reports a time of `0.5` at the same step; a pump across the boundary reports
the next step at time `0.0`; a single pump of ten step-lengths advances
**one** step, not ten; a press at the last step's end restarts at `(0, 0.0)`;
the end of the script stops the ticker with the pointer still down and writes
nothing further; `onDragStart` / `onDragEnd` fire exactly once around a press,
once around a press ended by the script's end, and once around a press ended
by `dispose`; rewind writes `(0, 0.0)` and starts no ticker; the row is
disabled when either pin is wired. The speed: `playStepSeconds` divides the base
and clamps its argument, ×1 takes twice the wall clock ×2 does, a change
mid-press writes nothing by itself and the next tick advances at the new rate,
and no value outside the row's range is ever reported.

Phase 6 adds to it: given a `playable` list, a boundary lands on the next entry
rather than on `current + 1`; a settle between two entries is never written; a
press from a step that is not in the list plays *that* step to its end before
jumping; a current step past the last entry ends the run at `(count, 1.0)`; and
an empty list is Phase 5's every-step walk. The rule that builds the list is
**not** tested here — it is Rust, and lives with `runs()`.

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
whether a played build actually reads as continuous at Phase 5's one second a
step, whether Phase 6 skips the right things — a run that reads as one
movement, a bulk phase that reads as one event — and the Flutter smoke test,
which stays the human's.

## Phases

### Phase 1 — Schema and engine

The `/4` parser (`envelope`, `reaction`, `duration`, the leg rule, `approach`
removed); `trajectory/envelope.rs` and `landing.rs` first — the feasibility
layer, with `match_step_in_scene` split out of `apply_step_in_scene` and the
apply landing every `tip` step — then `runs.rs` and `path.rs` with
`replay_scene_at`, the look-ahead and `replay_scene` as its `1.0` wrapper;
the engine tests above. The external generator is rebuilt against the new
`SceneEffect` (one added field), gains the one check that refuses a step
whose landing is not `reachable()`, and **moves its parks**: today it parks
the tools 9.33 Å above the top face with `si_tool` between the workpiece and
the reservoir, which is on the line of every reservoir-to-workpiece flight.
The parks go where §Reference guide tells a user to put them — each tool on
its own side of the work, clear of it. Nothing in the sweep depends on that
any more, so the kickoff check would pass either way; what the layout buys is
a scan that reports no crossed flight and a steric rule that refuses nothing. The fixture
migration of §Testing is the first commit. Kickoff check: the silicon v3
file set, regenerated as `/4` by the external generator with envelopes,
reaction points and the new parks, replays at `(k, 1.0)` with the workpiece
identical to milestone 1 for all 171 steps; every `tip` step is reachable
and vertical (the demo's sites are all on the top face); every visit's scan is
clear; at `(k, 0.5)` every cargo sits on
its target's reaction point; and the cover phase is **one run per tool**:
the shuttle leaves park once, alternates reservoir and workpiece for
sixty-two visits with the settles passing under it, and returns once.

### Phase 2 — Node, record, API, text format

The `time` property and pin, the five record fields, `APIMechanosynthData`
and `APIMechanosynthInfo`, the loader default, the text-format property,
FRB regeneration, node tests, the round-trip corpus. `Leg` and
`ToolMotion::leg_at` are Phase 1 work that Phase 2 found missing: the panel's
`leg` is a fact about the motion, so it is the engine's to state
(§Presentation). Two guide pages come with it rather than waiting for Phase 4 —
`nodes/atomic.md`'s new §*Scrubbing inside a step* and the `MechanosynthStep`
row in `nodes/math_programming.md` — because the pin, the property and the five
record fields are user-visible the moment this phase lands; what waits for Phase
4 is the panel, the cage, the preferences and the walkthrough.

### Phase 3 — Panel and preferences

The time row with drag bracketing, the two readout lines, the tool-row
marker; the overlay route (`EvalOutput::overlays` → `NodeSceneData` → the
wireframe pass), the cage segments on the node, the two preference fields
end to end (struct, defaults, API twin, conversion, window, keys).

### Phase 4 — Guide and walkthrough

The four guide pages, the layout advice included; the manual checklist —
with the envelope cage switched on in the preferences, so every item below
is watched with the cone visible —
on the silicon v3 demo as Phase 1's generator parks it: drag
the slider on a pickup and watch the shuttle leave park, descend on the
reservoir, take its atom, lift and fly across to hover over the workpiece
site; scrub the settles and see it wait; scrub the donation and see it
descend, hand over, lift and fly back toward the reservoir; find the run's
last donation and see it go home; park a tool directly over a site and watch
the next visit tilt; box a site in and read the blocked line while the tool
still visits; set `time` by wire from a `float` node; park a tool on
another's flight line and read the collision; undo one drag with one Ctrl-Z;
open a milestone-1 project with a `/3` library and read the regenerate
message. The Flutter smoke test stays the human's.

### Phase 5 — The play control

Dart only, and independent of Phase 4: `mechanosynth_transport.dart` — the
rewind and play buttons, the position readout, and the `Ticker` driver behind
them, with `SingleTickerProviderStateMixin` and the bracket closed on release,
cancel, the end of the script and `dispose`; the row mounted at the top of
`mechanosynth_editor.dart`, above the step scrubber, disabled when `step` or
`time` is wired; `PLAY_BASE_STEP_SECONDS` and the `×n` speed field, with the
multiplier held as session state on `StructureDesignerModel`; the Dart tests
above; the guide paragraph. Nothing in `rust/` is touched, so the Rust suites are run as
regressions rather than extended.

Kickoff check, on the silicon v3 demo: hold play from step 0 and watch the
build run without a break at a step boundary — a shuttle that descends on the
reservoir, lifts, flies and descends on the site without the scene jumping
between the two; release mid-flight and the scene holds there; press again and
it continues; hold to the end and the button lets go by itself at step 171;
press once more and it starts from the bare slab; one Ctrl-Z after a long run
returns to where the run began. Run the same demo at ×1 and at ×8 and confirm the
speed field is the only thing that changes — the same steps, the same stops,
just a different clock; change it mid-press and confirm the scene does not
jump.

### Phase 6 — Playing what moves

`playable_steps` in `trajectory/runs.rs` and its tests; `playable` on
`APIMechanosynthInfo`, filled in `mechanosynth_info` and empty in its no-script
branch, with the api test; `flutter_rust_bridge_codegen generate`; the
`playable` argument threaded from `mechanosynth_editor.dart` into
`MechanosynthTransportRow`, whose boundary picks the next entry; the Dart tests
above; the guide paragraph. The transport's other behaviour does not move, so
Phase 5's tests are the regression.

Kickoff check, on the silicon v3 demo: hold play and watch a shuttle's run come
out continuous — descend, react, lift, fly, descend — with no second of
stillness between visits where the settles used to be; watch a `bulk` phase go
by as one beat rather than one per exposure; confirm the workpiece at the end
of the run is the same atoms the un-skipped playback produced; scrub to a
settle by hand and confirm it is still there to look at, and that pressing play
on it plays it before moving on; unwire `ops` and confirm playback falls back
to every step rather than skipping the lot.

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

Phase 5's play button is **not** that clock and does not pre-empt it. It is the
scrub driven by a ticker at a fixed seconds-per-step, blind to `duration`, to
`event_indices` and to `runs`; what it settles is only the *transport* — that
holding a button is how a build is played, that a run is one undo entry, and
that one write per frame is fast enough to drive it. When the clock arrives it
supplies a better function from elapsed time to `(k, u)` and keeps the button.

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
  obstacle pass the slow part. `plan_landing` is the only caller.
- **A drawn path.** A second `OverlayKind` drawing the flights, the
  standoffs and the colliding sample in red, through the route the envelope
  cage opens — the same `Visit` rendered rather than scrubbed, with its own
  preference switch and colour.
- **Library-stated standoff and speed.** `"trajectory": { "standoff": 8.0 }`
  on the library, overriding `STANDOFF_HEIGHT`, when a second process shows six
  ångström is not one number for all.
- **Time on `mechanosynth_edit`**, if authors want to watch a path before
  committing a step; needs the hit test to read the moved scene.
- **Offers that know the approach.** `applicable_ops` running the sweep for
  each ready row, so the editor greys out a placement the tool could not
  reach before it is committed — the generator's check, in the editor. Until
  then an authored step on a blocked site is seen in the replayer, not where
  it was authored; the editor's block replay lands every step like the
  node's and fails for none of them.
- **A `parts: [HasAtoms]` output** (milestone 1's follow-up) — the moved tool
  as its own structure, for a downstream that wants only it.
- **A latched play.** A click that plays until clicked again, once the held
  button has shown whether a demo actually wants one. A loop switch belongs with
  it. The speed half of this follow-up was taken up and is §Playing the build's
  `×n` field; what is still open is **remembering it across restarts**, which is
  a `preferences.json` field and not a project one.
- **A dwell that knows the step.** Phase 6 answers the half of this that hurt
  — a step with no motion is not shortened but skipped. What is left is the
  *tip* steps: a visit that flies across the whole scene gets the same second as
  one that descends on the site it is already hovering over. Pacing those by the
  length of the flight, or by the library's `duration`, is the clock's job.

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
- **Making a flight collision an error.** A collision on a flight is the
  user's layout and the fix is visible and immediate; an error there would
  stop a demo for a re-park.
- **Making a blocked approach an error in the replay.** The first three
  drafts did. It coupled a presentation-motivated heuristic — an envelope
  whose radius the library is invited to overstate for an unmodelled shaft
  — into whether a milestone 1 project loads at all, and it reached the
  editor through its block replay, which binds tools and would have broken
  an authored block at the first crowded site while the offers that placed
  the step never ran the sweep. The generator wants the gate and gets it in
  one line; nobody else does.
- **A radial gap as the sweep's clearance** (`ρ − s · tan α`). It is not a
  distance: a sphere the sweep called clear by the full margin could sit
  inside the cone, by more than half an ångström at the probe's 55°, and
  the descent — then unscanned — would have carried the molecule through
  it. The gap is now the distance to the surface, and the descent is
  scanned anyway.
- **Interpolating the atoms themselves** (cargo sliding from apex to site).
  There is no path for an atom without a reaction coordinate; the handoff is
  instantaneous by the engine's founding rule and the coincident reaction
  points make it invisible.

## Open questions

- Whether `STANDOFF_HEIGHT = 6 Å`, `CLEAR_MARGIN = 0.5 Å` and the
  `0.45 / 0.55` dwell are right for the silicon demo's camera. Numbers to look
  at, not to argue about.
- Whether a flight that crosses another tool should be more than a report, now
  that the sweep does not look at tools at all. The path scan sees it and the
  panel says so, which is the feedback a designer needs to re-park; making it
  an error would put the layout back into whether a build loads.
- Whether a `bulk` step should end a run after all. It does here because an
  exposure is a regime the instrument retracts from; a process that doses
  while a tool waits nearby would want the run to continue, and the change
  is one line in `runs`.
- Whether a side-face process ever wants a preferred direction other than
  `+z`. The sweep finds the face's free directions either way; only the tie
  between two free ones would change. A per-library preference is the
  alternative, and nothing asks for it yet.
- Whether the record's `contact` should be a Bool `collides` beside the
  ratio, for a `switch` downstream. An `expr` on the ratio does it today.
