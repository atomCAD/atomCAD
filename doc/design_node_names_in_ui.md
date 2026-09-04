# Design: Node names in the GUI — see them, switch to them, jump to them

**Origin:** the human + AI co-editing session of 2026-09-03
(`reports/incremental_layout_coedit_session_2026-09-03.md`). Every AI edit,
every warning, the AI History *Layout* tab and the text format all refer to
nodes by their **custom name** (`expr49`, `apply_style1`, `map4/e1`), and the
GUI shows that name nowhere except, indirectly, the Text tab. A human reading
"I inserted `pass1` between `unfreeze1` and `apply_style1`" has no way to
find those three nodes on a 135-node canvas.

## Motivation

The custom name is the one identifier the AI, the text format, the Layout
tab, the corpus tests and the `.cnnd` file agree on. The canvas shows the
*type* name (`expr`, nine times on this design), the property panel shows the
type, the hover tooltips show the type. The name exists on every node and is
already delivered to Flutter (`NodeView.custom_name`), it is simply never
drawn — and never editable.

Five gaps, in the order a user hits them:

1. **"Which node is `expr49`?"** — no way to read a node's name from the
   canvas or the property panel.
2. **"Show me all the names at once"** — when scanning for a name, the type
   names are noise; the user wants to see names *instead of* types for a
   moment, then go back.
3. **"Take me to `map4/e1`"** — the AI names a node; the user should be able
   to type that name and land on it, the way Find Usages and error navigation
   already land on a node.
4. **"Call this one `chassis`"** — once the name is visible it is the natural
   thing to want to change, so that both the human and the AI talk about
   `chassis` instead of `union7`.
5. **"Show me *this* one"** — the AI History panel is where the human reads
   most of these names: every diff hunk and every moved-node row is titled by
   a name path. Having read `map4/e1` there, retyping it into a picker is one
   step too many; the name should be the link.

## Current state (analysis)

- **Canvas header** (`lib/structure_designer/node_network/node_widget.dart`
  ~940): `Text(getSimpleName(node.nodeTypeName))`, or a closure's
  user-supplied label. The header's `Tooltip` (~1028, ~1110 — two sites, one
  per header layout) shows the *full* type name. The optional subtitle line
  (~1166, `NodeData::get_subtitle`) is unrelated and stays.
- **Property panel**: every editor in `lib/structure_designer/node_data/`
  starts with `NodeEditorHeader(title: '<Type> Properties', nodeTypeName:)`
  (`node_editor_header.dart`) — 90 call sites with a literal title. The
  dispatcher is `node_data_widget.dart`.
- **Text tab** (`network_text_editor.dart` ~153): selecting a node highlights
  its `name = type { … }` line. This is the only place the name is visible
  today.
- **Error copy** (`node_widget.dart::_errorReportForNode`) and the AI History
  diff already use the name — which is why it feels half-present.
- **Every node has a name.** `Node.custom_name` is an `Option<String>` but is
  never `None` in practice: `add_node` mints one from the type through
  `generate_unique_display_name` (`sphere1`, `expr49`, per-network counter),
  and the `.cnnd` loader (`node_networks_serialization.rs` ~787) backfills any
  node that arrives without one. There is no "derived name for a nameless
  node" case to design for.
- **Copies do not keep the name.** `NodeNetwork::duplicate_node` and
  `copy_nodes_from` (the paste path, run twice: network → clipboard and
  clipboard → network) do not carry the copied `custom_name` at all; they
  mint a fresh type-counter name through `generate_unique_display_name`. So
  duplicating `cuboid1` gives `cuboid2`, and copy-pasting a node the user
  called `chassis` gives `union8`. Two tests pin the current behaviour:
  `test_duplicate_node_gets_unique_name` (`node_network_test.rs`) and
  `test_duplicated_nodes_have_unique_persistent_names` (`text_format_test.rs`).
  The undo commands (`undo/commands/duplicate_node.rs`, `paste_nodes.rs`) only
  restore on redo whatever name the copy was given.
- **And names are not unique.** Parameter nodes minted by
  promote-to-parameter (`promote_to_parameter.rs`), selection factoring
  (`selection_factoring.rs`) and closure→network conversion
  (`closure_network_conversion.rs`) take the parameter name as `custom_name`
  verbatim with no uniqueness check, and older files carry whatever earlier
  text-editor versions accepted, so a network can hold four nodes stored as
  `to_degrees`. The text format cannot write four statements with the same
  left-hand side, so on the way *out* `text_format::unique_node_names`
  renames the later ones by ascending id (`to_degrees`, `to_degrees_2`,
  `to_degrees_3`), skipping any spelling some other node already owns —
  without touching the stored names, until a text `--replace` makes the
  suffixes permanent. `node_inlining.rs::unique_name` already applies the
  same `_2` rule at inline time. This is the only situation where what the
  AI says differs from what the node stores. Body nodes are addressed by
  path, `map4/e1`.
