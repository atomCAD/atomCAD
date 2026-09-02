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

Node names in `nodes_created`, `nodes_updated`, `nodes_deleted` and in error text
are **full paths** (`m1/a`), since a body node's name is unique only within its
own body.

A freshly created HOF has an *empty* body, which is a blocking error until it is
given a `body` block or has a function wired into its `f:` pin — so
`m1 = map { xs: r }` on its own reports `success: false`, with `m1` named in
`errors`.

### Undoing an edit

Each `atomcad-cli edit` is recorded as a **single undo step** in the running
app, labelled *AI edit network* in the undo tooltip so it is distinguishable
from your own edits in the Text tab. Ctrl+Z in atomCAD reverts the whole edit,
zone bodies included; Ctrl+Y (redo) reapplies it.

An edit that applied but then failed validation is still undoable — that is the
case the step exists for.

### Every request is recorded

Each `atomcad-cli edit` — merge or `--replace`, applied or rejected — is
recorded in the running application's [AI History
panel](./ui.md#ai-history-panel): the submitted script, what came back, the
network before and after, and which nodes the layout pass moved. Edits refused
because the network is locked against CLI writes are recorded too; locking a
network does not hide the attempts.

Every *other* command — `query`, `evaluate`, `screenshot`, `camera`, `display`,
`networks …`, `load`, `save`, `new` — lands on the same timeline as a one-line
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

### Reading query output back in

`query` output contains blank lines, and `atomcad-cli edit` reading from
**stdin** stops at the first blank line. To feed query output straight back,
strip the blank lines first, or pass the text through `--code` with `\n` escape
sequences.
