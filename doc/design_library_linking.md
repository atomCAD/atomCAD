# Design: Library Linking — `.cnnd` files that reference other `.cnnd` files

Related issues: #434 (load two or more files at once), #242 (its older
duplicate), #84 (thumbnails — orthogonal, not covered here).

This document covers **linking only**: a `.cnnd` file declares that it uses
another `.cnnd` file, whose networks and record types then appear read-only in
the importer. Having **several documents open and editable at once** is a
separate, later design; this one is written so that design becomes an
extension rather than a rewrite (§12).

---

## 1. Motivation

Today "import" is a **copy**. `NodeNetworksImportManager::import_networks_and_clear`
moves the library's networks into the host registry under an optional name
prefix and forgets where they came from. The consequences (reported by
mechadense) are:

- **Nothing to swap.** Trying a newer demolib underneath existing work is
  impossible — the host holds a stale, private copy.
- **Wrong-way dependencies.** The copies are ordinary, editable members of the
  host's namespace, so "library" networks can quietly start depending on the
  code that uses them.
- **Monster files.** Everything accumulates in one file, and that file is what
  gets shared for debugging.
- **Latent bugs in the copy path** (found while writing this doc): record type
  defs are not copied, so imported `Named` record references dangle; existing
  networks are silently overwritten; nothing is re-validated.

Linking by reference fixes all three structural problems. The existing copy
import stays available (renamed "Import copy…", §10) for the cases where a copy
is what you want.

## 2. Goals and non-goals

**Goals**

1. A `.cnnd` file can link other `.cnnd` files by relative path, each under an
   **alias** chosen at link time.
2. Linked networks and record types are usable exactly like local ones
   (instances, wires, `Named` record types, function values) and are
   **read-only** in the importer.
3. Changing a dependency on disk (a library or a data file) is **detected** and
   the new version is brought in automatically — one undoable **refresh** step
   that repairs call sites by parameter identity and reports anything it
   disconnected.
4. Linked libraries may themselves link libraries (transitive), with cycle
   detection.
5. Swapping a version = pointing the alias at another file (**retarget**).

**Non-goals**

- **No version numbers, no resolver, no registry, no lock file.** A version
  *is* a file: `demolib_v2.cnnd` and `demolib_v3.cnnd` side by side (Go's
  "major version in the import path", with zero machinery). Git holds the
  history within a file.
- **No deduplication across mounts.** If two libraries both link `common.cnnd`,
  it is mounted twice under two names (D4). Correct, simple, slightly wasteful.
- **Several open, editable documents** — separate design (§12). Editing a
  library here means opening the library file.
- **Absolute library paths, URLs, search paths.** A library is linked by a path
  relative to the importing file (D6). (Data files may keep absolute paths;
  they are treated as external, D8.)
- **Output-pin identity across versions.** Output pins have no stable id
  (`OutputPinDefinition.id` exists only for record destructure). A library that
  *reorders* a network's output pins will mis-wire host consumers of pins ≥ 1
  (§7.3). Parameters are covered.

## 3. User-visible model

- **File > Link library…** picks a `.cnnd` file and asks for an alias (default:
  the file stem, e.g. `demolib_v3` → suggest `demolib`). The host must have been
  saved at least once (a relative path needs a base directory).
- The library appears in the node networks panel as a **mount folder**
  `demolib — demolib_v3.cnnd` (file name smaller and dimmer). Everything under
  it is dimmed with a link icon: `demolib.half_space`, `demolib.shapes.slab`,
  record `demolib.Miller`, …
- A library's own links appear as **nested mount folders** with their own file
  labels: `demolib.common — common.cnnd`.
- Linked networks can be opened and browsed (canvas, properties, 3D view) but
  not edited. Their context menu offers *Find Usages*, *Open library file*,
  *Duplicate into my file*. Rename / Move / Delete / New… are absent.
- The mount folder's context menu: *Refresh*, *Open library file*, *Change
  file…* (retarget), *Unlink*. *File > Refresh all dependencies* refreshes every
  library and data file.
- A dependency changed on disk is **refreshed automatically** — when the window
  regains focus, and by a light poll while it has focus (D7, D10). A snackbar
  says what happened: "Refreshed demolib", or, if something was disconnected,
  "Refreshed demolib — 3 wires disconnected [Details] [Undo]". The refresh is
  one undo step.
- A missing or unreadable library shows the **red error badge**; the import is
  kept, the host's instances of it show errors, and nothing is deleted.
- **Save As** into another folder lists the libraries and data files the design
  depends on and offers to copy them so every relative path still works
  (D11). *File > Export project bundle…* zips the design together with
  everything it depends on.

## 4. Core model (decisions)

### D1 — Mount by prefix into the one registry

Linked content is loaded into the **same** `NodeTypeRegistry` the host uses,
with every name prefixed by the mount path: network `half_space` in
`demolib_v3.cnnd` mounted under alias `demolib` becomes the registry entry
`demolib.half_space`.

Why: the evaluator, validator, type system, Find Usages, and the panel all
resolve networks and record types **by name against one registry**
(`network_evaluator.rs` custom-node dispatch, `lookup_record_type_def`,
`get_networks_in_dependency_order`). Mounting by prefix means none of them
changes. The existing namespace separator is `.` (`namespace_utils.dart`), so a
mount is just a folder.

Rejected alternative: a registry per document with scoped name resolution. It
is the "right" shape for multi-document editing but touches every lookup site
in the evaluator and validator. §12 explains why mount-by-prefix does not block
it later.

### D2 — The alias is the identity; the file is a detail

The host file stores node type names **alias-qualified** (`demolib.half_space`).
The alias → file binding lives in exactly one place, the host's import list.
Hence:

- **Retarget** (v2 → v3) rewrites nothing in the host — it swaps the file under
  the alias and refreshes (§7).
- **Unlink** is refused while anything in the host uses the mount (same shape as
  `check_delete_references`).

### D3 — A mount owns its folder completely

No local entity may be created in, renamed into, or moved into a mount path;
the alias may not collide with any existing local name or folder. Consequently
"is this linked?" is a **pure prefix test** — `mount_containing(name)` returns
the longest mount path `p` with `name == p` or `name.starts_with(p + ".")` —
with no per-entity origin field. It mirrors `is_cli_write_locked`'s
longest-prefix rule, and a Dart twin (like `nameIsTaken`) gives instant UI
feedback while Rust stays authoritative.

