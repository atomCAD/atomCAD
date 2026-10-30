# Headless Mode (CLI)

← Back to [Reference Guide hub](../atomCAD_reference_guide.md)

atomCAD can run in headless mode for batch processing and automation workflows. The CLI supports both single-run and batch-mode evaluation of node networks with parameterized inputs, exporting results to `.mol` or `.xyz` files.

For detailed usage instructions, see [CLI Usage Guide](../cli_usage.md).

## Scripting a running instance: `atomcad-cli`

A second, separate tool — `atomcad-cli` — talks to a **running** atomCAD over a
local port. It is what [Claude Code and other agents](./claude_code.md) use, but
it is an ordinary command-line tool and nothing about it is AI-specific.

- `atomcad-cli query` writes the active network out in the
  [node network text format](../node_network_text_format.md).
- `atomcad-cli edit` reads that same format back, either merging into the
  network (the default) or replacing it (`--replace`).
- `atomcad-cli run <node>` presses a [`chemisorb`](./nodes/atomic.md#chemisorb)
  node's **Run** button: evaluation never runs that search, so without `run` a
  script only ever sees the node's plan. It waits for the search and prints a
  summary: how many hypotheses were relaxed, how many candidates are listed
  (with, in parentheses, how many copies the node's `unique` setting merged —
  `6 listed (14 copies merged)`), the best listed strain, and warnings for
  unconverged relaxations or an exhausted budget.
- `atomcad-cli debug-select <node> [<path>] [--form seated|relaxed] [--show]`
  clicks an item of a `chemisorb` node's search tree (a leg `12-45,13-61`, or
  the next foot after it, `12-45,13-61,14`): the item shows on the node's
  `debug` and `debug_shapes` pins (`--show` displays them), and the command
  prints what it marks and its children's paths, to go one level deeper.

Query output is valid input to `edit --replace`, so a whole network can be read,
adjusted in a text editor, and applied back.

### Zone bodies are part of the text

The inline body of a `map`, `filter`, `fold`, `foreach`, `zip_with` or `closure`
node is projected as a `body { … }` block, so `query` shows what an HOF actually
computes rather than an empty-looking node, and `edit` can author one:

```
m1 = map {
  xs: r,
  body {
    d = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    output d
  }
}
```

