# Design: HOF bodies in the text format

**Status:** first draft for review. Prerequisite for `doc/design_incremental_layout.md`
and `doc/design_ai_edit_history.md`, both of which assume the AI can see and edit
what a network actually contains.

**Ordering against `design_incremental_layout.md`.** That design's §"HOF bodies"
says it "needs a proper revision before Phase 1" and points here, while an
earlier draft of D8 below pointed back at *its* Phase 1 for the `--replace`
name match. That was a cycle, and it is broken in one direction: **this design
carries its own name-match and its own position snapshot** (D8, built in
Phase 2), depending on nothing from the layout work. That design consumes them
instead of defining them.

**Problem.** The AI text format does not project zone bodies **at all**. The
serializer says so outright (`text_format/network_serializer.rs:326-328`): a body
is "a scope the text format does not project at all". Only two mentions of zones
exist in the entire `text_format/` module, neither in `serialize_node`.

Three consequences, in ascending order of severity:

1. **The AI reads an incomplete network.** An inline-body `map` serializes as
   `map1 = map { xs: r }` — the node appears to compute nothing. The AI reasons
   about a network that is not the one on screen.
2. **The AI cannot write one.** There is no syntax for a body, and a
   newly created HOF node gets an *empty* body from `ensure_zone_init`
   (`node_network.rs:886`), which is a blocking-error state unless `f:` is wired.
3. **`edit --replace` silently destroys existing bodies.** `clear_network`
   (`text_format/network_editor.rs:220`) drops every node; the rebuilt HOF gets a
   fresh empty body. The AI is handed `success: true`, because `EditResult`
   carries the editor's errors and not `validate_network`'s. And
   `ai_edit_network` records **no undo command at all** — there is no
   `push_undo` or `UndoCommand` anywhere in `ai_assistant_api.rs` — so the
   content is unrecoverable. The `atomcad` skill actively advertises the
   round-trip that triggers this: query output "is valid input to
   `edit --replace`" (`.claude/skills/atomcad/skill.md:173`). D1-D14 fix the
   destruction; **D15** fixes the silence, and Phase 5 the unrecoverability.

There is a second, narrower instance of the same class of bug: **`closure` nodes
serialize with no properties whatsoever.** `closure.rs` does not implement
`get_text_properties`, so it inherits the default empty impl
(`node_data.rs:235`). A closure round-trips as `c1 = closure { }`, losing its
`kind`, `param_names` and `type_args`.

### How much this costs, measured

Counted over the two hand-drawn `.cnnd` files in the repo root. They are two
saves of the same project and agree on every figure below to the node, so these
are **per-project** numbers rather than a sum: 89 networks, 2,303 nodes counting
body nodes.

| | |
|---|---|
| HOF nodes carrying a body | **48** |
| Nodes living inside those bodies | **141** |
| By owner type | `closure` 25, `map` 17, `zip_with` 4, `fold` 2 |
| Mean body size | 2.9 nodes |
| Max body nesting depth | 2 |
| Wires reading an **outer** HOF's iteration value (`ZoneInput`, depth 2) | **2** |
| Largest single network's body content | `0_staired-cover`, 19 body nodes |

So roughly 6% of the maintainer's nodes are behind the body wall, and the largest
single category is the one that also loses its stored data (D12).

---

## What a zone body is

The syntax follows from the data model, so it is worth stating precisely. A body
is a full `NodeNetwork` hanging off `Node.zone: Option<Arc<NodeNetwork>>`, and it
communicates with the outside in exactly four ways:

| Direction | Representation today |
|---|---|
| A body node reads the per-iteration value (`element`, `acc`, `element1`/`element2`, or a closure's `param_names`) | `SourcePin::ZoneInput { pin_index }` with `source_scope_depth = 1` |
| A body node in a **nested** body reads an **outer** HOF's per-iteration value | `SourcePin::ZoneInput { pin_index }` with `source_scope_depth ≥ 2` |
| A body node reads a node in an **enclosing** network (a capture) | `SourcePin::NodeOutput` with `source_scope_depth ≥ 1` |
| The body delivers its result (`result` / `new_acc`) | a wire on the **parent HOF node's** `zone_output_arguments[i]`, sourced from a body-scope node id |

(`node_network.rs:237-259`, `evaluator/zone_closure.rs:600-608`.)

**The depth arithmetic, because the two source kinds do not share a base.** For
a `NodeOutput` wire, `source_scope_depth = d` addresses the network `d` frames
up, so `d = 0` is the body itself. For a `ZoneInput` wire, the *owning HOF's
body frame* sits at `stack_len - d`, so `d = 1` is the immediately enclosing
HOF's iteration value and `d = 2` is the next one out
(`network_evaluator.rs:2197-2235`). A `ZoneInput` wire also carries the owning
**HOF node's** id in `source_node_id` — not a body node's — which the Pass 0
scope tree supplies directly from the scope path.

The off-by-one is not an accident, and it has a semantic edge: `is_capture`
(`zone_closure.rs:600-608`) reads `ZoneInput` at depth 1 as a per-iteration
value and at depth ≥ 2 as a **capture**, pre-evaluated once per closure
instantiation through the capture cache. An outer HOF's element is a capture in
exactly the sense D4 is about, which is why D4's `^` is the right operator to
reach it.

Four outward references means four new spellings, and nothing else.

Zone pin names, for reference. They are **not** uniform — only `map` and
`zip_with` call the output `result` — so the serializer must read them from the
resolved `NodeType::zone_input_pins` / `zone_output_pins` rather than assume a
name:

| Node | Zone inputs | Zone output |
|---|---|---|
| `map` | `element` | `result` |
| `filter` | `element` | **`keep`** (`Bool`) |
| `foreach` | `element` | **`out`** (`Unit`) |
| `fold` | `acc`, `element` | `new_acc` |
| `zip_with` | `element1` … `elementN` | `result` |
| `closure` | `ClosureData::param_names` | `ClosureKind::result_name()` |

(`map.rs:57-66`, `filter.rs:56-64`, `foreach.rs:66-74`, `fold.rs:59-67`,
`zip_with.rs:331-343`, `closure.rs:104-120`.) `zip_with` generates one input pin
per lane, so its arity is data-driven; every other type's is fixed.
`ClosureKind::result_name()` already returns `new_acc` / `out` / `result` per
kind — reuse it rather than restating the mapping anywhere.

---

## Design decisions

**D1 — Bodies become first-class in the text format: readable and writable.**
`query` emits them, `edit` accepts them. This is the whole point; every decision
below is about how.

**D2 — `body { … }` is a *block*, not a property value.** `{` in property-value
position is already taken: `parse_property_value` dispatches `LeftBrace` to
`parse_object_literal` (`parser.rs:1161`) for anonymous record literals. The two
are not *strictly* ambiguous — a record literal's contents are `key: value`
pairs and a body's are statements, so a parser could tell them apart by reading
inside. But telling them apart that way costs unbounded lookahead or a
backtracking retry, and yields an error message only after the whole block has
been consumed. Dropping the colon decides it on **one** token: inside a property
block, peek past the identifier — `:` is a property, `{` is the body. LL(2), no
backtracking. No node type has a parameter named `body`, so the name is free.

**D3 — Zone inputs are `$name`.** `$element`, `$acc`, `$element1`, or a
closure's own `$x`. A sigil rather than a bare name because a body node may
legitimately be *called* `element`, and because a zone input is not a node — it
is a per-iteration value, and it should not look like one. `$` is unused by the
lexer; of the punctuation the lexer does claim, only `#`, `@` and `.` act as
sigils — the rest (`=` `:` `,` `{}` `[]` `()` `"` `` ` `` `->`) is structural.

`$name` on its own always means **this** body's own zone input. Reaching an
outer HOF's iteration value composes with `^` rather than adding a sigil — D4.

**D4 — `^` is a scope operator, one `^` per level, and it composes with `$`.**
`^` does not mean "capture a node"; it means *go up one scope*, and what
follows names something in the scope it lands on. So `^scale` reads a node in
the immediately enclosing network, `^^scale` two levels out — and `^$element`
reads the enclosing HOF's iteration value from inside a nested body, because
`$element` is a valid thing to name one scope out.

Without the composition there is simply **no spelling for a `ZoneInput` wire at
depth ≥ 2**, and those wires exist — two of them in the maintainer's own project.
A nested `map` whose inner body touches the outer element is the ordinary way to
write a nested loop; it is not an exotic case.

The full set of reference forms, and nothing else:

| Written | Resolves to | Encoding |
|---|---|---|
| `n` | a node in this body | `NodeOutput`, depth 0 |
| `^n` | a node one scope out | `NodeOutput`, depth 1 |
| `^^n` | a node two scopes out | `NodeOutput`, depth 2 |
| `$element` | this body's own HOF's iteration value | `ZoneInput`, depth 1 |
| `^$element` | the enclosing HOF's iteration value | `ZoneInput`, depth 2 |
| `^^$element` | two HOFs out | `ZoneInput`, depth 3 |

One rule generates the table: **`k` carets → `NodeOutput` at depth `k`,
`$`-prefixed → `ZoneInput` at depth `k + 1`.** The `+ 1` is the base asymmetry
established above, and it is the only arithmetic in the format.

`^$` also reads honestly: per the same section it *is* a capture, so marking it
with the capture operator is not a syntactic convenience but a statement of
what the wire costs.

The **parser also accepts an unqualified name with lexical fallback** (this body,
then the parent, then outward; inner shadows outer), because that is what anyone
writing it expects. The fallback applies to **bare names only** — `$name` never
searches outward, because a silent promotion from a per-iteration read to a
capture would change evaluation semantics, not just the referent. The
**serializer always emits the explicit `^` form**, so a round-trip is never
ambiguous and never depends on shadowing.

**D5 — The body's result is an `output` statement inside the block.** It reuses
the existing statement verbatim. One thing must be said precisely because it is
counter-intuitive: `output` inside a body writes the **parent HOF node's**
`zone_output_arguments`, *not* the body network's `return_node_id`. Every zone
type today has exactly one zone-output pin (verified across all six in the
table above), so the bare form suffices; an `output <pin>: <node>` form is
reserved for a type that ever grows a second.

The converse has to be stated too, because the round-trip depends on it: a
`body { … }` block containing **no** `output` statement means the parent's
`zone_output_arguments` are **cleared**. That is what makes whole-body assign
(D6) total, and it is the only reading under which `query` → `edit --replace`
is exact. A *path*-addressed statement never touches the zone-output wire unless
it is itself an `output m1/x`, or a `delete m1/x` that removes the wire's
source. The merge table spells out every case.

**D6 — Two edit granularities, both in v1: the whole-body block, and
path-addressed statements.** Mentioning `body { … }` assigns the **whole** body;
a path-addressed statement merges into it.

Whole-body assign is the natural default and is *exactly* the format's existing
property rule — since `1b93cab8`, mentioning a property assigns its whole value
and omitting it leaves it alone. A body is just another property. With a mean
body size of 2.9 nodes, rewriting one is usually cheaper than addressing into it.

Path addressing exists because bodies are expected to grow. A 30-node body should
not have to be restated to change one node in it, and — with D8 — touching one
node must not disturb the other 29.

**D7 — The path rule: the prefix selects the scope, the last segment names the
node in it.** One rule, applied uniformly to assignment, `output` and `delete`:

```
m1/inner1 = mul { a: $element, b: ^scale }
output m1/inner1
delete m1/old
```

**The separator is `/`, not `.`** — `.` already means pin access in value
position (`node.pinname` for multi-output refs), so `output m1.d` would be
genuinely ambiguous between "scope `m1`, node `d`" and "the `d` output pin of
`m1`". `/` is completely unused by the lexer, reads as what it is, and keeps the
two concepts visually distinct. Paths split on `/` outside backticks only, so a
backtick-quoted identifier containing a slash (`` m1/`a/b` ``) is still
addressable — backticks, not double quotes, are what the lexer uses for relaxed
identifiers (`parser.rs:345-372`).

Within a path-addressed statement, references are **relative to the addressed
scope**: bare names resolve in that body, `$…` are its zone inputs, `^…` walks
outward from it. Nothing about the reference syntax changes.

**D8 — Identity is name-keyed, per scope, and carries layout state — and this
design builds that machinery itself.** When a `body { … }` block is applied,
rebuilt body nodes are matched to existing body nodes **by name**, and `position`
is carried across. A name that fails to match is a new node.

Without this, whole-body assign would throw away the layout of everything inside
the body — the exact failure `design_incremental_layout.md` exists to prevent,
one scope down.

**Why it cannot be borrowed.** Two facts. First, `position` is not in the text
format at all: the serializer emits none, and the editor synthesizes one via
`auto_layout::calculate_new_node_position` for every node it creates
(`network_editor.rs:328`). A position can only survive by being *carried*, never
by being written. Second, in `--replace` mode `clear_network`
(`network_editor.rs:220`) deletes every node before the first statement is
processed, so by the time a `body` block is applied there is nothing left to
match against.

So Phase 2 adds, **ahead of the clear**, a pre-edit snapshot keyed
`(scope_path, name) → (node_id, position)`, taken during Pass 0 over the whole
network including bodies and consulted whenever a node is created. It is small
and self-contained; `build_existing_name_map` (`network_editor.rs:245`) is its
single-scope, incremental-mode ancestor.

The match is **total**, so there is no unnamed-node fallback to design:
`get_node_name` records that "all nodes now have persistent names assigned at
creation" (`network_serializer.rs:194-203`), i.e. `custom_name` is always
`Some`.

`hand_moved`, drift repair and the push/shift passes stay entirely in
`design_incremental_layout.md`. D8 deliberately carries `position` and nothing
else, so neither design blocks the other.

**D9 — The serializer emits only the block form.** Path statements are
input-only sugar. This keeps `query` output single-valued, keeps the
round-trip exact, and keeps the *By node* diff in
`design_ai_edit_history.md` keyed on one statement shape per node.

**D10 — `EditResult` reports full paths.** `nodes_created`, `nodes_updated`,
`nodes_deleted` currently carry bare names. With bodies, `m1/a` and `m2/a` are
indistinguishable — to the AI, and to the history panel's name-keyed diff. Cheap
now, expensive to retrofit.

**D11 — `f:` overrides `body`; both serialize.** `map.rs:216` — the `f` function
pin, when wired, overrides the inline body. The serializer emits both when both
exist (faithfulness over tidiness), and the precedence is documented in the skill
so the AI does not write a body that silently never runs.

**D12 — `closure` gains text properties.** `kind`, `params` and the resolved
`type_args` become `get_text_properties` / `set_text_properties` entries. Without
this, the largest body-bearing category still loses its definition on every
round-trip, and `$x` has no parameter names to bind to.

**D13 — The editor creates zones eagerly, and binds `$name` from node data.**
`ensure_zone_init` runs during
*validation* (`node_type_registry.rs:1116`), i.e. **after** `text_edit_network`
returns. A script that writes `m1 = map { }` and then `m1/a = …` would find
`m1.zone == None` during the edit. The editor must initialise the zone at
node-creation time. Small, and it fails silently if missed.

**And zone-input *names* must be resolved from node data, never from a resolved
type.** `ensure_zone_init` hands the body a network; it does not tell the editor
what `$x` binds to. Those names live on the node's resolved `NodeType`, and
custom node types are (re)built by `initialize_custom_node_types_for_network` at
the **end** of `apply` and again at validation — so during Pass 1/2 they are
stale or absent for a node the same script just created. For `map` / `fold` /
`filter` / `foreach` / `zip_with` this never shows, because their zone-input
names are static and derivable from the node type name alone (the table in
*What a zone body is*). For `closure` it does: the names *are*
`ClosureData::param_names`, which the same statement's `params:` property is
setting. Hence two rules:

- resolve `$name` against `ClosureData` for closures and against the static
  per-type table for everything else — never against
  `node.custom_node_type.zone_input_pins` mid-edit;
- apply a statement's **properties before its body block**, unconditionally, so
  `params: ["x", "y"]` is in place before `$x` is bound. The parser preserves
  source order, but the editor must not depend on the author writing `params:`
  first.

This is the largest correctness trap in Phase 2, and it lands on the biggest
body-bearing category — 25 of the 48 bodies are closures.

**D14 — Bodies are serialized multi-line.** The serializer emits one line per
node today; a node carrying a body cannot. A zone-bearing node's statement spans
lines, with the body block indented. Consumers that assumed "one statement = one
line" must brace-match — notably the *By node* diff splitter in
`design_ai_edit_history.md`.

**D15 — `EditResult.success` means *validates*, not merely *parsed*.** Today
`ai_edit_network` builds the `EditResult`, reinserts the network, and only
*then* calls `validate_network` — discarding its verdict entirely. That is the
"silently" in consequence 3, and fixing the destruction without it would be half
a fix: `success` is the AI's only signal that a body edit went wrong, and this
design gives a bad edit more to damage.

The change is contained: run `validate_network` inside the same call, fold its
blocking errors into `EditResult::errors` and its non-blocking ones into
`warnings` (the severity split already exists — `ValidationError::warning()`,
`project_nonblocking_validation_errors`), and let `success` mean *parsed,
applied, and validates*. Errors originating inside a body carry their full path
(D10) so the AI can find the node. This is separable from Phases 1-3 and is
listed in Phase 2 because that is where body edits first become possible.

## Syntax

### Reading (what `query` emits)

```
r = range { start: 0, count: 5, step: 1 }
scale = int { value: 3 }

m1 = map {
  xs: r,
  body {
    d = mul { a: $element, b: ^scale }
    e = add { a: d, b: 1 }
    output e
  }
}

output m1
```

- `$element` — the `map`'s zone input.
- `^scale` — a capture of `scale` from the enclosing network.
- `output e` — inside the block, feeds the `map`'s `result` zone-output pin.
- `output m1` — outside the block, the network's own return node. Same keyword,
  different scope; the position disambiguates.

`fold` and `zip_with` differ only in their zone-input names:

```
total = fold {
  xs: r, init: zero,
  body {
    s = add { a: $acc, b: $element }
    output s
  }
}
```

A `closure` carries its parameters explicitly (D12), and its zone inputs are
named by them:

```
f1 = closure {
  kind: custom,
  params: ["x", "y"],
  body {
    p = mul { a: $x, b: $y }
    output p
  }
}
```

A body nested inside another body reaches the outer iteration value by
composing the two sigils (D4) — `^` walks out one scope, `$` names the zone
input it finds there:

```
outer = map {
  xs: rows,
  body {
    inner = map {
      xs: $element,
      body {
        p = mul { a: $element, b: ^$element }
        output p
      }
    }
    output inner
  }
}
```

`$element` in the inner body is the inner `map`'s element; `^$element` is the
outer `map`'s. Both are `ZoneInput` wires — they differ only in depth (1 and 2),
and only the second is a capture.

### Writing incrementally (path-addressed)

```
m1/d = mul { a: $element, b: 4 }      # replace one body node's properties
m1/new1 = sub { a: d, b: $element }   # add a node to the body
output m1/new1                         # re-point the body's result
delete m1/e                            # remove a body node
```

Nested bodies compose by the same rule, and two levels out is `^^` for a node,
`^$` for an iteration value:

```
outer/inner/n1 = add { a: $element, b: ^^base }
outer/inner/n2 = add { a: $element, b: ^$element }
```

### Grammar changes

Small and localized:

- **Lexer:** two new sigils, `$` and `^`, and `/` as a path separator. All three
  are currently unused. `^` lexes as a single-character token each time, never
  as a `^^` digraph — the parser counts them, so depth is not capped by the
  lexer.
- **`parse_assignment` / `parse_output_statement` / `parse_delete_statement`:**
  `expect_identifier()` → `expect_path()`. The left of `=` uses no `.` today, so
  no existing input changes meaning.
- **`parse_property_block`:** on an identifier, peek — `:` is a property, `{` is
  the body block. LL(2), no backtracking.
- **`parse_property_value`:** one new leaf form, `^* [$] ident` — consume the
  carets, then dispatch on whether a `$` follows. `$ident` → zone input,
  `^…ident` → capture of a node, `^…$ident` → capture of an outer HOF's zone
  input. A single production rather than two, alongside the existing identifier
  and `@ident`.

---

## Semantics

### Scoping

Names are unique **per scope**, not globally: `m1/a` and `m2/a` are different
nodes and both may exist. Resolution of a bare name inside a body is
body-first, then outward (D4).

`visible: true` works per scope, because `displayed_nodes` is a per-`NodeNetwork`
map. Comments inside a body are ordinary body nodes and their anchors resolve
within the body, which is already how `drop_dangling_anchors` recurses.

### Merge rules

Both columns matter: a body block is total over the body *and* over the parent's
zone-output wire (D5), while a path statement is surgical over both.

| Statement | Effect on the body | Effect on `zone_output_arguments` |
|---|---|---|
| `m1 = map { … }` with no `body` block | Untouched. Properties and wires assigned as today. | Untouched. |
| `m1 = map { … body { … output e } }` | **Replaced** wholesale; surviving names keep their ids and positions (D8). | Set to `e`. |
| `m1 = map { … body { … } }` with no `output` | **Replaced** wholesale. | **Cleared** — the block is total (D5). |
| `m1 = map { … body { } }` | Emptied. | Cleared. |
| `m1/x = …` | Merges into `m1`'s body: creates or updates `x`, leaves every other body node alone. | Untouched. |
| `output m1/x` | Untouched. | Set to `x`. |
| `delete m1/x` | Removes `x` from the body. | Cleared **iff** `x` was its source — never left dangling. |
| `delete m1` | The HOF node and its body go together. | — |

**Ordering within one script is significant, and the two forms interact.** A
script containing both `m1 = map { body { … } }` and `m1/x = …` applies them in
order: the block wipes, then the path statement merges into the result. This
follows from statements applying in order and needs no special rule, but it must
be stated in the skill so the AI never straddles the two and loses nodes.

### Wire assignment inside bodies

Unchanged: mentioning a property assigns that pin's *whole* inbound wire set
(`1b93cab8`), inside a body exactly as outside. `a: $element` on a pin that was
wired to a body node replaces that wire.

---

## Implementation

The grammar is the easy part. The work is that **`NetworkEditor` and the
serializer become scope-aware.**

`NetworkEditor` today is single-scope (`network_editor.rs:128-145`): a
`&'a mut NodeNetwork`, a flat `name_to_id` / `id_to_name`, flat
`pending_connections`, `visible_nodes`, `pending_anchors`. Every one of those
becomes keyed by scope path. `NodeRef { scope_path: Vec<u64>, node_id: u64 }`
already exists (`node_network.rs:206`) and is the natural key.

**The borrow discipline dictates the pass structure.** `Node.zone` is an
`Arc<NodeNetwork>` mutated through `zone_mut()` → `Arc::make_mut`, so a body
borrow cannot be held while the parent's name map is read — which is exactly what
resolving a `^capture` needs. Therefore:

1. **Pass 0 — build the scope tree, per-scope name maps, and the identity
   snapshot** for the whole pre-edit network, before any mutation — and
   crucially **before `clear_network`** in `--replace` mode, which is the only
   moment the old positions still exist (D8).
2. **Pass 1 — create and update nodes**, scope by scope, taking `zone_mut()`
   transiently and never across a scope boundary. Zones are initialised here
   (D13), not left to validation; a node matched in the snapshot inherits its
   position instead of calling `calculate_new_node_position`. Within one
   statement, properties are applied before its body block, so a closure's
   `params:` is in place before `$x` binds (D13).
3. **Pass 2 — resolve wires**, re-walking each path from the root. A path walk is
   O(depth) map lookups; depth is 1 or 2 in practice.
4. **Pass 3 — `output`, `delete`, visibility**, as today but scope-qualified.

This is the same shape as the "remove from registry → edit → reinsert" dance
`ai_edit_network` already performs for the same borrow reason.

Once that refactor exists, **path addressing is a small increment on top of it**
— roughly "derive the scope from a path" instead of "derive it from nesting". The
block form and the path form share Pass 0-3 entirely.

Validation and repair need no new *recursion* work: `validate_zones_recursive`
and `repair_zone_body` already descend into bodies. What changes is that
`ai_edit_network` must now **read** the validator's verdict instead of running
it for its side effects (D15).

---

## Phases

### Phase 1 — Reading
Serializer emits `body { … }` blocks, `$zone_input`, `^capture`, and the nested
`output`. Multi-line statement formatting for zone-bearing nodes (D14). `closure`
gains `get_text_properties` (D12, read half). No editor changes: `query` output
becomes complete, `edit` still rejects the new syntax.

*Tests:* a `map` with a two-node body serializes with both nodes and the
`output`; a body node reading `element` emits `$element`; a body node wired to a
parent node emits `^name`; a nested body emits `^^` at depth 2; a **nested body
node reading the outer HOF's iteration value emits `^$element`** — the
`ZoneInput`-at-depth-2 case that has no other spelling, and the one the two live
wires in the corpus exercise; a `fold` emits
`$acc`/`$element`; a `zip_with` emits `$element1`/`$element2`; **a `filter` and a
`foreach` round-trip their non-uniform zone-output names (`keep`, `out`) rather
than `result`** — the case a hand-written name table gets wrong; a `closure`
emits its `kind` and `params`; a body-less network's output is
**byte-identical** to today's (no regression for the 94% of nodes with no body).

### Phase 2 — The scope-aware editor
Pass 0-3 refactor. `body { … }` blocks parse and apply, with whole-body assign
semantics and name-keyed identity backed by the pre-clear position snapshot
(D8). Eager zone init and node-data-sourced zone-input names (D13). Full paths
in `EditResult` (D10), and `success` that reflects validation (D15). Closure
`set_text_properties` (D12, write half).

*Tests:* a `body` block creates a body from nothing; re-applying the same block
is a no-op that moves no node and changes no id; a block that renames one node
keeps the other nodes' positions and ids; `body { }` empties a body; a body node
wired with `$element` produces `ZoneInput { pin_index }` at depth 1; `^$element`
in a nested body produces `ZoneInput { pin_index }` at depth **2** with
`source_node_id` set to the **outer** HOF's node id (not the inner one, and not
a body node's); a `^capture` produces `NodeOutput` at depth 1; an unqualified
name that exists in both scopes resolves to the **body** one (shadowing);
`output` inside a block writes the parent's `zone_output_arguments` and **not**
`body.return_node_id`; a block carrying **no** `output` **clears** the parent's
zone-output wire (D5); two bodies may both contain a node named `a`;
`EditResult` reports `m1/a`, not `a`.

*Tests for the parts that are easy to leave out:* a `--replace` of an unchanged
script leaves every body node's `position` bit-identical, with no layout code
on the path (D8's snapshot — assert on positions, not on "it looks fine"); a
closure written with `body { … }` **before** `params: ["x", "y"]` in the same
statement still binds `$x` to parameter 0 (D13's ordering rule); an edit that
leaves a body blocking-invalid returns `success: false` with the offending
node's full path in `errors`, and a non-blocking one returns `success: true`
with a `warning` (D15).

**The regression test that motivates the whole design:** load a network with
inline bodies, `query` it, feed the output straight back through
`edit --replace`, and assert the body node count, wiring and positions are
unchanged. That round-trip destroys every body node in the **active** network
today — `query` and `edit` are active-network-only, so the worst single case in
the corpus is `0_staired-cover`'s 19, and 141 across the whole project.

### Phase 3 — Path-addressed edits
`expect_path()` in the three statement forms, `/` separator, relative resolution
inside the addressed scope.

*Tests:* `m1/x = …` updates one body node and leaves its siblings' ids and
positions untouched; `m1/new = …` appends to a body; `delete m1/x` removes one
body node and nothing else; `output m1/x` re-points the zone-output wire;
`delete m1/x` where `x` was the zone-output source clears that wire instead of
leaving it dangling; `outer/inner/n = …` reaches depth 2; `^^` inside it
resolves to the top level and `^$` to the outer HOF's element; a
path naming a non-existent scope is a clear error, not a silent no-op; a script
mixing a `body` block and a path statement for the same node applies them in
order; a path into a node with no zone is rejected.

### Phase 4 — Skill, guide, and reporting
`.claude/skills/atomcad/skill.md`: the body syntax, the `$`/`^`/`^$`/`/` forms
(with D4's reference-form table copied verbatim — the `k` / `k + 1` base
asymmetry is the one thing the AI will otherwise get wrong), the
`f:` vs `body` precedence (D11), and the "block replaces, path merges" rule —
including that a block without `output` clears the zone-output wire (D5).
Reference guide: `doc/reference_guide/headless_cli.md`, which also gains the
changed meaning of `success` (D15), and the `nodes/` pages for the HOF node
types, whose zone-pin names must match the table in *What a zone body is*.

*Manual verification* (per `feedback_manual_test_for_editor_ui`): query a real
network from `from_mechadense.cnnd` containing `closure` and `map` bodies, edit a
body node through a path statement, confirm the canvas shows the change inside
the body and that nothing else moved.

### Phase 5 (optional, recommended) — Make AI edits undoable
`ai_edit_network` records no undo command today. This design lets AI edits reach
into bodies, which raises what a bad edit can destroy. Wrapping the whole call in
one undo command is separable from Phases 1-4 and can be dropped without
affecting them, but it is the natural moment to do it.

---

## Interaction with other subsystems

**Incremental layout (`design_incremental_layout.md`).** The dependency runs
**one way**: this design defines and builds the per-scope name match and the
pre-clear position snapshot (D8); that design consumes them. Its §"HOF bodies"
says it "needs a proper revision before Phase 1" and points here — this is the
part it was waiting for — and its open question 8 ("does `replace` mode get
name-matching in v1?") is answered *yes, here*.

Two things follow. A body edit produces a **body-scope `EditDelta` shaped
exactly like a top-level one**, so the whole incremental layout algorithm
applies inside bodies with no new machinery — which is what that design's "run
it independently per network, including bodies" already assumes. And
`hand_moved`, drift repair and the push/shift passes stay wholly over there:
D8 carries `position` and nothing else, so the two can land in either order.

**AI edit history (`design_ai_edit_history.md`).** Two direct consequences.
First, its snapshots stop being blind to 141 nodes, so a body edit is actually
visible in the diff. Second, its *By node* diff splitter keys on `name =` at line
start and must now brace-match a multi-line statement (D14) — worth settling this
format **before** that diff viewer is built, which is the sequencing argument for
doing this design first.

**Nested bodies.** The syntax recurses to any depth (`^^…` for nodes, `^…$` for
iteration values), but `repair_zone_body` "deliberately skips depth ≥ 2"
(`zip_with.rs:211`), so a depth-2 wire to a dropped pin is flagged by validation
and never cleaned. Note that `^$element` is *exactly* a depth-2 `ZoneInput`, so
the first form this design makes writable is also the one the repair pass does
not cover: an AI can now author in one line a wire that a later pin-layout change
will strand. This design supports the syntax at depth ≥ 2 and does **not** fix
that repair gap; stated here so it is a known limitation rather than a surprise,
and a candidate for its own follow-up.

**Function pins.** Unchanged. `f:` and `@node` references keep working exactly as
they do, and D11 documents the precedence. A network that supplies its function
through `f:` needs nothing from this design.

---

## Open questions

1. **Should the lexical fallback in D4 exist at all?** Requiring an explicit `^`
   on input as well as output is stricter, harder to write, and impossible to get
   subtly wrong. The proposal accepts both, but only for **bare names** — `$name`
   is always exactly this body's zone input, and an outer one must be written
   `^$name`, because the fallback would otherwise silently turn a per-iteration
   read into a capture. If the AI turns out to write accidental captures on bare
   names too, tighten those the same way.
2. **`/` versus `.` as the path separator.** `/` is proposed because `.` is pin
   access. If a dotted path is strongly preferred for familiarity, the ambiguity
   is confined to `output` and `delete` and could be resolved by lookahead — but
   at the cost of two meanings for one token.
3. **Should a path statement be allowed to *create* the HOF node it addresses?**
   Proposed: no — `m1/x = …` requires `m1` to exist, created earlier in the same
   script or already present. Auto-creating an HOF from a path would have to
   guess its type.
4. **Does `query` emit bodies for *every* network, or only the active one?**
   Unchanged from today: the active network only, now including its bodies. A
   `--depth` flag to suppress bodies for a large network is a possible later
   convenience.
5. **Should Phase 5 (undo) be in scope?** It is adjacent, not required. Included
   as an explicitly separable phase.
