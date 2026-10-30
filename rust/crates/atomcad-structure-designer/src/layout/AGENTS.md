# Layout - Agent Instructions

Automatic layout algorithms for repositioning nodes in a network.

## Files

| File | Purpose |
|------|---------|
| `size.rs` | `rendered_node_size` — **the** node-size function (see below) |
| `delta.rs` | `EditDelta` / `diff_scope` — the incremental pass's input |
| `motion.rs` | `shift_half_plane` / `cascade` / `grow_rect` — the two motion primitives (see below) |
| `incremental.rs` | The incremental pass, step by step (all eight, and the driver) |
| `placement.rs` | Step 2½: the AI's layout statements (`place` / `arrange` / `straighten`), pinning, warnings (see below) |
| `report.rs` | `LayoutReport`: moved nodes with their causes, new nodes' boxes, findings (see below) |
| `readout.rs` | `layout_readout` — `query --layout`: one scope's boxes, tags and inferred groups (see below) |
| `common.rs` | Shared types and constants (`Gap`: the three gap words), depth computation, and the comment-placement pass |
| `arrange.rs` | `arrange(scope, registry, sizes, ids, ArrangeOp)` — align / stack / distribute / straighten / subset layout over chosen nodes (see below) |
| `topological_grid.rs` | Simple layered layout (fast, reliable) |
| `sugiyama.rs` | Sugiyama-style layout with crossing minimization |
| `wire_geometry.rs` | `WireCurve` — a wire as Flutter draws it, for the wire findings (see below) |

## Entry Points

- `layout_network(network, registry, algorithm)` → applies a full reflow
  in-place, **every scope, deepest first**
- `layout_network_scoped(network, registry, algorithm, respect_hand_moved)` →
  the same, reporting `Vec<ScopeLayoutMoves>` so the caller can make it undoable
  per scope. What `StructureDesigner::layout_active_network` calls.
- `compute_layout(network, registry, algorithm)` → one scope's positions without
  mutating
- `layout_subgraph(network, registry, ids, algorithm)` → positions for **only**
  the nodes in `ids`, wired to each other alone (see below)
- `incremental::layout_incremental(network, registry, snapshot, wires, algorithm)`
  → the incremental pass over every scope, inside-out.
  `layout_incremental_with_statements(…, statements)` is the same pass with the
  edit's layout statements as Step 2½. **That is what an AI edit runs**:
  `ai_text_edit` calls it unconditionally on an applied edit and records
  `LayoutPath::Incremental`. There is no preference gating it — the pass is
  repair, not layout — and `auto_layout_after_edit` is gone
  (`doc/design_incremental_layout.md`, open question 1).

## Algorithms

**TopologicalGrid:** Assigns nodes to layers by topological order, then arranges vertically within each layer. Simple and predictable.

**Sugiyama:** Multi-phase algorithm: layer assignment → crossing reduction → coordinate assignment. Better results for complex graphs but more expensive.

## Usage

Layout is triggered by:
- **AI text-format edits**, through `layout_incremental` — the repair pass, not
  a reflow. `text_format/auto_layout.rs` still places a node at creation time,
  but from Phase 5 that position is a throwaway the block placement overwrites.
