# Design: Multiple open documents (tabs)

Related issues: #434 (load two or more files at once), #242 (its older
duplicate). Builds on `doc/design_library_linking.md`, whose §12 anticipated
this design.

Today atomCAD has exactly one open document. *Open library file* (library linking
Phase 4) therefore **replaces** the design with the library and offers
*File > Back to `host.cnnd`*, which reopens the host from disk. This design
lets several `.cnnd` files be open at once, one per **tab**, with one of them
active.

**Terms.** A **document** is one open `.cnnd` file (or an unsaved *Untitled*
design) together with everything that belongs to editing it: its registry,
undo stack, dirty flag, file path, navigation history. The **active**
document is the one shown in the editor and viewport; every other document is
**parked**. **App state** is state that belongs to the application session,
not to any document (preferences, the console, the profiler switches). In Rust
a document is a `StructureDesigner`, called a **designer** below.
**D1…D11** are this document's decisions (§4); a decision of the library
linking design is always written out as *library linking D7*. **P1…P4** are
the phases of §9.

---

## 1. Motivation

mechadense's workflow with linked libraries is an edit–test loop: change a
network in `demolib.cnnd`, see what it does in the design that uses it, repeat.
Today every iteration is:

1. *Open library file* — the design is closed.
2. Edit the library, save.
3. *Back to `host.cnnd`* — the design is reopened from disk.
4. Look at the result. To change the library again, go to step 1.

Each round trip discards the design's **undo history**, its selection and
anything unsaved (the user is asked to save or discard first), and pays a full
load. The library's undo history is discarded on the way back too. With tabs
the loop becomes: edit the library tab, save, click the design tab — the
change is applied on activation (library linking D7 detects it), and both undo
histories are intact. The one exception is library linking D9: if the design
has redo history, the change is *held* rather than applied, and the user
applies it with *Refresh* (D6).

Outside of library authoring, comparing two designs side by side (by
switching) and keeping a scratch design open next to the real one are what
users of every other CAD and code editor expect.

## 2. Goals and non-goals

Goals:

- Several documents open at once, each with its own undo history, dirty flag,
  file path, camera and node-canvas view, selection and navigation history.
- *Open library file* opens the library in a tab (or switches to it).
- A library saved in one tab is picked up by every document that links it, by
  the existing change detection (library linking D7) — no new mechanism.
- **Copy in one tab, paste in another.** In particular, moving nodes between a
  design and a library it links (in both directions), with the names they
  refer to rewritten to what the target calls them.
- No change to the signatures of the existing ~600 API functions (except
  paste, which gains a refusal message), and none to their other Dart call
  sites.
- A failed open, a refused switch or a closed tab never loses or mixes up
  user work.

Non-goals (for this design):

- **Two documents visible at once** (split view, multiple windows). There is
  one viewport and one renderer. See §8 for what this would take.
- **Pasting what the target cannot see.** A paste that needs a network or
  record type the target document has no access to is refused with the list of
  missing names; it does not copy the definitions along (§4 D9). Carrying
  dependencies with the nodes is a possible later extension.
- **Restoring the open tabs on the next start.**
- **Seeing a library's unsaved edits in a document that links it.** The file
  on disk is the contract between documents, exactly as it is between
  sessions (D6).
- **Headless CLI mode** (`atomcad-cli` run without the GUI) is unaffected: it
  has one document, as today.

## 3. User-visible model

- Open documents are listed as **tabs**. Each tab shows the file name
  (*Untitled* for a new design), a `*` when dirty, and a close button. Hovering
  shows the full path. Clicking a tab activates it, and dragging one reorders
  the tabs. A preference sets **where the tabs are placed** (D10):
  - **Vertical, left of the viewport** (the default): a list, one document per
    row, docked to the viewport's left edge and as tall as the viewport, the
    mirror image of the properties panel on the viewport's right edge. A
    divider sets its width.
  - **Horizontal, above the node network editor**: a strip of tabs, like a
    browser's.
- **File > New** opens a new *Untitled* tab.
- **File > Open…** and **Open Recent** open the file in a **new tab**, except
  when the active tab is a *pristine* Untitled design (no path, not dirty, empty
  undo stack) — then the file replaces it, so starting the app and opening a
  file does not leave an empty tab behind.
- Opening a file that is **already open** switches to its tab instead.
- **File > Close Tab** (`Ctrl+W`) closes the active tab, and a tab's close
  button closes that tab; either asks to discard unsaved changes first.
  Closing the last tab leaves one fresh Untitled tab: there is always an
  active document.
- `Ctrl+Tab` / `Ctrl+Shift+Tab` cycle through the tabs.
- **Open library file** (right-click a linked network, record type or library
  folder) opens the library in a tab, or switches to it if it is open. The
  library is fully editable there — it is an ordinary document. *File > Back to
  `host.cnnd`* is removed: the host is still open in its own tab.
- **Copy and paste work across tabs.** There is one clipboard. Pasted nodes
  refer to the same definitions they referred to where they were copied: an
  instance of `demolib.slab` copied in the design becomes an instance of
  `slab` when pasted into `demolib.cnnd`'s own tab, and the reverse. Relative
  file paths in pasted nodes are re-spelled so they still point to the same
  file. When the target cannot see a definition the nodes need, the paste is
  refused and a dialog names what is missing and why (§4 D9).
- **Quitting** with unsaved changes lists every dirty document in one dialog.
- **Undo, Save, Save As, the CLI and the AI assistant** act on the active
  document. The CLI's `load` and `new` behave like *File > Open* and
  *File > New*: they open a tab (D8).
- **Preferences, the Console, the profiler, Recent Files and the remembered
  file-dialog folders** are application-wide and do not change when switching.

## 4. Decisions

### D1 — A document is a `StructureDesigner`; the active one is swapped in

`StructureDesigner` already *is* the document: besides the registry it owns
the undo stack, `file_path`, `is_dirty`, the navigation history, the active
network and record def, the error snapshots and every pending-drag state.
Multiplying only the `NodeTypeRegistry` would leave one undo stack shared by
several registries, and undo commands refer to networks by name — undo in one
tab would rewrite a network in another.

`CADInstance` keeps its `structure_designer` field, which always holds the
**active** document. Parked documents live beside it:

```rust
pub struct CADInstance {
    pub structure_designer: StructureDesigner, // the active document
    pub documents: DocumentSet,                // parked documents + tab order
    pub renderer: Renderer,
}
```

Switching tabs swaps a parked `StructureDesigner` into that field (§5.2). The
~600 API functions all reach the document as `cad_instance.structure_designer`
inside `with_[mut_]cad_instance`, so **they operate on the active document
with no change**. Rejected alternative: a `document_id` parameter on every API
function (§8).

### D2 — App state is moved across the swap, and the compiler enforces the split

Every `StructureDesigner` field is classified as **per-document** or **app
state** (§6). App state lives on whichever designer is active and is handed
over at every swap by one function, `StructureDesigner::hand_over_app_state(&mut
self, to: &mut StructureDesigner)`.

That function begins with an **exhaustive destructure** of `self` (no `..`), so
adding a field to `StructureDesigner` is a compile error until the author
decides which side it is on. A forgotten field is the one bug class this design
is prone to — state silently leaking between tabs or silently lost on a switch —
and a compile error is the only guard that cannot itself be forgotten.