**Aliases may contain dots** (`libs.demolib`), so several libraries can be
grouped under one folder. Each segment must be a valid identifier
(`validateUserName` / `is_valid_user_name`). The segments before the last
(`libs`) form an ordinary **local folder** that happens to contain only mounts;
the mount itself is `libs.demolib`. Rules that keep the prefix test exact:

- A new alias may not equal, contain, or be contained in an existing mount path
  (`libs` and `libs.demolib` cannot both be mounts).
- The alias may not equal or lie under an existing local *entity* name; it may
  lie under an existing local folder (then the mount appears inside it).
- Namespace operations (rename / move / delete folder) on a folder that
  contains a mount are **refused** in v1, because they would silently change an
  alias. Changing an alias is its own operation (*Rename alias…*, P6), which
  rewrites the host's `libs.demolib.*` references in one undoable step.

### D4 — Transitive links nest; each mount is independent

A library is loaded by the **same** load function the host uses, into a
temporary registry, recursively. So when `demolib_v3.cnnd` itself links
`common.cnnd` as `common`, its temp registry already contains
`common.foo`; prefixing the whole temp registry with `demolib` yields
`demolib.common.foo` and a nested mount `demolib.common`. No special
transitive code.

- **Cycles**: the load carries a stack of canonical absolute paths; a path
  already on the stack is a `Cycle` error on that link (the link's status, not a
  failed host load).
- **Direct vs transitive**: the host should only use its *direct* mounts (Go's
  rule — if demolib drops `common` in v4, a host that used `demolib.common.foo`
  directly breaks). Enforced softly: transitive mounts are browsable in the
  tree but hidden from the add-node popup, and a host node referencing a
  transitive mount gets a **non-blocking warning** "uses transitive library
  `demolib.common`; link it directly".
- **The same file twice** (host links `common.cnnd` directly *and* via demolib)
  mounts twice: `common.*` and `demolib.common.*` are distinct networks and
  distinct record types. Built-in types (Geometry, Crystal, Molecule, …) flow
  between them freely; named records of the two copies are structurally
  compatible (records are structurally typed), so this rarely bites.

### D5 — Only the host's own content is saved

Save writes: local networks, local record defs, local folders, local
`cli_access_rules`, and the **import list**. It never writes anything under a
mount path. Consequences:

- Any accidental in-memory mutation of linked content is **never saved** and is
  gone at the next refresh or reopen — defense in depth behind §6.
- Only *direct* links are written; transitive ones are the library's business.

### D6 — Relative paths never change meaning

Each import stores `path` relative to the importing file's directory, with
forward slashes. `..` is allowed (`../libs/demolib.cnnd`), so several project
folders can share one `libs/` folder next to them.

- **Paths are never rewritten.** Save and Save As write every import path
  exactly as it was authored. There is no silent re-pointing.
- **Absolute paths are not allowed for libraries.** A library on another drive
  (Windows), where no relative path exists, cannot be linked; the link dialog
  offers to copy it (with its own dependencies) next to the host instead (P5).
- The stored path is lexically normalized once, when the link is created
  (`libs/../x.cnnd` → `x.cnnd`); a path that climbs above the filesystem root is
  refused.

The invariant is: **a file and all of its dependencies form a fixed relative
layout.** Whatever moves the file must move that layout with it, which is what
the Save As dependency copy (D11) does. The smallest folder containing the file
and every dependency is self-contained; *Export project bundle* (D11) zips
exactly that folder.

Rejected alternative: banning `..` so that a file's folder is always
self-contained. It forces a flat `libs/` inside each project and makes sharing a
library across projects impossible without copying it into each one. D11 keeps
the property that matters (paths keep their meaning) without that cost.

### D7 — Change detection

**What is watched: the dependency list** — the same transitive list D11 uses
for Save As: every linked library (direct and nested) and every relative or
absolute data-file path stored in node data of the host and of every library.
Data files matter too: `import_xyz` and friends read their file once at load
and cache the result, so an edited `.xyz` is not picked up today either.
Paths that arrive through a wire are not watched; they are re-read on the next
evaluation anyway.

Each watched file records a `FileStamp { mtime, size, blake3 }`. Two stamps
per file:

- `loaded` — the content currently in memory;
- `last_seen` — the content last observed on disk.

A change is detected when the disk differs from **`last_seen`**, not from
`loaded`. The difference matters after an undo of a refresh (D9): memory is then
deliberately older than the disk, and comparing against `loaded` would refresh
again on the next focus.

**When it is checked:**

- when the window regains focus (`AppLifecycleListener.onResume`);
- by a **poll every ~2 s while the window has focus** — a script regenerating
  libraries, a cloud sync, or a `git pull` finishing in the background does not
  produce a focus event. Stat-ing a handful of files is trivial; only if
  `mtime` or `size` moved is the file re-hashed;
- **after Save As to another folder**: the host's folder changed, so every
  relative path now resolves to a new absolute location (the copied file, an
  existing one kept on conflict, or nothing). Rust re-resolves all watched paths
  and checks; a kept, different file is refreshed like any other change, a
  missing one goes `Missing` (D10);
- **on open**: the host file stores the hash of each **direct** import (`hash`
  field) as of its last save. If the library on disk differs, the host opens
  with the current file and the open report says "demolib changed since this
  file was last saved", with the call-site repair report (§7.2) — exactly what
  an automatic refresh would have reported. Nested libraries and data files
  have no stored hash; they are simply loaded as they are.

The check is skipped while a drag, a text edit, or a modal dialog is active and
runs at the next idle moment.

**What cannot be detected:** content that changed while `mtime` and `size` both
stayed the same. Tools that copy an older file over a library keep the *old*
file's timestamp, which still differs from the current one, so this needs a
deliberate timestamp forgery. *Refresh* / *Refresh all dependencies* (which
re-hash unconditionally) are the answer for that case and for any doubt.

### D8 — Relative file paths *inside* library networks resolve against the library