- **Names are not identifiers, but they do have rules.** `format_identifier`
  backtick-quotes any spelling the text format cannot write bare, and
  `x.shape` / `union#1` are tested node names (`relaxed_node_names_test.rs`).
  The rules live in `identifier::is_valid_user_name`, which the text editor
  applies to every created node (`network_editor.rs::create_node`): non-empty,
  no backtick (the one character the quoting cannot escape), no control
  characters, no leading or trailing whitespace. Uniqueness within a scope is
  the text format's, not the validator's. A slash is allowed today, and that
  is a problem for paths — see D8.
- **Paths join bare names with `/` and nothing else.** `error_node_path`
  (`scoped_validation_errors.rs`), the diff hunk titles (`ai_edit_diff.rs`)
  and the layout log (`ai_edit_log.rs`) all compose `map4/e1` by joining the
  stored `custom_name`s with `/`, unquoted. A root node named `a/b` and a
  body node `b` under an owner `a` therefore record the identical string.
  Only the text format's own path parser tells them apart, through the
  backticks it prints.
- **Nothing identifies a node by name except the text world.** Audit of every
  `custom_name` read outside `text_format/` (2026-09-03): display labels
  (profiler, print log, `network_usages::node_label`, `error_node_path`), the
  name-minting uniqueness check, derived names computed once at conversion
  time (closure↔network conversion, selection factoring), undo snapshots that
  restore the name, the Text-tab replace's position/selection restore (text
  world), and `find_node_id_by_name`, which serves the CLI `evaluate <name>`.
  Wires, comment anchors, displayed pins, selection, zone-output arguments and
  the `.cnnd` file all key on node id. **Renaming a node is therefore an
  in-memory relabel with no structural consequence** — the only name-keyed
  match anywhere is the layout identity snapshot across one text edit (D11).
- **Jumping exists.** `StructureDesignerModel::jumpToNode(hostNetwork,
  scopeChain, nodeId, {screenAnchor})` is the shared spine of Find Usages
  (`doc/design_find_usages.md` D4) and error navigation: activates the
  network (recorded in navigation history, so *Back* works), selects the node
  in its scope, scrolls it into view, never changes zoom. Only the *name →
  (network, scope, id)* resolution is missing.
- **AI History panel** (`ai_history_panel.dart`): the *Diff* tab's
  `_DiffHunkView` titles each hunk with `hunk.nodePath`, and the *Layout*
  tab's `_MovedNodeRow` prints `moved.path`; both are plain `Text` inside a
  `SelectionArea`. Both spellings come from `unique_node_names` at record
  time, so after D1 they are `custom_name` paths, identical to what
  `error_node_path` composes. An entry is keyed by its own `networkName`,
  which need not be the active network, and the expanded diff dialog
  (`_showDiffDialog`, a `showDialog`) re-renders the same `_DiffTab`.
  Entries are history: by the time a name is clicked the node may have been
  renamed (by the AI or by D3), deleted, or re-created under the same name.
- **Display panel** (`display_panel.dart`, `display_button_group.dart`,
  `node_display_widget.dart`): icon buttons in subject clusters; the node
  display policy is a three-button radio group backed by
  `NodeDisplayPreferences.display_policy`, persisted through the preferences
  round-trip (`preferences.rs` ↔ `api/structure_designer/structure_designer_preferences.rs`
  twin ↔ Dart `model.setPreferences`). It is *not* in the Preferences dialog;
  that is the established pattern for a display-panel switch.
- **Free shortcuts**: Ctrl+C/D/H/J/M/S/V/X/Y/Z are taken
  (`structure_designer.dart::_handleGlobalKeyEvent` and the node canvas).
  Ctrl+F is free.

## Non-goals

- Renaming from the AI side: the text format keeps its "a statement names a
  node" semantics (`doc/design_identity_vs_naming.md`); a rename through text
  is still delete + create. Only the GUI rename (D3) is new.
- Changing the suffix *rule*: `_2`, `_3` and the ascending-id order stay
  exactly as `unique_node_names` defines them. D1 widens where the rule is
  applied (copies, parameter minting, the loader) and does not alter it.
- Showing both name and type in the header at once. The user's explicit
  preference: two states, either/or, cheap to flip.
- Exposing the title mode through the AI HTTP server / CLI `display`
  command. Nothing on the AI side needs it.
