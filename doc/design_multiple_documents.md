# Design: Multiple open documents (tabs)

Related issues: #434 (load two or more files at once), #242 (its older
duplicate). Builds on `doc/design_library_linking.md`, whose §12 anticipated
this design.

atomCAD has exactly one open document. *Open library file* (library linking
Phase 4) therefore **replaces** the design with the library and offers
*File > Back to `host.cnnd`*, which reopens the host from disk. This design
lets several `.cnnd` files be open at once, one per **tab**, with one of them
active.

**Terms.** A **document** is one open `.cnnd` file (or an unsaved *Untitled*
design) together with everything that belongs to editing it: its registry,
undo stack, dirty flag, file path, navigation history. The **active**
document is the one shown in the editor and viewport; every other document is
**parked**. **App state** is state that belongs to the application session,
not to any document (preferences, the console, the profiler switches).
**P1…P4** are the phases of §9.

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
change is already applied (library linking D7 detects it), and both undo
histories are intact.

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

- A **tab bar** sits above the node-network canvas. Each tab shows the file
  name (*Untitled* for a new design), a `*` when dirty, and a close button.
  Hovering shows the full path. Clicking a tab activates it.
- **File > New** opens a new *Untitled* tab.
- **File > Open…** and **Open Recent** open the file in a **new tab**, except
  when the active tab is a *pristine* Untitled design (no path, not dirty, empty
  undo stack) — then the file replaces it, so starting the app and opening a
  file does not leave an empty tab behind.