File-using nodes — the readers and the two exporters — resolve relative paths
at **eval** time against `registry.design_file_name`, i.e. the host file:
`import_xyz.rs:63`, `import_cif.rs:137`, `import_cube.rs:200`,
`build_script.rs:150`, `mechanosynth.rs:456`, `ops_library.rs:172`,
`export_atoms.rs:84`, `export_build_script.rs:104`. (Their
*load-time* data loaders already use the file's own `design_dir`, so load and
eval would disagree.) A library in `libs/` with `import_xyz("tip.xyz")` would
read `host_dir/tip.xyz`.

Fix: one helper, `base_dir_for_eval(network_stack, registry) -> Option<String>`,
which finds the innermost frame that is a *network* (zone-body frames belong to
their owning network), takes its name, and returns the directory of the mount
containing it — or the host directory for a local network. The eight sites
switch to it. This is a real phase of work (P2), not an afterthought.

**Data files follow the same path rules as libraries** (D6, D11), with one
difference: absolute paths stay allowed for them and are treated as
**external** (never copied, listed separately). Today's behaviour already fits:
node data stores `file_name` as authored; a relative name is written back
verbatim, and the file picker's absolute path is made relative only when the
file is under the design folder (`try_make_relative`), otherwise it stays
absolute. What is missing is the other half — Save As does not move the files
a relative path points at, so it silently breaks them. D11 fixes that for data
files and libraries together.

Paths that arrive through a **wire** (a `String` computed by the network) are
only known at evaluation time and are not collected as dependencies; the Save As
dialog says so when the file contains such a node.

### D9 — Undo

