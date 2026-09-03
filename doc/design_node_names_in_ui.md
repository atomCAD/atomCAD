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

Three gaps, in the order a user hits them:

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
- **But names are not unique.** Duplicate and paste (`undo/commands/
  duplicate_node.rs`, `paste_nodes.rs`) restore the copied `custom_name`
  verbatim, so a network can hold four nodes stored as `to_degrees`. The text
  format cannot write four statements with the same left-hand side, so on the
  way *out* `text_format::unique_node_names` renames the later ones by
  ascending id (`to_degrees`, `to_degrees_2`, `to_degrees_3`) — without
  touching the stored names, until a text `--replace` makes the suffixes
  permanent. `node_inlining.rs::unique_name` already applies the same `_2`
  rule at inline time. This is the only situation where what the AI says
  differs from what the node stores. Body nodes are addressed by path,
  `map4/e1`.
- **Names are not identifiers.** `format_identifier` backtick-quotes any
  spelling the text format cannot write bare, and `x.shape` / `union#1` are
  tested node names (`relaxed_node_names_test.rs`). A name's only rules are
  *non-empty* and *unique within its scope*.
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
- Changing the naming *rule*: the `_2`, `_3` suffixing and the ascending-id
  order stay exactly as `unique_node_names` defines them; D1 only moves where
  the rule is applied.
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

- **Duplicate and paste** (`DuplicateNodeCommand`, `PasteNodesCommand`, and
  the Rust `duplicate_node` / `paste` entry points they wrap) rename a copied
  node to the first free `name_2`, `name_3`, … in the destination scope. A
  paste of a whole selection assigns names in ascending source-id order, the
  same order `unique_node_names` would have used, so the result is
  byte-identical to what the text format already printed for such a network.
  Redo restores the *renamed* spelling.
- **Load-time normalization**: the loader's existing "assign names to nodes
  without one" pass grows a second step that renames duplicates with the same
  rule. Existing files heal themselves on first open; the migration is
  invisible in the text format because the text already showed the suffixed
  names.
- **Text-format editor**: a `--replace` already assigns the suffixed spelling;
  incremental edits create nodes with the name the statement gives, which the
  editor already rejects when taken in that scope.
- **`unique_node_names` stays** as the serializer's name source and becomes a
  no-op safety net, guarded by a debug assertion (or a test on the corpus)
  that it never has to rename anything on a loaded network.

The invariant: *within one scope, `custom_name` is unique, and the text
format prints it verbatim.* With it, the GUI can show and edit
`custom_name` directly and the header, the property panel, the Text tab, the
Layout tab and every AI message agree by construction. No new view field.