Rejected: moving app state out of `StructureDesigner` into its own struct. It
is the cleaner shape, but `preferences` alone is read throughout evaluation and
tessellation, and threading a second argument through those paths is a large
refactor for no user-visible gain. It can be done later without changing this
design's behaviour.

### D3 — Parked documents keep their content, drop their caches

A parked document keeps everything a user could lose: its registry, undo
stack, dirty flag, selections, cameras and canvas views (both are stored per
network already — `camera_settings`, `canvas_viewport`).

It drops what can be recomputed: the evaluated scene
(`last_generated_structure_designer_scene`) and the CSG conversion caches.
Parking a heavy atomic design therefore frees its evaluated structures and
meshes, and the memory cost of an open tab is close to the memory cost of its
node data (imported `.xyz` atoms, `atom_edit` diffs), which cannot be dropped.

The price is that **activating a document re-evaluates it**: one full refresh,
as after opening the file but without the disk read and migration. With the
evaluation memo this is well under a second for typical designs; for the very
large atomic demos it can take seconds. That is accepted for this design.
Keeping parked scenes so activation skips the evaluation is deferred; if it is
ever wanted, it is a contained change to D3.

### D4 — Switching is refused during an open interaction

Every operation that changes the active document — `activate_document`,
`new_document`, `open_document`, and `close_document` of the active tab — is
refused while `StructureDesigner::open_interaction()` reports an open
interaction (a node, gadget or `atom_edit` drag; a comment edit; a zone
resize; a node-data drag — the set library linking P4 established). Swapping
during one would park a half-finished undo step in one document and end it in
another. Flutter disables the tabs during drags as well, but Rust must not
depend on that (the same rule as library linking §5).

A `mechanosynth_edit` metadata run (`pending_step_metadata_edit`) is
deliberately **not** an open interaction: it is keystroke coalescing keyed on
the undo push count, and `open_interaction()` leaves it out on purpose. It does
not block a switch. Instead `park` clears it. Otherwise, after a switch away and
back with no undo push in between, the next keystroke would merge with the
keystrokes typed before the switch into one undo step.

### D5 — One document per file

A file is open in at most one tab. Identity is the **canonicalized absolute
path** (case-insensitive on Windows). Opening an open file switches to it.
**Save As onto a path open in another tab is refused** with "`x.cnnd` is open
in another tab — close it first". Two in-memory copies of one file would make
"which one did I save last" a question with a wrong answer.

### D6 — Documents communicate only through files

A document that links a library sees the library **as it is on disk**, never
as it is in another tab's memory. Each document mounts its own copy of each
library it links (as today; library linking D1). Library linking §12
anticipated sharing mounts by absolute path; with D1 of this design that is
not needed — a library open in its own tab is simply a different document from
the mounts of it.

Consequences, all of them existing behaviour:

- Saving library `L` in tab B changes the file; tab A detects it with
  library linking D7's check, which runs as part of activating A (§5.2), and
  refreshes with call-site repair. The refresh is an undoable step in A
  (library linking D9).
- **Except while A has redo history.** Then the check *holds* the change
  instead of applying it, so that the redo history survives (library linking
  D9): the mount is marked changed on disk, and the activation's report says
  so. The user applies it with *Refresh* on the library folder, which truncates
  redo like any edit. This is easy to hit in the edit–test loop (undo in the
  design, edit the library, come back), and the activation report is where the
  user learns about it. Tabs do not change this rule.
- Unsaved edits to `L` in tab B are invisible to A until saved.
- A library open in a tab is editable there and read-only in every document
  that links it.

The one exception is the clipboard (D9), which carries nodes, not
definitions, and checks that the definitions match on both sides.

### D7 — Tab open, close and switch are not undo steps

Each document's undo stack is its own; `Ctrl+Z` undoes in the active document.
Opening, closing and switching tabs are not recorded anywhere.

### D8 — The CLI and the AI assistant act on the active document

The AI assistant's HTTP server calls the same API as the UI, so it edits the
active document. An agent working while the user switches tabs would then edit
a different document than it read. P4 adds a guard: CLI requests may carry
`--document <path-or-id>`, and a request naming anything but the active
document fails with a message naming the active one. A path names a saved
document. An Untitled document has no path, so it is named by its document id,
which a new `atomcad-cli documents` command lists (id, path or *Untitled*,
dirty, active). Agents (the `atomcad` skill) pass `--document` on every
request.

**The guard holds only if nothing can switch in the middle of a request.** The
AI server's handlers are Dart `async` functions, and a tab switch (a click,
`Ctrl+Tab`) can run at any `await`. A guard checked when the request arrives
does not protect API calls made after a later `await`. Today's `/load` is an
example: it checks `isDesignDirty()`, then awaits `file.exists()`, then
loads, so a switch in between would discard a dirty document that was never
checked. The rule, for every handler: **do all asynchronous work (reading the
body, file-system checks) first; then run the guard, any other precondition
checks and every API call of the request in one synchronous stretch, with no
`await` between the guard and the last API call.** The tabs and the API run on
the same isolate, so that stretch cannot be interleaved with a switch.

**`load` and `new` open tabs, like File > Open and File > New.** The CLI's
`/load` and `/new` call `load_node_networks` and `new_project` today, and these
overwrite the active designer **in place**. With tabs that would put a
different design behind an unchanged `DocumentId`. D5 (a second copy of a file
already open in another tab), D9 (the clipboard's "same document, no checks"
path pasting names from the file that was replaced) and D11 (the
document-keyed subtree not rebuilding) all assume a `DocumentId` names one
document for its whole life. So:

- The AI server's `/load` and `/new` call the model's `openDocument` and
  `newDocument` (§5.3; `/new` without direct editing, as today), which go through `withDocumentSwitch` like the menu
  items. Loading an already-open file activates its tab (D5), and a pristine
  Untitled tab is replaced, as in §3. Nothing is discarded, so the
  unsaved-changes refusal is dropped, as it is for File > Open. `--force` is
  still accepted and does nothing. The response carries the new document's id
  and path so the agent can pass `--document` from then on.
- As a backstop for every other caller, the Rust in-place functions
  (`load_node_networks`, `new_project`, `new_project_direct_editing`) give the
  active designer a **fresh `DocumentId`** when they replace its content, and
  `load_node_networks` refuses a path that is open in another tab. Headless
  mode (no `DocumentSet`) is unaffected.

The AI edit log (`ai_edit_log`) is per document, since its entries refer to
nodes of that document's networks.

### D9 — One clipboard; names are translated through the file that owns them

**The clipboard is app state.** A per-document clipboard is not a neutral
default: `Ctrl+C` in A followed by `Ctrl+V` in B would paste whatever B copied
earlier. So even without translation the clipboard would have to move to app
state.

**What a pasted node can refer to by name** is already enumerated, because
library linking needed the same list for frozen nodes (library linking §8,
`unresolved_mount_ref`):

- a custom network, through `node_type_name` (instances, and through them the
  `-1` function pin);
- a record def, through a schema/target string (`record_construct`,
  `record_destructure`, `product`) or a `Named` record embedded in a
  `DataType` in node data (parameter types, HOF element types, `closure` /
  `apply` type args, `expr` signatures, …) — the single enumeration is
  `collect_record_refs_in_node`;
- a data file, through a relative path in node data (`file_paths()`), which
  is relative to the folder of the file owning the network (library linking
  D8).

Built-in node types and built-in record defs are the same in every document
and need nothing.

**Origin, recorded at copy.** Along with the nodes the clipboard stores:

- the source `DocumentId`;
- the **base folder** of the network copied from
  (`library_refresh::base_dir_of`: a linked network's base is its library's
  folder, a local network's is the document's, none for an Untitled one);
- for every referenced user name, its **owner** — the canonical path of the
  file defining it and its name *inside that file*. A name under a mount is
  owned by the innermost mount's file, with the mount path stripped
  (`demolib.common.bar` → `common.cnnd`, `bar`); a local name is owned by the
  source document's file (none if Untitled);
- for each, the definition's **interface** as library linking D13 records it:
  per parameter `param_id`, name and type, plus the outputs, for a network;
  per field `FieldId`, name and type for a record def.

**Paste into the same document** keeps today's behaviour exactly: no name
changes, no checks — only the path rebase below.

**Paste into another document `T`** maps every owner `(file, name)` to `T`'s
name for it:

1. `file` is `T`'s own file → `name`, which must be local in `T`;
2. `T` mounts `file` (matched by canonical path, direct or nested; a direct
   mount wins, then the shortest mount path) at `P` → `P.name`;
3. otherwise the name is **unreachable** from `T`.

Then the definition `T` has under that name must have the **recorded
interface**. Two files that should agree can disagree when one side is stale —
the library tab has unsaved changes, or `T`'s mount has not been refreshed —
and a paste across a mismatched interface would leave nodes whose arguments
were built for a different pin layout.

**All or nothing.** If any name is unreachable, missing or mismatched, nothing
is pasted. The dialog lists each name with its reason and what to do:

- "`helper` is defined in `host.cnnd`, which `demolib.cnnd` does not link."
- "`demolib.slab` has different parameters in this design than where it was
  copied — save `demolib.cnnd`, or refresh it here."

A partial paste would produce frozen nodes, which are worse than no paste.

**Rewriting.** On a successful cross-document paste the clipboard snapshot is
rewritten before insertion: `node_type_name` to the mapped name, and every
record reference through a per-node form of
`rewrite_record_names_in_registry_with`. The registry-wide function is split
so that it calls the per-node one. That keeps a single rewrite enumeration
beside the single collect enumeration, as `collect_record_refs_in_node`'s doc
requires.

**Paths are rebased on every paste, same document included.** Every pasted
node's `rebase_file_paths` re-spells relative paths from the origin's base
folder to the target network's (the target is always local, so it is the
target document's folder). A path becomes absolute when the target is
Untitled or no relative path exists, as in *Make local copy*. Today the only
caller of `rebase_file_paths` is *Make local copy*
(`library_link_ops.rs`), so copying nodes out of a linked network into a local
one in the same design keeps relative paths that then resolve against the
wrong folder. P2 confirms this with a test and fixes it. *Duplicate into my
file* is checked for the same bug.

**Frozen nodes** paste within their own document as today. They are refused
across documents: their definition does not resolve in the source, so there is
no interface to check.

**Clipboard upkeep.** Renaming or deleting a network or record def, and
renaming a library alias, already rewrite or clear the clipboard
(`structure_designer.rs`, `library_link_ops.rs`). These keep doing so only
when the clipboard's origin is the document being edited. A rename in some
other document must not rewrite names that belong to the source. The sites
include the undo and redo paths: `structure_designer.rs` rewrites the
clipboard in the rename code that undo and redo of a rename also run.
`StructureDesigner` gains a `document_id` field, and every site goes through
one helper, `own_clipboard_mut() -> Option<&mut Clipboard>`, which returns the
clipboard only when its origin is this document. No site touches
`self.clipboard` directly, so the guard cannot be forgotten at one of them.
After such a rewrite the recorded owners are recomputed from the source's
registry. In every other respect the origin is a snapshot: closing
the source, Save As or a later refresh does not change it, and the interface
check at paste time catches whatever drifted.

### D10 — Tab placement is a preference; vertical is the default

There are two placements, because the two main users of today prefer different
ones:

- **Vertical**, docked to the viewport's left edge (mechadense's request). Each
  row has room for a long file name, and a design with several libraries open
  does not run out of width.
- **Horizontal**, above the node network editor, like a browser.

The default is **vertical**, because mechadense is the main user today.

The setting is a persisted application preference, not view state: it is a
standing choice, unlike panel folding or the layout orientation, which are
changed in passing. It lives in a new `InterfacePreferences` group
(`document_tab_placement: DocumentTabPlacement { LeftOfViewport,
AboveNetworkEditor }`, `#[serde(default)]` → `LeftOfViewport`). It goes in a
new group because the existing `LayoutPreferences` is about node-network
auto-layout, not the window. It is shown in the Preferences dialog under a new
*Interface* section and takes effect immediately, without a restart.

Both placements show the same tabs and offer the same gestures (click, close,
drag to reorder, middle-click close, tooltip). Only the arrangement differs.
Rules that follow from the window layout (`doc/reference_guide/ui.md`,
*Arranging the window*):

- **Vertical**: the list is taken off the viewport's left edge, after the
  properties panel is taken off its right edge. The list is as tall as the
  viewport, so folding the network editor (`Ctrl+2`) makes it full height,
  as it does the properties panel. The width divider is view state, like the
  other dividers.
- **Horizontal**: whenever the network editor is not shown — folded with
  `Ctrl+2`, or hidden because the document is in direct editing mode — the
  strip moves to the top edge of the viewport instead of disappearing with
  the editor. Otherwise the only way to switch documents would be `Ctrl+Tab`.
  Direct editing mode is per document (§6), so the strip can move on a switch.
- **Presentation Mode** (`Ctrl+0`) hides the tabs in either placement, since
  it exists to leave only the viewport. `Ctrl+Tab` still switches documents.

### D11 — Flutter flushes pending edits before a switch and rebuilds per document

Rust keeps documents apart (D1, D4). Flutter could still mix them up, in two
ways.

- **Late commits.** Input fields write their value when they lose focus
  (`lib/inputs/float_input.dart` and its siblings, and several node-data
  editors). The write goes through the model to the API, which acts on
  whichever document is active *at that moment*. If a switch happens first —
  `Ctrl+Tab` while a field has focus, or a click on a tab that does not take
  focus on desktop — the pending value is written into the newly active
  document, on whatever node there has the same network name and node id.
  Node ids restart in every network and most designs have a `Main`, so such a
  node almost always exists. The result is a silent edit of the wrong design.
- **Reused widget state.** Widgets are keyed by network name and node id
  (e.g. `'${network}|${scope}|${node.id}'` in `node_data_widget.dart`), and
  those keys collide across documents for the same reason. Flutter would keep
  A's `State` — a half-typed text, a scroll position, an expanded section — on
  B's node.

The decisions:

- **Every change of the active document goes through one Flutter function**,
  `withDocumentSwitch(action)`. `activateDocument`, `newDocument`,
  `openDocument` and `closeDocument` of the active tab all pass their API call
  to it as `action`. It (1) unfocuses the primary focus, (2) waits for the end
  of the current frame, so the focus listeners have run and their writes have
  reached the *outgoing* document, and (3) only then runs `action`. Keyboard
  shortcuts, menu items and tab clicks all reach the API through these model
  methods; none of them calls it directly. Switches are **queued**: one
  requested while another is waiting for its frame runs after it, never
  interleaved. A relative target (`Ctrl+Tab`'s "next tab") is resolved when the
  switch runs, not when it is requested.
- **The document-dependent UI is keyed by the document id**: the node network
  editor, the properties panel, the user-types panel and the display panel sit
  under one `KeyedSubtree(key: ValueKey(activeDocumentId))`. A switch rebuilds
  all of them from scratch, so no `State` survives from one document into
  another. Session UI (the menu bar, the tabs, the console, the camera control
  panel) sits outside that subtree.

## 5. Architecture

### 5.1 Rust — `atomcad-structure-designer`

New module `document_set.rs`:

```rust
pub struct DocumentId(pub u64);       // never reused within a session

pub struct DocumentSet {
    active: DocumentId,
    order: Vec<DocumentId>,           // tab order, includes the active id
    parked: HashMap<DocumentId, StructureDesigner>,
    next_id: u64,
}
```

It knows nothing about the renderer. It owns every rule about which document
is active and which are parked. The active designer itself stays in
`CADInstance` (D1), so the operations that change the active document take it
as a `&mut` slot:

- `activate(&mut self, active: &mut StructureDesigner, target) ->
  Result<Option<RefreshReport>, SwitchRefused>` — everything in the swap except
  the renderer (§5.2, steps 1–4 and 6–7a): refuse during an open interaction
  (D4), take `target` out of `parked`, `hand_over_app_state`, `mem::swap`,
  `park` the outgoing designer and put it back under its id, run the
  dependency check and `mark_full_refresh`. It does **not** evaluate. The
  caller refreshes. Its doc comment states the raw-pointer rule of §5.2.
- `new_document(&mut self, active, direct_editing: bool) -> Result<(),
  SwitchRefused>` and `open(&mut self, active, path) -> Result<OpenOutcome,
  OpenError>` — the whole of `new_document` / `open_document` in §5.2, minus
  the renderer: D4 refusal, D5 lookup (already open → `activate`), detached
  load, pristine replacement, insert, activate. `OpenError` is either the D4
  refusal or the load error. `OpenOutcome` carries `already_open`, the
  post-load reports and the activation's report.
- `insert(&mut self, designer, after: DocumentId) -> DocumentId` — adds a
  parked document to the tab order and assigns its id (used by the two above).
- `close(&mut self, active: &mut StructureDesigner, id) ->
  Result<Option<RefreshReport>, SwitchRefused>` — a parked tab is simply
  dropped (`Ok(None)`). The active tab is refused during an open interaction
  (D4); otherwise it first activates its neighbour (the tab to the right, else
  to the left), returning that activation's report, and is then dropped. The
  last tab is replaced by a fresh Untitled.
- `renumber_active(&mut self, active: &mut StructureDesigner)` — gives the
  active designer a new id after its content was replaced in place (D8).
  Flutter stash entries under the old id are dropped on the next
  `refreshFromKernel`, because that id is no longer listed.
- `path_open_elsewhere(&self, active, path) -> Option<DocumentId>` — the D5
  refusal shared by Save As and the in-place load backstop (D8).
- `check_guard(&self, active, spec: &str) -> Result<(), String>` — D8's
  `--document` match (canonical path or id, against the active document),
  with the message naming the active one.
- `tabs(&self, active) -> Vec<DocumentTab>` (the domain twin of
  `APIDocumentTab`: id, display name, path, dirty, active, in tab order),
  `move_to(id, index)`, `find_by_path(canonical) -> Option<DocumentId>`.

**Every rule lives here; the FFI wrappers stay thin.** The functions of §5.2
need the global `CADInstance`, whose renderer needs a GPU, so no Rust test can
call them. `library_links_api_test.rs` notes the same limit. Anything a wrapper
decides by itself is therefore untested. So each wrapper does exactly one
`DocumentSet` (or `StructureDesigner`) call, plus the renderer work (camera,
tessellation), plus the conversion to `API*` types through a view builder in
`view_builders.rs`, which *is* testable. A refusal, a lookup or a fallback that
lives in a wrapper is a review finding.

On `StructureDesigner`:

- `hand_over_app_state(&mut self, to: &mut StructureDesigner)` — D2. Moves
  (`mem::take`) the logs and clones the settings; re-applies the cache
  capacities from the preferences to `to`'s evaluator and scene, since the
  preferences may have changed while `to` was parked.
- `park(&mut self)` — D3: replaces the scene with an empty one (same
  invisible-node cache capacity) and clears the CSG caches. Clears
  `pending_step_metadata_edit` (D4). Asserts no open interaction.
- `is_pristine()` — no path, not dirty, empty undo stack (§3).
- `document_id: DocumentId` — set by `DocumentSet` when the designer joins it,
  and replaced with a fresh id when an in-place load or new project replaces
  its content (D8); `DocumentId(0)` in headless mode, where there is no
  `DocumentSet`. Read by the clipboard (D9) and, through `list_documents`, by
  the Flutter subtree key (D11).

A new or opened document starts as a plain `StructureDesigner::new()` and
receives the app state through the same `hand_over_app_state` when it is
activated. There is no second transfer path to drift from the first.

New module `clipboard.rs` (D9). `clipboard: Option<NodeNetwork>` becomes
`Option<Clipboard>`:

```rust
pub struct Clipboard {
    pub nodes: NodeNetwork,
    pub origin: ClipboardOrigin,
}
pub struct ClipboardOrigin {
    pub document: DocumentId,
    pub base_dir: Option<PathBuf>,
    pub owners: BTreeMap<String, Owner>,   // referenced user name → owner
}
pub struct Owner {
    pub file: Option<PathBuf>,            // canonical; None = Untitled source
    pub name: String,                     // the name inside that file
    pub interface: Interface,             // library linking D13's shape
}
```

- `Clipboard::capture(designer, network_name, nodes)` — what
  `copy_selection` calls. It walks the nodes (bodies included) for references,
  resolves owners through `library_links.mount_containing` and the mount's
  `abs_path`, and records interfaces.
- `Clipboard::for_target(&self, target: &StructureDesigner, target_network) ->
  Result<NodeNetwork, PasteRefusal>` — the translated, rebased snapshot both
  `paste_at_position` and `paste_at_position_scoped` paste from. It replaces
  their inline snapshot step. `PasteRefusal` holds one `(name, reason)` line
  per problem.
- The paste methods return `Result<Vec<u64>, PasteRefusal>`. The undo command
  (`PasteNodesCommand`, or the zone-body command) is unchanged: it snapshots
  the nodes as inserted.

### 5.2 Rust — `rust/src/api/`

`CADInstance` gains `documents: DocumentSet` (D1). `initialize_cad_instance_async`
registers the initial designer as document 1.

**The swap** (`activate_document_internal(instance, target)`). Steps 1–4, 6
and 7a are `DocumentSet::activate` in the domain crate; the API layer adds only
the renderer (steps 0, 5 and 7b):

0. (API) If the renderer's camera differs from the outgoing network's stored
   `camera_settings`, store it there (normally it is already current). This
   is **not** `sync_camera_to_active_network`, which also marks the design
   dirty: a switch must never dirty a document.
1. If `target` is active: nothing to do. If `open_interaction()` is `Some`:
   refuse (D4).
2. `incoming = take(target)`; `active.hand_over_app_state(&mut incoming)`.
3. `mem::swap(active, &mut incoming)`; `incoming` now holds the outgoing
   document.
4. `incoming.park()`, put it back under its id, mark `target` active.
5. (API) `apply_camera_settings(renderer, active network's camera_settings)`.
6. `check_dependencies()` — library linking D7's check, run *before* the
   refresh, so a library saved in another tab is applied and the document is
   evaluated once, not twice. Its report is returned to Flutter.
7. a. `mark_full_refresh()`; b. (API) `refresh_structure_designer_auto(instance)`.

Nothing after step 1 can fail, so a switch either happens completely or not
at all.

**Why the swap is safe, and the one assumption it rests on.** `mem::swap`
exchanges the inline bytes of the two `StructureDesigner` values. Their heap
data (networks, atoms, undo stack, caches) does not move; only the owning
handles change hands. This is an ordinary Rust move: there is no `Pin` or
self-referential state in the structure-designer crate or `rust/src/api/`, and
the two `&mut` borrows guarantee nothing else observes either value during the
swap. What it relies on is that **no code keeps a raw pointer into the active
`StructureDesigner` across API calls**: such a pointer would point at the
other document after a switch. The raw pointers that exist today
(`ai_text_edit.rs`, `structure_designer.rs` and `structure_designer_api.rs`,
all to split a borrow of `node_type_registry` for `validate_network`) live
inside one block. The evaluation memo's address-based keys
(`eval_frame_key`) point into the heap and are rebuilt every pass. The doc
comment of `DocumentSet::activate` states this assumption, and
`rust/AGENTS.md` repeats it.

**New FRB module** `rust/src/api/structure_designer/documents_api.rs` (must be
added to `rust_input` in `flutter_rust_bridge.yaml`), all `#[frb(sync)]`:

| Function | Behaviour |
|---|---|
| `list_documents() -> Vec<APIDocumentTab>` | Tab order; each `{ id, display_name, file_path, is_dirty, is_active }`. |
| `new_document(direct_editing: bool) -> APIResult` | Fresh designer, set up as `new_project_direct_editing` does when `direct_editing` is true and as `new_project` does otherwise, inserted after the active tab and activated. Refused during an open interaction (D4). File > New passes the active tab's direct-editing mode, as the model's `newProject` does today. The CLI's `/new` passes `false`, as its `newProject` call does today. |
| `open_document(file_path) -> APIOpenDocumentResult` | Already open (D5) → activate it, `already_open: true`. Otherwise build a fresh designer, **load into it while detached**, and only on success insert and activate it. A failed load drops the fresh designer; the active document was never touched. Refused during an open interaction (D4). If the active document `is_pristine()`, the new one takes its place in the tab order and the pristine one is dropped. Carries the same post-load reports as `load_node_networks` (param-id repairs, library report). |
| `activate_document(id) -> APIActivateResult` | The swap. `{ result: APIResult, library_report: Option<APIRefreshReport> }`: an error when refused (D4) or the id is unknown; otherwise what the dependency check of step 6 did, shown by Flutter exactly like a report from the 2 s poll. |
| `close_document(id) -> APIActivateResult` | No dirty check — Flutter asks first. Closing the active tab activates its neighbour first (its `library_report` is that activation's), then drops the closed designer. Closing the last tab leaves a fresh Untitled. Closing the active tab is refused during an open interaction (D4). |
| `move_document(id, new_index)` | Tab reordering by drag. |
| `check_document_guard(spec) -> APIResult` | D8's `--document` check (`DocumentSet::check_guard`), called by every AI-server handler that receives `--document`. The matching rules live in Rust so that they can be tested. |

Changed: `save_node_networks_as` refuses a target path open in another tab
(D5, through `DocumentSet::path_open_elsewhere`). `paste_at_position` returns `APIPasteResult { node_ids, error:
Option<String> }` instead of `Vec<u64>` (D9). It is the only existing
signature this design changes, and its one Dart caller
(`StructureDesignerModel.pasteAtPosition`) shows the error dialog.

Kept: `load_node_networks` (load into the *active* tab, replacing it) and
`new_project*`. Neither Flutter nor the AI server uses them any more (D8); Rust
tests and headless mode still do. When a `DocumentSet` exists, they take a
fresh id for the active designer from it (`DocumentSet::renumber_active`) after
replacing its content, and `load_node_networks` first refuses a path open in
another tab (D5, D8).

### 5.3 Flutter

`StructureDesignerModel` stays **one object**. It is a mirror of the kernel
rebuilt by `refreshFromKernel()`, so after a switch it mirrors the new active
document. Additions:

- `List<APIDocumentTab> documents`, refreshed in `refreshFromKernel()`;
  `newDocument`, `openDocument`, `activateDocument`, `closeDocument`,
  `moveDocument`.
- **Flutter-only per-document state.** Model fields that exist only in Dart
  and describe a document (rather than the session) are stashed per tab in a
  `Map<int, _DocumentUiState>` on switch and restored on return; fields with no
  stash entry fall back to their post-load default. P3 enumerates them;
  candidates seen so far: `activeScopeChain`, `propertyEditorScopeChain`,
  `selectedAiHistorySeq`, `lastLoadLibraryReport`. Where the kernel already
  holds a value, nothing is stashed. Tool choices (`activeAtomEditTool`, bond
  mode, element) are session state and stay put. `backToDesignPath` is removed.
- **`withDocumentSwitch(action)`** (D11): unfocus, wait for the end of the
  frame, then run the API call. `activateDocument`, `newDocument`,
  `openDocument` and `closeDocument` of the active tab go through it. Each of
  them shows the activation's `library_report` (§5.2 step 6) with the same
  snackbar the poll uses. The 2 s poll itself keeps checking only the active
  document.
- **`KeyedSubtree(ValueKey(activeDocumentId))`** around the document-dependent
  UI (D11).

UI:

- The tabs (D10). One `DocumentTabs` model of the gestures (activate, close,
  reorder, middle-click close, tooltip) and two thin layouts over it:
  `DocumentTabStrip` (horizontal, above the network editor, or above the
  viewport while the editor is not shown) and `DocumentTabList` (vertical, a
  dock on the viewport's left edge with its own width divider, built like the
  properties-panel dock). The window layout picks one from
  `preferences.interfacePreferences.documentTabPlacement` and hides both in
  Presentation Mode. Both are disabled while a pointer is down on the canvas
  or viewport (the same pointer count `structure_designer.dart` keeps for the
  dependency check).
- Preferences dialog: a new *Interface* section with a *Document tabs*
  dropdown (*Vertical, left of the viewport* / *Horizontal, above the node
  network*).
- File menu: *New* → `newDocument`; *Open…* / *Open Recent* → `openDocument`;
  *Close Tab* (`Ctrl+W`); *Back to …* removed. The `confirmDiscardChanges`
  calls before New/Open are dropped (nothing is being discarded any more) and
  move to *Close Tab*.
- `library_link_actions.dart`: `openLibraryFile` → `openDocument(mount.absPath)`;
  `backToDesign` and `_openDesign`'s `backTo` parameter are removed.
- Quit (`main.dart`, `setWindowShouldCloseHandler`): one dialog listing every
  dirty document, *Quit* / *Cancel* — the same two choices as today.
- Window title: `atomCAD - <active tab title>`, as today.

## 6. Field classification

Every field of `StructureDesigner` as of this writing. `hand_over_app_state`
encodes this table; §4 D2 makes the compiler keep it complete.

| Field | Side | On park / hand-over |
|---|---|---|
| `node_type_registry` (incl. `library_links`) | document | kept |
| `network_evaluator` | document | CSG caches cleared on park (D3); capacities re-applied from preferences on activation |
| `gadget` | document | kept (recreated by the refresh anyway) |
| `active_node_network_name`, `active_record_def_name` | document | kept |
| `last_generated_structure_designer_scene` | document | replaced with an empty scene on park (D3) |
| `node_display_policy_resolver` | document | stateless today; kept |
| `import_manager` | document | kept |
| `is_dirty`, `file_path` | document | kept |
| `pending_changes` | document | overwritten by `mark_full_refresh` on activation |
| `cli_top_level_parameters` | document | headless only; always `None` in the GUI |
| `navigation_history` | document | kept |
| `clipboard` | **app** (from P2; document in P1) | moved (D9); its origin says which document it came from |
| `document_id` (new) | document | kept |
| `undo_stack` | document | kept |
| `pending_move`, `pending_atom_edit_drag`, `pending_gadget_drag`, `pending_comment_edit`, `pending_zone_resize`, `pending_node_data_drag` | document | must be `None` — guaranteed by D4 |
| `pending_step_metadata_edit` | document | cleared on park (D4: not an open interaction) |
| `direct_editing_mode` | document | kept (a new design takes it from `new_document`'s argument; an opened file has what it was saved with) |
| `cli_access_rules` | document | kept (serialized in the `.cnnd`) |
| `ai_edit_log` | document | kept (D8) |
| `eval_error_snapshots` | document | kept |
| `pending_load_param_id_repairs` | document | drained right after the load; kept |
| `last_eval_profile`, `last_memo_counts` | document | transient; dropped on park |
| `preferences` | **app** | cloned to the incoming designer |
| `gadget_pick_context` | **app** | copied (it describes the viewport and camera) |
| `print_log` | **app** | moved — the Console shows the session |
| `refresh_profiles` | **app** | moved — the profiler shows the session |
| `eval_profiling_enabled`, `eval_self_check_enabled`, `eval_self_check_key_mode`, `eval_memo_enabled` | **app** | copied |

`Renderer` is app state by construction (there is one). Its only
per-document input, the camera, is stored per network and applied in step 5
of the swap.

## 7. Edge cases

- **Library open in a tab, host links it, host is activated.** Library
  linking D7's check compares the disk with the host's `last_seen` stamps; a
  save in the library tab moved them, so the host refreshes — or holds the
  change, if the host has redo history (D6). If the library tab is dirty and
  unsaved, nothing happens — correct by D6.
- **Two hosts link the same library** — each has its own mount and its own
  refresh; nothing is shared.
- **Save As of a library tab to a new path.** Hosts keep linking the old file.
  Same as today.
- **Host's Save As with dependency copy (library linking D11)** writes copies
  next to the new host; it never overwrites an existing file, so a library
  open in another tab is not written to behind its back.
- **The file of an open tab is changed on disk by something else.** Not
  detected for the document itself (only for its dependencies), the same as
  today with one document. A later design may add it; it is unchanged by tabs.
- **Open fails** (missing file, parse error, migration error): the error
  dialog shows; no tab is added; the active document is untouched (§5.2).
- **CLI request arrives during a switch.** The AI server runs on the UI
  isolate and each API call is synchronous, but a handler can be suspended at
  any `await`, and a switch can run there. Requests and switches are therefore
  **not** serialized as a whole; only the synchronous stretch D8 requires is.
  The guard and every API call of a request sit inside that stretch, so a
  request either sees one document throughout or is refused.
- **CLI `load` of a file open in another tab** activates that tab (D8, D5). It
  does not reload the file or replace the active document.
- **Pasting an instance of a library network into that network's own
  definition** (copy `demolib.slab` in the host, paste into `slab` in the
  library tab) creates a recursive network. Existing validation reports it,
  exactly as for a same-document paste of an instance into its own network.
- **Pasting into a document that links the source** (copy in the library
  tab, paste in the host). This is rule 2 of D9: the library's local `slab`
  becomes `demolib.slab`. Unsaved edits in the library tab that changed
  `slab`'s interface make the paste refuse with "save `demolib.cnnd` first".
- **Pasting into the source after it was closed and reopened.** It counts as
  another document (new `DocumentId`). Its own file owns the names, so rule 1
  maps them back, and the interface check confirms nothing changed.
- **Memory.** Each open tab costs its node data (D3). A user opening many
  heavy designs can exhaust memory exactly as they could by building one large
  design; no limit on tab count is imposed.

## 8. Rejected alternative: addressable documents

Every API function takes a `document_id`, and `CADInstance` holds a map of
designers with no notion of "the" one. This is what split view, several
windows, or an agent editing a background tab would require. It means changing
the signature of ~600 API functions, regenerating the bindings and editing
every call in `structure_designer_model.dart` (and making that model one
instance per document) — weeks of churn for capabilities no one has asked for.
Nothing in this design blocks it later: `DocumentSet` and the field
classification are what it would need first.

## 9. Phased implementation plan

**Where each test runs.** There are three automated layers. What none of them
can reach goes to the manual walkthrough.

| Layer | Location | Reaches | Cannot reach |
|---|---|---|---|
| Domain | `rust/crates/atomcad-structure-designer/tests/structure_designer/` (`document_set_test.rs`, `clipboard_test.rs`) | every rule: `DocumentSet`, the swap up to and including the dependency check, evaluation via `StructureDesigner::refresh`, load/save on temp-dir copies of fixtures, clipboard translation, CLI guard matching | the renderer |
| View builders | `rust/tests/structure_designer_api/documents_api_test.rs` | the `API*` shapes: `APIDocumentTab`, `APIOpenDocumentResult`, `APIActivateResult`, `APIPasteResult` | the FFI wrappers themselves |
| Dart unit / widget | `test/` | pure Dart: the stash, `withDocumentSwitch`'s ordering, the tab gesture model, the quit-dialog list, all fed FRB data classes built in the test | anything that calls the Rust library |

Two facts fix this split. The FFI wrappers need the GPU-backed `CADInstance`,
so no test calls them (§5.1, "every rule lives here"). And Dart tests do not
load the Rust library (`doc/testing.md`). The design is shaped so that the
untested parts are thin glue: each wrapper is one domain call plus the
renderer, and each Flutter piece with logic in it takes its API calls as
callbacks. The Flutter smoke test is human-only and is not part of this
story.

Tests that save files (D5, D6, the latent path bug) work on copies of
`rust/tests/fixtures/` in a temp dir, the way `library_links_api_test.rs`
copies its fixture tree. They never write into the fixtures.

The whole existing suite must stay green. The existing tests build a bare
`StructureDesigner` with no `DocumentSet` (`DocumentId(0)`, headless). That
is exactly the single-document path, so they also guard "nothing changes when
there is one document".

### Phase 1 — Rust: `DocumentSet`, the swap, the API

Work: `document_set.rs` with `activate`, `new_document`, `open`, `insert`,
`close`, `renumber_active`, `path_open_elsewhere`, `check_guard`, `tabs`;
the view builders for the new `API*` types; `hand_over_app_state` (exhaustive destructure), `park`,
`is_pristine`; the `document_id` field; `CADInstance.documents`;
`documents_api.rs` + `flutter_rust_bridge.yaml` + codegen; Save As refusal
(D5); the D8 backstop in `load_node_networks` / `new_project*` (fresh id,
refusal of a path open in another tab). The clipboard stays a document field
until P2: it is classified as document state in P1's `hand_over_app_state`
and moves to the app side in P2. New test files are registered in the
crate's harness (`tests/structure_designer.rs`, `#[path]` + `mod`). Update
`rust/AGENTS.md`
(`CADInstance` holds the active document; parked ones live in `documents`;
never keep a raw pointer into the active `StructureDesigner` across API calls,
because a tab switch swaps it, §5.2) and the structure-designer `AGENTS.md` (a
new field needs a side in `hand_over_app_state`; §6 is the reference). The
doc comment of `DocumentSet::activate` states the same raw-pointer rule.

Tests — domain (`document_set_test.rs`). A test "activates" by calling
`DocumentSet::activate` and then `refresh(&get_pending_changes())`, which is
what the wrapper does minus tessellation:
- **Isolation.** Open A and B; edit each; switch back and forth: each
  registry, dirty flag, undo stack and redo stack is the one it was. Undo in B
  does not touch A. Network names that exist in both are independent.
- **A switch changes nothing by itself.** Switching A → B → A with no edits
  and no library change leaves both documents' dirty flags, undo stacks and
  serialized form exactly as they were. This catches a switch that dirties a
  document, as `sync_camera_to_active_network` would (§5.2, step 0).
- **App state follows the session — every field of §6.** Table-driven over the
  app-state rows: set each to a non-default value in A (a preference, a print,
  a refresh profile, each eval flag, the pick context; P2 moves the clipboard
  row from the document side to this side), switch
  to B, and assert that B has it and that the parked A no longer holds the
  moved ones. Then the reverse over the document rows: each keeps its own
  value. The exhaustive destructure makes a *new* field a compile error; this
  table checks that the *existing* fields are on the side §6 says.
- **Per-network view state.** Each network's `camera_settings` and
  `canvas_viewport` in A and B survive a round of switches unchanged. Applying
  them to the renderer (step 5) is the existing `apply_camera_settings`, which
  the walkthrough sees.
- **D5.** Opening an open file activates it; path identity survives case and
  `..` differences; `path_open_elsewhere` refuses Save As onto another tab's
  path, and nothing is written.
- **In-place replacement (D8 backstop).** `load_node_networks` and
  `new_project` give the active designer a new id; `load_node_networks` of a
  path open in another tab is refused and changes nothing.
- **Failed open** (missing file, parse error) leaves the tab list and the
  active document byte-identical (serialize before and after).
- **D4.** For every interaction `open_interaction` reports, `activate`,
  `new_document`, `open` and `close` of the active tab are refused, and the
  interaction then completes into the right document's undo stack. A
  `mechanosynth_edit` metadata run does not refuse; after a switch away and
  back, the next keystroke starts a new undo step.
- **Close.** Closing the active tab activates the right neighbour (left at the
  end); closing the last tab leaves a pristine Untitled; pristine replacement
  on open.
- **D6 end to end.** Host A links L; open L in tab B; edit and save in B;
  activate A: the report names L, A's networks reflect the change, the refresh
  is one undo step in A, and A's call sites were repaired as library linking §7
  specifies. "Evaluated once" is checked structurally: `activate` does not
  evaluate (A's scene is still empty when it returns, and A's networks already
  show L's change), so the caller's single refresh is the only evaluation. The
  same with redo history in A: the change is held, the report lists it under
  `held`, and A's networks and redo stack are unchanged.
- **Park.** A parked document's scene is empty, its CSG cache is cleared and
  its `pending_step_metadata_edit` is `None`; activating it produces the same
  scene as before parking (compare the evaluated outputs).

Tests — view builders (`documents_api_test.rs`): `APIDocumentTab` (display
name *Untitled* and file name, `is_dirty`, `is_active`, tab order) and the
shapes of `APIOpenDocumentResult` and `APIActivateResult`, refusal included.

### Phase 2 — Rust: one clipboard, translated paste

Work: `clipboard.rs` (`Clipboard`, `ClipboardOrigin`, `Owner`, `capture`,
`for_target`, `PasteRefusal`); the clipboard moves to app state in
`hand_over_app_state`; `own_clipboard_mut` (reading P1's `document_id`),
through which every existing clipboard walk goes (network rename/delete,
record-def rename/delete, alias rename, and the undo/redo of each); the
per-node split of `rewrite_record_names_in_registry_with`; `rebase_file_paths`
on every paste; `APIPasteResult` + codegen. Add the
clipboard rule to the structure-designer `AGENTS.md`: a new kind of name
reference in node data must be added to `collect_record_refs_in_node`, the
per-node rewrite and the clipboard capture, or a cross-document paste silently
keeps the old name.

The phase lands before any UI exists, so no build ever shows tabs together
with a clipboard that pastes untranslated names.

Tests (`clipboard_test.rs`, domain: two documents held in a `DocumentSet`,
copy in one, `activate` the other, paste through the domain paste methods;
`documents_api_test.rs` only for the `APIPasteResult` shape) are table-driven
over three axes, the same way library linking's frozen-node matrix is:

- **reference kinds** — the library linking list: network instance (with
  wires in and out and a `-1` function-pin wire), `record_construct` /
  `record_destructure` / `product`, a parameter typed with a user record, a
  HOF element type, `closure` / `apply` type args, an `expr` signature, a
  node with a relative data-file path; each one at top level and inside a HOF
  body, pasted to top level and into a body;
- **relation between source and target** — same document; target mounts the
  owning file under the same alias; under another alias; nested (through
  another library); target *is* the owning file (`demolib.slab` → `slab`);
  source is a library of the target (`slab` → `demolib.slab`); unrelated
  documents (refused); Untitled source (refused across documents); target
  Untitled (paths become absolute);
- **staleness** — the owning file's definition changed its interface after
  the copy (unsaved in its tab, saved but not yet refreshed in the target):
  refused with the "save / refresh" reason; changed body, same interface:
  pastes.

Each cell asserts: the pasted nodes resolve and none is frozen; wires among
the pasted nodes are all present; every name and path is the expected
spelling; a refusal changes nothing in the target (serialize before and
after) and lists every offending name, not just the first; undo removes the
paste exactly.

Plus:
- **The latent bug.** Same document, copy out of a linked network whose
  library sits in a subfolder, paste into a local network: the relative path
  is rebased. Write this test first and check that it fails on `main`; then
  check *Duplicate into my file* the same way.
- **Upkeep guard.** Copy in A; rename the copied network in A: the clipboard
  follows; undo the rename in A: it follows back. Rename, undo and redo a
  network of the same name in B: the clipboard does not change. Delete in A:
  the clipboard is cleared, as today.
- **Frozen nodes** paste in their own document and are refused elsewhere.
- **Same-document paste is unchanged.** The existing paste tests
  (`copy_paste_test.rs` and the other domain tests that paste) pass with no
  assertion changed. The only edit to them is the mechanical `.unwrap()` that
  the new `Result` return type requires.
- **Headless.** A bare `StructureDesigner` with no `DocumentSet` copies and
  pastes as today. That is the CLI's headless mode.
- **The clipboard is app state.** P1's field-side table now lists the
  clipboard on the app side. Copy in A, switch to B: B holds the clipboard and
  the parked A does not.

### Phase 3 — Flutter: tabs

Work: model mirror and methods; the per-tab Dart stash (enumerate the
Flutter-only per-document fields — §5.3); `DocumentTabStrip` and
`DocumentTabList` (D10); the `InterfacePreferences` group in Rust (domain
struct, the `api` twin with its `From` impls, codegen) and its dialog section;
`withDocumentSwitch` and the document-keyed subtree (D11); the AI server's
`/load` and `/new` through `openDocument` / `newDocument(false)` (their
responses carrying the document's id and path), and every handler reordered so
that its precondition checks and API calls form one stretch with no `await`
in it (D8; the guard joins that stretch in P4). This lands with the tabs,
because the hazards exist as soon as tabs do. File menu changes;
`Ctrl+W`, `Ctrl+Tab`; quit dialog; the activation's library report;
the paste-refusal dialog (`showErrorDialog`, so it is copyable — #359).
Reference guide: `doc/reference_guide/ui.md` (the tabs in both placements,
*Arranging the window* with the vertical list as a fourth step, the
Preferences dialog's *Interface* section, File menu, shortcuts, copy and paste
across tabs); `lib/structure_designer/AGENTS.md` (the model mirrors the
active document; the stash; every change of the active document goes through
`withDocumentSwitch`).

Tests. Dart tests do not load the Rust library, so every Flutter piece with
logic in it is written to run without it. That is a design constraint on P3,
not only a testing note:

- **Stash round trip** (unit). Capture and restore are pure functions between
  a `StructureDesignerModel` (constructible without the library, as
  `motif_editor_selection_test.dart` does) and `_DocumentUiState`. Set every
  stashed field, capture under id 1, reset, restore: equal. Restore of an id
  with no entry gives the post-load defaults. An id no longer in the tab list
  is dropped.
- **`withDocumentSwitch` ordering** (widget). A `FloatInput` holds a typed,
  uncommitted value; `withDocumentSwitch` runs with a fake `action` that
  records when it ran. The field's commit callback must have run, with the
  typed value, **before** the action. For that, `withDocumentSwitch` takes its
  action as a plain callback and does not reach the API itself. A second case:
  two switches requested back to back run one after the other, never
  interleaved.
- **Tab gesture model** (unit). `DocumentTabs` is given a list of
  `APIDocumentTab` data classes and callbacks: reorder index arithmetic
  (dragging past the end, onto itself), middle-click and close-button both
  close, and the gestures are disabled while the pointer count is non-zero.
- **Quit dialog** (unit). Given tabs, it lists exactly the dirty ones, with
  *Untitled* for one that has no path.

Rust: a `preferences.json` without the new group loads with
`LeftOfViewport` (the existing `preferences_twin_test.rs` pattern, extended to
`InterfacePreferences`).

Manual (walkthrough): the two tab layouts, the document-keyed subtree
rebuilding, and D11 end to end — the value really lands in the outgoing
document and the incoming one is unchanged (step 8).

### Phase 4 — Library actions, CLI guard, docs

Work: *Open library file* → `openDocument`; remove *Back to …*,
`backToDesignPath`, `backToDesign`; the `--document <path-or-id>` guard
(`DocumentSet::check_guard` behind the `check_document_guard` FFI function,
called by each handler) and the `documents` command in the CLI, and both in
the `atomcad` skill (D8).
Reference guide: `doc/reference_guide/library_linking.md` (Open library file
opens a tab; the edit–test loop; moving nodes between a design and its
library by copy and paste) and
`doc/reference_guide/headless_cli.md` / `claude_code.md` (`--document`).
Update `doc/design_library_linking.md` §12 to point here.

Tests: domain tests for `DocumentSet::check_guard` (the active document's
path passes, also spelled with different case or `..`; the id of an active
Untitled document passes; a parked document's path or id fails with a message
naming the active one; an unknown path or id fails). The `documents` listing is
`list_documents`, already covered by P1's view-builder test.

Not automated: the Dart handlers' wiring of `--document` to
`check_document_guard` (walkthrough step 12). Also the D8 rule that no `await`
follows a handler's guard. That one is a code-review rule, stated in the AI
server's doc comment, because a test cannot easily schedule a switch at an
`await`.

### Manual walkthrough (P3 + P4, for the maintainer)

1. Start the app, open a design: it replaces the empty Untitled tab.
2. Open a second design; switch with clicks and `Ctrl+Tab`; edit both; undo in
   each; check `*` markers and the window title.
3. Right-click a linked library → *Open library file*: a new tab opens. Edit a
   network the host uses, save, switch to the host: the change is applied, the
   refresh report appears, `Ctrl+Z` in the host reverts it. Now, with that
   redo available, edit and save the library again and switch back: the
   report says the change is held, and *Refresh* on the library folder applies
   it.
4. Repeat *Open library file*: it switches to the existing tab.
5. In the host, copy a few nodes including an instance of a library network
   and an `import_xyz` with a relative path; paste them into the library tab:
   the instance is the library's own network, the file still loads. Copy back
   from the library into the host: the instance is `alias.network` again.
6. Add a parameter to that library network without saving; paste the
   instance into the host: refused, the dialog says to save. Save, wait for
   the refresh, paste again: it works.
7. Copy nodes that use one of the host's own networks; paste into an
   unrelated design: refused, naming the network and the file it lives in.
8. Start a drag in the viewport and try to switch tabs: nothing happens.
   Type a number into a property field without pressing Enter, then press
   `Ctrl+Tab`: switching back shows the value applied in the first document,
   and the second document is unchanged.
9. Tab placement: a fresh preferences file shows the vertical list left of
   the viewport; drag its divider; fold the network editor (`Ctrl+2`) and see
   it grow to full height. Switch the preference to horizontal: the strip
   appears above the network editor without a restart; fold the editor and
   the strip moves above the viewport, as it does in a direct-editing
   document. `Ctrl+0` hides the tabs in both placements.
10. Close a dirty tab (prompt), close the last tab (fresh Untitled), quit with
    two dirty tabs (both listed).
11. Change a preference in one tab; switch: it applies in the other.
12. With two tabs open, run `atomcad-cli load` on a third file: it opens in a
    new tab, and both existing tabs are unchanged. Run it on the file of the
    inactive tab: that tab is activated, and nothing is reloaded. Run any
    command with `--document` naming the inactive tab: it is refused, and
    the message names the active one. `atomcad-cli documents` lists all
    three tabs.
13. Run the Flutter smoke test (`flutter test integration_test/`) — human-only.

## 10. Effort

| Phase | Estimate |
|---|---|
| P1 Rust: documents and the swap | 2–3 days |
| P2 Rust: clipboard and translated paste | 2–3 days, most of it the test matrix |
| P3 Flutter: tabs (both placements) and the preference | 4–5 days |
| P4 Library actions, CLI guard, docs | 1 day |

Copy and paste costs this little because library linking already built the
hard parts. The ownership of every name comes from `mount_containing` and the
mount's `abs_path`, the reference enumeration from `collect_record_refs_in_node`,
the interface from library linking D13, and path re-spelling from
`rebase_file_paths`. P2 connects them; the only new algorithm is the owner →
target-name mapping of D9.

## 11. Open questions

1. **Should a library opened via *Open library file* be visually tied to the
   tab it came from** (placed next to it, a link icon on the tab)? Default:
   placed right after the active tab, no extra marking.
2. **Console per document?** Default: one session-wide console (§6). Revisit if
   prints from two documents become confusing; prefixing entries with the
   document name is the cheap fix.
3. **Should a refused paste offer a fix?** When every unreachable name is
   owned by one saved file, the dialog could offer *Link `x.cnnd`…*, which runs
   the ordinary link dialog and then retries the paste. Default: no, the
   dialog only explains; add it if the refusal turns out to be common.