Inside a body, `$element` names the owner's per-iteration value and `^name`
reaches a node one scope out. A body node can also be addressed directly from
outside its block with a path — `m1/d = …`, `output m1/d`, `delete m1/d` — which
merges into the body instead of replacing it. The full rules are in
[Zone Bodies](../node_network_text_format.md#zone-bodies).

Statements carrying a body span several lines, so a script that post-processes
query output must brace-match rather than split on newlines.

### What `edit` reports

`edit` prints a JSON result. `success: true` means the script **parsed, applied
and validates** — the network was re-validated after the edit and its verdict is
part of the answer, not just a side effect. Blocking problems are listed in
`errors` and fail the edit; advisory ones are listed in `warnings` and do not.

An edit is **all or nothing**. If a statement cannot be carried out (an unknown
node type, a property literal of the wrong type, a `delete` or `output` naming a
node that does not exist, a path into a body that does not exist), the network
is left exactly as it was, and the last error says *Nothing was applied*. Fix the
script and send all of it again. A validation error is different: the edit
applied and the network it produced is invalid (a type mismatch on a wire, a
wire cycle); that edit stays, and Ctrl+Z takes it back.

Node names in `nodes_created`, `nodes_updated`, `nodes_deleted` and in error text
are **full paths** (`m1/a`), since a body node's name is unique only within its
own body.

A freshly created HOF has an *empty* body, which is a blocking error until it is
given a `body` block or has a function wired into its `f:` pin — so
`m1 = map { xs: r }` on its own reports `success: false`, with `m1` named in
`errors`.

#### The `layout` section: what moved on the canvas, and why

An edit fits its new nodes into the existing drawing and leaves everything else
where it is ([Where nodes end up](node_networks.md#where-nodes-end-up-your-arrangement-and-what-may-move-it)).
An edit can also say where things go itself, with
[layout statements](../node_network_text_format.md#layout-statements) —
`place n below h push down`, `straighten [a, b]`. When an edit moves a node
that was already there, leaves a problem behind, or contains layout
statements, the result says so under `layout`:

```json
"layout": {
  "auto_placed": [ { "node": "n", "box": [520, 100, 160, 60] } ],
  "moved": [
    { "node": "b", "from": [520, 100], "to": [730, 100], "cause": "auto",
      "why": "made room for new node n" }
  ],
  "findings": [],
  "warnings": []
}
```

| Field | Content |
|---|---|
| `placed` | every node a `place` statement positioned, new or existing, with the box it ended in: `[x, y, width, height]` |
| `auto_placed` | every other node the edit created, with the box it landed in. This is how a script learns the real size of what it made, without querying again. |
| `moved` | every node that existed before the edit and whose position changed: `from` and `to`, the `cause` of its first move and a short `why`. If it moved again for another reason, `also` lists those causes in order. |
| `findings` | problems the edit created and nothing repaired, one line each (below) |
| `warnings` | layout statements that could not be carried out: `line`, `statement`, `problem`, and the `fallback` that happened instead |

Coordinates are canvas pixels — x to the right, y **down** — and a position is a
node's top-left corner. A node inside a higher-order function's body has body
coordinates, measured from the body's top-left. Node names are full paths, as
everywhere else in the result. Values are rounded to whole pixels.

**Causes:**

| `cause` | The node moved because… |
|---|---|
| `place` | a `place` statement put it there |
| `arrange` | an `arrange` or `straighten` statement listed it |
| `push` | a placed or arranged node landed on it; `dir` says which way it went (`up`, `down`, `right`). The `why` names the statement's line. |
| `grow` | a node this edit made bigger (a new pin, a fuller body) pushed it |
| `auto` | room had to be made for a new node, a new wire had to be kept running left to right, or a comment followed its anchor back |

**Findings** are only ever about what *this* edit created. Overlaps, backward
wires, hidden wires and crossings your drawing already had are left alone and
never reported.

| `kind` | Reported when |
|---|---|
| `overlap` | two nodes overlap after the edit and did not before (only possible where a layout statement put a node, since everything else is pushed clear) |
| `backward_wire` | a wire this edit created runs right to left and could not be repaired (typically because it closes a cycle, so no shift can put both ends in order) |
| `wire_over_node` | a wire runs behind a node it does not connect to, for at least 10 px, and did not before. `nodes` lists the wire's source, its destination and the node; the message gives the hidden length |
| `crossing` | two wires cross and did not before. `nodes` lists the first wire's source and destination, then the second's. Wires that share a pin, or join the same two nodes, are never reported |

The whole `layout` key is **absent** when there is nothing to say: no existing
node moved, nothing was found, and the edit had no layout statements. That is
the usual, good outcome — a new node alone does not produce it.

#### Previewing an edit: `--dry-run`

```bash
atomcad-cli edit --dry-run --code="n = int { value: 3 }"
```

`--dry-run` runs the whole edit — parse, apply, validate, layout — prints the
same result it would print for real, with `"dry_run": true` added, and then
puts everything back. Nothing is kept: no undo step, the design is not marked
modified, nothing appears in the AI History panel, and the canvas is not
redrawn. Use it to see what an edit in a crowded area would move before
committing to it. It works with `--replace` and with multi-line input too.

### Where things are on the canvas: `query --layout`

The text format carries no positions. `query --layout` prints the drawing
instead: one line per node with its box, plus the columns and rows the drawing
appears to be arranged in.

```bash
atomcad-cli query --layout                       # every node
atomcad-cli query --layout --nodes=d,note,hole_r # just these
atomcad-cli query --layout --box=0,0,700,600     # everything touching this area
atomcad-cli query --layout --scope=holes         # inside the body of `holes`
```

```
# Main: extent (100,20)–(1150,490), 11 nodes; 7 in box (0,0)–(700,600)
w        100,100  160×60
h        100,190  160×60
d        100,280  160×60
note     100,370  180×80   comment
dims     310,160  160×110
hole_r   310,390  160×60   ai
hole     520,390  160×80   ai
# groups (inferred)
column   w, h, d            gap 30
row      hole_r, hole       gap 50
```

- **Coordinates** are canvas pixels, x to the right and y **down**, rounded to
  whole pixels. A position is the node's top-left corner; the size is what the
  canvas draws. Inside a body (`--scope`, a `/`-separated path of node names
  such as `m1/m2`) they are relative to the body's top-left corner.
- **The header** gives the extent of the whole network (or body), whatever the
  filter: everything outside it is free space. An empty `--box` result means
  that area is free.
- **Tags:** `ai` marks a node an AI edit created that nobody has moved by hand
  since (a drag or an *Arrange* command takes it back; so does reopening the
  file, since the tag is never saved). `comment` is a comment box, `body` an
  expanded higher-order node whose contents `--scope` lists.
- **Groups** are a guess, labelled as such: nodes whose left edges line up
  within 8 px and that sit at most 60 px apart form a column; tops within 8 px
  and at most 100 px apart form a row. `gap` is the typical spacing. A group
  is shown when any of its members is listed; members that are not are `…`.
- `--nodes` and `--box` cannot be combined. A name that does not exist is
  listed as `(no such node)`. Write `--box=…` with an `=` when the first
  coordinate is negative.

In the REPL, `query --layout` takes the same options.

### Undoing an edit

Each `atomcad-cli edit` is recorded as a **single undo step** in the running
app, labelled *AI edit network* in the undo tooltip so it is distinguishable
from your own edits in the Text tab. Ctrl+Z in atomCAD reverts the whole edit,
zone bodies included; Ctrl+Y (redo) reapplies it.

An edit that applied but then failed validation is still undoable — that is the
case the step exists for. An edit with an error changed nothing, so it has no
undo step.

### Tabs: `documents` and `--document`

The application can have several designs open, one per
[tab](./ui.md#document-tabs), and every command acts on the **active** tab —
whichever one the user last clicked. `load` opens a file in a new tab (or
switches to the tab that has it), `new` opens a new *Untitled* tab; neither
discards anything. Both print the document id and path of the tab they
opened, and `file` prints the active tab's id (`Document: 2`).

Because the user can switch tabs at any time, a script can say which design it
means with `--document <path-or-id>`, before or after the command. A command
naming anything but the active tab is refused before it does anything, with a
message that names the active one:

```bash
atomcad-cli documents
# * 2  C:/work/host.cnnd  (modified)
#   3  Untitled

atomcad-cli --document C:/work/host.cnnd query
atomcad-cli edit --document 3 --code "s = sphere { radius: 4 }"
# Error: The request names document '3', but the active document is
#   C:/work/host.cnnd. Switch to that tab in atomCAD, or pass --document for
#   the active one.
```

A saved design is named by its path — any spelling of it; a relative path is
resolved against the CLI's working directory. An *Untitled* design has no
path and is named by the id `documents` lists. `load`, `new` and `documents`
are not guarded. The [Claude Code skill](./claude_code.md) passes `--document`
on every command.

### Every request is recorded

Each `atomcad-cli edit` — merge or `--replace`, applied or rejected — is
recorded in the running application's [AI History
panel](./ui.md#ai-history-panel): the submitted script, what came back, the
network before and after, and which nodes the layout pass moved. Edits refused
because the network is locked against CLI writes are recorded too; locking a
network does not hide the attempts. The one exception is `edit --dry-run`: a
preview leaves no trace, and that includes the log.

Every *other* command — `query`, `evaluate`, `run`, `screenshot`, `camera`, `display`,
`networks …`, `load`, `save`, `new`, `documents` — lands on the same timeline as a one-line
entry: the request, how it ended, how long it took. That is what makes the log
readable as a session rather than a list of edits: it shows whether the model
looked at the network before rewriting it, and what it did between two edits.
(`health`, which the CLI polls before every command, is not recorded.)

### Saying who you are: `--label`

atomCAD cannot tell which model — or which version of a prompt — is driving the
CLI. Tell it:

```bash
atomcad-cli --label "Opus 5 / skill v3" query
ATOMCAD_CLIENT_LABEL="Opus 5 / skill v3" atomcad-cli query
```

The label rides along as an `X-Client-Label` header on every request, is stamped
on every entry it produces, and is carried into exports. It replaces typing a
session label into the panel by hand, and unlike a session label it is
per-request — so one exported log can tell two models editing in turn apart. It
is entirely optional; nothing else changes when it is absent.

The environment variable is the form to prefer for an agent harness: set it
once, and every command of the session is attributed without touching the
command lines.

The log is a **session** log. It is kept in memory only, nothing about it goes
into your `.cnnd` file, and undo does not erase an entry — undoing an AI edit
puts the network back and leaves the record of it standing.

So if you want to keep a session — to refine a prompt against what the model
actually did, or to compare two models on the same task — **export it before
closing the application**. The panel's toolbar writes the whole log to a file:
JSON keeps everything, including both text snapshots per edit, and Markdown is
the readable form for pasting into a conversation. Both carry the non-edit
requests interleaved with the edits, in order. The session-label field beside it
stamps a name of your choosing ("Opus 5 / skill v3") into the export — use it
when the CLI is not sending `--label`, which says the same thing automatically.

### Linked libraries: `atomcad-cli libraries`

The [library links](library_linking.md) of the open design, from a script:

| Command | What it does |
|---|---|
| `atomcad-cli libraries` | Lists every linked library — direct ones and the ones they link, indented under them — with its path and any problem (missing, unreadable, older than the file on disk). |
| `atomcad-cli libraries link <path> <alias>` | Links a `.cnnd` file (a path relative to the design's file, or absolute) under an alias. The design must have been saved once. |
| `atomcad-cli libraries refresh [<mount>]` | Re-reads one library (`demolib`), or, without an argument, every library and data file. Prints what changed, and one line per wire it disconnected. |
| `atomcad-cli libraries unlink <alias>` | Unlinks a library; refused, with the nodes that use it, while anything does. |
| `atomcad-cli libraries rename <alias> <new-alias>` | Renames an alias together with every use of it in the design. |
| `atomcad-cli libraries make-local <alias>` | Makes a library part of the design (see [Making a library local](library_linking.md#making-a-library-local)). |

Every one of them is one undo step, exactly as from the menus. In the REPL,
`libs` is short for `libraries`.

`atomcad-cli save <path>` into another folder is *Save As* with the dependency
copy of [Moving a design](library_linking.md#moving-a-design-save-as-and-project-bundles).
When the design links libraries or reads data files by a relative path that
would not be at the new location, it refuses and lists them; add `--copy-deps`
to copy them along (a different file already at a target is kept, never
overwritten) or `--no-deps` to save the design alone.

`query` on a network that belongs to a linked library prints a
`# linked from <path> (library `<mount>`)` line under the `# Network:` header.
Such a network can be read, but `edit` on it is refused: to change it, open the
library file.

### Reading query output back in

`query` output contains blank lines, and `atomcad-cli edit` reading from
**stdin** stops at the first blank line. To feed query output straight back,
strip the blank lines first, or pass the text through `--code` with `\n` escape
sequences.