**D2 — Header tooltip: name first, type second.**
Both header `Tooltip`s become `` `xray1` · xray `` (text name, middle dot,
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
- Validation is the D1 invariant and nothing more: the trimmed name must be
  non-empty and not already used by another node in the same scope. A
  conflict shows an inline error under the field and leaves the stored name
  untouched; the text is never silently suffixed, since the user typed it on
  purpose.
- The rename goes through a new API `rename_node(scope_path, node_id,
  new_name) -> Result<(), String>` (scope-aware like every node-data setter,
  `rust/AGENTS.md`), which validates in Rust, writes `custom_name`, and pushes
  a `RenameNodeCommand` (id-keyed, stores old and new name) so it is one
  Ctrl+Z step. It marks the project dirty and triggers a *view* refresh only —
  no re-evaluation, since no evaluated state depends on the name.
- The Text tab re-renders with the new statement name; the AI's next `query`
  shows it. The AI History diff of the *next* AI edit shows the node as
  removed-and-added under its old and new names (D11).

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
case-insensitive substring on the text-format name path; results are sorted
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
and composing `map4/e1` paths with the same joiner `error_node_path` uses —
the spelling `query` prints and the AI uses. The active network's matches
come first. Flutter does no name computation of its own. An empty query
returns every node of the active network, so opening the picker doubles as a
name directory.

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
Two name-keyed structures exist, both in the text world, and a rename is
visible to both as delete + create of the same node id:
- the layout identity snapshot (`doc/design_incremental_layout.md` D14)
  matches nodes by name path *across one text edit*, so a node renamed in the
  GUI and then touched by the next AI edit is "new" to that edit's layout
  pass — the same thing that happens when the AI renames it. Acceptable; the
  layout log makes it visible.
- the AI History diff (`doc/design_ai_edit_history.md`) is a text diff, so
  the rename shows as a removed and an added statement. Recording a GUI
  rename as an activity entry ("renamed `union7` → `chassis`") is a
  follow-up.

## Phases

### Phase 0 — Unique names at the source (D1)

1. A shared `NodeNetwork::unique_name_for(desired, scope)` (or promote
   `node_inlining::unique_name`) as the one suffixing helper.
2. Duplicate and paste rename on collision, in ascending source-id order;
   their undo commands restore the renamed spelling on redo.
3. Loader normalization of duplicate names, beside the existing backfill.
4. Tests: duplicate → `x_2`; paste two `x` → `x_2`, `x_3` in the right order;
   a fixture with duplicates loads unique; and the corpus round-trip test
   (`text_format_roundtrip_corpus_test.rs`, demolib + private file) asserts
   `unique_node_names` renames nothing after load. Existing tests that build
   duplicates on purpose (`duplicate_node_names_are_written_uniquely…`) move
   to constructing them below the API, or are retired if the state is no
   longer reachable.
5. `text_format/AGENTS.md` "Round-trip invariants": the uniqueness bullet
   changes from "the GUI does not keep names unique" to "names are unique at
   the source; `unique_node_names` is the serializer's read and a safety
   net".

### Phase 1 — Tooltip; editable property-panel strip; copy

1. Header tooltips (D2), both sites — `custom_name` is already on the view.
2. `rename_node` API + `RenameNodeCommand` (D3), scope-aware, with tests:
   rename, reject empty, reject taken-in-scope, allow the same name in a
   sibling body, undo/redo, dirty flag.
3. Name strip in `node_data_widget.dart` (D3) with the editable field and
   *Copy name*; *Copy node name* in the node context menu (D9).
4. Reference guide: `doc/reference_guide/ui.md` — *Node network editor
   panel* (hover) and *Node Properties Panel* (strip, rename, copy).

### Phase 2 — Title mode

1. `NodeTitleMode` in `NodeDisplayPreferences` (Rust, twin, Dart), persisted
   (D4). A preferences round-trip test in the existing preferences test file.
2. Header rendering honours the mode for every node kind listed in D4;
   footprint untouched (D5) — assert in a Rust test that the node footprint
   is identical under both modes, since that is the invariant that keeps the
   toggle a repaint.
3. Display-panel radio group, View-menu checkable item, Ctrl+Shift+N (D6).
4. Reference guide: *Display Preferences Panel* gets a *Node titles*
   subsection; *Menu Bar* lists the item and shortcut.

### Phase 3 — Find node

1. `find_nodes_by_name` in the structure-designer crate + api twin (D8),
   with tests: active-only vs all-networks, body paths, `_2` names, ordering
   (exact, prefix, substring), empty query.
2. Flutter picker overlay (D7): field, list, keyboard handling, *all
   networks* switch, landing through `jumpToNode`.
3. Entry points: *Edit → Find node…* (Ctrl+F), tab-strip icon.
4. Widget test in `test/` for the picker's filtering and keyboard handling
   (it takes a plain list of matches, so it needs no kernel).
5. Reference guide: *Navigating in the node network editor panel* and *Menu
   Bar*.

### Manual walkthrough (human, after each phase)

The judgement calls here are visual: does the name strip crowd the panel,
does the ellipsized name in Name mode still read at the default zoom, does
the picker land where the eye expects. Per the project's rule for thin editor
UI, this is a manual walkthrough on a real design, not an integration test:
open `layout_test_scratch.cnnd` → `0_precursor_tests_incomplete_V5+covers`,
have the AI name three nodes, find them with each of the three surfaces,
rename one of them in the strip and confirm the AI's next `query` uses the
new name.

## Deferred / follow-ups

- Clicking a node name in the AI History *Layout* / *Diff* tabs jumps to that
  node (D8's resolver makes this a one-liner later).
- Fuzzy matching and matching on type / expression text in the picker.
- The picker as a general command palette (networks, actions).
- A GUI rename recorded as an AI History activity entry, and the layout
  identity snapshot following renames (D11).
- Renaming from the node header in place (double-click), once the strip has
  proven the semantics.