- Searching by anything other than the name (type, label, expression text).
  The picker's matching is on the text-format name; fuzzy matching is a
  follow-up.

## Design decisions

**D1 — Names are unique at the source, so `custom_name` *is* the name the
AI uses.**
Rather than deriving a display name in the view, every path that can put a
taken name into a scope applies the existing suffix rule at write time:

- **Duplicate and paste keep the copied name** and rename only on collision,
  to the first free `name_2`, `name_3`, … in the destination scope. This
  replaces the type-counter minting in `duplicate_node` and `copy_nodes_from`:
  a user-chosen `chassis` copies as `chassis_2`, not `union8`, which is what
  makes D3's rename worth doing. An auto-named `cuboid1` therefore copies as
  `cuboid1_2` rather than `cuboid2`; the two tests that pin the old spelling
  change expectation. A paste of a whole selection assigns names in ascending
  source-id order, the order `unique_node_names` uses, so the suffixes fall
  the way the text format would have assigned them. Redo restores the
  *renamed* spelling. The clipboard is a scope of its own, so the network →
  clipboard copy is also collision-free by construction.
- **Parameter minting** (promote-to-parameter, selection factoring,
  closure→network conversion) passes the parameter name through the same
  helper before storing it as `custom_name`; `param_name` is a separate field
  and is untouched.
- **Load-time normalization**: the loader's existing "assign names to nodes
  without one" pass grows a second step that renames duplicates with the same
  rule. Existing files heal themselves on first open; the migration is
  invisible in the text format because the text already showed the suffixed
  names.
- **Text-format editor**: a `--replace` already assigns the suffixed spelling;
  an incremental edit whose statement names a node that already exists in
  that scope *updates that node in place* (`network_editor.rs::process_assignment`
  looks the name up before creating), so it can never mint a duplicate. It
  is an update, not a rejection, and nothing here changes it.
- **`/` is no longer a legal character in a node name.** `is_valid_user_name`
  gains the rule, so the text editor and the D3 rename both refuse it; the
  loader normalization replaces a `/` in a legacy name with `_` before the
  uniqueness pass. The reason is D8: every path the AI is
  handed joins bare names with `/`, so a slash inside a name makes the path
  ambiguous. This is the one visible change to the text format's name
  rules: a backtick-quoted `` `a/b` `` is rejected where it used to load.
- **`unique_node_names` stays** as the serializer's name source and becomes a
  no-op on every network built through the API or the loader. The proof is
  the corpus test (Phase 0), not an assertion inside the function: the
  function must keep working on a network that holds duplicates, because the
  safety-net test builds one below the API on purpose.

The invariant: *within one scope, `custom_name` is unique, and the text
format prints it verbatim.* With it, the GUI can show and edit
`custom_name` directly and the header, the property panel, the Text tab, the
Layout tab and every AI message agree by construction. No new view field.

**D2 — Header tooltip: name first, type second.**
Both header `Tooltip`s become `` `xray1` · xray `` (node name, middle dot,
full type name; a closure with a label appends ` · <label>`). Hover was the
zero-cost surface; it needs no mode and no click.

**D3 — Property panel: one shared, editable name strip, not 90 edited
titles.**
`node_data_widget.dart` renders a one-line strip *above* whichever editor it
dispatches to: an editable text field holding `custom_name` in monospace,
the type in muted text (`xray1   xray`), and a *Copy name* icon button. For
a body node the strip shows the scope as a muted prefix (`map4 /`) before the
field, so the user sees the path and edits only the last segment. Editors and
`NodeEditorHeader` are untouched. This is the "`xray1 (xray)` in the panel
title" the user asked for, realised where a single change covers every node
type — and it is where the user experiences every node as *having* a name,
because every node does.

Editing semantics (the field follows the network-rename field's manners in
the node-networks list):

- Enter or focus loss commits; Esc reverts to the stored name. Nothing is
  written while typing.
- Validation is exactly what the text editor applies to a created node, plus
  the D1 invariant: the trimmed name passes `is_valid_user_name` (non-empty,
  no backtick, no control character, no `/`, no edge whitespace) and is not
  already used by another node in the same scope. Sharing the validator is
  what keeps a GUI-typed name printable: a backtick the field let through
  would be a name `format_identifier` cannot quote, and `query` would stop
  round-tripping. A rejection shows the validator's reason as an inline
  error under the field and leaves the stored name untouched; the text is
  never silently suffixed, since the user typed it on purpose.