- "Auto-Layout Network" menu action in the UI, through
  `StructureDesigner::layout_active_network()` — **not** `layout_network()`
  directly. That wrapper diffs old against new positions per scope and records
  the whole rearrangement as one undoable step (#270): a single
  `MoveNodesCommand` when only one scope moved, a `CompositeCommand` over one
  per scope when a body moved too. Calling `layout_network()` from a
  user-facing path would silently bypass undo.
- Node size estimation uses constants from `node_layout.rs` in the parent directory

## Comments are not invisible to layout any more

Comment nodes used to enter the layering as degenerate vertices and distort it:
depth 0 with no consumers put them at the bottom of the leftmost column
(`TopologicalGrid`), and having no wires at all made each its own singleton
component stacked below the whole graph in `HashMap`-iteration order
(`Sugiyama`). Since `doc/design_wire_annotations.md` Phase 5 they are handled by
a dedicated pass instead. Three rules to keep true:

1. **No comment ever participates in layer assignment** (D8). `TopologicalGrid`
   drops them from `depths`; `Sugiyama` excludes them from
   `find_connected_components`. One invariant covers both anchored comments
   (placed by rule afterwards) and unanchored ones (whose position is the thing
   being preserved). A new algorithm must do the same and then call
   `common::place_comments` as its last step.
2. **Comment boxes use the real `CommentData.width` / `.height`**, never
   `node_layout::estimate_node_height` — the estimate is 83 px against a default
   of 100 and a routine resized value of 300+, on a 210 px column pitch, so
   sizing one wrongly just produces overlaps in a tidier arrangement. This is
   now one case of the general rule in `size.rs`, which `common::node_size`
   forwards to.
3. **Comments never displace graph nodes.** They always yield: moving a node
   would undo the layout just computed and break column alignment. The two
   tiers - beside the anchor, else the nearest gutter - are documented on
   `place_comments` and `gutter_position`.

The whole section is **disposable scaffolding** (D10): a separate design will
rework auto-layout around preserving human intent for *all* nodes. Keep it small
and keep its constants cosmetic - no tuning surface, and nothing about a
placement written to the file. `doc/research_intent_preserving_layout.md` is
where the replacement is being worked out.

## One size function: `size::rendered_node_size`

Four size functions used to exist and no two agreed — the Sugiyama helper
ignored HOF bodies, the inlining helper ignored comments, the creation-time
placer knew only a type name, and Flutter's `ScopeResolver` knew both and is the
one that actually paints. `doc/design_incremental_layout.md` (Phase 1) collapsed
them into **`layout::size::rendered_node_size(node, registry)`**, which mirrors
`effectiveNodeSizeLogical` / `_computeBodySize` rule for rule: a comment is its
own `CommentData` box, an expanded HOF is gutters plus a *recursively measured*
body (`max(stored, content + padding)`), everything else is title + pins +
subtitle + padding. The subtitle row is **computed** from
`NodeData::get_subtitle`, not assumed present the way all three old estimates
assumed it.

Three rules follow:

- **Anything holding a real `Node` calls it.** `common::node_size` /
  `node_height` / `node_width`, `node_inlining::estimate_node_size_in_network`
  and `text_format/auto_layout`'s obstacle scan are all forwarders now. The one
  legitimate estimate left is `auto_layout::get_node_size`, which sizes a node
  that *does not exist yet* and so has no body, no comment box and no data to
  derive a subtitle from.
- **The stored `body_width` / `body_height` are a floor, never the value**, and
  layout never writes them back. A freshly built `closure` carries the flat
  320x180 default even when its body holds a nested `map` that renders far
  wider.
- **`tests/fixtures/layout_size_parity.json` pins the agreement with Flutter.**
  `test/layout_size_parity_test.dart` regenerates it from a network description
  the Rust test writes (`layout_size_parity_input.json`), so a Flutter-side size
  change fails a *Rust* test until the fixture is refreshed. That is the point:
  the two rules have drifted before and the drift was invisible until something
  overlapped on screen. The refresh cycle is in `layout_size_test.rs`'s module
  docs — Rust, then Dart, then Rust again.

## Two motion primitives, one per axis

`doc/design_incremental_layout.md` D6. Everything that has to make room in an
existing drawing — the incremental pass, and since Phase 2 the GUI's
growth reflow as well — goes through exactly two operations in `motion.rs`,
and adding a third is a design change, not a refactor:

- **`shift_half_plane(T, dx, fixed)`** — horizontal, rigid, global. Every placed
  node with `x >= T` moves right by `dx`, except a `fixed` set. `fixed` must be
  closed upstream (`upstream_closure`), and that is the whole correctness
  argument: with it, a wire from a moved node into an unmoved one cannot exist,
  so **a forward wire can never turn backward**. Drop the closure and the
  primitive stops being wire-safe.
- **`cascade(R, dir, fixed, ignore)`** — vertical, minimal, local. Only nodes
  that actually collide move, by the least that clears the collision, and the
  push propagates **only through overlaps the cascade itself created**. A
  pre-existing overlap is never repaired and never blocks anything — the design
  rule is *repair only what this edit broke*, and both hand-drawn corpora
  contain overlaps their authors are content with.

The pairing is not symmetric by accident: x carries a directional invariant
(rightward flow) and y carries none, so x gets the operation that provably
preserves order and y gets the one that moves the least.

**Two rules the demolib corpus run taught the cascade, and both are load-bearing:**

- **"Already overlapping" means a *real* overlap, at zero clearance — never
  "within `VERTICAL_GAP`".** Hand-drawn networks are full of neighbours a dozen
  pixels apart, well inside the 30 px clearance. Test the two at one inflated
  strictness and every such pair reads as a pre-existing overlap, so a moving
  node slides straight *through* its neighbour and nothing repairs it. Both
  "did this move newly collide" tests (`newly_overlaps_y` / `newly_collides`)
  therefore ask the question at **both** strictnesses and take the union. The
  mirror of the same rule is `grow_rect`'s `ignore` set: it is not "overlapped
  the old rect" but "the growth did not newly reach it", because a neighbour
  17 px *above* a downward growth has exactly the clearance it always had and
  must not be swept along — forcing `Down` on it shoves it past the node it was
  sitting above (363 px, in the case that found this).
- **A queued blocker is stored by id and re-read at pop time.** A pusher can
  itself be pushed again before the node it enqueued is popped; a rect captured
  at enqueue time then makes that node clear the place its blocker *used to be*.
  That is a chain, not a corner case, and it left two demolib nodes fully
  overlapping after a cascade that visited both.

**`grow_rect(node, old, new)` is the one growth operation** (D11) — width delta
as a shift, height delta as a downward cascade, in that order. Every path where
a node's footprint grows in place calls it: `reflow_for_footprint_change`,
`inline_custom_node`, `convert_instance_to_closure`, and Step 2 of the
incremental pass. It replaced `node_inlining::make_space_for_inline`, the
quadrant shift, which swept the whole lower-right region on both axes — moving
nodes nothing was going to collide with, and occasionally driving its
right-moved and down-moved halves into each other.

Two things to know before calling any of them:

- **They take a measured `sizes` map, not a `&NodeTypeRegistry`.** A
  `NodeNetwork` lives *inside* the registry (`NodeTypeRegistry::node_networks`),
  so `&mut NodeNetwork` and `&NodeTypeRegistry` cannot be held at once. Call
  `measure_scope` before taking the mutable borrow. Sizes are position-
  independent, so one measurement is good for a whole pass over one scope.
- **The map's keys are the placed set.** A node absent from `sizes` is invisible:
  not an obstacle, not moved. From Phase 3 that is how a freshly added node stays
  out of the way until its block is placed, so do not "helpfully" fall back to
  every node in the network.

## The incremental pass: blocks, and the invisibility rule

`incremental.rs` runs all eight steps per scope, inside-out over
`delta::scopes_inside_out`. Three things about it are easy to break and
expensive to debug:

- **An added node is invisible until its block is placed.** It is not an
  obstacle, not a snap candidate, not in the drawing's bounding box, and neither
  primitive moves it. This is expressed by *excluding it from the `sizes` map*
  (`measure_scope` minus `delta.added`), which is why "the map's keys are the
  placed set" above is a rule and not a description. It joins the map the moment
  its block lands. The same exclusion applies to the body measurement in
  `body_frame`: a body's right edge — where the `output` anchor sits — is where
  the body renders *before* this edit's nodes are placed, never a reading of
  their throwaway creation-time positions.
- **The inside-out order is what feeds a body's growth to its parent.** An HOF's
  footprint is `max(stored, body content + padding)`, so a settled body makes its
  owner measure bigger, and the parent's `EditDelta` — computed at the parent's
  own turn, against the live network — already classifies the owner `grown` at
  its settled size. Do not *also* thread a `grown` accumulator up from the child:
  `grow_rect` moves by `new - old`, so repairing one HOF twice with two different
  `new`s shifts its neighbours twice.
- **A block is placed rigidly.** `Block.rects` are block-local with the bounding
  box at the origin, so a placement is one translation; nothing inside a block is
  ever re-derived against the drawing it landed in.

Comments are ordinary nodes on this path (D9): obstacles at their real size,
pushed by the cascade, never re-placed by rule. `place_comments` is *not* called
here — it keeps serving the full reflow — which is why `layout_subgraph` stops
short of it and each algorithm's `layout` calls it afterwards.

The two comment steps are the exceptions, and both are narrow. **Step 7** places
a comment the edit *created*, beside its anchor, through `common`'s own
`anchor_placement_box` / `surrounding_candidates` — one rule for "beside its
anchor", shared with the full reflow rather than reimplemented. Those four
candidate positions sit at `ANCHOR_GAP` (24 px), which is *less* than the
layout's ordinary 30 px vertical clearance, so they must be collision-tested at
`common::COMMENT_CLEARANCE` (16 px) — the same constant and the same reason
`place_comments` has. Test them at 30 and all four are rejected every time, and
every new anchored comment silently falls through to the Step 5 fallback.
**Step 8** pulls a comment whose distance to its anchor *grew* during the pass
back to its exact original offset, if that spot is free, else to the free
`surrounding_candidates` spot nearest it that is closer to the anchor than the
comment is now. "Free" is tested at `COMMENT_CLEARANCE` like Step 7, and
**ignores every node that kept its offset to the anchor** (the anchor itself,
the rest of a group moved together, a wire anchor's ends): the exact offset
restores the comment's relation to precisely those nodes. Test it at
`VERTICAL_GAP` with the anchor as an obstacle and a comment drawn beside its
node can never return — which is how comments came to stay behind when their
anchor was placed. It can never move a comment away from its anchor, never
create an overlap, and never act on an anchor it cannot resolve on both the
before and the after side.

## The report: every move has a cause, recorded at the call site

`doc/design_deliberate_node_placement.md` A3. `layout_incremental` returns a
`LayoutReport` (`report.rs`), which `ai_text_edit` hands to the AI as the edit
result's `layout` section and copies into the AI edit log. Three rules:

- **`from` / `to` come from the pass's one before/after diff, never from the
  primitives**, so a reported move cannot disagree with an actual one. The
  *why* is recorded separately, into the scope's `Causes`, from what each
  primitive call returns (`cascade`'s ids, `shift_half_plane`'s
  `ShiftOutcome` — both halves — `grow_rect`'s moves). The primitives
  themselves know nothing about causes; keep it that way.
- **A new motion call in the pass must record a cause.** Statement calls
  record `place` / `arrange` on their subjects and `push(dir)` on what they
  moved, the direction read off the node's y (a split cascade goes both
  ways); everything else records `grow` / `auto`. Forget it and the
  node is reported with no `cause`; `layout_test_support`'s `run()` and the
  corpus run both fail on that. The one call that must *not* record is
  `choose_cascade_dir`'s simulation, which restores what it moved.
- **Findings are measured independently of the repair**, per scope at the end
  of its turn, against the pre-edit `PositionSnapshot`: new overlaps at zero
  clearance, and new same-scope wires left backward. The oracle checks the
  overlap findings in both directions with its own overlap test, so do not
  make `check_overlap_findings` call `scope_findings`.
- **Wire findings judge the curve Flutter draws** (`wire_geometry::WireCurve`:
  `_getPath`'s cubic, control points `WIRE_CONTROL_OFFSET` out), never the
  rectangle between the pins. "Before" is rebuilt from the snapshot's boxes
  for every wire not in `added_wires`. A pair with nothing changed is never
  tested and the rest are box-tested first; the demolib worst case (no
  snapshot, every pair) is timed by `wire_findings_are_fast_on_demolib`.
  Mirror-image wires meet exactly at a sample point, so the segment test
  counts touching; wires sharing a pin, or joining the same two nodes (their
  crossing is pin order, which no placement fixes), are skipped instead.
- **Absolute findings are `scope_findings` against no baseline, never a
  second implementation.** `absolute_findings` passes an empty snapshot and
  every wire of the scope as added, so everything there now is reported; it
  feeds both `query --layout --findings` (`readout::findings_readout`) and the
  edit's `totals` (`finding_totals`, before = the pre-edit clone
  `ai_text_edit` keeps for rollback, after = the edited network). `totals` is
  computed only when an existing node moved: two whole-scope passes per
  touched scope. Keep `findings` itself baseline-relative — that is what lets
  an agent trust it as "what my edit broke".

## Step 2½: layout statements and the pinned set

`doc/design_deliberate_node_placement.md` A2 / D6. The editor parses `place`,
`arrange` and `straighten` but never applies them; `edit_network_with_layout`
hands them back, `ai_text_edit` passes them on, and `placement.rs` runs each
scope's statements between Step 2 and Step 3, in written order. Rules:

- **It adds no motion.** A placed subject is put down and what it lands on is
  cleared with `cascade` (no `push` → `choose_cascade_dir`; `up` / `down`), or
  for `push right` with a half-plane shift whose `fixed` is the subject's
  upstream closure — the same primitives, nothing new. Every check runs before
  the first position is written, so a rejected statement moves nothing.
- **Pinned = held in cascades, never `fixed` in a shift.** The pinned set (what
  a statement placed or arranged, plus — after the last one — what it measured
  against, `all` excluded) is the `fixed` argument of every later `cascade` and
  the `hold` of `shift_half_plane_holding`, whose landing clean-up then never
  pushes a pinned node vertically. Putting pinned nodes into a shift's `fixed`
  would break the upstream-closure argument; a shift carries them with their
  side instead. A new motion call after Step 2½ must take the pinned set.
- **A statement's push ignores what its move did not newly reach**
  (`motion::newly_collides`, both strictnesses), like `grow_rect`. Without it,
  `place x at (<its own position>)` pushes every pre-existing overlap of `x`,
  and the read-out's coordinates stop round-tripping (the corpus checks this).
- **Step 6 also covers statement-handled nodes**: every same-scope wire
  touching one, unless it ran backward before the edit, because such a node
  never went through Step 4. This wins over the statement: a subject placed
  right of a node it feeds has that node's side shifted past it, so its
  horizontal relation flips. The statement oracle therefore assumes generated
  relations run with the wires.
- A scope with statements runs even when its delta is empty (`place x by …`
  alone is an edit). A statement for a scope that never comes up is a warning.

## `hand_moved` is a tiebreaker, in exactly two places

It is set only by deliberate human placements: a drag (`end_move_nodes`) and
an Arrange command (`arrange_selected_nodes`), each inside its
`MoveNodesCommand` so one Ctrl+Z takes back the claim too. Nothing automatic
sets it.

`Node::hand_moved` (D5) never blocks a repair — the pass reads it only where it
already had a free choice, and both sites have a control test with the flag off:

- **Which way Step 5b cascades.** `CascadeDir::Split` is the specified direction
  and stays the default; when the scope holds hand-placed nodes, all three
  directions are simulated (run the real cascade, restore the y positions,
  score) and ranked by *fewer hand-moved nodes displaced, then fewer nodes, then
  less total displacement*. With nothing hand-placed the simulation is skipped
  entirely, so the common path costs nothing.
- **How far Step 5a slides.** When every node blocking the ordinary
  `SLIDE_WINDOW` is hand-placed, the window is doubled and searched again before
  the pass gives up and pushes. Sliding moves nothing; pushing a node a human
  placed is the most expensive thing this pass can do.

The flag's third documented use landed in Phase 5 and is the one place it is a
**hard constraint**: `LayoutPreferences::respect_hand_moved_in_reflow` (off by
default) makes the *full* reflow lay the movable nodes out among themselves,
leave the flagged ones bit-identical, and cascade whatever the arrangement
landed on one. It is a constraint the user asked for, which is what makes it
different in kind from the two tiebreakers above.

## The corpus run, and a known limit of the window rule

`tests/structure_designer/layout_corpus_test.rs` runs five generated edits
against every network of `demolib/baselib_with_demos.cnnd` (tracked, so it runs
in CI) and hands each before/after pair to the oracle. Point it at the
maintainer's working file with `ATOMCAD_LAYOUT_CORPUS=<path>`. **It is the only
test that sees real hand-drawn geometry**, and every cascade bug listed above
came out of it rather than out of a hand-written scenario — a synthetic fixture
puts nodes 30 px apart because that is the constant in the code, and a person
does not.

The generator is blind, so `at_least_one_edit_of_every_kind_lands` guards
against a kind quietly ceasing to produce valid scripts and the run passing
vacuously. Keep that guard green; without it the whole file can go silent.

**The window rule's snap gives up in dense regions, and that is not a bug in
the code.** The snap looks at most one node width left of the default for an x
where no movable node's box straddles the line — and 160 px-wide boxes tile a
dense drawing, so no such x exists. Every strictly-8 px loose column in demolib
(`lib_icorner_z_cotahedral{111}`'s four `half_space` nodes are the clearest)
therefore keeps the default and is cut in two by a shift. The rule works where
there is whitespace within reach, which
`a_loose_column_straddling_the_shift_line_moves_as_a_whole` pins; widening its
reach is a design change, not a fix.

## `readout.rs` and `Node::ai_placed`: what the AI is shown

`doc/design_deliberate_node_placement.md` A1 / D12. The read-out prints one
scope as plain boxes in stored coordinates (body-local inside a body), names
spelled by `unique_node_names` + `format_identifier` exactly as `query` spells
them. Three rules:

- **Groups use the motion code's constants, never their own.** Lefts/tops
  within `motion::COLUMN_TOLERANCE`, gaps up to `Gap::Wide`, so the read-out
  and the window rule agree on what a column is. Groups are detected over the
  whole scope and only *filtered* for display; a box that cuts a column must
  not cut the group.
- **The `ai` tag is `Node::ai_placed`, which is runtime-only.** It is not in
  `SerializableNode`, so anything rebuilt from the serialized form (load, the
  undo snapshot restores, a `--dry-run`'s revert) comes back untagged — the
  safe direction. The one path that must not lose it is the dry run, which
  promises to change nothing: `ai_text_edit` re-applies the flags from the
  pre-edit `PositionSnapshot` (which carries them, as `--replace` needs) after
  the revert. A new restore path that must be invisible needs the same.
- **Set in exactly one place, cleared in two.** `ai_text_edit` tags the nodes
  whose name path the pre-edit snapshot lacks (the incremental pass's own
  notion of "added"); `end_move_nodes` and `arrange_selected_nodes` clear it
  inside their `MoveNodesCommand` (`ai_placed_before`), so Ctrl+Z gives the
  node back to the AI. Copies (paste, duplicate, factoring, inlining) start
  untagged, bodies included.

## `arrange.rs`: one implementation for the GUI and the AI

`doc/design_deliberate_node_placement.md` (B2, A2). The Arrange menu and the
AI's `arrange` / `straighten` statements and `[…] column` / `row` groups use
the same words, so they use this one function. Two rules:

- **It is pure and returns positions for listed nodes only.** The caller
  commits. The GUI commits through one `MoveNodesCommand` and pushes nothing
  (D11); the AI path commits through placement, which pushes. Do not add a
  cascade here, or the GUI starts moving unselected nodes.
- **Pin heights come from Flutter's formula, not `node_layout`'s.**
  `node_layout::input_pin_position` / `output_pin_position` sit every pin half
  a row (11 px) above where Flutter draws it. That cancels between two
  ordinary pins, which is why the comment pass gets away with it, but not
  against a title-bar pin (the function pin, a `closure`'s output, 16 px).
  `straighten` therefore uses its own offsets mirroring
  `ScopeResolver._pinPositionNormal`.

Gaps go through `common::Gap` (`across()` / `down()`), never a literal.

## `layout_subgraph`: the same algorithms over a subset

`layout_subgraph(network, registry, ids, algorithm)` restricts a full layout to
`ids`: only those nodes are placed, and **only the wires between them are
edges**. Step 3 lays a block out with it, and `layout` in both algorithms is now
`layout_subgraph` over every id plus `place_comments`, so the two cannot drift.

The restriction goes all the way down — `common::compute_node_depths_within`,
component discovery, dummy nodes, barycenters. Two consequences worth knowing:

- A node fed only from outside the subset is a **source at depth 0**, which is
  what lets a block start its own drawing instead of inheriting the column index
  its producer happens to sit in.
- **A cross-scope wire is never an edge** (`source_scope_depth != 0`). `$element`
  names the owning HOF in the *parent* scope and ids are unique per network, so
  a body node can carry that same number without being connected to anything.
  Every graph walk here filters on depth for that reason.

## Sugiyama layering: left-packed, then pulled toward consumers

`compute_node_depths_within` packs every node as far left as it can go, so a
constant feeding the last node of a chain sits in column 0 and its wire spans
the drawing. Sugiyama therefore follows it with
`common::pull_toward_consumers`, which moves a node to the column before its
nearest consumer when it has at least as many wires out as in (the total wire
span cannot grow). On demolib this cut crossings by ~37 % and wires behind
nodes by ~78 %, with no network worse by more than one. Keep
`compute_node_depths_within` itself left-packed: `TopologicalGrid`,
`straighten`'s ordering and its own tests rely on that meaning. A test that
needs a long edge must give its source an early consumer too, or the pull
shortens the edge away.

## Sugiyama columns are per layer

A column is as wide as its widest member plus `common::COLUMN_GAP`, not a fixed
`COLUMN_WIDTH` pitch. A network of ordinary 160 px nodes lands exactly where it
did before; an expanded HOF no longer overhangs the next column. Keep the
equality test (`layout_columns_test::a_uniform_network_keeps_the_old_column_pitch`)
green — a "tidier" pitch would shift every existing drawing the first time
someone reflows it.

**Crossing reduction is pin-aware.** Every `LayerEdge` carries the `Port` of
the pin at each end (a fraction down the node's side; dummies sit at the
middle), and both the barycenters and `count_crossings` measure a wire end as
*layer index + port*. Counting between nodes alone, two producers feeding
`diff.base` and `diff.sub` tie at zero crossings and the drawn order is an
accident. A new edge must carry real ports, not a constant.

**Coordinate assignment keeps the order crossing reduction chose.** The
refinement moves each node to the nearest y inside its free range (its
neighbours in layer order, gaps kept), in three sweeps: toward outputs, toward
inputs, toward outputs. A long wire is drawn as one curve between its two pins,
not through its dummies, so a dummy says which side of the wire each node of
its layer belongs on: `clear_long_wire_lanes` pushes nodes off a long wire's
lane, but only for wires that drop at most `MAX_CLEARED_DROP`. Clearing steep
fan-out wires too costs about twice the height for few extra uncovered wires
(the measurements are on the constant). Judge a change here by wires drawn as
Flutter draws them (a cubic with control points 50 px out), not by the
rectangle between the pin heights: that rectangle overstates a steep wire's
footprint and favours clearing everything.

`find_connected_components` seeds its BFS from **sorted** ids. The size sort
after it is stable, so `HashMap` order used to decide how two equal-size
components stacked, per process. Any new iteration over nodes or scopes must be
ordered the same way (`doc/design_incremental_layout.md` D7).

`node_layout::input_pin_position` / `output_pin_position` estimate where a wire
meets a node. A wire's anchor point is its Bezier midpoint, which reduces to the
mean of its two pin positions; the painter's `_anchorTargetPoint`
(`node_network_painter.dart`) computes the same point from real geometry, and
the two must not diverge - it is one decision, not two.
