# Design: Comment anchors — associating comments with wires and nodes (issue #427)

**Issue:** https://github.com/atomCAD/atomCAD/issues/427 — "implement feature
for associating comment nodes to node network wires & more" (mechadense).
**Discussion:** https://github.com/atomCAD/atomCAD/discussions/423 — "Proposal
for improvement on comment nodes & co".

The discussion is a bundle of loosely coupled proposals and explicitly leaves
the choice of what to build open. This doc implements one of them — the
"**Light**" variant of *Documentation via wire annotations*:

> An existing comment node could point to the center of a wire with a dashed
> line to show the association in a minimalist way — drag from anywhere within
> the comment node to anywhere on the wire (or vice-versa) and the dashed
> association line is added / appears.

extended, at the maintainer's request, so that a comment can equally be
anchored to a **node**. Everything else in the discussion is out of scope; see
[Non-goals](#non-goals) for what and why.

## Motivation

Comment nodes are free-floating. Their association with the part of the network
they document exists only in the reader's head, inferred from canvas proximity.
Two things break that inference:

1. **Manual rearrangement.** Moving a cluster of nodes leaves its notes behind.
2. **Automatic layout.** `layout_network` (`layout/topological_grid.rs`,
   `layout/sugiyama.rs`) has *zero* comment awareness — neither file so much as
   mentions comments. A comment node has no incoming wires, so both algorithms
   actively displace it; see [Current layout behavior](#current-layout-behavior)
   for exactly where each one puts it.

Point 2 is the near-term driver, and it is not hypothetical: the default
algorithm is **Sugiyama** (`LayoutPreferences::default`) and
`auto_layout_after_edit` defaults to **true**, so a full relayout already runs
after every AI text edit. The planned work on AI/human co-editing needs
an auto-layout pass that reflows AI-induced changes while preserving
human-authored structure. Such a pass cannot preserve a comment's association
with its subject unless that association is *stored*, because there is nothing
to preserve — proximity is an output of layout, not an input to it. Comment
anchors are that input.

## Current state (analysis)

**Comment nodes are ordinary nodes.** `CommentData { label, text, width,
height }` (`nodes/comment.rs:19`); the node type declares `parameters: vec![]`,
`output_pins: OutputPinDefinition::single(DataType::None)`, category
`NodeTypeCategory::Annotation`, and an `eval` that returns
`EvalOutput::single(NetworkResult::None)`. Text properties round-trip through
the generic `get_text_properties` / `set_text_properties` pair.

**Wires are not stored; they are assembled.** `Wire` (`node_network.rs:493`) is
a *view* struct. The persistent form is an `IncomingWire` inside
`Argument.incoming_wires` on the **destination** node. There is no wire id and
nowhere on a wire to store anything, so an association must live on the comment
side regardless.

**A wire's identity needs four fields, not six.** `Argument::add_source`
(`node_network.rs:~375`) dedupes incoming wires on `source_node_id` alone —
re-adding an existing source *updates* its pin rather than appending — and
`Argument::remove_source` finds by `source_node_id` alone. The invariant is
stated outright on `argument_output_pins`: *"Phase 1 invariant guarantees no
duplicate source ids in the result."* So within one argument slot the source
node id is a key, and `source_pin` / `source_scope_depth` are derivable from the
resolved wire rather than part of its identity.

**Argument indices can shift, but only where a parameter id exists.** The one
fragile field in a destination-side key is `destination_argument_index`, a
positional index into `node.arguments`. `function_pin_roles` documents the same
hazard. But `Parameter.id: Option<u64>` (`node_type.rs:57`, *"Persistent
identifier for wire preservation across renames"*) is distributed
complementarily: there are 264 `id: None` occurrences across the static
built-in node types, and `id: Some(_)` appears **only** in the dynamic-arity
types (`expr`, `sequence`, `switch`, `zip_with`, `product`,
`record_construct`) and in network parameters
(`closure_network_conversion.rs`). That is exactly the split that matters —
built-in pin layouts are fixed, so their indices never move; the layouts that
*can* move are precisely the ones carrying ids.

**Repair has a home and an established safety rule.**
`NodeTypeRegistry::repair_node_network` (`node_type_registry.rs:2769`) is the
network-level pass that already remaps and prunes index-keyed state after node
types change, and it states the rule this design adopts verbatim:

> *"a deleted field's wire is dropped rather than silently re-pointed at
> whatever field slid into its old index"*

**The text format already parses half of what is needed.**
`PropertyValue::NodeRef(String, Option<String>)` (`text_format/parser.rs:113`)
already accepts `mybox` and `mybox.diff`. `->` is a token of the **expr**
language (`expr/lexer.rs`), not of the network text format, so it is free.

**And there is a precedent for a network-level property on a node.**
`visible` is not a `NodeData` property — it lives in
`NodeNetwork.displayed_nodes` — yet it round-trips through the node's property
braces via three special cases: emitted in the serializer's "third pass"
(`text_format/network_serializer.rs:274`), skipped in the editor's literal-props
pass (`network_editor.rs:471`), and handled in the connection-collection pass
(`network_editor.rs:542`) by deferring to a `visible_nodes` list resolved after
all nodes exist. An anchor property follows the identical shape — and must,
because `NodeData::set_text_properties` receives only a `HashMap<String,
TextValue>` and cannot resolve node *names* to ids.

**Dashed rendering already exists.** `NodeNetworkPainter._dashedPath(path, on,
off)` and `_dashPatternFor` (`node_network_painter.dart:472`, `:455`) draw
dashed wires today for motif/lattice alignment warnings.

### Current layout behavior

Both algorithms share four stages: compute depths → group into columns by depth
→ order within each column by the barycenter of neighbouring columns → assign
coordinates (`x = START_X + col * COLUMN_WIDTH`, i.e. `100 + col * 210`).
Sugiyama adds a connected-component split up front, dummy nodes for long edges,
and an iterated sweep in place of the two-pass ordering.

`compute_node_depths` asks one question — "what are your inputs?" — so a
comment is **always** depth 0. What happens next differs:

| Algorithm | Mechanism | Result |
|---|---|---|
| `TopologicalGrid` | Depth 0; the backward ordering pass gives a node with no consumers a barycenter of `f64::MAX / 2.0` ("push to bottom") | Bottom of the **leftmost column**, interleaved with real source nodes |
| `Sugiyama` (default) | No wires at all ⇒ its own **singleton connected component**; components are sorted largest-first and stacked vertically with `COMPONENT_GAP` | A vertical run of every comment, stacked **below the whole graph** |

Two further details a fix has to account for:

- **Sizes are estimated from pin counts, not read.** Both algorithms size nodes
  via `node_layout::estimate_node_height(params, outputs, subtitle)`; for a
  comment that is `(0, 1, true)` → **83 px**, against an assumed
  `NODE_WIDTH` of 160. `CommentData` defaults to **200 × 100** and users
  routinely resize to 400 × 300. With a 210 px column pitch, a widened note
  overhangs the next column by ~190 px and the algorithm cannot see it.
  Placement and sizing must be fixed together or the notes just overlap in a
  tidier position.
- **Sugiyama's comment order is not deterministic.** The singleton components
  tie on size and `sort_by_key` is stable, so their relative order comes from
  iteration over `network.nodes` — a `HashMap` whose order std randomizes per
  process. Nothing depends on it today; it is noted because it disappears for
  free once comments leave the component walk.

**Wire and node hit-testing already exist.** `findWireAtPosition` (used by
`NodeNetworkState._handleCanvasTap`, `node_network.dart:1542`) and
`ScopeResolver.findNodeAtScreenPosition`. Wires are already first-class
selectable objects (`NodeNetwork.selected_wires`).

## Rejected alternative: the association as a real wire

Before settling on stored anchors, one cheaper mechanism was evaluated and
rejected. It is recorded here because it is the obvious idea and will be
re-proposed otherwise.

Give the comment node an optional input pin `on: Unit`. `DataType::Unit` is a
universal sink — *"Universal `T → Unit` widening (the 'discard' rule). Any
source type … coerces to `Unit`"* (`data_type.rs:587`) — and there is no
generic "required input must be connected" validation rule, so the pin would
accept a wire from any output of any type with no type-system work at all.
Storage, `.cnnd` serialization, text format, undo, repair, scope handling,
selection and delete-cascade would all be inherited from the existing wire
machinery for free, and the layout pass would get a real DAG edge.

**Why it was rejected:** an incoming wire records `(source_node, source_pin)`.
It can express "this note is about the value `mybox` produces" but *cannot*
express "this note is about the wire from `mybox` to `union.a`" — when a value
fans out to several consumers, every branch is the same incoming wire. Wire
granularity is the thing the issue actually asks for, so the mechanism is not
merely cheaper, it is insufficient. Having established that stored state is
required for wire anchors, routing node anchors through a *second*, different
mechanism would leave the feature with two representations of one concept.

## Non-goals

- **Documentation via 0-ary closure frames** (discussion §2) — the
  comment↔closure interconversion, upstream frame expansion to the nearest
  named value, and the closure "main comment". Explicitly excluded by the
  maintainer.
- **In-wire comment nodes** (the discussion's "Heavy" and "Mid" variants) — a
  comment spliced into the data path as a pass-through node. This puts
  documentation inside evaluation: `eval`, output-type resolution, memoization
  keys, `build_reverse_dependency_map`, cycle detection and `.cnnd` hashing
  would all have to learn to see through it, and every annotated wire gains a
  hop. Large blast radius, no semantic gain over an anchor.
- **Named intermediate values / wire variable names.** atomCAD already has
  them: `Node.custom_name` is the let-binding name the text projection prints
  (`mybox = cuboid { … }`), with multi-output references qualified by a
  type-level pin name (`format_reference`, `network_serializer.rs:296`). Making
  those names visible on the canvas is a worthwhile separate change; adding a
  *second*, wire-level naming channel is not, because the text projection would
  then have two candidate names for one value. Tracked separately.
- **Explicit typed holes, steady typing, blame shifting** (discussion §3) — a
  type-system feature, not a comment feature. atomCAD's error-management
  subsystem (`doc/design_error_management.md`, error chaining, error
  navigation) is the right home for verbose type diagnostics.
- **Hash addressing** (discussion §4) — orthogonal.
- **Alignment structure on arrangement trees** — deferred by the proposal
  itself.

## Design decisions

**D1 — The anchor lives on the comment, as stored state.** `CommentData` gains
an anchor list. Wires have nowhere to store anything, and keeping the reference
one-sided makes copy/paste, deletion and snapshotting single-ended.

**D2 — `Vec<CommentAnchor>`, not `Option<CommentAnchor>`.** A note that explains
two wires is plausible, and widening `Option` → `Vec` later is a serde
migration for no benefit. The initial UI may still create at most one; the
model does not care. Empty vec is the default and serializes as absent.

**D3 — Two anchor variants, one mechanism.**

```rust
pub enum CommentAnchor {
    Node(u64),
    Wire(WireAnchor),
}
```

**D4 — A wire anchor is keyed by destination slot + source node.**

```rust
pub struct WireAnchor {
    pub destination_node_id: u64,
    pub destination_argument_kind: ArgumentKind,
    pub destination_argument_index: usize,
    /// Present iff the destination parameter carries a persistent id
    /// (dynamic-arity node types only). Authoritative over the index when set.
    pub destination_param_id: Option<u64>,
    pub source_node_id: u64,
}
```

`source_pin` and `source_scope_depth` are deliberately **not** stored: they are
properties of the resolved wire, and duplicating them creates a drift question
("which is authoritative?") with no upside. `destination_param_id` and
`destination_argument_index` are *not* redundant in that sense — they are
alternative addressings of the same slot with a defined precedence (id first),
and per the analysis above exactly one of them is reliable in any given case.

**D5 — Anchors are scope-local.** A comment may only anchor to a wire or node in
its own scope (its own network, or its own HOF/closure body). This keeps the
anchor scope-path-free and matches how a reader uses it. A wire whose *source*
lives in an ancestor scope (`source_scope_depth > 0`) is still anchorable — such
a wire is stored in the destination's scope, and the anchor addresses it from
the destination side.

**D6 — A dangling anchor is dropped, never re-resolved.** Adopted from
`repair_node_network`'s existing rule. A dashed line pointing at the wrong wire
is worse than no dashed line: it is documentation that actively lies. Repair
distinguishes two cases — *the slot moved* (remap via `destination_param_id`)
from *the wire or node is gone* (drop). Only the latter drops.

**D7 — Anchors do not affect evaluation.** They are inert with respect to
`eval`, type resolution, validation, dirty propagation and memoization. A
comment node remains a DAG isolate. In particular an anchor is **not** a
dependency edge: anchoring a note to a node must not make that node "used" for
the purposes of delete-reference checks or dead-code reasoning.

**D8 — No comment node ever participates in layer assignment.** Anchored ones
are excluded because they are placed by rule *after* the main pass, against
their resolved anchor; unanchored ones are excluded because there is no
rule and their position is the thing being preserved. One invariant covers both
cases, and it is far easier to keep true than per-case conditions threaded
through two layout algorithms. Note this is a *simplification* of the current
code as well as an addition: today comments enter the layering as degenerate
vertices and distort it (slots in column 0; singleton components that add
vertical stacking).

**D9 — Unanchored comments translate with the drawing, then place normally.**
Two rules, no heuristics:

1. **(D9.1) Translate by the graph's bounding-box origin delta.** Layout canonicalizes
   every position to start at `(START_X, START_Y) = (100, 100)`, so a network
   authored out at `(3000, 2000)` currently leaves its notes 3000 px away —
   off-screen from their own graph. Preserving each note's offset relative to
   the graph's top-left keeps it with the drawing. This is *not* inference: it
   claims no association with any particular node, only that the whole drawing
   moved.
2. **(D9.2) Run them through the same placement routine** anchored comments need
   anyway (see [Layout](#layout)). Their "natural spot" is simply the
   translated position, so in the common case — nothing collides — they do not
   move at all; when something does collide they fall to the gutter like any
   other comment.

Translation only — **not** translate-and-scale to the new bounding box. When
layout reorders the graph, a proportionally-placed note lands beside an
arbitrary unrelated node and *implies* an association that does not exist,
whereas a note visibly off to the side implies nothing. **Obviously-detached is
more honest than wrongly-attached.**

**D10 — The unanchored rule is deliberately scaffolding.** A separate design
will rework auto-layout around preserving human intent for *all* nodes, not just
comments; this rule exists to be replaced by it. That constraint is what rules
out proximity inference here, not the quality of the idea (see
[Deferred](#deferred-to-the-auto-layout-rework)) — a heuristic with four tuning
constants is precisely the work that rework would discard, and **persisting
inferred anchors would be worse than discarding them**: guesses written into
`.cnnd` files are indistinguishable from human-authored anchors, so the rework
would inherit a corpus of unknown provenance it cannot separate or undo. It
would damage the very input the intent-preservation work needs.

The general form of this decision, worth applying to the whole document:
`CommentAnchor` is **permanent** — persisted in the file format and the text
format, outliving any layout algorithm — and gets corresponding care. The layout
rule is **disposable** and has a known replacement date, so it should be small
enough that replacing it is trivial and honest enough that it does not corrupt
what replaces it.

## Data model

```rust
// nodes/comment.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentData {
    pub label: String,
    pub text: String,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<CommentAnchor>,
}
```

`#[serde(default)]` keeps every existing `.cnnd` file loading unchanged, and
`skip_serializing_if` keeps unanchored comments byte-identical to today's
output — which matters, because the roundtrip tests compare serialized bytes.

Resolution is a pure function against the containing network:

```rust
pub enum ResolvedAnchor {
    Node(u64),
    Wire(Wire),   // the assembled view struct
}

impl CommentAnchor {
    /// `None` if the anchor is dangling.
    pub fn resolve(&self, network: &NodeNetwork) -> Option<ResolvedAnchor>;
}
```

`resolve` is the single place that implements D4's precedence: for a wire
anchor, locate the destination node, pick the argument list per
`destination_argument_kind`, find the slot by `destination_param_id` when set
(scanning the node type's parameters) and by `destination_argument_index`
otherwise, then find the incoming wire whose `source_node_id` matches.

`resolve` answers *identity*, not geometry — it returns which node or which
wire, and the caller derives coordinates from the positions it already has
(the laid-out positions, for the layout pass; the `ScopeResolver`'s pin
positions, for the painter). It must not depend on layout having run.

### The anchor a comment is placed against

`anchors` is a list (D2), but placement needs exactly one target. **Placement
uses `anchors[0]`** — the first resolvable anchor in the list; the remaining
anchors only draw leader lines. This is deterministic, needs no centroid rule,
and matches the interaction surface, which creates at most one anchor
(see [Interaction](#interaction)).

That target resolves to a **box**, which is what the placement routine consumes:

| `ResolvedAnchor` | Placement box |
|---|---|
| `Node(id)` | the target node's laid-out box (position + estimated size) |
| `Wire(w)` | the wire's Bezier midpoint as a zero-size box |

## Text format

Two forms, both inside the comment's existing property braces:

```
note1 = comment { label: "chassis", text: "…", on: mybox }
note2 = comment { label: "passivation", text: "…", on: mybox -> union.a }
note3 = comment { text: "…", on: atom_edit1.diff -> union.b }
note4 = comment { text: "…", on: [mybox, sphere1 -> union.a] }
```

- `on: <node>` — node anchor.
- `on: <source>[.<pin>] -> <dest>.<param>` — wire anchor. The source side reuses
  the existing `NodeRef` shape (optional output-pin qualifier); the destination
  side names the receiving parameter.
- An array combines several anchors (D2).

This is the AI-facing surface and the reason the text format is not optional
work: an anchor that does not survive a text round-trip is an anchor the AI
silently deletes on its next edit, which defeats the feature's stated purpose.

Implementation follows the `visible` precedent exactly:

| Site | Change |
|---|---|
| `network_serializer.rs:274` (third pass) | Fourth pass: emit `on:` from resolved anchors, using `get_node_name` / `format_reference` for the source side. |
| `network_editor.rs:471` | Skip `on` alongside `visible` so it is not fed to `set_text_properties` and does not warn as an unknown property. |
| `network_editor.rs:542` | Handle `on` alongside `visible`: parse into a `pending_anchors` list resolved in the second pass, once every node exists and names map to ids. |
| `text_format/parser.rs` | New `PropertyValue::WireRef { source, source_pin, dest, dest_param }` plus `->` in the lexer. The array form is the existing `PropertyValue::Array` holding a **mix** of `NodeRef` and `WireRef` entries. |

If the `->` token turns out to complicate the grammar, `on: [mybox, union.a]`
parses today with no parser change at all (an `Array` of two `NodeRef`s) — but
it reads worse and overloads `NodeRef`'s output-pin slot with an input parameter
name, so the small grammar addition is preferred.

**Unresolvable names produce a warning, not a silent drop** —
`self.result.add_warning(...)`, matching how unknown properties are reported.

## Repair, copy/paste and deletion

**Repair** (`repair_node_network`, `node_type_registry.rs:2769`): after the
existing per-node type refresh, walk every comment node — via `walk_all_nodes`,
so body comments are covered; bare `nodes.values()` iteration is the documented
bug class here — and drop each anchor whose `resolve` returns `None`. The
`destination_param_id` precedence in `resolve` means a pin reorder on a
dynamic-arity node *remaps* rather than dropping.

**Copy / paste / duplicate**: these paths build an `old_to_new` node-id map
(`node_network.rs:~1296`). Anchors are node ids and must be remapped through
it. An anchor whose target is **not** in the copied set drops — pasting a lone
comment must not leave it pointing at an unrelated node in the destination
network that happens to share an id. This is easy to miss because the anchor
rides invisibly inside `node_data_json`.

**Deletion**: deleting a comment is already handled — its anchors go with it.
The reverse is the gotcha: deleting a *target* wire or node must clear anchors
on comments that are **not** in the delete set, and those comments' before-state
is therefore not snapshotted by `DeleteNodesCommand` / `DeleteWiresCommand`.
Without care, undo restores the wire but not the association.

## Undo

Per `undo/AGENTS.md`, a new persisted mutation needs a command:
`SetCommentAnchorsCommand` (before/after `Vec<CommentAnchor>` for one comment
node, `UndoRefreshMode::Lightweight` — anchors change nothing evaluable, D7).

For the deletion gotcha above, prefer `CompositeCommand`
(`undo/commands/composite.rs`, *"Bundles N child commands into one undo step"*)
bundling the delete with one `SetCommentAnchorsCommand` per affected comment.
This needs no change to the existing delete commands and keeps the whole thing
one undo step.

A comment's own anchors ride inside `NodeSnapshot.node_data_json`
(`undo/snapshot.rs:20`) for free wherever the comment itself is snapshotted.

## Rendering

One dashed leader line **per anchor** (D2), each from the comment's border —
at the point nearest the target — to:

- **Node anchor** — the point on the target node's border nearest the comment.
- **Wire anchor** — the midpoint of the wire's cubic Bezier (`t = 0.5`).

Reuse `NodeNetworkPainter._dashedPath` (`node_network_painter.dart:472`) with a
distinct pattern and a muted color, so leader lines never read as data flow.

**This must be rendered from `node_network_content.dart`, not from
`NodeNetworkState`.** `node_network/AGENTS.md` is emphatic: *File → Export node
network image* re-renders the canvas through `appendCanvasNodeWidgets` /
`canvasNodeWidget`, and anything added only to `NodeNetworkState` is missing
from every exported PNG while looking perfectly fine on screen.

Leader lines must also honour `hideSelection` and repaint on
`model.dragRepaint` so they track a dragged comment or a dragged target without
a full rebuild (the drag fast-path invariant in the same AGENTS.md).

## Interaction

A small anchor handle on the comment (alongside the existing bottom-right
resize handle, `comment_node_widget.dart:245`): drag from it and drop on a wire
or a node. Hit-testing is already available on both — `findWireAtPosition` and
`ScopeResolver.findNodeAtScreenPosition`. Dropping on empty space, or on a
target in another scope (D5), cancels.

Two hazards, both documented in `node_network/AGENTS.md` and both already
survived once by the resize handle and the in-place note editor:

1. The drag must not fight the comment's own `onPanStart` move-drag — use a
   dedicated handle with its own `GestureDetector`, as the resize handle does.
2. It must be inert while the note is being edited in place
   (`StructureDesignerModel.inPlaceEditRef`).

Removal: a context-menu item on the comment (*Remove anchor*), and re-dragging
the handle onto a new target replaces the anchor.

**The first implementation creates at most one anchor per comment** — the handle
sets `anchors[0]`, replacing whatever was there, and *Remove anchor* clears the
list. The model is a `Vec` so that multi-anchor notes need no migration later
(D2), and the renderer already draws one leader per entry, so a multi-anchor
comment authored through the text format displays correctly. Only the drag
gesture is single-anchor, and only for now — which is also why *Remove anchor*
needs no "which one?" disambiguation yet.

## API

Following the existing comment API naming (`structure_designer_api.rs:8600+`:
`resize_comment_node`, `update_comment_node`, `begin_edit_comment_node`,
`get_comment_data`):

```rust
#[frb(sync)]
pub fn set_comment_anchors(scope_path: Vec<u64>, node_id: u64,
                           anchors: Vec<APICommentAnchor>);
```

`scope_path` is mandatory on node-data getters and setters (`rust/AGENTS.md`).
`APICommentData` gains the anchor list so the canvas can draw leader lines. Per
the FRB rule, `APICommentAnchor` is a twin type defined in `rust/src/api/`, not
a re-export of the domain enum — a `pub use` re-export is invisible to codegen
and degrades silently to an opaque handle, so declare it in `api/` with `From`
impls both ways.

The twin mirrors `CommentAnchor`: a two-variant enum, the wire variant carrying
`WireAnchor`'s five fields (`ArgumentKind` needs its own twin or an `i32`
discriminant, whichever matches how the codebase already exposes it). Flutter
needs no resolved geometry — it already has node positions and the wire list,
so ids are enough to locate both ends of a leader line.

## Layout

`layout/common.rs` gains a comment-placement step (D8/D9) after the existing
pipeline, applied by both `topological_grid` and `sugiyama`:

1. **Partition** the node set into graph nodes, anchored comments, and
   unanchored comments.
2. **Lay out the graph alone** — the existing pipeline, unchanged, over
   non-comment nodes only. Keep two products of this step, **both computed over
   graph nodes only** (a comment must never influence either):
   - the graph's bounding box *after* layout — the gutters are its edges;
   - the **origin delta** = (bbox top-left after) − (bbox top-left before),
     which is what D9.1 translates unanchored comments by.
3. **Place the comments** by the routine below, one at a time in a
   deterministic order, each placed comment joining the obstacle set so later
   ones avoid it.

### Placement is a margin problem, not a gap-filling one

A laid-out graph has no interior space a comment could occupy:

| Free space in a laid-out graph | Size | A comment needs |
|---|---|---|
| Between columns | 50 px (`COLUMN_WIDTH` 210 − `NODE_WIDTH` 160) | 200 px default, 400+ resized |
| Between nodes in a column | 30 px (`VERTICAL_GAP`) | 100 px default, 300+ resized |

Interior gaps are 4–6× too small on both axes, so any search for a nearby free
rectangle will fail and end up outside the graph anyway. **That is the right
outcome, not a degraded one** — annotations in the margin with leader lines
pointing into the drawing is the standard drafting idiom, and it is what the
issue's own mock-ups show. The design therefore places comments in the margin
*deliberately* rather than arriving there by exhausting a search.

One genuine pocket of interior space exists and is deliberately **not** used:
`assign_positions` centres each column against the tallest
(`y_offset = (max_column_height - column_height) / 2`), so short columns have
real slack above and below. Exploiting it is a maximal-empty-rectangle problem —
out of scope for a rule with a known replacement date (D10).

### The placement routine

Obstacles are every graph node's box plus every comment already placed in this
pass. **Comments never displace graph nodes** — they always yield. Moving a node
would undo the layout just computed and break column alignment.

*Tier 1 — the natural spot.* Accept it if it is collision-free:

- *Anchored:* the four sides of the **placement box** (see [The anchor a comment
  is placed against](#the-anchor-a-comment-is-placed-against)) at gap `G`, tried
  above, below, right, left in that order. "Above" means the comment's bottom
  edge sits `G` above the box's top edge, horizontally centred on the box; the
  other three are the obvious rotations. For a wire anchor the box is a point,
  so the four candidates surround the wire's midpoint.
- *Unanchored:* the translated position itself — the common case, where nothing
  collides and the note does not move at all.

*Tier 2 — the gutter.* Otherwise project to the nearest edge of the graph's
bounding box and place just outside it, preserving the placement box's centre
`x` (top/bottom gutter) or centre `y` (left/right gutter), then slide **along**
the gutter axis until clear. An unanchored comment has no placement box; it
projects from its own translated position instead.

Prefer the top and bottom gutters. A column layout is wide and moderately tall,
so those bands run the full graph width — ample capacity — and their tethers are
short vertical lines rather than long diagonals crossing the graph. Left and
right gutters are short and are the last resort.

There is deliberately **no "least overlap" fallback**. An earlier draft of this
section used an expanding ring search around the anchor with a small cap, which
in a dense graph exhausts its rings and falls back to placing the comment *on
top of* a node — the exact outcome the pass exists to prevent. The gutter is a
fallback that always exists.

### Ordering

Anchored comments first (by node id), then unanchored (by node id). Anchored go
first because their position carries information — it must be near the anchor —
while an unanchored note's position is only a soft preference, so it should
yield. The node-id tiebreak keeps the result deterministic; `network.nodes`
iteration order is not (see [Current layout behavior](#current-layout-behavior)).

This is a single pass with a bounded search per comment: no iteration, no
convergence criterion, no damping. A force-directed relaxation would solve a
harder problem than exists here — the graph nodes are fixed obstacles, not
participants — at the cost of exactly the tuning surface D10 says to avoid.

### Sizing

Comment boxes use `CommentData.width` / `.height` throughout, **never**
`node_layout::estimate_node_height`. The estimate is 83 px against a real
default of 100 and a routine resized value of 300+, on a 210 px column pitch
(see [Current layout behavior](#current-layout-behavior)); placing correctly
while sizing wrongly just produces overlaps in a tidier arrangement.

### Notes for the implementer

`node_inlining::make_space_for_inline` is existing prior art for making room,
and is **not** reusable here: it has the opposite polarity (it bulk-shifts a
lower-right band of *other* nodes to accommodate one that grew) and does no
collision testing at all — it is a blanket translation split on the grown node's
diagonal.

Constants are `G` (gap, ~24 px), the gutter offset, and the slide increment.
All three are cosmetic. Keep it that way (D10): no further tuning surface, and
no state written to the file.

Known limitations, recorded rather than solved:

- A note anchored deep inside a wide graph gets a long tether to the gutter.
  Shortening it needs either interior placement or the freedom to move graph
  nodes apart; both are the rework's call.
- Several comments anchored near the same point produce a lopsided pile where a
  relaxation would spread them evenly. Rare, and the rework can fix it.

The user-facing entry point stays `StructureDesigner::layout_active_network()`,
which wraps the whole rearrangement in one `MoveNodesCommand` (#270) — calling
`layout_network()` directly from a user path silently bypasses undo.

## Deferred to the auto-layout rework

A separate design will rework auto-layout around preserving human intent for
**all** nodes, not just comments (D10). Two things belong to it rather than
here; both are recorded so they are not re-derived from scratch.

### Proximity inference for unanchored comments

The idea: infer which nodes a comment "is about" from where the human put it,
and use that instead of its raw coordinates. Sketch, from the design review —

*Selection, before layout:* measure the comment's distance to every node in its
scope; let `d₁` be the closest. Keep every node within `2 × d₁`, then truncate
to at most the 3 nearest. The relative threshold is the good part: an absolute
radius behaves differently in a dense region than a sparse one, and the ratio
naturally yields one candidate when a single node is markedly closest, two when
two are, three in a genuine cluster.

*Placement, after layout:* take the centroid of the survivors' **new** positions.
If they have scattered, drop the last (by original proximity rank) and recurse;
one survivor means "place beside that node".

**The durable insight is the candidate set, not the arithmetic.** Carrying 1–3
candidates and deferring the choice until after layout resolves an ambiguity
that is genuinely unresolvable beforehand: if the candidates stay adjacent,
between them is right; if they scatter, the cluster hypothesis was wrong and you
learn that only from the layout's output. That shape generalizes directly to
ordinary nodes, which is why it belongs in the rework.

Guards it needs, identified in review and equally deferred:

- **A floor on the ratio.** `d₁ → 0` (notes routinely touch or overlap a node)
  collapses the threshold: `d₁ = 0.5`, `d₂ = 3` drops a candidate that is
  visually identical. Use `max(2·d₁, d₁ + ~80)` — about half a node width.
- **Box-to-box distance, not centre-to-centre.** Centre distance badly
  penalizes large nodes; a tall HOF's centre is far while its edge is adjacent.
- **The centroid is a seek target, not a position.** It can land on an unrelated
  node or in a wire bundle; place at the nearest free spot to it.
- **A "nothing is near" cutoff.** A title block parked in empty space has a huge
  `d₁` and the ratio will still anchor it to something arbitrary. Above roughly
  `2 × COLUMN_WIDTH`, infer nothing.
- **Scope-local candidates** (D5) and a node-id tiebreak on equal distances, or
  the result is nondeterministic in the same way Sugiyama's comment stacking is
  today.

Two further notes for whoever implements it. If the surviving candidates are
exactly two nodes **with a wire between them**, the honest inference is a
*wire* anchor, not a node anchor — a note between `union` and `materialize` is
almost certainly about the wire, and `CommentAnchor` can express that. And
whether inferred anchors are **persisted** is the load-bearing decision: writing
them into `.cnnd` makes them visible, editable and undoable, but also
indistinguishable from human-authored anchors — which is exactly why this design
does not do it (D10). Resolve that question with the intent model in hand.

### Bulk anchor migration

An explicit *infer anchors from current positions* command, applied once to a
design that predates anchors, producing real editable anchors rather than
per-pass guesses. It is the same inference in a mechanism where the user is in
the loop, and it is the natural migration tool for existing files — but it
should be designed against the rework's intent model, not this document's
placeholder.

## Phases

### Phase 1 — Rust core

`CommentAnchor` / `WireAnchor` / `ResolvedAnchor`, `CommentData.anchors`,
`resolve`, and repair in `repair_node_network`. `.cnnd` round-trip with
`#[serde(default)]` back-compat.

Tests: resolve a wire anchor on a built-in destination (index path) and on an
`expr`/`switch` destination (param-id path); anchor survives a pin reorder on a
dynamic-arity node; anchor drops when the wire is deleted; anchor drops when
either endpoint node is deleted; unanchored comment serializes byte-identically
to today; a pre-anchor `.cnnd` file loads.

### Phase 2 — Text format

`PropertyValue::WireRef` + `->` lexing; serializer fourth pass; the two
`network_editor.rs` special cases; warnings for unresolvable names.

Tests: round-trip both anchor forms and the array form; multi-output source
(`atom_edit1.diff -> union.b`); anchors inside a zone body; a hand-written
`on:` naming a nonexistent node warns and drops; and the motivating case —
serialize a network with anchors, feed the text back through the editor
unchanged, and confirm the anchors survive (this is what an AI edit does, and
it is the scenario the whole text-format phase exists for).

**After this phase anchors are authorable without any UI**, through the text
editor. That is worth knowing while implementing Phases 3–5: it is the only way
to exercise anchors by hand before the Phase 4 drag gesture exists.

### Phase 3 — API + undo

`set_comment_anchors`, `APICommentAnchor`, `APICommentData` extension,
`SetCommentAnchorsCommand`, and the `CompositeCommand` bundling on the delete
paths. Copy/paste/duplicate `old_to_new` remapping with the not-in-set drop
rule. FRB codegen (`cargo fmt` **before** `flutter_rust_bridge_codegen
generate`).

Tests in `tests/structure_designer/undo_test.rs`: set/undo/redo an anchor;
delete an anchored wire, undo, anchor is back; paste a comment with and without
its target.

### Phase 4 — Flutter rendering + interaction

Leader-line painting in `node_network_content.dart` / the painter; the anchor
handle and its drag; the *Remove anchor* context-menu item.

Documentation in the same change: `doc/reference_guide/nodes/annotation.md`
(the anchor, how to create and remove it) and
`doc/reference_guide/node_networks.md` if the anchor gets a mention in the
wires section.

Manual verification (per `feedback_manual_test_for_editor_ui`): anchor to a
wire and to a node; drag comment and target, leader tracks both; anchor inside
an HOF body; body collapse; export a PNG and confirm the leader line appears;
delete the target and confirm the line vanishes; undo.

### Phase 5 — Layout

The comment-placement step in `layout/common.rs` (D8, D9, D10): the partition,
the graph-only layout with its bbox and origin delta, and the two-tier placement
routine — including the sizing fix (real `CommentData` dimensions, never
`estimate_node_height`), which is a prerequisite, not a polish item. Plus
`layout/AGENTS.md` (comments are no longer invisible to layout) and
`node_network/AGENTS.md` (the new leader-line canvas element).

Tests: **tier 1** — in a sparse graph an anchored comment lands beside its
anchor, at its real `CommentData` size (not the 83 px estimate), and an
unanchored comment whose translated position is clear does not move at all;
**tier 2** — in a dense graph (interior gaps below comment size) an anchored
comment lands in the nearest gutter aligned to its anchor's `x`, and **never**
overlaps a node; anchored comment follows when its target moves columns; two
comments anchored to the same point do not overlap each other; unanchored
comment keeps its offset relative to the graph's top-left under a pure
translation (D9.1); a comment with two anchors is placed against `anchors[0]`;
no comment influences the graph's own column assignment (D8); placement is
deterministic across runs (node-id tiebreak); the whole reflow is one undo step.

## Open questions

1. **Should the leader attach to the Bezier midpoint or the nearest point on
   the Bezier?** The midpoint is what the proposal sketches and reads well for
   short wires, but on a long wire crossing the canvas the midpoint can be far
   from anything meaningful. Nearest-point is a cheap refinement. Note it is
   **one** decision, not two: the same point is the leader's endpoint (Rendering)
   and the wire anchor's placement box (Layout), and they must not diverge.
2. **Should a wire anchor degrade to a node anchor when the wire is deleted?**
   D6 says drop. The alternative preserves some linkage across a rewire, at the
   cost of silently changing what the note claims to be about. Deliberately
   left as drop until there is evidence users want otherwise.
3. **Should selecting a node also select/drag its anchored notes?** It would
   make anchors useful for manual editing too, not just auto-layout — but it
   changes existing selection semantics and belongs in its own change.