- The rename goes through a new API `rename_node(scope_path, node_id,
  new_name) -> Result<(), String>` (scope-aware like every node-data setter,
  `rust/AGENTS.md`), which validates in Rust, writes `custom_name`, and pushes
  a `RenameNodeCommand` (id-keyed, stores old and new name) so it is one
  Ctrl+Z step. It marks the project dirty and triggers a *view* refresh only —
  no re-evaluation, since no evaluated state depends on the name.
- The Text tab re-renders with the new statement name; the AI's next `query`
  shows it. The AI History entry of the *next* AI edit carries the
  *diverged* marker, since the network's text changed outside `edit` (D11).

**D4 — Title mode is a persisted preference with exactly two states.**
`NodeDisplayPreferences` gains `title_mode: NodeTitleMode { Type, Name }`
(`#[serde(default)]`, default `Type`, twin in the api preferences file, Dart
enum). In `Name` mode the canvas header text is `custom_name` for **every**
node: builtin, custom-network instance (the name `0_styling1`, not the type
`0_styling`), closure (the label is replaced too — the label is the closure's
own choice of title, the name is the network's), comment, HOF/zone owner and
the placeholder of a collapsed body. Nothing else changes: colours, badges,
pins, the subtitle line, the header tooltip (D2 already carries both).

**D5 — The mode never changes a node's footprint.**
Node size is derived from the type (`doc/design_reflow_on_footprint_change.md`:
a footprint change triggers a reflow), so the header must render the name
inside the *existing* width with `TextOverflow.ellipsis`, and the footprint
computation must not look at the mode. Flipping the mode is a repaint, not a
layout — that is what makes "toggle quickly and it behaves as if both were
shown" true. A long name that ellipsizes is still fully readable in the D2
tooltip and the D3 strip.

**D6 — Where the toggle lives: the display panel's node cluster, plus a
shortcut.**
Two `DisplayIconButton`s as a radio group inside `nodeDisplayCluster`
(`node_display_widget.dart`), after the policy group: *Node titles: type
names* / *Node titles: node names* (`isSelected` mirrors the preference,
`onPressed` writes it through `model.setPreferences`, exactly as the policy
buttons do). Icon suggestions: `Icons.widgets_outlined` for type,
`Icons.label_outline` for name; the implementer may pick better ones. A
global shortcut **Ctrl+Shift+N** flips the mode
(`_handleGlobalKeyEvent`), and the *View* menu gets *Node titles: names*
as a checkable item with that shortcut, so the state is discoverable from
the menu too. No Preferences-dialog entry, matching the display policy.

**D7 — Find node: a popup picker, opened from the Edit menu, Ctrl+F, and a
canvas icon.**
Trigger: *Edit → Find node…* with shortcut **Ctrl+F**, and an always-visible
search icon button at the right end of the node-network editor's tab strip
(next to the existing `<>` Text-tab button), so it is reachable by mouse
without knowing the shortcut. The menu item is the canonical home; the icon
is the discoverable one. Both open the same picker.

The picker is a compact overlay anchored at the top-centre of the canvas (an
IDE "go to symbol" box, not a modal dialog): a text field that has focus on
open, a result list below it, Esc closes, Enter jumps to the highlighted
result, Up/Down move the highlight, a click on a row jumps. Matching is
case-insensitive substring on the name path (`map4/e1`, bare names, never
the backtick-quoted spelling the text format prints); results are sorted
exact-match first, then prefix, then substring, then by path. Each row shows
the name path in monospace, the type in muted text, and — when the
*all networks* switch is on — the network name as a trailing chip.

Scope: the **active network including its bodies** by default; a switch in
the picker (*all networks*, remembered for the session) widens it to every
network. Landing reuses `jumpToNode` with no `screenAnchor` (viewport-centred
landing; the user has no "where I was looking" position when typing). For a
body node `jumpToNode` already selects in-scope and expands the body view.

**D8 — Name resolution is a Rust API, so there is one path rule.**
`find_nodes_by_name(query: String, all_networks: bool) ->
Vec<APINodeNameMatch { network: String, scope_path: Vec<u64>, node_id: u64,
name_path: String, node_type_name: String }>` walks every scope
(`walk_all_nodes`-style recursion, `rust/AGENTS.md`) reading `custom_name`
and composing `map4/e1` paths from the bare names with the same joiner
`error_node_path` uses — the spelling the AI is handed in error paths, the
Layout tab and the diff. Backtick quoting is a text-format concern and never
appears in a path. The active network's matches come first. Flutter does no
name computation of its own. An empty query returns every node of the active
network, so opening the picker doubles as a name directory.

The same walker has an exact form, `resolve_node_path(network: String,
path: String) -> Option<APINodeRef { scope_path: Vec<u64>, node_id: u64 }>`,
which D12 uses and the substring search is built over. It splits the path
on `/` and matches each segment against the stored `custom_name`s of the
current scope, descending into the matched node's body for the next
segment. Splitting is sound only because D1 bans `/` from names: with a
slash allowed, a root node `a/b` and a body node `b` under an owner `a`
record the same string (see *Current state*), and no rule on a single
string can resolve both. A network that does not exist, a segment that
matches nothing, or a segment that descends into a node without a body, is
`None`.

**D9 — Copy in both directions.**
The D3 strip's *Copy name* and a *Copy node name* entry in the node context
menu give the human the exact spelling to paste into a prompt for the AI
(the reverse of D7). The copied text is the path form for body nodes.

**D10 — Why not a derived display name.**
A `text_name` view field computed by `unique_node_names` was the first
draft. It would have shown the right thing but left the stored name wrong,
so a rename field would have had to start from a spelling the node did not
hold, and every other consumer of `custom_name` (print log, profiler, error
paths, the usages panel) would have kept the ambiguous one. Fixing
uniqueness at the source is smaller and removes a class of "the AI said X,
the GUI shows Y" reports for good.

**D11 — What a GUI rename does to the AI-side bookkeeping.**
Two name-keyed structures exist, both in the text world, and neither learns
of a rename as such:
- the layout identity snapshot (`doc/design_incremental_layout.md` D14)
  matches nodes by name path *across one text edit*, so a node renamed in the
  GUI and then touched by the next AI edit is "new" to that edit's layout
  pass — the same thing that happens when the AI renames it. Acceptable; the
  layout log makes it visible.
- the AI History (`doc/design_ai_edit_history.md`) never sees a GUI rename
  as a diff: an entry's diff compares that edit's own before and after
  snapshots, and the rename happened between entries. It shows instead as
  the *diverged* marker on the next AI edit — the plain "the network changed
  outside `edit`" wording, set by the one before/after string comparison in
  `ai_edit_log.rs`. The `diverged_by_undo` variant is not involved: it is
  set only when an AI-edit command itself was undone since the last record,
  and the `RenameNodeCommand` plays no part in the detection. An AI-side
  rename, which *is* a delete and a create inside one edit, shows as a
  removed and an added statement; with D12 the added statement's title links
  to the renamed node and the removed one's title misses. Recording a GUI
  rename as an activity entry ("renamed `union7` → `chassis`") is a
  follow-up.

**D12 — Every name path in the AI History panel is a link to the node.**
The hunk title in the *Diff* tab (docked pane and expanded dialog alike)
and the path cell of a moved-node row in the *Layout* tab become clickable:
hover underlines the path and shows a `Go to <path>` tooltip, a click
jumps. Only the path text is the link; the diff lines stay plain text inside
the `SelectionArea`, so copying a hunk still works and a click on the path
does not start a selection.

Resolution is live, not recorded: the click resolves `(entry.networkName,
path)` through `resolve_node_path` (D8) against the network *as it is now*,
and lands through `jumpToNode(network, scope_path, node_id)` with no
`screenAnchor` — viewport-centred, as in D7, because the panel is not the
canvas and there is no "where I was looking" position to preserve. The
target network is activated if it is not the active one (recorded in the
navigation history, so *Back* returns), and a body node is selected in
scope with its body expanded, exactly as Find Usages lands. From the
expanded dialog the jump closes the dialog first, so the canvas is visible
when the node is selected; the docked panel stays open.

Every path is a link regardless of hunk kind, and a miss is a SnackBar:
*"No node `map4/e1` in `0_styling` — renamed or deleted since edit #12"*.
This is the expected outcome for a removed hunk (`−`), whose statement names
a node this very edit deleted, and it is the honest outcome for a node the
AI or the user renamed afterwards. Greying removed hunks out was
considered and rejected: a later edit may have re-created the node under
the same name, and the name is the identity the AI reasons in
(`doc/design_identity_vs_naming.md`), so a link that resolves to *today's*
holder of that name is the right answer. A network renamed after the entry
was recorded misses the same way, with the entry's old network name in the
message; following network renames through the history is not attempted.

No new record field is stored and the history format does not change: the
paths the panel already holds are the resolver's input by construction,
because both were produced by the D1 spelling.

## Phases

Each phase ends with its Rust tests, `flutter analyze`, the Dart tests under
`test/` where the phase adds any, and a manual walkthrough run by the
maintainer. The walkthroughs are the verification for the thin editor UI
(tooltips, the strip, the header text, the picker), per the project's rule
that a manual pass on a real design beats a mandated integration test; the
judgement calls are visual — does the strip crowd the panel, does an
ellipsized name still read at the default zoom, does the picker land where
the eye expects. The design used throughout is `layout_test_scratch.cnnd` →
`0_precursor_tests_incomplete_V5+covers`, the network of the originating
session, with an AI connected over the HTTP server so `query` can be checked
against the GUI.

### Phase 0 — Unique names at the source (D1)

1. A shared `NodeNetwork::unique_name_for(desired)` (or promote
   `node_inlining::unique_name`) as the one suffixing helper: first free of
   `desired`, `desired_2`, `desired_3`, … against the stored names of that
   scope — the rule `unique_node_names` already implements.
2. `duplicate_node` and `copy_nodes_from` carry the copied name through the
   helper instead of minting a type-counter name; a multi-node paste assigns
   in ascending source-id order. The undo commands need no change — they
   snapshot the name the copy was given — but the redo expectation is now the
   suffixed spelling.
3. Parameter minting (promote-to-parameter, selection factoring,
   closure→network conversion) goes through the helper.
4. Loader normalization of duplicate names, beside the existing backfill in
   `node_networks_serialization.rs`.
5. `is_valid_user_name` gains the `/` rule (`InvalidNameReason::ContainsSlash`);
   the loader normalization replaces `/` with `_` before its uniqueness
   step. `unique_node_names` keeps its signature and its behaviour — no
   assertion inside it, see D1.
6. `text_format/AGENTS.md` "Round-trip invariants": the uniqueness bullet
   changes from "the GUI does not keep names unique" to "names are unique at
   the source; `unique_node_names` is the serializer's read and a safety
   net", and the name-rules bullet lists `/` among the rejected characters.

*Tests* (Rust; file per bullet):

- `copy_paste_test.rs`: paste one `x` beside `x` → `x_2`; paste a selection
  of `x` and `x_2` when both already exist → `x_3`, `x_2_2` in ascending
  source-id order, and the text format prints the stored names verbatim.
  Copy-paste of a node the user renamed keeps the name (`chassis` →
  `chassis_2`); paste into a zone body where the *top level* already holds
  `x` gives `x` unchanged, because uniqueness is per scope.
- `node_network_test.rs`: `test_duplicate_node_gets_unique_name` and
  `text_format_test.rs::test_duplicated_nodes_have_unique_persistent_names`
  change expectation from `cuboid2` / `sphere2` to `cuboid1_2` / `sphere1_2`.
  Duplicate of `x_2` when `x_2_2` is free → `x_2_2` (pin the spelling so the
  rule cannot drift).
- `undo_test.rs`: undo then redo of a paste and of a duplicate restores the
  suffixed spelling, and the network → text output is identical before undo
  and after redo.
- `promote_to_parameter` / `selection_factoring` / `closure_network_conversion`
  tests: promoting to a parameter named `x` in a scope that already holds a
  node `x` yields a parameter node `x_2` whose `param_name` is still `x`.
- Loader: a hand-written fixture with three `to_degrees` nodes under the
  crate's `tests/fixtures/` loads as `to_degrees`, `to_degrees_2`,
  `to_degrees_3` in ascending id order, and saving it back writes the
  suffixed names. The same fixture holds a node stored as `a/b`, which loads
  as `a_b`.
- `relaxed_node_names_test.rs`: `is_valid_user_name` rejects `a/b`, and an
  incremental text edit `` `a/b` = int {…} `` is refused with that reason,
  beside the file's existing invalid-name cases.
- `text_format_roundtrip_corpus_test.rs` (demolib + private file): after
  load, `unique_node_names` returns every node's stored name unchanged — the
  corpus is the proof that the invariant holds on real files.
  `duplicate_node_names_are_written_uniquely_and_match_back` keeps building
  its duplicates below the API (it already writes `custom_name` directly) and
  becomes the test of the safety net, not of GUI behaviour.
- The text-format editor's in-place update of a taken name in an incremental
  edit (`x = int {…}` when `x` exists edits that node, creates nothing) is a
  precondition of D1; cite the existing test in `text_format_test.rs` or add
  one that asserts the node count is unchanged.

*Manual walkthrough:* open a file known to hold duplicate names (the private
corpus file with the four `to_degrees` nodes) and confirm the Text tab reads
exactly as it did before the change. Copy-paste a node named `x` twice and
read `x_2`, `x_3` in the Text tab; Ctrl+Z twice, Ctrl+Y twice, and the names
come back identical. Duplicate `x` with Ctrl+D and read `x_4`. Save, reopen, and
confirm the names persisted and no node was renamed on load.

### Phase 1 — Tooltip; editable property-panel strip; copy

1. Header tooltips (D2), both sites — `custom_name` is already on the view.
2. `rename_node(scope_path, node_id, new_name)` API + `RenameNodeCommand`
   (D3), scope-aware.
3. Name strip in `node_data_widget.dart` (D3) with the editable field and
   *Copy name*; *Copy node name* in the node context menu (D9).
4. Reference guide: `doc/reference_guide/ui.md` — *Node network editor
   panel* (hover) and *Node Properties Panel* (strip, rename, copy).

*Tests* (Rust, beside `rename_node_network_rejects_invalid_name` in
`relaxed_node_names_test.rs`, and in `undo_test.rs` for the command):

- The contract with the AI: rename `union7` → `chassis`, serialize, and the
  statement reads `chassis = union {…}` while every downstream argument that
  referenced `union7` now references `chassis`; a `--replace` of that text is
  a no-op, so wires, comment anchors and displayed pins survived the
  rename by id.
- Reject an empty or whitespace-only name, a name with a backtick, and a
  name with a `/`, each with the `is_valid_user_name` reason; the stored
  name is untouched and no undo entry is pushed.
- Reject a name held by another node in the same scope; allow the same name
  in a sibling body and in the parent scope; rename a body node by scope
  path.
- Rename to the identical name is a no-op: no undo entry, dirty flag
  unchanged. Surrounding whitespace is trimmed before the comparison.
- A name the text format must quote (`x.shape`) is accepted and round-trips
  through `query` → `--replace` unchanged.
- Undo restores the old name, redo the new one; the dirty flag is set by the
  rename and cleared by undo back to the saved state.
- After a rename, the corpus invariant still holds (`unique_node_names`
  renames nothing).

*Manual walkthrough:* hover a header and read `` `xray1` · xray ``; hover a
labelled closure and read the label as the third segment. Select a node,
edit the strip to a name another node holds, press Enter, and read the
inline error with the stored name untouched; press Esc and the field
reverts. Rename `union7` to `chassis`: the Text tab statement changes, the
AI's next `query` prints `chassis`, and Ctrl+Z brings `union7` back in both.
Select a node inside a `map` body and confirm the strip shows `map4 /` as a
muted prefix and edits only the last segment. Use *Copy node name* from the
context menu on a body node and paste it into a prompt: it reads `map4/e1`.
Confirm the rename did not re-evaluate anything (the profiler panel shows no
new pass).

### Phase 2 — Title mode

1. `NodeTitleMode` in `NodeDisplayPreferences` (Rust, twin, Dart), persisted
   (D4).
2. Header rendering honours the mode for every node kind listed in D4;
   footprint untouched (D5).
3. Display-panel radio group, View-menu checkable item, Ctrl+Shift+N (D6).
4. Reference guide: *Display Preferences Panel* gets a *Node titles*
   subsection; *Menu Bar* lists the item and shortcut.

*Tests:*

- `preferences_test.rs`: `title_mode` round-trips through JSON, a
  preferences file without the field loads as `Type`, and the api twin
  converts both ways.
- The footprint invariant lives in Dart, not Rust — `rendered_node_size`
  never sees preferences, so a Rust assertion would pass vacuously. A widget
  test in `test/` pumps a `NodeWidget` with a 40-character name and the
  short type name under both modes and asserts the render box size is
  identical, the header ellipsizes, and no overflow error is reported.
  `ScopeResolver.effectiveNodeSizeLogical` must not take the mode as an
  input; `test/layout_size_parity_test.dart` keeps passing unchanged.

*Manual walkthrough:* flip the mode from the display panel, from the View
menu and with Ctrl+Shift+N; each surface reflects the other two. In Name
mode walk the D4 list on the test design: a builtin, a custom-network
instance (reads `0_styling1`), a closure with a label (label replaced), a
comment, a `map` owner and its collapsed placeholder. Rename a node to a
40-character name and confirm it ellipsizes without moving any node or wire
(flip the mode back and forth; nothing on the canvas shifts). Restart the
app and confirm the mode persisted.

### Phase 3 — Find node

1. `resolve_node_path` and, over it, `find_nodes_by_name` in the
   structure-designer crate + api twins (D8).
2. Flutter picker overlay (D7): field, list, keyboard handling, *all
   networks* switch, landing through `jumpToNode`.
3. Entry points: *Edit → Find node…* (Ctrl+F), tab-strip icon.
4. Reference guide: *Navigating in the node network editor panel* and *Menu
   Bar*.

*Tests:*

- Rust (`find_nodes_by_name_test.rs`), the exact resolver first: a root
  node, a body node at depth two, a backtick-quoted name that is not an
  identifier (`x.shape`, resolved from the bare spelling), an unknown
  network, an unknown segment, a segment that descends into a node without
  a body, and a path whose last segment is a body *owner* (resolves to the
  owner, not into it).
- Rust, the search: active-only vs all-networks, with the
  active network's matches first; a body node at depth two reports the
  `map4/c1/e1` path and the full scope path; nodes inside a *collapsed* body
  are included; `_2` names match on the stored spelling; matching is
  case-insensitive on the path string (`map4/e1`, bare names joined by `/`,
  never the backtick-quoted spelling `format_identifier` would print), so
  `e1` matches `map4/e1` as a substring; ordering is exact, then prefix,
  then substring, then by path; an empty query returns every node of the
  active network.
- Dart widget test in `test/` for the picker: it takes a plain list of
  matches, so it needs no kernel. Typing filters and re-sorts; Up/Down move
  the highlight and wrap at the ends; Enter fires the jump callback with the
  highlighted match; Esc closes without a jump; the *all networks* switch
  re-queries and shows the network chip on rows from other networks.

*Manual walkthrough:* press Ctrl+F, type `e1`, and read the ordering:
`e1`, then `e1…` prefixes, then substrings such as `map4/e1`. Press Enter on
a body node and confirm the body expands and the node is selected and
centred, zoom unchanged; press Back and land on the previous network. Toggle
*all networks*, pick a node from another network, and confirm the chip and
the landing. Open the picker with an empty field and confirm it lists every
node of the active network. Open it from the Edit menu and from the
tab-strip icon; both behave identically.

### Phase 4 — Jump from the AI History panel (D12)

Depends on Phase 3's resolver.

1. `StructureDesignerModel.jumpToNodePath(network, path, {seq})`: resolves
   through `resolve_node_path`, lands through `jumpToNode` with no anchor,
   and on a miss shows the D12 SnackBar naming the path, the network and
   the edit number.
2. `_DiffHunkView`'s title and `_MovedNodeRow`'s path cell become link
   widgets (hover underline, `Go to <path>` tooltip, pointer cursor) that
   call it with the entry's `networkName`. The hunk placeholder `(hunk)` for
   an empty `nodePath` stays plain text.
3. The expanded diff dialog passes a callback that pops the dialog before
   the jump.
4. Reference guide: `doc/reference_guide/ui.md`, the AI History panel
   section — the *Diff* and *Layout* tab descriptions gain "click a node
   path to jump to that node; a node renamed or deleted since the edit
   reports a miss".

*Tests:*

- Rust: no new kernel behaviour beyond Phase 3's resolver. One test in
  `ai_edit_history_test.rs` pins the contract the panel relies on: after an
  AI edit that adds `m1/d`, every `nodePath` in the recorded diff and every
  `moved.path` in the layout outcome resolves through `resolve_node_path`
  against the entry's network — the recorded spelling and the resolver's
  spelling are the same by construction, and this is the test that keeps
  them so if either side changes its joiner.
- Dart widget test in `test/`, the first for this panel. The two tabs are
  private today, so the link is a small public widget (`NodeNameLink(path,
  network, onJump)`) that the tabs compose and the test pumps directly, and
  the tabs take an `onJumpToNode(network, path)` callback rather than
  reaching for the model. The test asserts: a tap reports the path and
  network; a hunk with an empty `nodePath` renders no link; a diff line is
  not a link; the dialog variant pops before invoking the callback.

*Manual walkthrough:* have the AI make an edit that adds a node inside a
`map` body and moves two root nodes. In the *Diff* tab click the added
hunk's title: the body expands, the node is selected and centred, zoom
unchanged. Press *Back*. In the *Layout* tab click a moved root node and
land on it. Open the expanded diff, click a title, and confirm the dialog
closes before the canvas selects the node. Activate a different network,
click a path in an older entry, and confirm the entry's network is
activated and *Back* returns. Rename the added node in the D3 strip, click
its old title in the diff, and read the miss SnackBar with the edit number;
Ctrl+Z the rename and click the same title again to land. Have the AI
rename a node through `edit` (delete and re-create under a new name): in
that entry's diff the removed title misses and the added title lands.
Finally select text across a hunk's lines and copy it: clicking the title
must not have broken selection.

## Deferred / follow-ups

- Fuzzy matching and matching on type / expression text in the picker.
- The picker as a general command palette (networks, actions).
- A GUI rename recorded as an AI History activity entry, and the layout
  identity snapshot following renames (D11).
- Renaming from the node header in place (double-click), once the strip has
  proven the semantics.