- Opening a file that is **already open** switches to its tab instead.
- **File > Close Tab** (`Ctrl+W`, plus the tab's close button) closes the
  active tab, asking to discard unsaved changes first. Closing the last tab
  leaves one fresh Untitled tab: there is always an active document.
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
  document.
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
large atomic demos it can take seconds. P1 measures it on the T-centre demo. If
it turns out to matter, keeping the parked scene (and only its tessellated
meshes being rebuilt) is a contained change to D3 — not something to build
before it is measured.

### D4 — Switching is refused during an open interaction

`activate_document` and `close_document` are refused while
`StructureDesigner::open_interaction()` reports an open interaction (a node,
gadget or `atom_edit` drag; a comment edit; a zone resize; a node-data drag; a
`mechanosynth_edit` metadata run — the set library linking P4 established).
Swapping during one would park a half-finished undo step in one document and
end it in another. Flutter disables the tab bar during drags as well, but Rust
must not depend on that (the same rule as library linking §5).

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

- Saving library `L` in tab B changes the file; tab A detects it with the D7
  check, which runs when A is activated (§5.3), and refreshes with call-site
  repair. The refresh is an undoable step in A (library linking D9).
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
`--document <path>`, and a request whose path is not the active document's
fails with a message naming the active one. Agents (the `atomcad` skill) pass
it. The AI edit log (`ai_edit_log`) is per document, since its entries refer to
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
other document must not rewrite names that belong to the source.
`StructureDesigner` gains a `document_id` field so this check is one
comparison. After such a rewrite the recorded owners are recomputed from the
source's registry. In every other respect the origin is a snapshot: closing
the source, Save As or a later refresh does not change it, and the interface
check at paste time catches whatever drifted.

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

It knows nothing about the renderer, which keeps all of it testable in the
crate's `tests/`. Operations: `insert_parked`, `take_parked`, `put_parked`,
`order`, `find_by_path(canonical) -> Option<DocumentId>`, `neighbour_of(id)`
(for close: the tab to the right, else to the left).

On `StructureDesigner`:

- `hand_over_app_state(&mut self, to: &mut StructureDesigner)` — D2. Moves
  (`mem::take`) the logs and clones the settings; re-applies the cache
  capacities from the preferences to `to`'s evaluator and scene, since the
  preferences may have changed while `to` was parked.
- `park(&mut self)` — D3: replaces the scene with an empty one (same
  invisible-node cache capacity) and clears the CSG caches. Asserts no open
  interaction.
- `new_with_app_state_of(other: &StructureDesigner) -> StructureDesigner` — a
  fresh designer for a new or opened document, starting from a copy of the
  active one's settings rather than re-reading preferences from disk.
- `is_pristine()` — no path, not dirty, empty undo stack (§3).
- `document_id: u64` — set by `DocumentSet` when the designer joins it; `0` in
  headless mode. Used only by the clipboard upkeep guard (D9).

New module `clipboard.rs` (D9). `clipboard: Option<NodeNetwork>` becomes
`Option<Clipboard>`:

```rust
pub struct Clipboard {
    pub nodes: NodeNetwork,
    pub origin: ClipboardOrigin,
}
pub struct ClipboardOrigin {
    pub document: u64,
    pub base_dir: Option<PathBuf>,
    pub owners: BTreeMap<String, Owner>,   // referenced user name → owner
}
pub struct Owner {
    pub file: Option<PathBuf>,            // canonical; None = Untitled source
    pub name: String,                     // the name inside that file
    pub interface: Interface,             // D13's shape
}
```

- `Clipboard::capture(registry, network_name, nodes, document_id)` — what
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

**The swap** (`activate_document_internal(instance, target)`):

1. If `target` is active: nothing to do. If `open_interaction()` is `Some`:
   refuse (D4).
2. `sync_camera_to_active_network` (normally already current).
3. `incoming = documents.take_parked(target)`.
4. `instance.structure_designer.hand_over_app_state(&mut incoming)`.
5. `mem::swap(&mut instance.structure_designer, &mut incoming)`; `incoming` now
   holds the outgoing document. `incoming.park()`, then
   `documents.put_parked(old_active, incoming)`; mark `target` active.
6. `apply_camera_settings(renderer, active network's camera_settings)`.
7. `mark_full_refresh()`, `refresh_structure_designer_auto(instance)`.

No step after 4 can fail, so a switch either happens completely or not at all.

**New FRB module** `rust/src/api/structure_designer/documents_api.rs` (must be
added to `rust_input` in `flutter_rust_bridge.yaml`), all `#[frb(sync)]`:

| Function | Behaviour |
|---|---|
| `list_documents() -> Vec<APIDocumentTab>` | Tab order; each `{ id, display_name, file_path, is_dirty, is_active }`. |
| `new_document() -> u64` | Fresh designer (`new_with_app_state_of`; direct-editing mode as for `new_project_direct_editing`), inserted after the active tab and activated. |
| `open_document(file_path) -> APIOpenDocumentResult` | Already open (D5) → activate it, `already_open: true`. Otherwise build a fresh designer, **load into it while detached**, and only on success insert and activate it. A failed load drops the fresh designer; the active document was never touched. If the active document `is_pristine()`, the new one takes its place in the tab order and the pristine one is dropped. Carries the same post-load reports as `load_node_networks` (param-id repairs, library report). |
| `activate_document(id) -> APIResult` | The swap. Error when refused (D4) or unknown id. |
| `close_document(id) -> APIResult` | No dirty check — Flutter asks first. Closing the active tab activates its neighbour first, then drops the closed designer. Closing the last tab leaves a fresh Untitled. Refused during an open interaction. |
| `move_document(id, new_index)` | Tab reordering by drag. |

Changed: `save_node_networks_as` refuses a target path open in another tab
(D5). `paste_at_position` returns `APIPasteResult { node_ids, error:
Option<String> }` instead of `Vec<u64>` (D9). It is the only existing
signature this design changes, and its one Dart caller
(`StructureDesignerModel.pasteAtPosition`) shows the error dialog. The existing `load_node_networks` (load into the *active* tab, replacing
it) and `new_project*` are kept; Flutter stops using them for File > Open and
File > New, but the CLI and tests still use them.

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
- **After every switch**, the dependency check that `structure_designer.dart`
  runs on focus and on the 2 s poll (`_checkDependencies`) runs once more,
  immediately. This is what makes a library saved in another tab show up
  (D6). The poll itself keeps checking only the active document.

UI:

- `DocumentTabBar` above the node-network canvas: tabs, close buttons,
  drag-to-reorder, middle-click close, tooltip with the full path. Disabled
  while a pointer is down on the canvas or viewport (the same pointer count
  `structure_designer.dart` keeps for the dependency check).
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
| `clipboard` | **app** | moved (D9); its origin says which document it came from |
| `document_id` (new) | document | kept |
| `undo_stack` | document | kept |
| `pending_move`, `pending_atom_edit_drag`, `pending_gadget_drag`, `pending_comment_edit`, `pending_zone_resize`, `pending_node_data_drag`, `pending_step_metadata_edit` | document | must be `None` — guaranteed by D4 |
| `direct_editing_mode` | document | kept (a new design starts in direct editing, an opened library does not) |
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
per-document input, the camera, is stored per network and applied in step 6
of the swap.

## 7. Edge cases

- **Library open in a tab, host links it, host is activated.** D7's check
  compares the disk with the host's `last_seen` stamps; a save in the library
  tab moved them, so the host refreshes. If the library tab is dirty and
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
  isolate and the API is synchronous, so requests and switches are serialized;
  D8's `--document` guard handles the logical race.
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

Rust tests go in the owning crate's `tests/` directory: domain tests in
`rust/crates/atomcad-structure-designer/tests/`, API tests in
`rust/tests/structure_designer_api/documents_api_test.rs`.

### Phase 1 — Rust: `DocumentSet`, the swap, the API

Work: `document_set.rs`; `hand_over_app_state` (exhaustive destructure),
`park`, `new_with_app_state_of`, `is_pristine`; `CADInstance.documents`;
`documents_api.rs` + `flutter_rust_bridge.yaml` + codegen; Save As refusal
(D5). The clipboard stays per document until P2. Update `rust/AGENTS.md`
(`CADInstance` holds the active document; parked ones live in `documents`) and the structure-designer `AGENTS.md` (a new field
needs a side in `hand_over_app_state`; §6 is the reference).

Tests:
- **Isolation.** Open A and B; edit each; switch back and forth: each
  registry, dirty flag, undo stack and redo stack is the one it was. Undo in B
  does not touch A. Network names that exist in both are independent.
- **App state follows the session.** A preference changed while B is active is
  in effect after switching to A; a print in A is still in the log after
  switching to B.
- **Camera and canvas view** restored per document and per network.
- **D5.** Opening an open file activates it; path identity survives case and
  `..` differences; Save As onto another tab's path is refused and writes
  nothing.
- **Failed open** leaves the tab list and the active document byte-identical
  (serialize before and after).
- **D4.** For every interaction `open_interaction` reports, `activate_document`
  and `close_document` are refused and the interaction completes into the
  right document's undo stack.
- **Close.** Closing the active tab activates the right neighbour (left at the
  end); closing the last tab leaves a pristine Untitled; pristine replacement
  on open.
- **D6 end to end.** Host A links L; open L in tab B; edit and save in B;
  activate A; run the dependency check: A's networks reflect the change, the
  refresh is one undo step in A, and A's call sites were repaired as library
  linking §7 specifies.
- **Park.** A parked document's scene is empty and its CSG cache is cleared;
  activating it produces the same scene as before parking (compare the
  evaluated outputs).
- **Measurement.** Time the activation of the largest demo design. Record it
  in this document.

### Phase 2 — Rust: one clipboard, translated paste

Work: `clipboard.rs` (`Clipboard`, `ClipboardOrigin`, `Owner`, `capture`,
`for_target`, `PasteRefusal`); the clipboard moves to app state in
`hand_over_app_state`; the `document_id` field and the upkeep guard on the
existing clipboard walks (network rename/delete, record-def rename/delete,
alias rename); the per-node split of `rewrite_record_names_in_registry_with`;
`rebase_file_paths` on every paste; `APIPasteResult` + codegen. Add the
clipboard rule to the structure-designer `AGENTS.md`: a new kind of name
reference in node data must be added to `collect_record_refs_in_node`, the
per-node rewrite and the clipboard capture, or a cross-document paste silently
keeps the old name.

The phase lands before any UI exists, so no build ever shows tabs together
with a clipboard that pastes untranslated names.

Tests (`rust/crates/atomcad-structure-designer/tests/clipboard_test.rs` for
the translation, `documents_api_test.rs` for the API) are table-driven over
three axes, the same way library linking's frozen-node matrix is:

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
  follows. Rename a network of the same name in B: the clipboard does not
  change. Delete in A: the clipboard is cleared, as today.
- **Frozen nodes** paste in their own document and are refused elsewhere.
- **Same-document paste is unchanged.** The existing paste tests pass
  untouched.

### Phase 3 — Flutter: tabs

Work: model mirror and methods; the per-tab Dart stash (enumerate the
Flutter-only per-document fields — §5.3); `DocumentTabBar`; File menu
changes; `Ctrl+W`, `Ctrl+Tab`; quit dialog; dependency check after switch;
the paste-refusal dialog (`showErrorDialog`, so it is copyable — #359).
Reference guide: `doc/reference_guide/ui.md` (tab bar, File menu, shortcuts,
copy and paste across tabs); `lib/structure_designer/AGENTS.md` (the model
mirrors the active document; the stash).

Tests: Dart unit tests for the stash round trip and the quit-dialog document
list. The tab bar itself is thin UI — manual walkthrough.

### Phase 4 — Library actions, CLI guard, docs

Work: *Open library file* → `openDocument`; remove *Back to …*,
`backToDesignPath`, `backToDesign`; `--document` guard in the CLI and the
`atomcad` skill (D8). Reference guide: `doc/reference_guide/library_linking.md`
(Open library file opens a tab; the edit–test loop; moving nodes between a
design and its library by copy and paste) and
`doc/reference_guide/headless_cli.md` / `claude_code.md` (`--document`).
Update `doc/design_library_linking.md` §12 to point here.

Tests: API test for the guard (matching path passes, other path fails, no
guard passes).

### Manual walkthrough (P3 + P4, for the maintainer)

1. Start the app, open a design: it replaces the empty Untitled tab.
2. Open a second design; switch with clicks and `Ctrl+Tab`; edit both; undo in
   each; check `*` markers and the window title.
3. Right-click a linked library → *Open library file*: a new tab opens. Edit a
   network the host uses, save, switch to the host: the change is applied, the
   refresh report appears, `Ctrl+Z` in the host reverts it.
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
9. Close a dirty tab (prompt), close the last tab (fresh Untitled), quit with
   two dirty tabs (both listed).
10. Change a preference in one tab; switch: it applies in the other.
11. Run the Flutter smoke test (`flutter test integration_test/`) — human-only.

## 10. Effort

| Phase | Estimate |
|---|---|
| P1 Rust: documents and the swap | 1–2 days |
| P2 Rust: clipboard and translated paste | 2–3 days, most of it the test matrix |
| P3 Flutter: tabs | 3–4 days |
| P4 Library actions, CLI guard, docs | 1 day |

Copy and paste costs this little because library linking already built the
hard parts. The ownership of every name comes from `mount_containing` and the
mount's `abs_path`, the reference enumeration from `collect_record_refs_in_node`,
the interface from D13, and path re-spelling from `rebase_file_paths`. This
phase connects them; the only new algorithm is the owner → target-name mapping
of D9.

## 11. Open questions

1. **Activation cost on very large designs** (D3) — answered by the P1
   measurement. If it is several seconds, keep parked scenes for the most
   recently used tab only.
2. **Should a library opened via *Open library file* be visually tied to the
   tab it came from** (placed next to it, a link icon on the tab)? Default:
   placed right after the active tab, no extra marking.
3. **Console per document?** Default: one session-wide console (§6). Revisit if
   prints from two documents become confusing; prefixing entries with the
   document name is the cheap fix.
4. **Should a refused paste offer a fix?** When every unreachable name is
   owned by one saved file, the dialog could offer *Link `x.cnnd`…*, which runs
   the ordinary link dialog and then retries the paste. Default: no, the
   dialog only explains; add it if the refusal turns out to be common.
