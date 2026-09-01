# Research: intent-preserving auto-layout for AI/human co-editing

**Status:** research only — no decisions, no design. This is the survey step
requested before designing the replacement for `layout_network` on the AI edit
path. A design doc will follow and will pick from what is catalogued here.

**Context:** `doc/design_wire_annotations.md` (issue #427) defers its unanchored
comment rule to "a separate design [that] will rework auto-layout around
preserving human intent for *all* nodes" (D10). This is the research for that
rework.

---

## 1. The problem, grounded in the current code

### What happens today

The AI edit path is **already incremental** — this is the single most important
fact for everything below.

`NetworkEditor::apply(code, replace)`
(`text_format/network_editor.rs:143`) merges the AI's statements into the
*existing* `NodeNetwork` when `replace == false`: node ids survive, untouched
nodes keep their `position`, and each genuinely new node is placed by
`text_format::auto_layout::calculate_new_node_position` — a local rule ("right
of the rightmost source, at the average y of the sources, slide down until it
does not overlap"). It even reports the delta explicitly:

```rust
pub struct EditResult {
    pub nodes_created: Vec<String>,
    pub nodes_updated: Vec<String>,
    pub nodes_deleted: Vec<String>,
    pub connections_made: Vec<String>,
    ...
}
```

Then `ai_assistant_api.rs:~228` throws all of that away:

```rust
if result.success && prefs.layout_preferences.auto_layout_after_edit {
    layout::layout_network(network, registry, algorithm);   // default: Sugiyama
}
```

`layout_network` recomputes **every** position from the topology alone. It reads
`network.nodes` and nothing else — no previous coordinates, no delta, no
provenance. `auto_layout_after_edit` defaults to `true` and the default
algorithm is `Sugiyama`, so the default experience is: *every AI edit
re-derives the whole drawing from scratch.*

So the question is narrower and better-posed than "write a better layout
algorithm":

> Given (a) a drawing a human has arranged, (b) a known, small structural
> delta, and (c) the old drawing's coordinates — what is the *smallest* change
> to the drawing that accommodates the delta and stays readable?

### Facts about the current implementation worth carrying into the design

| Fact | Where | Why it matters |
|---|---|---|
| Both algorithms share 4 stages: depths → columns → barycenter ordering → coordinates | `layout/common.rs`, `topological_grid.rs`, `sugiyama.rs` | Each stage is a separate place a "previous layout" seed can be injected. Stability does not have to be all-or-nothing. |
| `Node` has **only** `position: DVec2` | `node_network.rs` | There is no `pinned`, no `manually_placed`, no constraint list. Any provenance-based approach needs new persisted state. |
| Layout never recurses into HOF bodies | `grep zone/walk_all_nodes layout/` → no matches | Bodies (`node.zone`) are laid out never, and `body_width`/`body_height` are themselves layout variables. Any rework inherits this gap. |
| Node sizes are *estimated* from pin counts, not measured | `node_layout::estimate_node_height` | Comments (200×100 default) and HOF bodies (arbitrary) are badly mis-sized. Already flagged in the anchors doc. |
| Within-layer order is seeded by ascending node id | `group_by_depth` ends with `layer.sort()`; every downstream sort is stable | The barycenter sweep is an order-sensitive heuristic, so this seed shapes the result. It is atomCAD's de-facto model order (§8). Residual nondeterminism is narrow: equal-size disconnected components, seeded from a `HashMap`. |
| `layout_active_network()` wraps the reflow in one `MoveNodesCommand` | `structure_designer.rs`, #270 | Undo is already correct at the whole-reflow granularity. |
| The text format carries **no coordinates** | `text_format/network_serializer.rs` | Position lives only in `.cnnd`. If the AI ever operates in `replace` mode, all positional intent is lost before layout runs. |

### What "human intent" actually consists of in a node graph

Worth naming explicitly, because different algorithms preserve different
subsets, and the design will have to say which ones it cares about:

1. **Absolute position** — "this cluster lives in the top-left". Weakest; often
   incidental.
2. **Relative order** — "the X input is above the Y input"; "the passivation
   chain is below the main chain". This is Misue et al.'s *orthogonal ordering*
   and is usually the strongest and cheapest thing to preserve.
3. **Proximity / grouping** — "these six nodes are the tip assembly". Often the
   real semantic content of a hand layout, and completely invisible to the
   topology.
4. **Alignment** — nodes sharing an x or y within a few pixels, deliberate
   columns, deliberate rows.
5. **Whitespace** — a deliberate gap that says "different subsystem". Auto-layout
   compacts it away; that is a *loss of information*, not a cleanup.
6. **Deviation from the algorithm** — the human moved a node *off* the grid
   because the grid was wrong. This is the highest-value signal and the one a
   pure recompute destroys most reliably.
7. **Off-graph annotation placement** — comments and (future) anchors; covered
   by #427.

---

## 2. The useful reframing: minimal perturbation

Classic layout: *maximize readability; hope the result resembles the last one.*

Intent-preserving layout: **minimize change; subject to readability constraints.**
The old drawing becomes the objective function and readability becomes the
feasible region, rather than the reverse.

This flip is what most of the literature below is about, and it has a practical
corollary: the algorithm should be able to answer "why did this node move?"
with a constraint, not with "the barycenter came out differently". That property
is worth a lot in an AI co-editing tool, because the user's real complaint is
never "the layout is ugly" — it is "I didn't ask you to move that".

The literature calls this the **stability vs. readability trade-off**, and it is
known to be a genuine trade-off, not a free lunch: enforcing model order or
previous positions demonstrably *increases* edge crossings
([Domrös et al. 2024](https://arxiv.org/html/2406.11393v1)). Empirical work on
whether mental-map preservation actually helps users is mixed
([Purchase & Samra 2008](https://link.springer.com/chapter/10.1007/978-3-540-87730-1_9);
[Archambault & Purchase 2013](https://www.sciencedirect.com/science/article/abs/pii/S107158191300102X)),
with a recurring finding that *extremes* are better than the middle — either
keep it really stable or fully re-optimize, but do not half-move things. That is
a design-relevant result: a mushy compromise may be the worst option.

---

## 3. Where intent can come from (four sources)

Independent of algorithm choice. A design can use several.

### S1 — Implicit, in the previous drawing (free, needs inference)
Coordinates are already there. Ordering, alignment groups, clusters and
whitespace can all be read out of them. No UI, no file-format change, no user
burden. Downside: it is inference, and a wrong inference is invisible.

### S2 — Explicit, stored (highest fidelity, costs UI + file format)
Pins ("don't move this"), alignment constraints, groups/frames, comment anchors
(#427). The user says what they mean, once, and it survives forever. This is the
Dunnart / SetCoLa school. Downside: a feature the user must learn, and the
existing corpus of `.cnnd` files has none of it.

### S3 — Model order (free, no coordinates needed) ⭐
ELK's insight: the **textual order** of elements in the source model is itself a
deliberate human signal, and can be used as a tie-breaker or a hard constraint
throughout layered layout
([ELK: Constraining the Model](https://eclipse.dev/elk/blog/posts/2023/23-01-09-constraining-the-model.html),
[Domrös et al. 2024](https://arxiv.org/html/2406.11393v1)). They report it makes
layouts predictable enough that users stopped complaining, at the cost of some
crossings, and that "most models intuitively employ model order".

This is unusually well-shaped for atomCAD, because **the AI's edit medium is
text**. Statement order in the network text format is authored by a human or by
the AI following a human's description, it round-trips today, and it is stable
under coordinate changes. It is also the *only* intent channel that survives
`replace`-mode edits.

### S4 — Provenance (cheap, needs one bit of persisted state)
Record, per node, whether its position was set by a human or by an algorithm.
Then layout is free to move algorithm-placed nodes and must justify moving
human-placed ones. One `enum PositionAuthority { Derived, Manual }` on `Node`
plus a flip in the drag handler. This is the smallest possible intent capture
and by far the highest signal-to-noise: it is not inference, the user does not
have to learn anything, and it is exactly the distinction that matters for
"the AI ruined my layout".

---

## 4. Algorithm families

Ordered roughly by implementation cost. Each entry: what it is, what intent it
preserves, cost, fit.

### F1 — Partial / incremental layout: lay out only the delta
**Idea.** Freeze everything that already exists. Compute positions only for new
elements, fitting them into the existing drawing without disturbing it.
Commercially this is [yFiles `PartialLayout`](https://docs.yworks.com/yfiles-html/dguide/layout/partial_layout.html):
partial elements are grouped into subgraph components, then placed against the
fixed remainder, whose layout is "left completely unaltered". Its stated
advantage over "incremental layout" is that it works on a drawing *of arbitrary
origin and style* — i.e. one a human made by hand.

**Preserves.** Everything, by construction. Positions, order, grouping,
whitespace, deviations.

**Cost.** Low. atomCAD already has the primitive (`text_format/auto_layout.rs`)
and already knows the delta (`EditResult`). The missing pieces are: making room
when the new nodes don't fit (`node_inlining::make_space_for_inline` is prior
art, though the anchors doc correctly notes it does no collision testing), and
handling *rewiring* (a wire change can make an existing node's position wrong
even though the node is untouched).

**Fit.** Very high as the default behaviour. This is essentially "turn
`auto_layout_after_edit` off and make the incremental placer good", and it is
the honest baseline every fancier option must beat.

**Weakness.** Drift. After twenty AI edits, a graph laid out only locally
degrades — new nodes accrete to the right, long wires accumulate, and nothing
ever cleans up. Needs a companion story: an explicit "tidy this up" command
(which exists: `layout_active_network`), or a drift metric that triggers a
suggestion rather than an automatic reflow.

### F2 — Pinning / sticky nodes
**Idea.** Nodes carry a "don't move me" flag (S4). Global layout runs, but
pinned nodes are fixed obstacles and constraints rather than free variables.
Standard everywhere: Graphviz `neato` `pin=true`, d3-force `fx`/`fy`,
[dagre forks that add rank/order pinning](https://github.com/HassanMojab/dagre).

**Preserves.** Exactly what the human explicitly protected — nothing more,
nothing less. Zero false inference.

**Cost.** Low–medium. Persisted field + undo + a UI affordance + every layout
stage learning to treat pinned nodes as fixed. The last part is where layered
algorithms get awkward: a pinned node fixes its *layer* too, which can conflict
with the topology (a pinned node left of its own input). Needs a documented
conflict rule.

**Fit.** High, especially as **implicit** pinning: "a node the user dragged is
pinned until the user un-pins it". No new UI at all, just a flag set in the drag
handler. Risk: silent, invisible state — a user who dragged a node six months
ago wonders why auto-layout skips it. Needs a visual tell.

### F3 — Seeded / "interactive" layered layout ⭐
**Idea.** Keep the existing Sugiyama pipeline but feed each phase the previous
drawing instead of a blank slate. This is what ELK calls **interactive**
strategies and what DynaDAG does for the online case.

Per phase:

| Phase | Static version (today) | Seeded version |
|---|---|---|
| Cycle breaking | arbitrary | previous direction / model order (`cycleBreaking.strategy: INTERACTIVE`, `MODEL_ORDER`) |
| Layer assignment | `depth = max(input depths)+1` | previous **x** determines the layer (`layering.strategy: INTERACTIVE`); or network-simplex with a penalty on deviation from the previous rank — [DynaDAG](https://link.springer.com/chapter/10.1007/BFb0021824) adds "explicit variables and constraints that penalize level assignments by their variance from some given assignment (usually the previous layout)" |
| Crossing minimization | barycenter sweeps from a hash-ordered start | initialize the permutation from the previous **y** order, then sweep; or freeze it entirely ([`crossingMinimization.semiInteractive`](https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-crossingMinimization-semiInteractive.html)), or use model order as tie-breaker (`considerModelOrder.strategy`) |
| Coordinate assignment | centre each column | 1-D optimization minimizing displacement from previous y subject to separation constraints (see F5); or Brandes–Köpf for determinism |

**Preserves.** Relative order (2) and, with interactive layering, coarse
absolute position (1). Not proximity (3), alignment (4) or whitespace (5).

**Cost.** Medium, but **incremental and stage-by-stage** — the highest
value-per-effort item in this document. Seeding *just* the crossing-minimization
permutation from previous y is maybe 30 lines and removes most of the
gratuitous vertical churn (and fixes the known `HashMap` non-determinism as a
side effect). Interactive layering is another small step. Each stage can be
adopted, measured and kept or discarded independently.

**Fit.** Very high. It is a modification of code that already exists rather than
a new subsystem, and it degrades gracefully: with no previous layout (a fresh
network) it falls back to exactly today's behaviour.

**Reference implementations to read:** ELK Layered (`INTERACTIVE` strategies,
`considerModelOrder`), KIELER/KLighD's two-run architecture — a first normal run
produces positions, then a **second run with interactive strategies** consumes
them, with user drags recorded as `layerChoiceConstraint` / `positionChoiceConstraint`
that are translated into pseudo-positions.

### F4 — Layout adjustment: keep the drawing, fix only the violations
**Idea.** Do not re-derive anything. Take the human drawing exactly as it is and
apply the minimum displacement that removes overlaps / restores spacing, subject
to preserving **orthogonal ordering** (if A was left of B, A stays left of B).
This is the [Misue, Eades, Lai & Sugiyama (1995)](https://www.sciencedirect.com/science/article/abs/pii/S1045926X85710105)
mental-map paper — it introduced *orthogonal ordering, proximity, topology* as
the three models of the mental map, and the **Force-Scan** algorithm.
Successors: PRISM / [proximity-stress overlap removal](https://link.springer.com/chapter/10.1007/978-3-642-00219-9_20) (Gansner & Hu),
[fast node overlap removal via VPSC](https://people.eng.unimelb.edu.au/pstuckey/papers/gd2005b.pdf) (Dwyer, Marriott, Stuckey — a quadratic program solved by separation constraints, provably minimal displacement in each axis),
[FORBID](https://arxiv.org/pdf/2208.10334) (SGD-based), and a
[constant-factor approximation](https://arxiv.org/pdf/1502.03847) for the
orthogonal-order-preserving version.

**Preserves.** Almost everything — this family is *defined* by minimal
displacement.

**Cost.** Low–medium. Force-Scan is simple and well-documented. VPSC
(variable placement with separation constraints) is a few hundred lines and is
the better engine because it is exactly "closest positions satisfying these
separation constraints" — the minimal-perturbation objective of §2, solved
optimally, per axis.

**Fit.** High as the *finishing pass* for any of the other families: F1 places
new nodes, F4 makes room for them without scrambling anything. Not sufficient
alone (it never improves a bad layout, only repairs a broken one) — which is
fine, because "never improves" is also "never ruins".

### F5 — Constrained optimization (stress majorization + separation constraints)
**Idea.** The sophisticated end. Model layout as: minimize a stress function
(edge lengths ≈ graph distances) **plus an anchoring term** pulling nodes toward
their previous positions, subject to **hard separation constraints** (non-overlap,
alignment, DAG flow direction, cluster containment). Solve by stress majorization
with gradient projection.

Canonical work: [IPSep-CoLa](https://research.monash.edu/en/publications/ipsep-cola-an-incremental-procedure-for-separation-constraint-lay/)
(Dwyer, Koren & Marriott, TVCG 2006) — separation constraints expressive enough
for "layout of directed graphs to better show flow", non-overlapping labels, and
clusters, solved incrementally by gradient projection.
Implementations: [Adaptagrams](https://www.adaptagrams.org/) (libcola / libvpsc /
libavoid, C++), [WebCoLa](https://ialab.it.monash.edu/webcola/) (JS).
Refinements: [diagonally-scaled gradient projection](https://link.springer.com/chapter/10.1007/978-3-540-77537-9_23),
[Revisiting Stress Majorization as a Unified Framework for Interactive Constrained Graph Visualization](https://www.semanticscholar.org/paper/a0280c232a103d93c35e68f326401f677001b6a3),
and [(GD)² gradient-descent graph drawing](https://arxiv.org/pdf/2008.05584)
(any differentiable objective, optimized by autodiff — the modern, very flexible
formulation).

**Preserves.** Whatever you write into the objective. Genuinely everything, in
principle: proximity, alignment, grouping, whitespace, previous position — each
as a weighted term or a hard constraint.

**Cost.** High. A real solver, real numerics, real tuning weights, and a
"flow layout" that produces DAG columns is *softer* than the crisp column grid
atomCAD has today — users would notice the aesthetic change immediately. There
is no mature Rust port; libcola would have to be reimplemented or FFI'd.

**Fit.** Right answer for the general problem, likely wrong answer for this
codebase's next step. Worth keeping as the north star: it is the framework in
which all the cheap heuristics below are special cases, and the framework in
which "human intent = a set of constraints" is literally true. Note the
`design_wire_annotations.md` D10 warning against tuning surfaces applies with
full force here.

### F6 — Force-directed with anchor springs / pinned nodes
**Idea.** Ordinary force simulation plus a spring from each node to its previous
position; pinned nodes have infinite spring constant. The classic online variant
is Frishman & Tal's *Online Dynamic Graph Drawing* (2008), which pins a subset
and relaxes the rest. Simulated annealing variants (Davidson & Harel) can add a
"stay near the start" energy term equally easily.

**Preserves.** Absolute position and proximity, softly. Not order (nothing
prevents a swap), not alignment, not columns.

**Cost.** Low to implement, high to *tune*, and non-deterministic without care.

**Fit.** Low. atomCAD's graph is a DAG that reads left-to-right in columns;
force-directed layout does not naturally produce that, and the tuning surface is
exactly what D10 warns against. Listed for completeness.

### F7 — Offline / foresighted layout (the supergraph approach)
**Idea.** If you know the *whole sequence* of graphs in advance, lay out their
union (the "supergraph") once and derive each frame from it; nodes then never
move across the sequence. Diehl & Görg, *Preserving the Mental Map using
Foresighted Layout*.

**Fit.** Not applicable — future edits are unknown. Recorded because it names
the theoretical bound: perfect stability is achievable only with foreknowledge,
so every online method is a heuristic approximation of it. There is one
speculative use: an undo/redo *history* is a known sequence, so a supergraph
layout could make undo/redo visually stable. Probably not worth it.

### F8 — Post-hoc rigid alignment (Procrustes)
**Idea.** Run whatever layout you like, then apply the single rigid transform
(translation, optionally uniform scale) that minimizes total displacement from
the previous drawing. Cheap, and it fixes the most jarring artefact of all:
the whole graph teleporting because layout canonicalizes to `(100, 100)`.

**Cost.** Trivial. `design_wire_annotations.md`'s D9.1 origin-delta translation
is exactly this, restricted to translation and applied only to comments.
Generalizing it to the whole network is a few lines.

**Fit.** High as a cheap global postprocess; worthless on its own. Also worth
pairing with a **matching check**: compute the assignment between old and new
positions and report total displacement — that number is the stability metric of
§5 and it costs nothing once you have both layouts.

### F9 — Constraint inference / beautification
**Idea.** Read constraints *out of* the human drawing: nodes whose x agree
within ε form an alignment group; nodes with equal spacing form a distribution;
tight clusters separated by whitespace form groups. Promote those to hard
constraints and hand them to F3/F5. The idea is old and good — Pavlidis & Van
Wyk's *automatic beautifier for drawings and illustrations* (SIGGRAPH 1985)
inferred near-alignments and snapped them — and it is what
[Dunnart](https://users.monash.edu/~mwybrow/dunnart/) does interactively:
"continuous network layout that continuously adjusts the layout in response to
user interaction, while still maintaining the layout style and, where reasonable,
the current layout topology", with alignment/distribution constraints created by
the author.
[SetCoLa](https://jhoffswell.github.io/website/resources/papers/2018-SetCoLa-EuroVis.pdf)
(Hoffswell, Borning & Heer 2018) is the declarative layer above this: define
node *sets* by data/graph properties, then apply high-level constraints per set,
reducing hand-authored constraints by 1–2 orders of magnitude.

**Preserves.** Alignment (4) and grouping (3) — precisely the two things F3
cannot preserve and that humans care about most.

**Cost.** Medium, and the risk is subtle: an inferred constraint is a *guess
about intent*, and `design_wire_annotations.md` D10 already argues at length
that persisting guesses is worse than discarding them, because they become
indistinguishable from authored intent. The same argument applies here verbatim.
Safe form: infer per-pass, never persist. Better form: infer, then **show** them
and let the user confirm — the bulk-migration idea already sketched in D10's
"Deferred" section.

**Cluster detection specifics** (if pursued): single-linkage clustering on
box-to-box distance with a gap threshold tied to `COLUMN_WIDTH`; the anchors
doc's deferred section already works out the guards (relative threshold with a
floor, box-to-box not centre-to-centre, a "nothing is near" cutoff, node-id
tiebreak for determinism). Those guards generalize from comments to nodes
unchanged.

### F10 — Model-order steering ⭐
**Idea.** Use the order of statements in the network text format as the
tie-breaker (or constraint) for every ordering decision in layout. See S3.
ELK ships this as `considerModelOrder.strategy` with `PREFER_EDGES` /
`NODES_AND_EDGES` / `PREFER_NODES`, plus `MODEL_ORDER` cycle breaking and
model-order component ordering.

**Preserves.** Relative order (2), robustly, with *no coordinates at all* — so
it is the only mechanism that survives a full `replace`-mode rewrite, a
copy-paste into a new network, or a file that predates any of this work.

**Cost.** Low. The serializer already emits a deterministic order, and the
editor already parses statements in order. It needs a stable per-node
`model_order: usize` (or simply: node id, which is already allocation-ordered —
though ids are not *re*-orderable by the user, which is the whole point of model
order, so a real field is better).

**Fit.** High, and unusually so because of the AI angle: an AI that writes the
text is also writing the layout hint, and a human who reorders the text is
re-arranging the diagram. It makes the text format the single source of intent,
which fits atomCAD's text-first AI story. Evaluation in
[Domrös et al. 2024](https://arxiv.org/html/2406.11393v1): over a year of
deployment, zero reports of confusing layouts; developers preferred stability
over crossing minimization.

### F11 — "Layout is data": store constraints, not coordinates
**Idea.** The end state of the S2/F5/F9 direction. The file stores what the
human *meant* — these are aligned, these are a group, this is pinned here, this
comment is about that wire — and coordinates are always derived. Nothing to
preserve, because nothing positional is authoritative.

**Fit.** Genuinely the right long-term shape, and it rhymes with the repo's
`project_identity_vs_naming` north-star thinking (store identity, derive
presentation). Also enormous: file format, UI, undo, and a constraint solver.
Worth naming as the destination so that intermediate steps can be chosen to
point toward it rather than away from it — e.g. F2's pin flag and #427's comment
anchors are both *already* small instances of "stored intent", and adding more
of them incrementally is a viable path to F11 without a big-bang rewrite.

---

## 5. Cross-cutting concerns worth designing in

**Determinism first.** Stability is untestable without it — "did this edit move
anything?" has no answer if two runs disagree. Layout is mostly deterministic
already (`group_by_depth` sorts; the barycenter sort is stable), with one hole:
`find_connected_components` seeds its BFS from `network.nodes.keys()` and the
size sort is stable, so equal-size components stack in a per-process-random
order. Cheap to close, and a prerequisite. See §8 for where order actually
decides the drawing.

**Stability metrics, as test assertions.** The rework needs numbers, or reviews
become taste arguments:
- total / max node displacement between old and new layout;
- number of **orthogonal-order inversions** (pairs whose left-of or above
  relation flipped) — the mental-map metric;
- change in edge crossings (the readability side of the trade-off);
- number of nodes that moved at all (often the metric users actually feel).

A test like "adding one leaf node moves ≤ 1 existing node" is worth more than
any amount of prose.

**A stability/quality knob.** GraphAnimation-style: one parameter bounding how
much the layout may change. Useful, but note the "extremes are better" empirical
result — the knob's useful positions may be only its ends.

**Blast-radius limiting.** Not everything needs relayout. A change confined to
one connected component, or one downstream cone, can be relaid out alone. The
repo already thinks in cones (`project_error_management` P3 "cone-scoped
blocking"); the same scoping applies. This makes "the AI edited one corner"
literally a local operation.

**HOF bodies.** Layout ignores them entirely today. Any rework has to decide:
recurse (with the body box as a nested layout region whose size is itself an
output), or explicitly not. `body_width`/`body_height` are persisted, so a body
resize is human intent too.

**Viewport stability.** A distinct and often-more-important thing: even a
perfectly stable layout is disorienting if the canvas scrolls. Keeping the
selected/focused node fixed *on screen* across a reflow is cheap and may buy
more perceived stability than the layout algorithm does.

**Undo granularity.** Already right (`MoveNodesCommand`, #270). But if layout
becomes *partial*, the undo entry should describe what actually moved, not "all
nodes".

**Rust ecosystem reality.** There is no libcola/adaptagrams in Rust. VPSC is the
only sophisticated piece worth reimplementing at moderate cost (~300–500 lines,
well-specified in the Dwyer–Marriott–Stuckey paper). Network simplex for layer
assignment is available in graph crates or writable. Everything in F1–F4 and
F8/F10 is plain arithmetic over data structures the codebase already has.

---

## 6. Preliminary shortlist (not a decision)

Grouped by how much they cost versus what they buy. My reading of the material,
offered for the design discussion:

**Tier 0 — prerequisites, worth doing regardless**
- Deterministic tie-breaking everywhere (kills the `HashMap` order dependence).
- Stability metrics available to tests.
- Fix node sizing to use real dimensions (comments, HOF bodies) — already a
  prerequisite in `design_wire_annotations.md` Phase 5.

**Tier 1 — the pragmatic core (F1 + F2 + F8)**
Default the AI path to **partial layout**: freeze what exists, place only the
delta well, make room with a minimal-displacement adjustment (F4/VPSC or
Force-Scan), and keep the global reflow as an explicit user command. Add
provenance (F2) so that even the explicit reflow respects nodes the human
placed. This is the smallest change that directly answers "the AI ruined my
layout", and every piece of it is code the repo already half-has.

**Tier 2 — make the explicit reflow itself intent-aware (F3 + F10)**
Seed the existing Sugiyama phases from the previous drawing and from model
order, one phase at a time, measuring with the Tier 0 metrics. Low risk because
it degrades to today's behaviour when there is no previous drawing.

**Tier 3 — inferred and authored structure (F9, then F11)**
Alignment/cluster inference, shown to the user rather than silently persisted;
then explicit stored constraints as the long-term shape. Both should wait until
Tier 1–2 are in and measured.

**Explicitly not recommended:** F6 (force-directed) — wrong aesthetic for a
column DAG and a tuning trap; F7 (foresighted) — inapplicable; F5 as a
*near-term* choice — right framework, wrong size for the next step, and it would
visibly change the look of every existing network.

---

## 7. Open questions for the design phase

1. **Is the default "never reflow automatically"?** Turning
   `auto_layout_after_edit` off by default plus a good partial placer may be 80%
   of the win. What is lost?
2. **Is dragging a node an act of intent?** (Implicit pinning.) If yes, how is
   it made visible, and how is it cleared?
3. **Does model order become a first-class persisted concept**, or is it just
   "text order at edit time"? It only works as intent if the user can reorder
   the text and see the diagram follow.
4. **What happens on `replace`-mode edits**, where the AI rewrites the whole
   network? Coordinates cannot survive by id if ids do not survive. Does the
   design require id-stable replace, or accept full reflow there?
5. **Does layout recurse into HOF bodies?** And is body size an input or an
   output?
6. **How does this interact with #427's comment placement?** That design
   deliberately builds disposable scaffolding (D10) and expects to be replaced.
   Its `CommentAnchor` is permanent and should be treated as the first instance
   of stored intent (S2/F11), not as a special case.
7. **What is the acceptance test?** Concretely: a human arranges a 40-node
   network, the AI adds a 5-node subassembly, and ... what must be true
   afterwards? Writing that sentence down probably decides the algorithm.

---

## 8. How much layout should the AI control?

A separate axis from everything above: the algorithms in §4 assume the AI edits
the *network* and something else decides the *drawing*. Should the AI have more
say, and if so through what channel? This section records the analysis; the
conclusion is **keep the text format geometry-free**.

### The spectrum

| Level | Channel | Verdict |
|---|---|---|
| 0 | Text only, no layout information | today, and `doc/design_incremental_layout.md` |
| 1 | Statement order used as the within-layer seed | the only candidate worth building — but not free, see below |
| 2 | Declarative grouping / ordering hints in the text | defer; atomCAD already has a better idiom |
| 3 | Qualitative relative hints ("below", "adjacent to") | no — needs a constraint solver (F5) to consume |
| 4 | Absolute coordinates emitted by the AI | reject |

A second axis matters more than the level: **whose nodes**. "The AI may position
nodes it creates, never ones it did not" is far safer than blanket control at
any level.

### Why level 4 is rejected

Not primarily because language models are weak at spatial reasoning, though they
are. Three sharper reasons:

- **No feedback loop.** The AI cannot see the rendered canvas. Node heights are
  derived at runtime from parameter counts, comment boxes are whatever the user
  dragged them to, HOF body boxes are arbitrary. An AI emitting coordinates is
  doing collision detection against dimensions it does not have, and never
  learns it was wrong.
- **Coordinates are not the AI's information to give.** Existing positions are
  human intent (§1). A coordinate channel is a channel for overwriting it — and
  it *must* overwrite, because a partial coordinate set is inconsistent with the
  rest of the drawing. Coordinate control is inherently non-incremental.
- **Attention is a budget.** Emitting positions spends output tokens on the task
  the model is worst at, in direct competition with designing the network.

### The distinction that matters: geometry vs. semantics

"Text ↔ coordinates" collapses two different things. Models are weak at
*absolute geometry* and strong at *semantic-relational* facts — grouping,
reading order, which chain is the spine — because when the AI authored the
network those facts are its own authorship, not inference from pixels. The
second kind is exactly what F3 / F9 / F10 wanted and could not get.

### Why levels 2–4 are squeezed from both sides

- **On the incremental path the wiring already carries it.** Step 3 of the
  incremental design derives placement from anchors: upstream *and* downstream
  anchors ⇒ an insertion, placed between them; upstream only ⇒ it hangs off the
  end. The AI must emit that wiring anyway. A hint saying "this is an insertion"
  restates information already required.
- **On the full-reflow path the user opted into an algorithmic result.** That is
  the point of making it an explicit command.

A hint channel therefore has to find value in the gap between "already implied
by the wiring" and "the user asked for algorithmic output". That gap is thin.

There is also a fixed cost per channel: **anything in the text format must
round-trip or the AI silently deletes it on its next edit** —
`design_wire_annotations.md` makes exactly this point about anchors. Each
channel is a serializer pass, a parser change, an editor special case and a
repair rule.

### On grouping specifically (level 2)

atomCAD already has two ways to say "these nodes are one thing": make it a
**subnetwork** (custom node type), or use a **zone / HOF body**. Both are real
containment with real semantics and both already scope layout. A layout-only
`group` annotation would be a weaker parallel channel for something the language
expresses better.

The one case with genuine value is a **greenfield network**, where there is no
baseline to preserve and topology alone underdetermines the drawing — Sugiyama
may interleave three parallel synthesis routes that should read as three blocks.
But consuming a grouping hint requires clustered / compound layout, which the
current Sugiyama cannot do. Real value, expensive to use. Revisit only if
greenfield layouts prove visibly bad.

### Level 1 in detail: what "model order" would actually change

ELK's *model order* (§3 S3, §4 F10) is described in the literature as a
tie-breaker. In atomCAD's code the picture is more specific, and less favourable
than it first looks.

**Where order matters in the current pipeline:**

| # | Decision | Ties when | Resolved today by |
|---|---|---|---|
| 1 | Initial within-layer permutation — the **seed** for barycenter iteration | always (see below) | `group_by_depth` ends with `layer.sort()` ⇒ ascending node id |
| 2 | Equal barycenter values during a sweep | two nodes share an average neighbour position, e.g. two constants feeding one node | `sort_by(partial_cmp)` is **stable** ⇒ the seed |
| 3 | Nodes with no neighbour in the sweep direction | `compute_barycenter` returns `f64::MAX` for all of them | stable sort ⇒ the seed |
| 4 | Equal-size disconnected components | same node count | stable sort over `HashMap`-seeded BFS discovery ⇒ nondeterministic (see the incremental design's D7) |

**#1 is not a tie at all, and it is the important one.** `minimize_crossings` is
an iterative local heuristic — sweep down, sweep up, repeat until no
improvement. A local heuristic reaches a different fixed point from a different
starting permutation *even when no two barycenters are ever equal*. The initial
order is a **seed**, not a tiebreak, and #2 and #3 simply inherit whatever it
decided, because every downstream sort is stable. #3 is a large class: on a
backward sweep every node lacking an input in the previous layer piles up at
`f64::MAX`, in seed order.

**The deflating finding: text order is not an independent signal.**
`network_serializer.rs:90` emits statements in topological order and its DFS
sorts ids at every choice point (`node_ids.sort()`, `dep_ids.sort()`), so text
order is a deterministic function of (topology, node ids). The layout seed is
*also* node id. New nodes receive ids in creation order, which follows the order
the AI wrote them. **atomCAD therefore already has a crude model order — it is
called "node id", it means "creation order", and it already seeds the layout.**

What it cannot do is be *re-ordered*: moving a statement in the text changes no
id, so it changes no drawing. That — making reading order an editable control —
is the actual feature, and it costs a persisted `model_order` per node,
maintained across every edit path, undoable, and round-tripping. Not free.

One apparent conflict resolves cleanly: model order could not be a free
permutation if the serializer must emit topologically. But it is only ever
consulted **within a layer**, and same-depth nodes are mutually independent by
construction, so any within-layer permutation is topologically valid.

### Recommendation

**Keep the text format geometry-free.** Reject level 4, decline level 3, defer
level 2 in favour of subnetworks and zones.

Level 1 remains the only channel worth building, and for the right reason:
reordering statements is a **semantic** act ("this reads first"), not a spatial
one; it needs no spatial reasoning from the AI; it round-trips for free because
it *is* the text; and a human gets identical control by dragging a line in the
editor. But it should be adopted on the strength of that argument, not on a
false claim of being free. Sequence it after the incremental design's Phases
1–4, and only if within-layer ordering proves to be a visible annoyance.

Two things worth more than any hint channel:

1. **Let the AI recommend a reflow, not perform one.** After a large
   restructuring the AI knows the edit was substantial. One boolean on the edit
   result, surfaced as an offer and never automatic, is AI control over layout
   at the right granularity — a *decision*, not coordinates.
2. **Edit discipline beats hints.** An AI emitting minimal incremental diffs
   preserves the layout for free; one rewriting the network in `replace` mode
   destroys it however good the layout algorithm is. That is a prompting and
   tooling problem, and it is why the name-matching fallback in
   `design_incremental_layout.md` matters.

The principle, stated once: **the AI should influence layout through meaning,
not geometry** — and most of the meaning it can usefully convey, it is already
conveying.

---

## 9. References

**Mental map & stability**
- Misue, Eades, Lai, Sugiyama, *Layout Adjustment and the Mental Map*, JVLC 1995 — [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S1045926X85710105) · [Semantic Scholar](https://www.semanticscholar.org/paper/8f3ea4c4374a59f2625a68e9498da03195f2efc0)
- Beck, Burch, Diehl, Weiskopf, *The State of the Art in Visualizing Dynamic Graphs* — [PDF](https://www.visus.uni-stuttgart.de/documentcenter/forschung/visualisierung_und_visual_analytics/eurovis14-star.pdf)
- Purchase & Samra, *Extremes Are Better: Investigating Mental Map Preservation in Dynamic Graphs*, GD 2008 — [Springer](https://link.springer.com/chapter/10.1007/978-3-540-87730-1_9)
- Archambault & Purchase, *The "Map" in the mental map*, IJHCS 2013 — [ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S107158191300102X)
- *Visual Stability in Dynamic Graph Drawings* — [De Gruyter](https://www.degruyterbrill.com/document/doi/10.1515/icom-2015-0038/html?lang=en)
- *Towards Faithful Graph Visualizations* — [arXiv 1701.00921](https://arxiv.org/pdf/1701.00921)

**Incremental / online layered layout**
- North, *Incremental Layout in DynaDAG*, GD 1996 — [Springer](https://link.springer.com/chapter/10.1007/BFb0021824) · [PDF](https://link.springer.com/content/pdf/10.1007/BFb0021824.pdf)
- North & Woodhull, *Online Hierarchical Graph Drawing*, GD 2001 — [PDF](https://www.graphviz.org/documentation/NW01.pdf)
- Healy & Nikolov, *Hierarchical Drawing Algorithms* (GD Handbook ch. 13) — [PDF](https://cs.brown.edu/people/rtamassi/gdhandbook/chapters/hierarchical.pdf)

**Interactive constraints / model order**
- ELK, *Layered: Constraining the Model* — [eclipse.dev](https://eclipse.dev/elk/blog/posts/2023/23-01-09-constraining-the-model.html)
- ELK, *Semi-Interactive Crossing Minimization* — [reference](https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-crossingMinimization-semiInteractive.html) · [Crossing Minimization Strategy](https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-crossingMinimization-strategy.html)
- Domrös et al., *Diagram Control and Model Order for Sugiyama Layouts*, 2024 — [arXiv 2406.11393](https://arxiv.org/html/2406.11393v1)
- Domrös et al., *The Eclipse Layout Kernel*, GD 2024 — [LIPIcs PDF](https://drops.dagstuhl.de/storage/00lipics/lipics-vol320-gd2024/LIPIcs.GD.2024.56/LIPIcs.GD.2024.56.pdf) · [arXiv 2311.00533](https://arxiv.org/pdf/2311.00533)
- KLighD interactive constraints — [npm @kieler/klighd-interactive](https://www.npmjs.com/package/@kieler/klighd-interactive)

**Constrained layout**
- Dwyer, Koren & Marriott, *IPSep-CoLa*, TVCG 2006 — [PDF](https://www.researchgate.net/profile/Tim-Dwyer-5/publication/6715571_IPSep-CoLa_An_Incremental_Procedure_for_Separation_Constraint_Layout_of_Graphs/links/0fcfd5081c588735c8000000/IPSep-CoLa-An-Incremental-Procedure-for-Separation-Constraint-Layout-of-Graphs.pdf) · [Monash](https://research.monash.edu/en/publications/ipsep-cola-an-incremental-procedure-for-separation-constraint-lay/)
- Dwyer, Marriott & Wybrow, *Dunnart: A Constraint-Based Network Diagram Authoring Tool*, GD 2008 — [PDF](https://users.monash.edu/~mwybrow/papers/dwyer-gd-2008-2.pdf) · [project page](https://users.monash.edu/~mwybrow/dunnart/)
- Hoffswell, Borning & Heer, *SetCoLa*, EuroVis 2018 — [PDF](https://jhoffswell.github.io/website/resources/papers/2018-SetCoLa-EuroVis.pdf) · [GitHub](https://github.com/uwdata/setcola)
- Dwyer & Marriott, *Constrained Stress Majorization Using Diagonally Scaled Gradient Projection*, GD 2007 — [Springer](https://link.springer.com/chapter/10.1007/978-3-540-77537-9_23)
- *Incremental Grid-like Layout Using Soft and Hard Constraints* — [arXiv 1308.6368](https://arxiv.org/pdf/1308.6368)
- Ahmed et al., *Graph Drawing via Gradient Descent, (GD)²* — [arXiv 2008.05584](https://arxiv.org/pdf/2008.05584)
- *Revisiting Stress Majorization as a Unified Framework for Interactive Constrained Graph Visualization* — [Semantic Scholar](https://www.semanticscholar.org/paper/a0280c232a103d93c35e68f326401f677001b6a3)

**Overlap removal / layout adjustment**
- Dwyer, Marriott & Stuckey, *Fast Node Overlap Removal* (VPSC), GD 2005 — [PDF](https://people.eng.unimelb.edu.au/pstuckey/papers/gd2005b.pdf)
- Gansner & Hu, *Efficient Node Overlap Removal Using a Proximity Stress Model* (PRISM), GD 2008 — [Springer](https://link.springer.com/chapter/10.1007/978-3-642-00219-9_20)
- *A Constant Factor Approximation for Orthogonal Order Preserving Layout Adjustment* — [arXiv 1502.03847](https://arxiv.org/pdf/1502.03847)
- *FORBID: Fast Overlap Removal By stochastic gradIent Descent* — [arXiv 2208.10334](https://arxiv.org/pdf/2208.10334)

**Industrial practice**
- yFiles, *Partial Layout* — [docs](https://docs.yworks.com/yfiles-html/dguide/layout/partial_layout.html) · [Incremental Diagram Layout](https://www.yworks.com/pages/incremental-diagram-layout)
- Adaptagrams (libcola / libvpsc / libavoid) — [adaptagrams.org](https://www.adaptagrams.org/)
- WebCoLa — [ialab.it.monash.edu/webcola](https://ialab.it.monash.edu/webcola/)
- dagre fork with rank/order pinning and re-layout preservation — [GitHub](https://github.com/HassanMojab/dagre)
- React Flow layouting overview (the practical "when layout runs matters as much as what it computes" problem) — [reactflow.dev](https://reactflow.dev/learn/layouting/layouting)