- **Link** and **Unlink** are undoable commands. Undo of Link unmounts; redo
  remounts from disk. Unlink is only allowed when unused, so its undo is a
  remount too. (Redo reads the disk, so it is faithful to the library's
  *current* content — acceptable, and stated in the command's doc comment.)
- **Refresh** (automatic or manual) and **Retarget** are **one undoable step
  each** — `RefreshDependenciesCommand`. The undo stack is never cleared by
  them. The command holds:
  - for each refreshed mount, the **previously mounted content** (the networks,
    record defs, folders and nested mounts that were under the mount path — it
    is already in memory, it is simply kept instead of dropped) and the new
    content;
  - for each refreshed host data file, the node data before and after
    (like `SetNodeDataCommand`);
  - **snapshots of the host call sites** the repair touched (§7.1 step 4), before
    and after.

  Undo swaps the old content and the old call sites back; redo swaps the new
  ones in. Earlier commands stay consistent because undoing past a refresh
  always undoes the refresh first — they are replayed against the interface
  they were recorded against. Undo leaves memory older than the disk: the
  affected mounts get status `OlderThanDisk` (a marker in the panel), and, by
  D7's `last_seen` rule, no automatic refresh follows; *Refresh* or *Refresh all
  dependencies* brings them forward again.

### D10 — Refresh everything that changed, automatically

Every detected change is refreshed immediately — there is no "safe / unsafe"
classification and no pending state. This is what editors do with a file
changed on disk when the buffer has no unsaved edits, and linked content can
never have unsaved edits (it is read-only).

What makes that acceptable is D9: the refresh is **one undo step**, and the
notification always says what it did:

- clean — transient snackbar "Refreshed demolib (libs/demolib_v3.cnnd)";
- something disconnected, removed, or flagged — a persistent snackbar
  "Refreshed demolib — 3 wires disconnected [Details] [Undo]". *Details* opens
  the report of §7.2 (navigable rows); *Undo* is the ordinary undo.

Several files changing at once (a `git pull`) are refreshed in **one** command
and reported together.

A file that becomes **missing or unparseable** is not a refresh: the loaded
version keeps evaluating, the mount shows the red error badge (`Missing` /
`Error`), and nothing is removed. When the file is back, the next check
refreshes it normally.

Retarget (*Change file…*) is the same command with a different source file.

### D11 — Save As copies dependencies into the same relative layout

When **Save As** targets a different folder and at least one relative
dependency would need copying or conflicts, a dialog appears before anything is
written (details below).

**What counts as a dependency** (collected transitively by Rust):

- every direct and nested linked library (`imports`, recursively);
- every relative data-file path stored in node data of the host **and** of every
  linked library (the file-reading nodes of D8);
- absolute data-file paths are listed as **external** and never copied.

**Where each dependency goes.** For each dependency, take its canonical
absolute path, compute its path **relative to the host file's folder**
(normalized, may contain `..`), and join that onto the new folder. This is
computed from absolute paths, not by concatenating each library's own relative
paths, so a library in `../libs/` whose data file is `tip.xyz` lands at
`<new>/../libs/tip.xyz`, where it has to be. Every relative path in every file
keeps its meaning without being rewritten.

**The dialog** shows three groups:

1. inside the destination folder;
2. **outside the destination folder** (targets reached through `..`), each shown
   with its full absolute target path — a copy that lands outside the folder the
   user picked must be visible;
3. external (absolute paths) — not copied.

Each entry has a status: **will copy**, **already there** (the target is the
same file as the source, or has identical content → skipped), or **different
file already there** (conflict). Buttons:

- **Copy dependencies** (default). If there are conflicts, the user chooses
  explicitly between *overwrite* and *keep existing*; keeping means the saved
  file will link to a different version, and the dialog says so.
- **Save without dependencies** — warns that N libraries and N data files will
  be missing at the new location.
- **Cancel**.

**Order and failure.** Dependencies are copied first; the host is written last.
If any copy fails, the host is not written and the error lists the file. A
target that would climb above the filesystem root is refused.

**Moving inside a workspace costs nothing.** `proj1/host.cnnd` linking
`../libs/demolib.cnnd`, saved as `proj2/host.cnnd` (a sibling folder), resolves
to the same `libs/demolib.cnnd`: status *already there*, nothing to copy, no
dialog. The dialog appears only if at least one dependency would need copying
or conflicts.

**Export project bundle (.zip).** File menu command. Rust computes the smallest
folder containing the file and all its relative dependencies, and writes a zip
of the file plus those dependencies with their paths relative to that folder.
Unzipping anywhere reproduces the layout. External files are not included; the
command lists them. This is the answer to "send me the file to debug".

Saving a **library** opened as its own document is the same operation — D11
does not care whether the file is a host or a library.

### D12 — File format

Add one top-level field to `SerializableNodeTypeRegistryNetworks`:

```json
"imports": [
  { "alias": "demolib", "path": "libs/demolib_v3.cnnd", "hash": "b3:…" }
]
```

`#[serde(default, skip_serializing_if = "Vec::is_empty")]`, sorted by alias.
`SERIALIZATION_VERSION` goes 8 → 9 **with no migration pass** (purely additive).
The bump is deliberate: an older atomCAD ignores unknown fields, so it would
open a linking host *without* its libraries and — on save — destroy it. Refusing
("file is from a newer version") is the right failure.

Files without imports serialize identically apart from the version number;
fixtures and snapshots that pin the version string are updated once.

## 5. Architecture: what lives where

The rule used throughout the codebase applies: **Rust owns every semantic
decision and all state that must be correct; Flutter owns presentation, the
moments at which to ask, and user confirmation.** A missed Flutter gate must
produce an error message, never corrupted state.

### 5.1 Rust — `atomcad-structure-designer`

New module **`library_links.rs`** (plus a test file mirroring it):

```rust
pub struct LibraryMount {
    pub mount_path: String,          // "libs.demolib" or "libs.demolib.common"
    pub alias: String,               // as written by the importer, may be dotted;
                                     // mount_path = parent's mount_path + "." + alias
    pub rel_path: String,            // as written by the importer
    pub abs_path: PathBuf,           // canonical
    pub parent: Option<String>,      // mount_path of the importing mount; None = direct
    pub status: MountStatus,         // Loaded | OlderThanDisk | Missing | Error(String) | Cycle
    pub loaded: Option<FileStamp>,   // (mtime, size, blake3) of the content in memory
    pub last_seen: Option<FileStamp>,// last observed on disk (D7)
    pub stored_hash: Option<String>, // from the host file (direct mounts only)
}

pub struct LibraryLinks {
    mounts: BTreeMap<String, LibraryMount>,  // keyed by mount_path
}
```

- `LibraryLinks` lives on **`NodeTypeRegistry`** (field `library_links`), not on
  `StructureDesigner`, because the registry is where the prefix test is needed
  (name checks, save filtering, validation warnings) and it is what the temp
  registry of a library carries up during a recursive mount.
- **Mounting** (`mount_library(registry, alias, rel_path, host_dir, load_stack)`):
  load the file into a fresh registry through `load_node_networks_from_file`
  (which recurses into its imports), **prefix everything** in that temp registry
  (networks + `node_type_name` references, record defs + `Named` references +
  `record_construct`/`record_destructure`/`product` schema strings, folders,
  nested mounts), then move it into the host registry. The prefixing is a
  registry-level batch rename built from the pieces `rename_namespace` already
  dispatches to — `apply_rename_core` per network,
  `rename_record_type_def_unchecked` per record def — applied from the root.
  (The current copy import's hand-rolled `node_type_name` rewrite is exactly
  what misses record defs; do not copy it.)
- **Load** (`load_node_networks_from_file`): after local networks are inserted,
  mount each import (status failures are recorded, never fatal); then
  `StructureDesigner::load_node_networks` validates **everything** in dependency
  order as today.
- **Save**: filter by `mount_containing(name).is_none()` for networks, record
  defs, folders and CLI rules; write `imports` from direct mounts, with fresh
  hashes; paths written verbatim (D6).
- **Dependencies** (`file_dependencies.rs`, D11): `collect_file_dependencies(target_dir)`
  (transitive: imports + relative data-file paths of host and libraries, with
  per-entry target path and status), `save_as_with_dependencies(path, choice)`
  (copy then write, all-or-nothing), `export_project_bundle(zip_path)`. Data-file
  paths are reported by the nodes themselves through a
  `file_paths(&self) -> Vec<&str>` method on `NodeData` (default: none), so a
  new file-reading node is picked up by overriding one method. The same module
  keeps the watch state for data files (`FileStamp` `loaded` / `last_seen`, D7);
  libraries keep theirs on `LibraryMount`.
- **Read-only guard** (§6), **refresh + call-site remap** (§7), **eval base dir**
  (D8), **check-on-disk** (D7).

### 5.2 Rust — `rust/src/api/` (FFI surface)

Thin wrappers, one new file `library_links_api.rs` (added to
`flutter_rust_bridge.yaml`'s `rust_input` — a new FRB module must be listed
there):

| Function | Returns |
|---|---|
| `link_library(path: String, alias: String)` | `APIResult` (validates alias, path, cycle) |
| `link_library_copying(path, target_rel_path, alias)` | `APIResult` — copies a file that has no relative path (another drive) plus its dependencies next to the host, then links it (D6) |
| `unlink_library(alias)` | `APIResult` (refused with the list of users if used) |
| `retarget_library(alias, path)` | `APIRefreshReport` |
| `refresh_library(mount_path)` / `refresh_all_dependencies()` | `APIRefreshReport` (re-hash unconditionally) |
| `check_dependencies()` | `Option<APIRefreshReport>` — stats every watched file (D7), refreshes whatever changed in one undoable command (D10); `None` if nothing changed |
| `get_linked_libraries()` | `Vec<APILibraryMount>` (no disk access) |
| `take_load_library_report()` | report from the last file open (like `take_load_param_id_repairs`) |
| `collect_file_dependencies(target_path)` | `APIDependencyPlan { entries: Vec<APIDependency { source_abs, target_abs, rel_path, kind: Library\|DataFile, group: Inside\|Outside\|External, status: WillCopy\|AlreadyThere\|Conflict }>, has_wired_paths }` |
| `save_as_with_dependencies(path, copy: bool, overwrite_conflicts: bool)` | `APIResult` |
| `export_project_bundle(zip_path)` | `APIResult` with the list of external files left out |

`APILibraryMount { mount_path, alias, rel_path, abs_path, file_name, parent,
direct, status, status_message }`; `APIRefreshReport { refreshed_mounts,
refreshed_data_files, dropped_wires: Vec<APIDroppedWire { network, scope_path,
node_id, pin_name, reason }>, removed_networks_in_use, errors }` — dropped
wires are navigable, like validation errors. `is_clean()` decides between the
transient and the persistent snackbar.

Existing view types gain one field each:
`APINetworkWithValidationErrors.read_only` and `NodeNetworkView.read_only`
(authoritative, from Rust). Record defs are covered by the Dart prefix twin
(§5.3) — they are a `List<String>` today and do not need a new struct.

`view_builders.rs`'s add-node list excludes networks under a **transitive**
mount and marks linked ones (for grouping in the popup, optional).

### 5.3 Flutter

Presentation and triggers only; no path logic, no hashing, no name rewriting.

- **Model** (`structure_designer_model.dart`): `linkedLibraries:
  List<APILibraryMount>` refreshed in `refreshFromKernel`; methods
  `linkLibrary`, `unlinkLibrary`, `retargetLibrary`, `refreshLibrary`,
  `refreshAllDependencies`, `checkDependencies`; `mountFor(name)` Dart twin of
  the prefix test (in `namespace_utils.dart`, next to `nameIsTaken`).
- **Change triggers** (D7): an `AppLifecycleListener` (`onResume`), a ~2 s
  `Timer.periodic` while the window has focus, and a call after Save As — each
  calls `checkDependencies()`. Rust decides and refreshes; Flutter only shows
  the result: `refreshFromKernel()`, then the transient snackbar for a clean
  report or the persistent one with *Details* / *Undo* otherwise. Checks are
  skipped while a drag, a text edit, or a modal dialog is active. There is no
  focus handling in `lib/` today; this is the first.
- **Panel** (`node_networks_list/`): tree view renders a mount folder's label as
  `alias — file_name` (file name `AppTextStyles` small, dimmed); every row under
  a mount dimmed with a link icon; mount folders get the error badge
  (`Missing` / `Error` / `Cycle`) and, after an undo of a refresh, a small
  neutral "older than disk" marker whose click is *Refresh* — the badges go in
  `network_row_badges.dart` so list and tree share them. List view: link icon +
  dimmed text per row, file in the tooltip, badge per row. Context menus
  per §3. `_isValidDrop` refuses drops into a mount and drags of mounted rows;
  inline rename is not offered.
- **Read-only canvas**: when `NodeNetworkView.read_only`, the node network
  editor disables node drag, wire creation/deletion, add-node, paste, delete,
  and the property panel renders its editors disabled. A banner at the top of
  the canvas: "Linked from `libs/demolib_v3.cnnd` — read-only. *Open library
  file*". Selection, hover values, display toggles of the **3D view** and
  navigation remain.
- **Dialogs**: *Link library…* (file picker with a new `FileDialogPurpose`
  variant — do not change existing `key()` strings — then an alias field with
  `validateUserName` + `mountFor`/`nameIsTaken` checks), *Change file…*,
  refresh report (dropped wires and removed networks, each navigable). All
  dialogs draggable; errors via `showErrorSnackBar`.
- **Save As**: before saving to a different folder, call
  `collectFileDependencies`; if any entry is *will copy* or *conflict*, show the
  D11 dialog (three groups, full target paths for the outside group), then
  `saveAsWithDependencies`. Otherwise save as today. *Export project bundle…*
  in the File menu uses a save-file picker for the `.zip`.
- **Open library file**: Flutter-only — `_confirmDiscardChanges()` then
  `loadNodeNetworks(abs_path)`; remember the previous path for a "Back to
  `host.cnnd`" entry in the File menu for this session.

### 5.4 Display / view state of linked networks

`NodeNetwork` carries view state (`selected_node_ids`, `active_node_id`,
`displayed_nodes`, `camera_settings`, `canvas_viewport`), so *browsing* a linked
network mutates it. That is allowed — view state is not content.

**A library file keeps its own view state.** Opened as a document in its own
right, a library is an ordinary `.cnnd`: its per-network `camera_settings` and
`canvas_viewport` are loaded and saved as always. Linking changes nothing in
that file.

**When linked, the library's view state is ignored in both directions:**

- On mount, the loaded `camera_settings` / `canvas_viewport` of every linked
  network are cleared, so browsing a linked network starts from the default
  framing, the same as a freshly created network.
- Browsing a linked network changes its in-memory view state for the session
  only. It does **not** set the host dirty (`sync_camera_to_active_network` and
  `set_active_network_canvas_viewport` check `mount_containing`), and none of it
  is saved, to the host (D5) or to the library (the host never writes library
  files). Display toggles on a linked network are session-only for the same
  reason. A refresh resets them.

## 6. Read-only enforcement

There is **no single mutation chokepoint** (≈42 direct `node_networks.get_mut`
sites in `structure_designer.rs`, ~49 uses of `get_active_node_network_mut` /
`get_scope_network_mut`, plus `ai_text_edit.rs`, the api layer, undo). So
enforcement is layered:

1. **Namespace layer (registry).** `name_is_taken` (or a sibling
   `name_in_mount`) rejects creating/renaming/moving anything to a name inside a
   mount. Covers add network/record/folder (incl. `…_in_namespace`), rename,
   `rename_namespace`, duplicate target, paste-as-new-network, factor into
   subnetwork, closure→network extraction.
2. **Entity layer (StructureDesigner).** One helper
   `ensure_editable(network_name) -> Result<(), String>` called at the top of
   every `pub fn … (&mut self` that mutates a network's *content* or a record
   def. The two scope accessors used for content edits
   (`get_scope_network_mut`, `get_active_node_network_mut`) get checked
   variants, and the view-state paths (§5.4) keep unchecked ones — this split is
   the review point. `ai_text_edit` checks next to the existing CLI-lock check
   (`ai_text_edit.rs:95`), so CLI/AI edits are covered by the same code.
3. **Save layer.** D5 — linked content is never written.
4. **UI layer.** Flutter hides/disables (§5.3).

`delete_node_network`, `rename_node_network`, record-def update/rename/delete
and namespace ops are also reachable from the CLI (`networks delete|rename`),
which today bypasses the CLI lock; they are covered by layer 2 regardless of
caller.

**Tripwire for tests**: `mount_fingerprint(registry, mount_path) -> String` —
the canonical text-format dump (`text_format` serializer) of every network +
record def under the mount, excluding view state. The enforcement test (§11,
P2) runs the whole mutation alphabet against linked targets and asserts both
`Err` **and** an unchanged fingerprint; a guard that errors *after* partially
mutating fails it.

## 7. Refresh, retarget, and call-site repair

### 7.1 Procedure (refresh and retarget alike)

All changed files found by one check are handled by one
`RefreshDependenciesCommand`:

1. Capture, for every network under each affected mount (recursively), its
   **old interface**: `node_type.parameters` (with `param_id`s) and output pins.
2. Detach everything under the mount path from the registry (networks, record
   defs, folders, nested mounts) and **keep it in the command** (D9).
3. Mount again (for retarget: from the new path). A changed data file used by
   a library refreshes that library's mount; a changed data file used by the
   host keeps its node data in the command and re-reads the file.
4. For each re-mounted network whose interface changed, snapshot the host call
   sites, then run the identity-based call-site repair on them:
   `repair_call_sites_for_network` (`network_validator.rs:279`, today private
   and only triggered by an in-memory interface change) with the captured old
   parameters and the new ones. It maps by `param_id` first, name second — which
   is exactly right across versions, because `demolib_v3.cnnd` made by copying
   v2 **keeps v2's `param_id`s** for surviving parameters. Snapshot the call
   sites again after.
5. Collect every wire the repair dropped into the report.
6. Validate everything in dependency order; push the command (never clear the
   undo stack); set dirty if a call site was repaired or a direct import's hash
   changed (the host now pins a different library); full refresh.

### 7.2 Report

Dropped wires (parameter gone, or the retype makes the wire incompatible →
kept but flagged by validation, consistent with the existing repair), networks
that disappeared (host instances now show "Unknown node type"), and mount
status changes. A clean report → transient snackbar; otherwise a persistent
snackbar whose *Details* opens a dialog with navigable rows (D10).

### 7.3 Known limitation

Output pins are matched positionally. A library version that inserts or
reorders a network's output pins mis-wires host consumers of pin ≥ 1. The
report lists every host wire from an output pin ≥ 1 of a network whose output
list changed, so it is visible, not silent. Real output-pin identity is P5 of
`doc/design_identity_vs_naming.md`.

## 8. Missing libraries must be lossless

If `libs/demolib_v3.cnnd` is missing, the host must still open, show errors,
and **save byte-identically** — the instance nodes of `demolib.*` and their
wires must survive. This is the riskiest assumption in the design: load-time
repair passes (`initialize_custom_node_types_for_network`,
`repair_network_arguments`) have a history of truncating the arguments of nodes
whose type they cannot resolve (see `serialization/AGENTS.md`, "Load pipeline &
derived state"). **P1 starts with a red-first test for exactly this**, and if it
fails, the fix is that argument-count repair skips nodes whose
`node_type_name` is under a mount with a non-`Loaded` status (unknown types
elsewhere keep today's behaviour).

## 9. Mapping to mechadense's concerns

| Concern | Answer |
|---|---|
| Names shift from files to namespaces | Mount folders show the file; thumbnails (#84) are separate |
| Library depending on its user | Impossible: a library file never sees its importers; linked content is read-only |
| Renaming someone else's library | Not possible in the importer; write a local wrapper network |
| Monster all-in-one file for debugging | Files stay separate; *Export project bundle* zips the file with every dependency (D11), or vendor (§10) when one file is needed |
| Swap demolib underneath | Retarget the alias (§7) |
| Fragile wires on swap | Call sites repaired by `param_id` (§7.1); limitation on output pins (§7.3) |

## 10. Existing copy import and vendoring

- The current *File > Import from .cnnd library* becomes **Import copy…**,
  behaviour unchanged. Its record-def gap is a separate bug; this design does
  not depend on fixing it.
- **Vendor** (optional, P6): *Make local copy* on a mount folder turns a mount
  into local content. Under D1/D3 that is just removing the mount record — the
  networks are already in the registry under `demolib.*` and will now be saved.
  Nested mounts are vendored with it. Undoable (restore the mount record).

## 11. Phased implementation plan

Each phase ends green: `cargo test -j 4` (plus `--test structure_designer_api`
and `--test integration` explicitly), `cargo clippy`, `cargo fmt`,
`flutter analyze`. Tests go in
`rust/crates/atomcad-structure-designer/tests/structure_designer/`, registered
in `tests/structure_designer.rs`; fixtures under
`rust/tests/fixtures/library_linking/`, addressed only via
`atomcad_test_support::fixture_path`. Tests that modify libraries copy fixtures
into a `tempfile::TempDir` first. Adding `.cnnd` fixtures updates the corpus
snapshots (text-format round-trip corpus) — review, don't blindly accept.

Fixture set (P1): `host.cnnd` → links `lib_a.cnnd` (`a`) and
`libs/lib_b.cnnd` (`b`); `lib_a` links `libs/common.cnnd` (`common`) and uses
a record def, a HOF with a body instance of its own network, and a function
value; `cycle_x.cnnd` ↔ `cycle_y.cnnd`; `self_link.cnnd`;
`host_missing.cnnd` (links a file that does not exist); `libs/with_xyz.cnnd` +
`libs/tip.xyz` (relative-path eval).

### Phase 1 — Format, mounting, save (Rust only; no UI)

Work: `library_links.rs` (`LibraryMount`, `LibraryLinks`, `mount_containing`,
mount/unmount), registry field, prefix batch-rename, recursive load with cycle
stack, save filtering + `imports` field (paths verbatim, normalized at link time), version 9,
`link_library`/`unlink_library` in `StructureDesigner` with undo commands.

Tests (`library_links_test.rs`):
- **Red-first**: `host_missing.cnnd` loads, reports `Missing`, and
  load → save is byte-identical (instance nodes and their wires intact) (§8).
- Mount prefixes networks, record defs, folders; internal references rewritten
  including inside zone bodies, `Named` record refs, and
  `record_construct`/`record_destructure`/`product` schema strings.
- **Eval equivalence**: a host instance of `a.foo` produces the same result as
  `foo` evaluated in `lib_a.cnnd` opened standalone (atoms compared with
  `assert_structures_equivalent`).
- Transitive: `a.common.*` exists with `parent = "a"`; host save writes only
  `a` and `b`.
- Cycle and self-link: host loads, the offending mount has status `Cycle`,
  nothing else affected.
- Save round-trip: saved host contains no `a.*`/`b.*` networks, record defs, or
  folders; `imports` sorted, relative, forward slashes; load → save → load is
  stable.
- Paths verbatim: `../libs/x.cnnd` survives load → save → Save As unchanged;
  link-time normalization (`libs/../x.cnnd` → `x.cnnd`); absolute path and a
  path climbing above the root → `Err`.
- Link validation: alias taken by a local network / record def / built-in →
  `Err`; host unsaved → `Err`; path to non-`.cnnd` or unreadable file → `Err`,
  nothing mounted.
- Dotted aliases: `libs.demolib` mounts as `libs.demolib.*`, and `libs` shows as
  an ordinary folder; linking `libs.other` alongside works; `libs` as a second
  mount is refused (contains a mount), and so is `libs.demolib.x` (inside one);
  an alias under an existing local folder is accepted; rename / move / delete of
  folder `libs` is refused while it contains a mount; an invalid segment
  (`libs..x`, `1lib`) is refused.
- View state: a library saved with camera/canvas settings has them cleared once
  mounted, and opening the same library file on its own still restores them.
- Unlink refused while used (returns users); allowed when unused; undo/redo of
  link and unlink restores the registry and `imports`.
- Version: v8 fixtures still load; a v9 file is rejected by a reader capped at 8
  (unit test on the version gate).

### Phase 2 — Read-only enforcement and eval base directory (Rust only)

Work: `name_in_mount` in the namespace checks; `ensure_editable` + checked
scope accessors; `ai_text_edit` guard; view-state paths don't dirty on linked
networks; `base_dir_for_eval` and the eight file-using call sites; non-blocking
"transitive library" warning; `mount_fingerprint`.

Tests (`library_links_readonly_test.rs`):
- **Mutation alphabet**, table-driven: for each entry point — add node, delete
  selected, connect/delete wire, move nodes, set node data, paste, duplicate
  node, factor selection, inline instance, closure⇄network conversion,
  set description/summary, rename/delete/duplicate network, rename/move/delete
  namespace, add network/record/folder inside the mount, record-def
  add/update/rename/delete, `ai_text_edit` (edit and replace), set function-pin
  role, set collapse mode — targeting linked content: returns `Err` **and**
  `mount_fingerprint` unchanged **and** the undo stack unchanged.
- The same alphabet on a **local** network still succeeds (guards against a
  guard that is too broad).
- Browsing: activate a linked network, select, change camera/canvas viewport,
  toggle display → allowed, host **not** dirty, nothing saved.
- Duplicate *into my file* (target namespace outside the mount) succeeds and the
  copy is editable; its references to other `a.*` networks remain links.
- Eval base dir: `libs/with_xyz.cnnd` linked from `host.cnnd` in the parent
  directory evaluates `import_xyz("tip.xyz")` from `libs/`; a local network in
  the host still resolves against the host directory; a HOF body inside a
  linked network resolves against the library.
- Transitive warning emitted for a host node using `a.common.*`, and not for
  `a.*`.

### Phase 3 — Change detection, refresh, retarget (Rust only)

Work: `FileStamp` with `loaded` / `last_seen`; the watched list (libraries plus
data files, from the D11 collector — the collector moves forward into this
phase, the Save As dialog stays in P5); `check_dependencies`; stored-hash
comparison on open; `RefreshDependenciesCommand` (§7.1) with
`repair_call_sites_for_network` made `pub(crate)` and driven with captured
interfaces; host data-file refresh through a `NodeData` hook that re-reads the
cached file content; the report.

Tests (`library_links_refresh_test.rs`, all in temp dirs):
- Detection: unchanged file → nothing; touch without content change → nothing
  (hash decides); content change → refreshed; delete → `Missing`, nothing
  removed, loaded version keeps evaluating; file restored → refreshed.
- Refresh picks up new content (eval result changes) — for a library, a nested
  library, a library's data file, and a host data file (`import_xyz`).
- Several files changed at once → **one** command, one report.
- Parameter edits in the library (add at end, add in middle, remove, reorder,
  rename, compatible retype, incompatible retype) → host wires follow
  `param_id`; removed parameter drops exactly its wire (in the report);
  incompatible retype is flagged by validation, not dropped. Include a host call
  site **inside a HOF body**.
- Network removed from the library → host instances show "Unknown node type",
  are preserved, save byte-identically (same guarantee as §8), and are listed
  in the report.
- Retarget from `lib_v2.cnnd` to `lib_v3.cnnd` created by copying v2 and
  editing it → wires preserved by `param_id`; host file's node type names
  unchanged.
- Output-pin list change → affected host wires from pin ≥ 1 listed in the report
  (§7.3).
- **Undo round trips** (the core of D9), for each of: clean refresh, refresh
  that dropped wires, refresh that removed a used network, retarget, data-file
  refresh:
  - undo restores the host (text-format dump of every local network
    byte-identical to before) **and** the old mounted content (mount
    fingerprint identical), and evaluation gives the old result;
  - redo restores the refreshed state exactly;
  - an edit made *before* the refresh can still be undone after undoing the
    refresh, and one made *after* it can be undone before it (interleaving);
  - the undo stack is never cleared by a refresh.
- After undoing a refresh, `check_dependencies` does **not** refresh again
  (`last_seen` rule); an explicit `refresh_library` does; a *new* disk change
  does.
- Opening a host whose stored hash differs → `take_load_library_report` says so.

### Phase 4 — Flutter

Work: `library_links_api.rs` + codegen; model fields and methods; `mountFor`;
focus listener; panel (tree label, dimming, icon, badges, menus, drag rules);
read-only canvas and property panel; link / change-file / refresh-report
dialogs; File menu: *Link library…*, *Refresh all dependencies*, *Import copy…*
(renamed), *Back to `host.cnnd`*.

Tests:
- Dart unit tests (`test/`): `mountFor` longest-prefix cases (mirrors the Rust
  table case for case), mount-label formatting, the alias validator.
- Rust API tests (`rust/tests/structure_designer_api/`) for the view fields:
  `read_only` set exactly on networks under a mount; add-node list excludes
  transitive mounts.
- **Manual walkthrough (maintainer)** — thin editor UI is verified by hand:
  link, browse, try every forbidden gesture on a linked canvas, edit the library
  in its own file with an internal change and return (transient snackbar), then
  with a parameter removed and return (persistent snackbar, *Details*, *Undo*,
  "older than disk" marker, *Refresh*), change a library while atomCAD keeps
  focus (poll picks it up), retarget v2→v3, unlink refused/allowed, missing
  file badge. The Flutter
  smoke test (`flutter test integration_test/`) is also run by the maintainer,
  not by an agent.

### Phase 5 — Save As dependency copy and project bundle (Rust + Flutter)

Work: `NodeData::file_paths` for the file-reading nodes (`import_xyz`,
`import_cif`, `import_cube`, `build_script`, `mechanosynth`, `ops_library`;
`export_atoms` / `export_build_script` write outputs and are not dependencies);
`file_dependencies.rs` (collection, target computation, status, copy-then-write,
bundle, `link_library_copying`); API functions; the Flutter Save As dialog,
*Export project bundle…*, and the copy-in offer of the link dialog.

Fixture layout for this phase (built in a `TempDir`): `ws/proj1/host.cnnd`
linking `../libs/demolib.cnnd` and `libs_local/b.cnnd`; `ws/libs/demolib.cnnd`
linking `common.cnnd` and using `import_xyz("tip.xyz")`; `ws/libs/common.cnnd`;
`ws/libs/tip.xyz`; `ws/proj1/data/local.xyz` used by the host; one host
`import_cif` with an absolute path (external).

Tests (`file_dependencies_test.rs`):
- Collection is transitive and complete: both libraries, `common.cnnd`,
  `tip.xyz` (via the library), `local.xyz`; the absolute CIF is *External*;
  nothing is listed twice.
- Target computation: Save As to `ws/proj2/` → `../libs/*` entries are
  *AlreadyThere* (same file), `libs_local/b.cnnd` and `data/local.xyz` are
  *WillCopy* into `ws/proj2/`; Save As to `other/deep/proj/` → `../libs/*` go to
  `other/deep/libs/*` (group *Outside*); a target above the filesystem root →
  refused.
- Status: identical content at target → *AlreadyThere*; different content →
  *Conflict*; `overwrite_conflicts = false` keeps the existing file, `true`
  replaces it.
- **Round trip**: after `save_as_with_dependencies(copy = true)` the new host
  opens with every mount `Loaded` and evaluates identically to the original
  (eval equivalence), and every path string in every file is byte-identical to
  before.
- Save without dependencies: host written, paths verbatim, mounts `Missing` on
  reopen, nothing lost (§8).
- All-or-nothing: make one copy fail (read-only target file) → host not written,
  error names the file, already-copied files left in place and reported.
- `has_wired_paths` set when a file-reading node's path pin is wired.
- After Save As, watched paths are re-resolved against the new folder: a
  *keep existing* conflict with different content is refreshed (one command,
  report); *Save without dependencies* leaves the mounts `Missing`.
- `link_library_copying`: a library (with a data file and a nested link) from a
  second temp root is copied next to the host and linked; paths inside it are
  unchanged.
- Bundle: zip root is `ws/`; unzip into an empty temp dir; the host opens with
  every mount `Loaded` and evaluates identically; the external CIF is reported
  and absent.
- Manual walkthrough: the dialog's three groups, the conflict choice, the
  full-path display for *Outside* entries, the copy-in offer when linking a file
  from another drive.

### Phase 6 — CLI, vendoring, documentation

Work: CLI `libraries` subcommand (`list`, `link <path> <alias>`, `refresh
[<mount>]`, `unlink <alias>`) over new HTTP routes in
`lib/ai_assistant/http_server.dart`; `query` shows a `# linked from …`
header for a linked network; *Make local copy* (vendoring, §10).

Docs (part of "done", per `AGENTS.md`):
- Reference guide: new page `doc/reference_guide/library_linking.md` linked
  from the hub; update `ui.md` (File menu, panel, read-only canvas) and
  `headless_cli.md`.
- `AGENTS.md` updates: `serialization/AGENTS.md` (the `imports` field, v9, save
  filtering, "never write under a mount"); `atomcad-structure-designer/src/AGENTS.md`
  (library links: mount-by-prefix, `ensure_editable` is required for every new
  content-mutating entry point, `base_dir_for_eval` is the only way to resolve a
  relative path at eval time); `lib/structure_designer/AGENTS.md` (read-only
  rendering, `mountFor`).
- `doc/cnnd_versioning.md`: note v9.

Tests: CLI routes covered by Rust tests of the underlying `StructureDesigner`
methods (already in P1–P3); a vendoring test (vendor → save → the file now
contains the networks and no `imports` entry; undo restores the mount).

## 12. Forward compatibility with multiple open documents

The multi-document design will want each open file to be editable in its own
window/tab. What this design already provides:

- A precise notion of **which file owns which entity** (`mount_containing` →
  `LibraryMount.abs_path`); "editable" becomes "owned by the document being
  edited", with the same `ensure_editable` choke.
- Save that writes **only owned content** and the import list.
- Refresh with call-site repair, which is exactly what should happen in document
  A when document B (a library of A) is saved.

What it will need to change: `ensure_editable` gains a notion of the current
document, and — if two documents may share one mounted library — mounts may be
shared by absolute path rather than duplicated per importer. Mount-by-prefix
(D1) can stay: the prefix is a presentation of ownership, and names inside a
library file never contain it.

## 13. Resolved questions

1. **Dotted aliases (2026-09-28)** — allowed (`libs.demolib`); rules in D3.
2. **Camera / canvas state of library networks (2026-09-28)** — a library file saves its own
   view state as any `.cnnd` does; when linked, that state is ignored on mount
   and browsing changes are session-only (§5.4).
3. **Refreshing changed dependencies (revised 2026-09-29)** — everything that
   changed is refreshed automatically (focus, poll, save, open), as one undoable
   step with a report; manual *Refresh* per mount and *Refresh all
   dependencies*. The earlier safe/unsafe classification was dropped as too
   complex for implementers and users alike (D7, D9, D10).
4. **Paths and moving files (2026-09-29)** — relative paths may use `..` and
   are never rewritten; Save As copies dependencies (libraries and data files)
   into the same relative layout around the new location, via a dialog; absolute
   paths are refused for libraries and treated as external for data files;
   *Export project bundle* zips the file with its dependencies (D6, D11).
