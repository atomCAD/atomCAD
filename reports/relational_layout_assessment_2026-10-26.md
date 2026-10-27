# Assessment: relational node placement in the text format

**Date:** 2026-10-26 · **Status:** assessment for discussion, no decision taken.

**Question (from mark and mechadense):** today the AI edits the network text
format, knows nothing about node positions, and placement is automatic. Could
the text format be extended so that the AI can *read* and *write* positional
**relations** ("A is right of B", "these three are a column"), never raw
coordinates? mark pointed at [reladraw](https://github.com/reladraw/reladraw);
mechadense sketched rows / columns / grids with constant spacing, joined
through hub points in a star topology, with the guides normally hidden
(Inkscape / draw.io style).

**Short answer:** yes, there is a variant that is cheap and worth building. It
is **not** "store the layout as relations" (reladraw-style, or mechadense's
group tree). It is:

1. **`place` as a one-shot instruction** in the text format. The AI states
   relations, the editor turns them into ordinary coordinates once, using real
   node sizes, and nothing relational is stored.
2. **An opt-in relational read-out** (`query --layout`) that describes the
   current drawing in the same vocabulary, so the AI can see the layout.
3. **Layout findings** in reladraw's style: overlaps and backward wires
   reported as text, so the AI learns when its placement went wrong.

Persistent relations are the right long-term direction, but they are a large
change, and the large part is the GUI rather than the text format. Section 5
explains why.

---

## 1. Where we actually are

The colleagues' description is half right, and the half that's wrong matters
for the design.

- **The AI path does not re-layout the drawing.** Since the incremental layout
  work (`doc/design_incremental_layout.md`, Phases 1–5 done), an AI edit keeps
  every existing node where it is. Only *new* nodes are placed, by rule:
  Sugiyama on the added block, a target taken from its wired anchors
  ("Step 4"), then a slide, push or half-plane shift to make room. Existing
  nodes move only when forced, and then by a rigid translation. The full
  Sugiyama reflow runs only when the user asks for it.
- **The automatic part really is complex and somewhat fragile.** About 4.9k
  lines live in `layout/`. Known limits are documented (the window rule cuts
  loose columns in dense drawings), and the corpus run found three cascade
  bugs that no hand-written test had caught.
- **The AI has no say and no sight.** `query` emits no geometry, so the AI
  cannot tell that its new `ivec3` landed under an unrelated cluster, or that
  the three parameters it added belong in one column next to the node they
  feed. Step 4 has to guess placement from the wiring. Wiring is a good signal
  for left/right and a weak one for vertical order and grouping.

So the real gap is **new-node placement quality and AI visibility**, not
destruction of human layouts. That narrows what a relational channel has to
do.

Our earlier research (`doc/research_intent_preserving_layout.md` §8) turned
down "qualitative relative hints" (level 3) for three reasons. Two of them
reladraw refutes, and the next section explains how:

| §8 objection | Still holds? |
|---|---|
| Relations need a constraint solver (F5) to consume | **No.** reladraw shows that a restricted language evaluates in a single pass. |
| The AI has no feedback loop on geometry | **No, it can have one.** reladraw's *findings* report the problem in text. |
| Round-trip tax, overwriting human intent, attention budget | **Yes**, for *persisted* relations. A one-shot instruction avoids all three. |

---

## 2. What reladraw actually is

A text diagram language, about a year old (v0.19, TypeScript, built for agent
authoring; it ships a Claude Code skill). The ideas that matter here:

- **Relations only, no coordinates.** `right of a`, `below a (gap: tight)`,
  `even with a`, `top even with a`, `x even with a`, several targets at once
  (`below a and b`), parts against parts (`top 3 below a bottom`).
- **Each axis is stated exactly once, and dependencies are acyclic.** A box's
  x comes from one statement and its y from one statement. "Said twice",
  "circular" and "axis left open" are errors. That is why no solver is needed:
  positions are computed in dependency order, exactly once each. **This is the
  key design trick**, and it is what our §8 analysis missed.
- **Exactly one anchor.** One box places itself, and everything else hangs off
  it directly or through others.
- **Nothing moves a box except its own statement.** There is no overlap
  avoidance and no routing around obstacles. When the picture may not read as
  meant (two boxes overlapping, a line crossing a box), it is drawn anyway and
  a one-line **finding** is printed with the location and size, for example
  `file:12: warning(node-overlap): … overlap is 3 units wide`. The author (or
  the agent) fixes the source.
- **Containers with `layout: row | column`** place children in written order
  with a common gap or centre spacing. `grid`, `star` and `circle` are listed
  as undecided.
- **Guides**: invisible nodes, points and regions that other boxes are placed
  against. These are exactly mechadense's dashed lines and hub crosses.

**What reladraw is not:** an editor. It is text → SVG, one way. There is no
dragging, so it never faces the question of what a mouse drag does to a
relation. Its own README places it between Mermaid (auto-layout) and draw.io
(absolute placement), for people who author in text. atomCAD is the reverse
case: a direct-manipulation editor that an AI also edits.

**mechadense's sketch** belongs to the same family. Groups (row, column, grid)
have constant internal spacing, and each group's position is an offset from a
hub point, which gives a tree of relative offsets. In reladraw terms these are
containers with `layout:` plus guide points. mechadense's own caveat ("a
projection-specific aspect not carrying over to other projections") is exactly
the round-trip problem in §5.

---

## 3. The recommended variant

### 3a. `place`: a one-shot placement instruction

A new statement kind, alongside `output` and `delete`. It is an
**instruction**, not a property: it acts once, during the edit, and `query`
never prints it back.

```
# a new parameter column left of the node it feeds
px = int { value: 1 }
py = int { value: 2 }
pz = int { value: 3 }
v  = ivec3 { x: px, y: py, z: pz }
place [px, py, pz] column left_of v (gap: normal)

# a new node beside an existing one, top-aligned with another
cut = diff { base: cube1, sub: [hole] }
place cut right_of cube1, top_even_with sphere3

# move an existing node because the user asked for it ("tidy that up")
place legend below main_chain (gap: wide)
```

**Vocabulary.** Keep it small and copy reladraw's semantics exactly where they
overlap:

- `right_of`, `left_of`, `above`, `below`, with an optional `(gap:
  tight|normal|wide)`
- `even_with` (y centres), `x_even_with`, `top_even_with`, `left_even_with`
- `row` / `column` for a list, in written order, with one gap

That covers the rows and columns in mechadense's picture. Grids and hubs can
wait until they are asked for.

**Rules, all borrowed from reladraw:**
- one statement per axis per node, with "said twice" and "axis left open"
  reported as errors;
- targets resolve in dependency order, cycles are errors;
- a lone direction also centres on its target (`right_of a` means level with
  `a`);
- targets must be in the same scope (root or one body); crossing scopes is an
  error.

**Semantics.** The editor resolves the statements **once**, after nodes exist
and their real sizes are known (`layout/size.rs::rendered_node_size`). The
result is written into `Node.position`, and the relation is then forgotten.

- **For a new node**, `place` replaces Step 4's guessed target for that node.
  A node without a `place` statement goes through today's pipeline unchanged,
  so an AI that never writes `place` gets exactly today's behaviour.
- **For an existing node**, `place` is an explicit move. It should be used
  only when the user asks for it, and the skill's guidance should say so. The
  whole AI edit is already one undo snapshot, so it takes back with one
  Ctrl+Z.
- **Overlaps are reported, not repaired.** That is the reladraw rule, and the
  reason is that the AI's statement then means exactly what it says. The edit
  result carries findings (see 3c), and the AI can follow up. Running Step 5
  (slide or push) afterwards would be the softer alternative. Our own research
  cites the "extremes are better" result, which argues against half-honouring
  a stated position. Pick one rule and document it.

**Why this is cheap:**
- nothing is persisted: no `.cnnd` change, no migration, no new undo command;
- no GUI change, so dragging stays exactly what it is today;
- no round-trip tax, because `query` never emits it, just as it never emits
  `delete`;
- the resolver is a topological evaluation over a handful of statements:
  parser, a small resolver, and a hook in `layout_incremental` that excludes
  placed nodes from Step 4.

### 3b. `query --layout`: a relational read-out

Opt-in, so the normal `query` output and its token cost stay unchanged. For
each node, it prints the nearest strong relation it can **infer** from the
coordinates, in the same vocabulary as `place`:

```
# layout (inferred, read-only)
v      right_of px (gap: 48), even_with py
px     column [px, py, pz] (gap: 30)
cut    right_of cube1 (gap: 52), top_even_with sphere3 (±3)
note   overlaps sphere3 by 40×12
```

This is constraint *inference* (research F9). It is safe here only because
it is **never stored**: the research doc and `design_wire_annotations.md` D10
both warn that persisted guesses become indistinguishable from intent. As a
read-only view it lets the AI answer "where is X relative to Y?", which is
what the colleagues asked for, and it gives the AI the target names it needs
to write a sensible `place`.

### 3c. Layout findings

These are independently useful, so they come first. After every AI edit
(inside the edit result JSON), and on demand (`atomcad-cli check-layout`),
report in reladraw's one-line form:

- node-node overlaps that this edit **created** (pre-existing ones are
  baseline; 152 of them exist in the private corpus and nobody minds);
- backward wires this edit created;
- `place` statements whose result overlaps something.

This closes the "no feedback loop" gap whether or not `place` is ever built.

---

## 4. What this buys, honestly

**It buys:**
- The AI can state the semantic grouping it already knows ("these feed that,
  as a column"), which Step 4 can only approximate from wires. This is where
  §8 of the research says the AI is strong: semantics, not geometry.
- The AI can see the layout and answer questions about it.
- The AI can carry out "tidy up this area" requests precisely, instead of
  reaching for a full reflow.

**It does not buy:**
- **Removing the incremental layout machinery.** GUI actions that grow a node
  (adding a pin, expanding a body, inlining) still need `grow_rect` and the
  cascade, and an AI that writes no `place` still needs Step 4. `place`
  reduces how often the heuristics decide; it does not replace them. If the
  colleagues' real goal is "get rid of the fragile layout code", this variant
  does not achieve it, and no variant does without also changing what a GUI
  edit means (§5).
- **Durable structure.** Once a human drags a placed node, the relation is
  gone, because there never was one stored. That is a deliberate trade.

---

## 5. Why not persist the relations (reladraw-full, or mechadense's group tree)

This is the research doc's F11 ("layout is data"): the right destination, but
the wrong next step. The obstacles, in order of weight:

1. **Dragging.** atomCAD is used mostly by hand. If positions are derived from
   relations, every drag needs a rule. Does it break the node's relations and
   make it absolute? Then relations erode back to today's state within a few
   sessions. Does it edit the gap? That is ambiguous for diagonal drags and
   for nodes with two relations. Does it move the whole group? That is what
   mechadense's star tree implies, and it needs visible group UI. Dunnart (the
   research system closest to this) spent years on exactly these questions.
   This is where the cost is, and it is all GUI work, not text-format work.
2. **The corpus has zero relations.** That is 3,200 hand-placed nodes across
   the two measured corpora. Every node would start as "self-anchored", so
   reladraw's single-anchor rule cannot apply, and the system lives
   permanently in a mixed absolute-plus-relational state.
3. **Round-trip tax.** Anything stored must be printed by `query`, or the AI
   deletes it on its next `--replace`. That means a placement clause on
   possibly every node, paid on every query, in direct competition with the
   network design for the AI's attention. This is mechadense's "projection
   specific" worry, made concrete.
4. **No avoidance means GUI edits create overlaps.** reladraw's "report,
   don't move" rule works because a human or agent is reading the findings
   after every render. A GUI user who adds a pin, so that a node grows into a
   neighbour that isn't related to it, has nobody reading the finding. So
   persistent relations would still need the push machinery as well.

**mechadense's idea has a better first home: the GUI.** Draw.io-style smart
guides while dragging (snap to a neighbour's centre or edge, show equal
spacing), plus "align / distribute" commands on a selection. These are pure
GUI features, independent of the text format. They make hand layouts more
regular, which in turn makes the §3b read-out cleaner. If people then want
alignments to *stay* aligned, persisted groups are a natural next step
toward F11, taken incrementally and driven by evidence of use.

---

## 6. Suggested order, with a cheap experiment first

0. **Experiment, before writing code.** Pick 5–10 real co-edit tasks (the
   2026-09-03 session plan has some). For each, have the AI write the edit
   plus the `place` lines it *would* write. Resolve them by hand, or by a
   throwaway script, and compare against what Step 4 produced. If the AI's
   relations are not clearly better than Step 4's guesses, stop here. That
   would be a useful result in itself.
1. **Layout findings** (§3c). Small, useful on its own, needed by 2 and 3.
2. **`query --layout` read-out** (§3b). Read-only, no risk.
3. **`place` instruction** (§3a), plus skill guidance: new nodes freely,
   existing nodes only on request.
4. *(GUI track, independent)* smart guides and align/distribute.
5. *(Later, only on evidence)* persisted groups or relations.

Each step is useful on its own, and none of them commits to F11 or rules it
out.

---

## 7. Open questions for mark and mechadense

1. Is the pain mainly **where new nodes land**, or that the AI **can't see or
   talk about** the layout? 3a addresses the first and 3b/3c the second. The
   order above assumes both matter.
2. For a `place` that overlaps something: honour it exactly and report (the
   reladraw rule), or honour it and then slide/push (the atomCAD habit)?
3. Should the AI ever move nodes it did not create without being asked?
   (Recommendation: no.)
4. For mechadense: are the grid and hub groups meant to **persist** and move
   together when dragged? If so, that is the §5 track and mainly a GUI
   design. Smart guides plus align/distribute might deliver most of the
   value first.
