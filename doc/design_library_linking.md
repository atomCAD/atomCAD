# Design: Library Linking — `.cnnd` files that reference other `.cnnd` files

Related issues: #434 (load two or more files at once), #242 (its older
duplicate), #84 (thumbnails — orthogonal, not covered here).

This document covers **linking only**: a `.cnnd` file declares that it uses
another `.cnnd` file, whose networks and record types then appear read-only in
the importer. Having **several documents open and editable at once** is a
separate, later design; this one is written so that design becomes an
extension rather than a rewrite (§12).

**Terms used throughout.** The **host** is the file open in the editor. A
**library** is a `.cnnd` file linked by another file. The **importing file**
of a link is whichever file declares it — the host for a direct link, a
library for a nested one; rules stated for "the importing file" apply at every
level. A **mount** is a linked library's content in the registry, under its
**mount path** (`demolib`, `demolib.common`). **Local** content is content of
the host itself, i.e. not under any mount path. A **frozen** node is a node
whose reference into a mount does not resolve (§8). **P1…P6** are the phases
of §11.

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
  file…* (retarget), *Rename alias…*, *Make local copy* (§10), *Unlink*.
  *File > Refresh all dependencies* refreshes every library and data file.
- A dependency changed on disk is **refreshed automatically** — when the window
  regains focus, and by a light poll while it has focus (D7, D10). A snackbar
  says what happened: "Refreshed demolib", or, if something was disconnected,
  "Refreshed demolib — 3 wires disconnected [Details] [Undo]". The refresh is
  one undo step. While there is something to redo, the refresh is **held**
  instead, so it cannot wipe the redo history: the mount shows an "older than
  disk" marker and applies the change after the next edit (or at once on
  *Refresh*).
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
  `check_delete_references`). "Uses" is every reference kind of §8 — instances,
  record schemas, **and** `DataType`s in node data that name a record under the
  mount — so unlinking can never turn a working host node into a frozen one.

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
  (A host reference *into* a transitive mount is still recorded, in the direct
  import's `uses` table — D13.)
- Each import carries the interfaces the host is wired against (`uses`, D13),
  taken from memory, not from disk.

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
- **on open**: the host always opens with the libraries as they are on disk
  now, and **reconciles its wiring against the interfaces it recorded at its
  last save** (`uses`, D13) — the §7.1 procedure with the old interface read
  from the file instead of captured from memory. If an interface changed, the
  open report carries the same repair report (§7.2) a refresh would have
  produced. If only the content changed (`hash` differs, interfaces equal), it
  says "demolib changed since this file was last saved". Nested libraries
  reconcile against *their* recorded `uses` the same way, in memory, at mount
  time. Data files have no recorded state; they are simply loaded as they are.
  Opening is not an undo step — there is no "before" to go back to.

The check is skipped while a drag, a text edit, or a modal dialog is active and
runs at the next idle moment. Flutter does the skipping (§5.3); Rust must not
depend on it — see the P4 item on Rust-side interactions.

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
  them, and an automatic one never truncates the redo tail (below). The command holds:
  - for each refreshed mount, the **previously mounted content** (the networks,
    record defs, folders and nested mounts that were under the mount path — it
    is already in memory, it is simply kept instead of dropped) and the new
    content, together with the `LibraryMount` record before and after (for a
    retarget that includes `rel_path` / `abs_path`, so undo points the alias
    back at the old file; status and stamps are part of it too);
  - for each refreshed host data file, the node data before and after
    (like `SetNodeDataCommand`);
  - each refreshed mount's `stored_uses` before and after (§7.1 step 1), so a
    save after undo or redo records the interfaces the host is wired against
    in that state (D13);
  - **snapshots of every host network that refers to the mount** (§7.1 steps 2
    and 7), before the re-mount and after validation — including those whose
    referenced network or record def disappeared.

  Undo swaps the old content and the old host networks back; redo swaps the
  new ones in. Earlier commands stay consistent because undoing past a refresh
  always undoes the refresh first — they are replayed against the interface
  they were recorded against. Undo leaves memory older than the disk: the
  affected mounts get status `OlderThanDisk` (a marker in the panel), and, by
  D7's `last_seen` rule, no automatic refresh follows; *Refresh* or *Refresh all
  dependencies* brings them forward again.
- **An automatic refresh never destroys redo history.** Pushing a command
  truncates the redo tail (`UndoStack::push`), and a refresh cannot be slotted
  in *before* the redo tail either: the redoable commands were recorded against
  the old interface. So while the redo tail is **non-empty**, a detected change
  is **held**, not applied: the mount gets status `ChangedOnDisk` (the same
  neutral marker as `OlderThanDisk`, click = *Refresh*), and a transient
  snackbar says "demolib changed on disk — refresh held while redo is
  available [Refresh]". A held change does **not** advance `last_seen` (D7), so
  it stays detected. The hold ends by itself: the next command the user pushes
  truncates the redo tail anyway, and the next check (poll or focus) finds the
  change still pending and applies it normally. Undoing further while a change
  is held keeps it held. An explicit *Refresh* / *Refresh all dependencies* / *Retarget* is
  a user action like any edit and truncates the redo tail as edits do. This is
  the one exception to D10's "no pending state", and it is keyed on a fact Rust
  already owns (`cursor < history.len()`), so there is no classification to get
  wrong.
- **Ctrl+Z can undo a refresh the user did not ask for.** An automatic refresh
  is an ordinary entry in the history, so a user who presses Ctrl+Z to take
  back their own last edit may take back the refresh instead. This is accepted
  — an edit made *before* the refresh cannot be undone without undoing the
  refresh first (the argument above) — and mitigated: the undo snackbar names
  the command ("Undid: Refresh demolib"), and the mount then shows the
  `OlderThanDisk` marker.

### D10 — Refresh everything that changed, automatically

Every detected change is refreshed immediately — there is no "safe / unsafe"
classification and no pending state, with one exception keyed on the undo
stack rather than on the change: while redo history exists, the refresh is
held so that it does not destroy it (D9). This is what editors do with a file
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
If any copy fails, the host is not written and the error lists the file; the
copies already made stay in place and are listed too (they are copies, so
nothing of the user's is lost, and removing them could delete a file that was
there before). A target that would climb above the filesystem root is refused.

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
  { "alias": "demolib", "path": "libs/demolib_v3.cnnd", "hash": "b3:…", "uses": { … } }
]
```

`#[serde(default, skip_serializing_if = "Vec::is_empty")]`, sorted by alias.
`SERIALIZATION_VERSION` goes 8 → 9 **with no migration pass** (purely additive).
The bump is deliberate: an older atomCAD ignores unknown fields, so it would
open a linking host *without* its libraries and — on save — destroy it. Refusing
("file is from a newer version") is the right failure.

Files without imports serialize identically apart from the version number;
fixtures and snapshots that pin the version string are updated once.

Each import also carries a `uses` table — the interfaces the host is wired
against (D13):

```json
"imports": [{
  "alias": "demolib", "path": "libs/demolib_v3.cnnd", "hash": "b3:…",
  "uses": {
    "networks": {
      "half_space": { "params":  [{ "id": 3, "name": "miller", "type": "IVec3" }],
                      "outputs": [{ "name": "shape", "type": "Blueprint" }] },
      "common.slab": { "params": [ … ], "outputs": [ … ] }
    },
    "records": {
      "Miller": { "fields": [{ "id": 1, "name": "h", "type": "Int" }, … ] }
    }
  }
}]
```

Keys are names **relative to the import's alias** (`half_space` means
`demolib.half_space`; `common.slab` is a reference into a transitive mount,
D4). Both maps are `BTreeMap`s and omitted when empty, so the output is
deterministic. Types are written with the same `DataType` string form the
file already uses for parameters.

### D13 — The host records the interfaces it is wired against

**The problem.** Wires are stored **positionally**: an `Argument` is a list of
incoming wires and nothing else, and the serialized `SerializableParameter` has
no `param_id`. Wires still survive interface edits today because of a
self-check that only works inside one file: every network stores its own last
known interface (`node_type.parameters`), and on load `check_interface_changed`
(`network_validator.rs:445`) compares it with the network's actual parameter
nodes and, on a difference, runs `repair_call_sites_for_network` (id first, name
second) over every call site. The "old" interface is always there because the
network and all its callers are in one file.

Linking breaks that premise. The host calls networks whose stored interface
lives in *another* file, and that file changes while the host is closed. With
nothing to compare against, the host's positional arguments would be realigned
by count alone (`repair_network_arguments`) — wires shift silently. The same
holds for host `record_construct` / `record_destructure` / `product` nodes on a
linked record def (their field-id matching in `set_custom_node_type` needs the
previous pin layout, which a fresh load does not have), and one level down for
a library's calls into its own imports. And the most common workflow hits it:
*Open library file* replaces the document (§5.3), so "edit the library, go
back to the host" is always a fresh open, never a refresh.

**The decision.** The host stores, per import, the interface of every mounted
network and record def it references — the `uses` table (D12) — and **opening
is a refresh from the stored interface**. The one procedure of §7.1 serves
both: a refresh captures the old interface from memory, an open reads it from
the file; the repair and the report are shared.

- **What is recorded.** Every name under the import's mount (direct *or*
  transitive) that a **local** node of the importing file refers to as a
  custom-network instance (`node_type_name`) or a record node (schema /
  target), recursively into HOF bodies. The walk is the same one
  `unresolved_mount_ref` (§8) uses; of the reference kinds it finds, only these
  two produce entries.
  Networks record `params` (`param_id`, name, type) and `outputs` (name, type);
  record defs record `fields` (`FieldId`, name, type). Named-record types used
  only as *types* (a parameter typed `demolib.Miller`) need no entry: they carry
  no wire position.
- **Where the recorded interface comes from.** From the **in-memory** content
  the host's wires have been reconciled against — never from the file on disk.
  The invariant that makes this correct: after every load, refresh, undo and
  redo, the host is wired against the in-memory interface of every mounted name
  it resolves. So saving right after an undo of a refresh (`OlderThanDisk`)
  records the *old* interface, and the next open repairs forward from it.
- **Unresolved names carry over.** A name the host refers to that does not
  currently resolve (mount `Missing`, or the name was dropped and the node is
  frozen, §8) keeps the entry read from the file, verbatim. Mounts therefore
  keep the `uses` they were loaded with (`LibraryMount.stored_uses`), and save
  merges: resolved names from memory, unresolved names from `stored_uses`.
  A missing library thus saves byte-identically (§8).
- **The hash becomes a hint.** `hash` no longer decides anything about wiring;
  it only lets the open report say "demolib changed since this file was last
  saved" even when no interface changed (internal edits). It is the hash of
  the **loaded** content, for the same reason as above.
- **Nested libraries do the same.** A library file records `uses` for its own
  imports; when `demolib.cnnd` is mounted and `common.cnnd` has changed since
  demolib was last saved, demolib's calls into `common` are repaired **in
  memory** at mount time, from demolib's own `uses`. Linked content is never
  saved (D5), so this repair is recomputed identically on every load — until
  someone opens and re-saves demolib.
- **`param_id` stability is what makes this work** across versions:
  `demolib_v3.cnnd` made by copying v2 keeps v2's ids, and a library edited in
  place keeps its ids through renames and reorders (the F1 discipline of
  `doc/design_parameter_wire_stability.md`). When ids are absent or were
  re-assigned by the load-time duplicate-id heal
  (`dedupe_param_ids_in_network`), matching falls back to names — the worst
  case is a *dropped* wire, which is reported, never a wire moved to the wrong
  pin.
- **Record fields need the same stability, and before P3 they do not have it.**
  Found in P1: `FieldId`s are not saved — they are re-assigned in authored
  order on every load — so a library that inserts a field at the front shifts
  every id, and the ids in `uses` identify nothing across versions. Decision
  (§13 item 7): P3 starts by persisting field ids and both id counters, so
  that neither kind of id is re-assigned or recycled across a save.

**Rejected alternative: a `param_id` on every stored argument.** It is the
general fix, but it changes the serialized shape of every node, it is exactly
the "identity-keyed wires" refactor `doc/design_parameter_wire_stability.md` §5
parked for lack of justification, it covers neither record fields nor output
pins by itself, and it bloats every local instance, which never needs it. D13
extends the mechanism that already works to the one place its premise fails,
and changes nothing for a file without imports.

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
    pub status: MountStatus,         // Loaded | OlderThanDisk | ChangedOnDisk | Missing | Error(String) | Cycle
    pub loaded: Option<FileStamp>,   // (mtime, size, blake3) of the content in memory
    pub last_seen: Option<FileStamp>,// last observed on disk (D7)
    pub stored_hash: Option<String>, // from the importing file (D13: a hint only)
    pub stored_uses: UsedInterfaces, // `uses` read from the importing file (D13)
}

/// The interfaces an importing file is wired against, keyed by name relative
/// to the import's alias. Serialized as the `uses` table (D12).
pub struct UsedInterfaces {
    pub networks: BTreeMap<String, UsedNetworkInterface>, // params (id, name, type) + outputs (name, type)
    pub records: BTreeMap<String, UsedRecordInterface>,   // fields (FieldId, name, type)
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
- **Load** (`load_node_networks_from_file`): mount each import **before** the
  local networks are inserted — each local network is repaired as it is
  inserted, and that repair must already know which names are under a mount
  or it realigns the nodes that should be frozen (§8) (status failures are
  recorded, never fatal); then
  **reconcile** (`reconcile_used_interfaces`, D13): for every entry of each
  mount's `stored_uses` whose name resolves and whose interface differs from
  the mounted one, repair the file's **local** call sites and record nodes from
  the stored interface to the current one (§7.1 step 5, same code). Then
  `StructureDesigner::load_node_networks` validates **everything** in dependency
  order as today. The same sequence runs inside every recursive mount, so a
  library reconciles its own calls into its imports before it is prefixed.

  The load-time heals that today run only in `StructureDesigner::load_node_networks`
  — above all the duplicate-`param_id` heal (`dedupe_param_ids_in_network`) —
  must also run on a library's temp registry inside `mount_library`, **before**
  the importing file reconciles against it: reconciliation matches by
  `param_id`, so it must see the ids the library will actually have.

  **The order is load-bearing:** reconciliation must run after mounting and
  **before** `repair_network_arguments` or any custom-node-type cache rebuild
  with `refresh_args = true` — either of those realigns arguments by count and
  leaves the id match nothing to work with. (Record nodes: install a previous
  custom node type built from the stored fields — `Parameter { id: FieldId,
  name, type }` — then rebuild with `refresh_args = true`, so
  `set_custom_node_type`'s existing id-first matching moves the wires.)
- **Save**: filter by `mount_containing(name).is_none()` for networks, record
  defs, folders and CLI rules; write `imports` from direct mounts, with the
  hash of the loaded content and the `uses` table built by
  `collect_used_interfaces` (D13: resolved names from memory, unresolved names
  from `stored_uses`); paths written verbatim (D6).
- **Dependencies** (`file_dependencies.rs`, D11): `collect_file_dependencies(target_dir)`
  (transitive: imports + relative data-file paths of host and libraries, with
  per-entry target path and status), `save_as_with_dependencies(path, choice)`
  (copies first, host last; the host is written only if every copy succeeded),
  `export_project_bundle(zip_path)`. Data-file
  paths are reported by the nodes themselves through a
  `file_paths(&self) -> Vec<&str>` method on `NodeData` (default: none), so a
  new file-reading node is picked up by overriding one method. The same module
  keeps the watch state for data files (`FileStamp` `loaded` / `last_seen`, D7);
  libraries keep theirs on `LibraryMount`.
- **Read-only guard** (§6), **refresh + call-site remap** (§7), **eval base dir**
  (D8), **check-on-disk** (D7).
- **Disk access** for all of the above goes through one small trait, `LinkFs`
  (read, stat, copy, write, rename), with the real filesystem as the only
  production impl; the tests substitute a fault-injecting impl (§11.1). Two
  rules it makes enforceable: a library is **read once** per mount or refresh,
  and its stamp and hash are computed from exactly the bytes that were parsed;
  and every `.cnnd` write (host save, Save As, copies) writes a temp file in
  the target directory and renames it over the target, so a failed write
  never leaves a truncated file. (If today's save does not already do this,
  this design makes it so.)

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
| `check_dependencies()` | `Option<APIRefreshReport>` — stats every watched file (D7), refreshes whatever changed in one undoable command (D10) — or, while the redo tail is non-empty, holds it and reports it as `held` (D9); `None` when nothing was refreshed and nothing was newly held |
| `get_linked_libraries()` | `Vec<APILibraryMount>` (no disk access) |
| `take_load_library_report()` | `Option<APIRefreshReport>` — the report of the last file open (D7, D13), drained like `take_load_param_id_repairs` |
| `collect_file_dependencies(target_path)` | `APIDependencyPlan { entries: Vec<APIDependency { source_abs, target_abs, rel_path, kind: Library\|DataFile, group: Inside\|Outside\|External, status: WillCopy\|AlreadyThere\|Conflict }>, has_wired_paths }` |
| `save_as_with_dependencies(path, copy: bool, overwrite_conflicts: bool)` | `APIResult` |
| `export_project_bundle(zip_path)` | `APIResult` with the list of external files left out |

`APILibraryMount { mount_path, alias, rel_path, abs_path, file_name, parent,
direct, status, status_message }`; `APIRefreshReport { refreshed_mounts,
refreshed_data_files, dropped_wires: Vec<APIDroppedWire { network, scope_path,
node_id, pin_name, reason }>, removed_networks_in_use, frozen_nodes, held,
errors }` — dropped wires and frozen nodes are navigable, like validation
errors; `held` lists the changes not applied because redo history exists (D9).
`is_clean()` decides between the transient and the persistent snackbar.

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
  report or the persistent one with *Details* / *Undo* otherwise, or the
  "refresh held while redo is available [Refresh]" snackbar for a report that
  is only `held` (shown once per newly held change, not on every poll). Checks are
  skipped while a drag, a text edit, or a modal dialog is active. There is no
  focus handling in `lib/` today; this is the first.
- **Panel** (`node_networks_list/`): tree view renders a mount folder's label as
  `alias — file_name` (file name `AppTextStyles` small, dimmed); every row under
  a mount dimmed with a link icon; mount folders get the error badge
  (`Missing` / `Error` / `Cycle`) and, after an undo of a refresh or while a
  refresh is held (`OlderThanDisk` / `ChangedOnDisk`), a small neutral "older
  than disk" marker whose click is *Refresh* — the badges go in
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
  refresh report (dropped wires, frozen nodes, removed networks and record
  defs, each navigable). All
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
the canonical `.cnnd` JSON of every network + record def + folder under the
mount, view state stripped — the same serialization as the local fingerprint
O1 (§11.1), and JSON for the same reason: it records wires by exact pin index. The enforcement test (§11,
P2) runs the whole mutation alphabet against linked targets and asserts both
`Err` **and** an unchanged fingerprint; a guard that errors *after* partially
mutating fails it.

## 7. Refresh, retarget, and call-site repair

### 7.1 Procedure (refresh and retarget alike)

All changed files found by one check are handled by one
`RefreshDependenciesCommand`. Opening a file runs steps 4–6 and the
validation of step 7, with the old interface read from the file's `uses`
table instead of captured in step 1, and without snapshots or a command — an
open is not an undo step (D7, D13).

1. Capture the **old interface** of every name under each affected mount that
   a host local node refers to, as a `UsedInterfaces` (D13) — the same
   structure the file stores, built by the same `collect_used_interfaces`.
   Write it into the mount's `stored_uses` too (the command keeps the previous
   `stored_uses` for undo): a name this refresh drops must carry over the
   interface its frozen nodes are actually wired against, not whatever the
   file said at load time.
2. **Snapshot the affected host nodes, before anything touches them.** The
   affected set is every host node (in every local network, recursively into
   HOF bodies) that refers to a name under an affected mount by **any**
   reference kind of §8 — instance, record schema / target, or a `DataType` in
   its data — found by the `unresolved_mount_ref` walk. Snapshotting *every* referring
   node rather than only "call sites of a network whose interface changed" is
   deliberate: which ones change is not known until after the re-mount, and
   the ones whose target **disappears** are exactly the ones a
   changed-interface filter would miss. The snapshot is per containing local
   network, taken as its node map (cheap: only networks that reference the
   mount).
3. Detach everything under the mount path from the registry (networks, record
   defs, folders, nested mounts) and **keep it in the command** (D9).
4. Mount again (for retarget: from the new path). A changed data file used by
   a library refreshes that library's mount; a changed data file used by the
   host keeps its node data in the command and re-reads the file.
5. For each re-mounted name whose interface differs from the old one, run the
   identity-based repair (`reconcile_used_interfaces`, shared with load):
   instances through `repair_call_sites_for_network`
   (`network_validator.rs:279`, today private and only triggered by an
   in-memory interface change) with the old parameters and the new ones. It
   maps by `param_id` first, name second — which is exactly right across
   versions, because `demolib_v3.cnnd` made by copying v2 **keeps v2's
   `param_id`s** for surviving parameters. Record nodes on a changed def are
   repaired by the existing field-id matching of `set_custom_node_type`, given
   a previous custom node type built from the old fields. Nodes whose target no
   longer exists are frozen (§8), not repaired.

   **The repair is restricted to the importing file's local networks.**
   `repair_call_sites_for_network` today walks every parent found by
   `find_parent_networks`, and in one registry that includes the linked
   networks that call the refreshed network (`demolib.b` calling
   `demolib.a`). Those were just re-mounted and are already wired against the
   new interface; applying the old → new mapping to them again would corrupt
   them. The function gains a parent filter (`mount_containing(parent) ==
   the importing file's own mount`, i.e. `None` for the host).
6. Collect every wire the repair dropped, and every frozen node, into the
   report.
7. Validate everything in dependency order, then **snapshot the affected host
   networks again** — after validation, because validation-time repair is part
   of what the refresh did to them. Push the command (D9 for when an automatic
   refresh is held instead); set dirty if a host node changed, a recorded
   interface changed, or a direct import's hash changed (the next save writes a
   different `uses` / `hash`); full refresh. An open that reconciled anything
   sets dirty for the same reason — not saving is harmless (the next open
   repeats the same reconciliation from the same recorded interfaces), but the
   title bar should say the file in memory is not the file on disk.

Undo restores the step-2 snapshots and redo the step-7 ones, wholesale per
network, so the round trip is exact whatever the repair and validation passes
did in between.

### 7.2 Report

The report is the same whether it comes from a refresh or from an open
(`take_load_library_report`). It lists:

- **dropped wires** — a parameter or record field that no longer exists, or an
  output pin that no longer exists (the output-pin-count pass drops those);
- **flagged wires** — kept, but now incompatible after a retype; validation
  marks them, consistent with the existing repair;
- **frozen nodes** — host nodes whose referenced network or record def
  disappeared (§8); they show "Unknown node type" / "Unknown record type";
- **output-pin warnings** — §7.3;
- **mount status changes** (`Missing`, `Error`, …).

**Every wire that is gone after the operation appears in this report** — that
is the property the wire ledger (O2, §11.1) tests. A report with only status
changes to `Loaded` is clean → transient snackbar; anything else → persistent
snackbar whose *Details* opens a dialog with navigable rows (D10).

### 7.3 Known limitation

Output pins are matched positionally. A library version that inserts or
reorders a network's output pins mis-wires host consumers of pin ≥ 1. (One
that *removes* output pins drops the wires from them; those are listed as
dropped wires, §7.2.) The report lists every host wire from an output pin ≥ 1
of a network whose output list changed, so it is visible, not silent — on open as well as on refresh,
because the recorded interface (D13) includes the output list. Real output-pin
identity is P5 of `doc/design_identity_vs_naming.md`. (The recorded output
*names* would also allow matching outputs by name, turning a reorder into a
no-op and a rename into a reported drop; that is a possible later extension,
not part of v1.)

**Function values (settled in P3).** An `apply` whose `f` is fed by a linked
network instance's function pin (`-1`, the text format's `@node`) derives its
argument layout from that network's parameters, and a parameter reorder in the
library did mis-wire its `arg0…` wires. Reconciliation moves them with the
instance's own parameter mapping, on refresh and on open alike (P3 status).

## 8. Missing libraries must be lossless

If `libs/demolib_v3.cnnd` is missing, the host must still open, show errors,
and **save byte-identically** — every host node that refers to `demolib.*` and
its wires must survive. The same guarantee holds when a *loaded* library stops
defining a name the host uses (a network or record def removed by a refresh or
retarget, §7).

A host node refers to a mounted name as an instance, through a record
schema, or through a type. The first two fail on the node itself:

- **Instances of a linked network** (`node_type_name == "demolib.half_space"`).
  These are already safe: when the type does not resolve,
  `repair_network_arguments` returns early and leaves `arguments` alone.
- **Record nodes whose schema names a linked record def** — `record_construct`
  / `record_destructure` (`schema`) and `product` (`target`) with
  `"demolib.Miller"`. Their `node_type_name` is a built-in, so they resolve;
  but `build_node_type_for_schema_with_defs` builds them with **zero
  parameters** when the def is missing (`record_construct.rs`, the `else`
  branch), and `repair_network_arguments` then **truncates their arguments to
  zero**. Every wire into them is lost, on load and on refresh alike. This is
  the real hazard.

Wires are also lost on the **outgoing** side and through **type-only**
references, because several passes drop a wire by looking at the *source*
node's type or at a type check:

- **Wires out of a frozen `record_destructure`.** With its def missing it is
  built with a single placeholder `result` pin (`record_destructure.rs`, the
  `_ =>` arm). The output-pin-count passes (`repair_output_pin_wires` in
  `network_validator.rs`, the count check in `repair_node_network`) then drop
  every consumer wire from pin ≥ 1, and `repair_node_network`'s `FieldId`
  remap (R3) treats every field as deleted and drops the rest.
- **Wires whose type check mentions an unresolved name.** A dangling
  `Named(_)` is "incompatible with anything" (`data_type.rs`), and
  `repair_zone_body` drops body wires whose zone-input type no longer
  converts. A `map` over `Array[demolib.Miller]` thus loses every body wire
  reading `element` when demolib is missing — a host node that refers to the
  library only through a *type* (HOF `input_type`, a parameter or a local
  record field typed `demolib.Miller`).
- `repair_node_network` runs after **every** structural edit
  (`finish_node_structure_edit`), not only on load and refresh, so these are
  hit the next time the user edits the network, too.

The rule has two parts:

1. **A host node that refers to an unresolved name under a mount path is
   frozen.** Its custom node type is not rebuilt, its `arguments` (and
   `zone_output_arguments`, `function_pin_roles`) are neither grown nor
   truncated, and no repair pass disconnects a wire **into or out of** it. It
   shows a blocking error ("Unknown node type `demolib.x`" / "Unknown record
   type `demolib.Miller`") and cone-poisons as usual.
2. **No repair pass disconnects a wire whose compatibility check involves an
   unresolved name under a mount** (a `DataType` walk for a `Named` under a
   mount), wherever that wire is — including inside the body of a frozen HOF.
   Validation reports such a wire as an error instead.

"Refers to" is one predicate, `unresolved_mount_ref(node, registry) ->
Option<String>`, which checks `node_type_name`, the record schema strings,
**and** every `DataType` stored in the node's data (HOF element/accumulator
types, parameter types, `closure` signatures). "Under a mount" is
`mount_containing(name).is_some()`, **whatever the mount's status** (a
`Loaded` mount whose new version dropped the name counts). Unknown names
outside any mount keep today's behaviour.

After a refresh a frozen node keeps its **last resolved** custom node type
(already in memory), so it still draws with its pins. **On load, a frozen
node gets its layout from the recorded interface** (§13 item 6, P2): the
`uses` entry of the name it refers to — carried over for every unresolved name
by D13 — gives an instance its parameters and outputs and a record node its
fields, so it keeps its pin names, draws with its pins, and can be written in
the text format (done in P2). A frozen node whose name has no `uses` entry (a
hand-edited file) still has no layout on load: an instance is not drawn, a
record node draws with no input pins, and the Flutter canvas skips wires to
pins it does not draw (`ScopeResolver` returns `null` for them — checked in
P1); nothing panics, everything survives save, and the text format spells its
wires positionally.

Byte-identical save also covers the import entry itself: a `Missing` mount
writes back its `hash` and its `uses` table exactly as read (`stored_hash`,
`stored_uses`, D13), since there is no loaded content to derive them from.

**P1 starts with a red-first test for exactly this**, using a host that has
both an instance of `demolib.foo` *and* a `record_construct` on
`demolib.Miller` with wired fields — and the full frozen-node matrix of §11.1
(outgoing wires, type-only references, every repair pass) follows it in the
same phase.

## 9. Mapping to mechadense's concerns

| Concern | Answer |
|---|---|
| Names shift from files to namespaces | Mount folders show the file; thumbnails (#84) are separate |
| Library depending on its user | Impossible: a library file never sees its importers; linked content is read-only |
| Renaming someone else's library | Not possible in the importer; write a local wrapper network |
| Monster all-in-one file for debugging | Files stay separate; *Export project bundle* zips the file with every dependency (D11), or vendor (§10) when one file is needed |
| Swap demolib underneath | Retarget the alias (§7) |
| Fragile wires on swap | Call sites repaired by `param_id` on refresh (§7.1) and on reopen, from the recorded interfaces (D13); limitation on output pins (§7.3) |

## 10. Existing copy import and vendoring

- The current *File > Import from .cnnd library* becomes **Import copy…**,
  behaviour unchanged. Its record-def gap is a separate bug; this design does
  not depend on fixing it.
- **Vendor** (optional, P6): *Make local copy* on a mount folder turns a mount
  into local content. Under D1/D3 that is just removing the mount record — the
  networks are already in the registry under `demolib.*` and will now be saved.
  Nested mounts are vendored with it. Undoable (restore the mount record).
  Offered only on a direct mount with status `Loaded` (there is nothing to
  vendor from a missing file); the host's `imports` entry for it is removed,
  and the vendored content takes the mount's in-memory state, not the disk's.
  (As built, P6: any status with content — `OlderThanDisk` / `ChangedOnDisk`
  too, which is exactly when "in memory, not the disk's" matters — and only
  when every nested mount has content as well. Refused while anything refers
  to a name under the mount that does not resolve: a frozen node is safe only
  under a mount. Relative data-file paths of the library's nodes are rebased
  onto the host's folder, or they would silently read other files.)

## 11. Phased implementation plan

Each phase ends green: `cargo test -j 4` (plus `--test structure_designer_api`
and `--test integration` explicitly), `cargo clippy`, `cargo fmt`,
`flutter analyze` — including the §11.1 oracles and, from P3 on, the fuzz seed
set. No test of this design is ever marked `#[ignore]` to get a phase green; a
test that cannot pass yet is a bug in the phase, not in the test. Tests go in
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

### 11.1 Testing strategy — no silent loss of user work

The requirement that outranks every other in this design: **a user never loses
an edit without being told.** Linking adds new ways to lose work that did not
exist before — a library changing underneath the host, passes that realign or
drop wires they cannot resolve, automatic commands in the undo history, and
file copies at Save As. Example-based tests per feature (the phase lists below)
are necessary but not enough: each one proves the case its author thought of.
So the suite is built on **oracles that every test applies**, a **randomized
harness** that looks for sequences nobody thought of, and **fault injection**
for the disk.

**What counts as user work.** Local networks (nodes, node data, wires including
their pin indices, zone bodies, comments), local record defs, folders, CLI
access rules, the import list (aliases, paths, `uses`), **every file on disk
the user did not ask to change** (library files above all), and the **redo
history**. View state (camera, canvas viewport, selection) is not.

#### Oracles (shared helpers, land in P1, used by every later test)

They live in one test-support module,
`tests/structure_designer/library_links_support.rs`, so no test re-implements
them:

- **O1 — local fingerprint.** `local_fingerprint(designer) -> String`: the
  canonical serialized form (`.cnnd` JSON of the local content only, the same
  filter as D5, view state stripped) of everything under "user work" above
  that lives in memory. JSON rather than the text format because it records
  wires by exact pin index; a moved wire must change the fingerprint.
- **O2 — wire ledger.** `wire_ledger(before, after, report)`: for every host
  wire before an operation, it must be present after — keyed by identity like
  the existing wire-identity oracle in `invariants_test.rs`
  (`(source, source pin, dest, dest param_id / FieldId)`), so a wire that
  *followed* its parameter to a new index counts as present — **or** be listed
  in the operation's report (§7.2). A wire that is neither is a **silent
  loss** and fails the test. The ledger also fails on a wire that *appears*
  (a wire re-pointed to the wrong parameter shows up as one loss plus one
  appearance). A wire into a **frozen** node has no resolvable `param_id`; it
  is keyed by its positional index instead, which a frozen node must keep
  anyway. The ledger applies to operations that are **not supposed to change
  the host's wiring except as reported**: open, check / refresh / retarget,
  link, unlink, vendoring, rename alias, save → reopen, and an unrelated edit
  judged only on the wires it did not touch. Undo and redo are checked by O5
  instead, and a user edit is checked by its own test.
- **O3 — disk tripwire.** `DiskTripwire::arm(temp_dir)` hashes every file under
  the test's `TempDir`; `assert_only_changed(&[allowed paths])` checks that
  nothing else changed. Library files are never in the allowed list outside
  Save As copies and `link_library_copying`. Every test that touches disk
  arms one.
- **O4 — persistence.** `assert_save_reopen_identity(designer)`: save to a temp
  path, reopen in a fresh `StructureDesigner`, and require (a) O1 of the reopened
  designer equals O1 before saving (with unchanged libraries on disk, an open
  reconciles nothing), and (b) save → load → save is byte-identical.
- **O5 — undo inverse.** `assert_undo_inverse(designer, op)`: record O1 and
  every `mount_fingerprint`, run the op, undo, require both unchanged; redo,
  require the post-op state exactly (O1 and mount fingerprints). Separately,
  an automatic check run while the redo tail is non-empty leaves the redo tail
  exactly as it was (D9).
- **O6 — document invariants.** `check_document_invariants` (the Phase 0
  checker) reports nothing fatal after every step, and gains one check:
  `arguments.len()` of a node equals its type's pin count **unless** the node
  is frozen (§8), subject to the exceptions the checker already makes for
  derived layouts (`apply`) — so a frozen node that got realigned, or a
  non-frozen node left misaligned, is caught by every existing invariant test
  too.

Each oracle gets a **negative control**: a test that performs a deliberate
silent drop / wire move / library write / non-inverse undo and asserts that
the oracle *fails*. An oracle that cannot fail protects nothing.

#### The frozen-node matrix (P1, extended in P2 and P3)

Table-driven over three axes, every cell asserting O2 (nothing dropped) and
O6:

- **reference kinds**: linked-network instance (single and multi-output,
  wires in *and* out, a `-1` function-pin wire out); `record_construct`,
  `record_destructure` (wires out of pins ≥ 1), `product` on a linked def;
  type-only references — a `map` / `filter` / `fold` whose element type is
  `demolib.Miller` with body wires reading `element`, a local record def with
  a field typed `demolib.Miller` and a wired `record_construct` of it, a
  parameter typed `demolib.Miller`; a `closure` whose signature mentions a
  linked record; each of those **inside a HOF body** as well as at top level;
- **causes of non-resolution**: mount `Missing`, `Error` (unparseable file),
  `Cycle`; mount `Loaded` but the name was removed by a refresh; by a retarget;
- **passes that could touch it**: load; `validate_active_network`;
  `repair_node_network` via an unrelated structural edit in the same network
  (add a node, delete a neighbour, move, connect an unrelated wire); record-def
  add / update / rename of an unrelated **local** def (`repair_all_networks`);
  undo and redo of each; save → reopen.

And the way back: when the name resolves again (file restored, retarget back,
library re-adds the network), every frozen node **unfreezes with its wires
intact**, reconciled against the carried-over `uses` (D13) — including when
the library's interface changed while the node was frozen.

#### Randomized harness (P3, extended by each later phase)

`library_links_fuzz_test.rs`, seeded and deterministic like the existing
property suite in `invariants_test.rs` (same generator style; no new
dependency). One run is a sequence of ~40 steps drawn from:

- **host edits**: a subset of the mutation alphabet of Phase 2 applied to
  local networks, including edits in networks containing frozen nodes;
- **library edits written to disk**, as another process would: parameter add
  (end / middle) / remove / reorder / rename / retype, record field add /
  remove / reorder / rename, output pin add / reorder, network or record def
  removed and re-added, file deleted and restored, file made unparseable and
  fixed, the same edits in a nested library;
- **app actions**: `check_dependencies`, `refresh_library`, retarget to a
  sibling version, undo, redo (including runs of several), save, close and
  reopen, link and unlink when allowed.

Oracles after **every** step: O2 on the steps it applies to (the report of
that step being the only allowed explanation for a missing wire); O5 on undo
and redo steps; O3 (libraries are only written by the harness's own "library
edit" steps); O6. At the end of a run: undo all the way back and require O1
equal to the start (after a reopen, which starts a fresh history, undo back
to the reopen point instead), and O4 on the final state. Runs are kept shorter
than `max_history`, so undoing back never meets an evicted command.

CI runs a fixed seed set (sized to stay under ~30 s in a `-j 4` run); a longer
run is `LIBRARY_LINKING_FUZZ_CASES=5000 cargo test -j 4 library_links_fuzz`.
A failure prints the seed and the step list. **Every failure found, in CI or
locally, is minimized by hand into a named example-based regression test**
before it is fixed, so the fix is guarded independently of the seed set.

#### Fault injection (P3 and P5)

Disk operations go through the `LinkFs` trait (§5.1); the test impl can fail,
or change a file, at a chosen call. Tests:

- a library read fails midway, or returns a truncated/corrupt file → the mount
  goes `Error`, the previously loaded content keeps evaluating, O2 and O1
  unchanged, nothing partially mounted;
- a library changes **between** stat and read, or between two reads of one
  refresh → the stamp and hash recorded are those of the bytes actually parsed
  (the content is read **once** per refresh), so the next check detects the
  newer version rather than believing it is loaded;
- a write of the host fails midway → the file on disk is either the old file
  or the new one, never a truncated one (the temp-file-and-rename rule of
  §5.1);
- Save As copy failures (P5 list).

#### Rules for every test in this design

- Everything that writes goes through a `TempDir`, never a committed fixture
  path or a hard-coded scratch directory.
- Library edits in tests are made the way a user makes them: open the library
  file in its own `StructureDesigner`, edit through the public methods, save.
  Hand-edited JSON is used only for the corrupt-file and legacy cases.
- Change-detection tests never depend on the filesystem's timestamp
  granularity (a same-size rewrite within one mtime tick is invisible to the
  stat check by design, D7): they go through the `LinkFs` test impl, or set the
  mtime explicitly, or call `refresh_library`. No test sleeps.
- A test that asserts "nothing lost" also asserts that the thing was *there*
  (non-empty wire set, non-trivial fingerprint), so a fixture that silently
  lost its wires before the test ran cannot pass vacuously.
- Linking hosts join the **text-format round-trip corpus** and the `.cnnd`
  round-trip tests: `query` → `--replace` on a host network with linked
  instances and frozen nodes must be a no-op (O1 unchanged).

### Phase 1 — Format, mounting, save (Rust only; no UI)

Work: `library_links.rs` (`LibraryMount`, `LibraryLinks`, `mount_containing`,
mount/unmount), registry field, prefix batch-rename, recursive load with cycle
stack, save filtering + `imports` field (paths verbatim, normalized at link time), version 9,
`link_library`/`unlink_library` in `StructureDesigner` with undo commands,
`unresolved_mount_ref` and the frozen-node rule of §8 in the load-time and
validation-time repair passes; the `uses` table (D12/D13) —
`UsedInterfaces`, `collect_used_interfaces`, `stored_uses` read on load and
carried over on save; the `LinkFs` trait with read-once mounting and
temp-file-and-rename writes (§5.1); the load-time heals on the temp registry
inside `mount_library`. *Using* the `uses` table on open (reconciliation) is Phase 3, where it
shares the refresh code; until then an open with a changed library is known to
realign by position, which no user sees before Phase 4.

Tests (`library_links_test.rs`):
- The §11.1 oracles O1–O6 and `library_links_support.rs`, each with its
  negative control, **before** any feature test.
- **Red-first**: `host_missing.cnnd` — which has an instance of the missing
  library's network **and** a `record_construct`, a `record_destructure` and a
  `product` on one of its record defs, all with wired pins, one of them inside
  a HOF body — loads, reports `Missing`, and load → save is byte-identical
  (every such node and its wires intact) (§8).
- Frozen-node predicate: `unresolved_mount_ref` fires for an unknown
  `node_type_name` under a mount, for a record schema under a mount, and for a
  `DataType` in node data that names a record under a mount, for every mount
  status; an unknown name *outside* any mount keeps today's behaviour
  (arguments realigned).
- **Frozen-node matrix** (§11.1) for the causes available in P1 (`Missing`,
  `Error`, `Cycle`) × every reference kind × load, validation, unrelated
  structural edits in the same network, unrelated local record-def edits,
  undo/redo of those, save → reopen: O2 and O6 in every cell. Specifically
  covers wires **out of** a frozen `record_destructure` (pins ≥ 1) and body
  wires of a `map` over `Array[demolib.Miller]`.
- Save filtering never drops local content: a local network in folder `libs`
  beside the mount `libs.demolib` is saved; a local network named `demolibx`
  or `demolib_extra` is not taken for part of mount `demolib`; local record
  defs and CLI rules likewise; checked by O4 on a host that has all of these.
- Byte-identical save for every non-`Loaded` status (`Missing`, `Error`,
  `Cycle`), not only `Missing`.
- A failed `link_library` (bad alias, unreadable file, cycle) leaves O1 and the
  undo stack unchanged; `link` and `unlink` pass O5.
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
- `uses` table: lists exactly the names host local nodes refer to (instances
  and record nodes, including inside HOF bodies, including a reference into
  `a.common`), keyed relative to the alias, with `param_id`s / `FieldId`s,
  sorted; a name referred to only as a parameter *type* is not listed; a host
  with no references to a mount writes no `uses` for it; `lib_a.cnnd` saved on
  its own records its `uses` of `common`.
- `uses` carry-over: on `host_missing.cnnd` the `uses` table and `hash` are
  written back byte-identically.
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

**Status (2026-09-29): done.** Deviations and findings, all folded into the
sections above: mounting runs *before* the local networks are inserted
(§5.1); consumers of a frozen node's function pin are protected like frozen
nodes (`library_links::protected_node_ids` — an `apply` fed by `@frozen` would
otherwise lose its `arg0…` wires); save is now deterministic (node order was
`HashMap` order, which made byte-identical saves impossible); `link_library`
accepts an absolute path and stores it relative, refusing only when no
relative path exists; the fixture generator is an `#[ignore]`d tool, like
`generate_healthy_fixture`. Open for later phases: §13 items 6 and 7.

### Phase 2 — Read-only enforcement and eval base directory (Rust only)

Work: `name_in_mount` in the namespace checks; `ensure_editable` + checked
scope accessors; `ai_text_edit` guard; view-state paths don't dirty on linked
networks; `base_dir_for_eval` and the eight file-using call sites; non-blocking
"transitive library" warning; `mount_fingerprint` (already landed in P1, used
by O5); **the recorded layout of frozen nodes and the positional pin spelling
of the text format (§13 item 6)** — the `ai_text_edit` test below cannot pass
without them.

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
- The same alphabet on a local network that **contains frozen nodes** (§8):
  every edit succeeds, and every frozen node with its wires in and out is
  unchanged afterwards (O2 restricted to the frozen nodes' wires), after undo,
  and after save → reopen. Includes copy / paste and duplicate of a frozen node
  (the copy is frozen too, wires kept per the usual paste rules), deleting a
  frozen node (undo restores it with its wires), and factoring a selection that
  contains one.
- `ai_text_edit` on a host network with linked instances and frozen nodes:
  `query` then `replace` with the same text is a no-op (O1), and an unrelated
  edit statement leaves every linked instance's wires as they were (O2).
  `--replace` mints fresh node ids, so "no-op" is judged as the text
  round-trip corpus judges it — identical text back, same wire count, and O2
  keyed by node *name path* rather than id. Cases: `host_missing.cnnd` (all
  frozen nodes get recorded layouts), the same with its `uses` table deleted
  (every frozen wire spelled `@i`), and an `apply` fed by a frozen node's
  function pin.
- Recorded layout: on `host_missing.cnnd`, every frozen instance and record
  node has the pin names of its `uses` entry; `apply` fed by `@f2` derives
  `[f, x, y]` from it; nothing unfreezes and O6 holds. A frozen node whose name
  has no `uses` entry keeps an empty layout and its wires.
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

**Status (2026-09-29): done.** Deviations and findings:

- **Guards, not a checked-accessor split.** Every content-mutating
  `StructureDesigner` entry point (≈60, including the mechanosynth / chemisorb
  ops and the api-level Text tab and comment resize/update) calls
  `ensure_editable` / `ensure_active_editable` at its top; the node-data
  accessors `get_node_network_data_mut[_scoped]` (the atom_edit / facet_shell /
  mechanosynth route) return `None` on a linked network. The scope accessors
  stay unchecked, because selection, display, validation and snapshots all go
  through them. The mutation alphabet test is what keeps new entry points
  honest (§6).
- `name_is_taken` also answers `true` inside a mount and for the folder
  holding one, so layer 1 needed no per-caller change; new error variants
  `InvalidNameReason::InLinkedLibrary`, `RecordTypeDefError::Linked`.
- View state on a linked network: camera and canvas viewport never dirty the
  host, and display toggles push no undo step (they are session-only).
- *Duplicate into my file* puts the copy at the local root under its simple
  name (`a.shapes.slab` → `slab`, else `slab_copy`).
- **Found: undo could not bring back a frozen instance.** Five undo paths
  looked a node's data saver/loader up by a type name that must resolve, so
  deleting, duplicating or pasting a frozen instance recorded no undo step.
  They now go through `node_data_saver_for` / `node_data_loader_for`, with the
  `CustomNodeData` fallback the `.cnnd` loader already used.
- **Found: `base_dir_for_eval` cannot find the owning network of a lazy
  body.** The lazy walkers (`map` / `filter` are lazy) run a body on a
  body-only stack. A `ZoneClosure` now records its `home` network and its body
  invocation frame carries it (`NetworkStackElement::home`); a closure defined
  in a library and run by the host reads the library's files.
- Validation no longer invents errors around a frozen node without a layout:
  a wire from a frozen source is not type-checked (the source carries its own
  error, the consumer is in its cone), and a node fed by a frozen function pin
  is not arity-checked. The validation corpus snapshot changed accordingly.
- Recorded layouts are installed in `repair_node_network`'s frozen branch
  (always, when a `uses` entry exists — P3's refresh writes the last resolved
  interface into `stored_uses`, so this *is* the last resolved layout), in
  `initialize_custom_node_types_for_network`, at mount time for a library's own
  frozen nodes (types in nested `uses` are prefixed), and in the text editor.
  Which file's `uses` applies is decided by the node's owner mount, threaded
  into zone bodies.

### Phase 3 — Change detection, refresh, retarget (Rust only)

Work: **first, persistent ids (§13 item 7)** — before any reconciliation
code, since reconciliation is only as good as the ids it matches on. Then
`FileStamp` with `loaded` / `last_seen`; the watched list (libraries plus
data files, from the D11 collector — the collector moves forward into this
phase, the Save As dialog stays in P5); `check_dependencies`;
`reconcile_used_interfaces` (D13), run on open before any argument-count
repair and inside every recursive mount, and by the refresh; stored-hash
comparison on open (report only); `RefreshDependenciesCommand` (§7.1) with
`repair_call_sites_for_network` made `pub(crate)`, given a parent filter
(local networks of the importing file only), and driven with recorded
interfaces; host data-file refresh through a `NodeData` hook that re-reads the
cached file content; the report. Before starting: settle §7.3's function-value
question.

**Unfreezing must drop the recorded layout (found in P2).** A frozen
*instance* carries its recorded layout (§13 item 6) as its cached
`custom_node_type`, and `get_node_type_for_node` consults that cache **before**
the network's own type. On a fresh open this never matters — a name that
resolves is not frozen, so nothing installs a layout. But when a refresh or
retarget makes a frozen name resolve again in the same session, the stale
recorded layout would silently shadow the network's real interface. The
unfreeze step must therefore clear an instance's `custom_node_type` and then
reconcile its arguments from the recorded interface to the live one (step 5 of
§7.1) — a by-name rebuild alone is not enough. Record nodes are not affected:
their cache is rebuilt from the live def as soon as it resolves. The
frozen-node matrix's unfreeze cases cover this: after unfreezing,
`get_node_type_for_node` must return the network's own type.

Tests (`library_links_refresh_test.rs`, all in temp dirs). **Every test below
runs O2 (wire ledger) and O3 (disk tripwire), and every test that pushes a
command runs O5 (undo inverse)** — the bullets state only what is specific to
them. Plus the §11.1 randomized harness and the fault-injection tests of
refresh.
- Frozen-node matrix, P3 causes: name removed by a refresh, by a retarget;
  and **unfreezing** — file restored / retarget back / name re-added, with and
  without an interface change in between — every frozen node's wires intact
  and reconciled from the carried-over `uses`.
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
  in the report. The mount stays `Loaded` throughout — this is the case a
  status-keyed exemption would miss.
- Record def removed from the library → host `record_construct` /
  `record_destructure` / `product` nodes on it keep every wire (frozen, §8),
  show "Unknown record type", and are listed in the report. Record def with a
  field removed → exactly that field's wire drops (field-id matching), in the
  report.
- Retarget from `lib_v2.cnnd` to `lib_v3.cnnd` created by copying v2 and
  editing it → wires preserved by `param_id`; host file's node type names
  unchanged.
- Output-pin list change → affected host wires from pin ≥ 1 listed in the report
  (§7.3).
- **Undo round trips** (the core of D9), for each of: clean refresh, refresh
  that dropped wires, refresh that removed a used network, refresh that
  removed a used record def, retarget, data-file refresh:
  - undo restores the host (O1 identical to before — including the networks
    whose referenced network or record def disappeared) **and** the old
    mounted content (`mount_fingerprint` identical), and evaluation gives the
    old result;
  - redo restores the refreshed state exactly (post-validation snapshot);
  - an edit made *before* the refresh can still be undone after undoing the
    refresh, and one made *after* it can be undone before it (interleaving);
  - the undo stack is never cleared by a refresh.
- **Redo is never destroyed by an automatic refresh**: undo two edits, change
  the library on disk, `check_dependencies` → nothing applied, report `held`,
  mount `ChangedOnDisk`, both edits still redoable; repeated checks do not
  report it again; redo both → still held; make a new edit → the next check
  applies the refresh as one command. An explicit `refresh_library` while held
  applies immediately and truncates the redo tail like any edit.
- After undoing a refresh, `check_dependencies` does **not** refresh again
  (`last_seen` rule); an explicit `refresh_library` does; a *new* disk change
  is held (the undone refresh is itself on the redo tail) and applied after the
  next edit.
- **Open after the library changed while the host was closed** (D13, the
  common case): for every parameter edit of the list above, save the host,
  edit the library on disk, reopen the host → wires follow `param_id` exactly
  as a live refresh would, and `take_load_library_report` equals the report of
  the equivalent refresh. Same for record-def field add / remove / reorder /
  rename, and for a host call site inside a HOF body.
- Nested: `lib_a` saved against `common` v1, `common` changed to v2 with a
  parameter inserted in the middle, host opened → `a.common.*` call sites
  inside `a.*` are repaired in memory; `lib_a.cnnd` on disk is untouched.
- Refresh leaves linked callers alone: `a.bar` calls `a.foo`, `foo` gets a
  parameter inserted, refresh → `a.bar`'s wiring equals `lib_a.cnnd` opened
  standalone (the parent filter of §7.1 step 5).
- Save after undo: refresh, undo it, save, reopen → the host is repaired
  forward to the current library (the `uses` table recorded the old
  interface), not left positionally shifted.
- Content-only change (same interfaces) → the open report says "changed since
  last saved", nothing is repaired, the host is not dirty.
- Duplicate-id library: a library whose parameter nodes share a `param_id`
  (healed on load by `dedupe_param_ids_in_network`) → host wires match by
  name or drop with a report; none moves to the wrong pin.
- Missing then back with a different interface: host saved while the library
  is `Missing` (the `uses` carried over byte-identically), library restored
  with a parameter inserted in the middle, host reopened → reconciled from the
  carried-over `uses`, no shift.
- The active network, or a network in the navigation history, is under a
  mount that a refresh removes → the editor falls back to a local network
  without error; undo of the refresh makes it available again.
- Host data-file refresh (`import_xyz`, `import_cif`, `import_cube`) replaces
  only the cached file content: every user-set field of the node's data
  (options, tags, transforms, names) is unchanged, compared field by field.
- A refresh while the host is dirty keeps it dirty; a host edit, then a
  refresh, then save → reopen keeps the edit (O4).
- A held refresh (D9) survives save: undo two edits, change the library, save,
  reopen → the reopened host is reconciled to the current library, and the
  pre-save redo tail was usable up to the save.
- Undo history eviction: fill the history past `max_history` with edits after
  a refresh → the refresh command is evicted like any other; the state is
  unchanged and saves correctly.

**Status (2026-09-29): done** (`library_refresh.rs`, `library_refresh_ops.rs`,
`undo/commands/refresh_dependencies.rs`; tests `library_links_refresh_test.rs`
and the harness `library_links_fuzz_test.rs`). Deviations and findings:

- **Persistent ids are written only when a load could not re-derive them.**
  `RecordTypeDef` gains `field_ids` (parallel to `fields`) and
  `next_field_id`, a network gains `next_param_id` — each omitted while it
  equals what a load derives (`0..n` / `n`, `max(param_id) + 1`). No fixture
  or snapshot changed; a def or network that was reordered, lost a field at
  the end or gained one in the middle writes them.
- **§7.3's function-value question: yes, it mis-wired.** An `apply` fed by an
  instance's function pin takes its `arg…` pins positionally from the
  instance's parameters, so a library reorder swapped them. Reconciliation
  now moves an `apply`'s `arg…` wires by the instance's own parameter mapping
  (through `node_network::function_pin_dispositions_for`, so the partition
  stays single-sourced). O2 keys `apply` arguments by the parameter they
  stand for.
- **Found: a library file can carry stale interfaces.** A record-def edit
  refreshes only the networks that are validated afterwards, so a saved
  library can list a network's *old* output pins; the host then reconciled
  against the stale list and the post-mount validation shifted its wires a
  second time, silently. `mount_library` now validates the library's temp
  registry (in dependency order) before anything reconciles against it.
- **Found: ids are unique only within one network's history.** A network (or
  record def) deleted and created again under the same name hands out the
  same small ids afresh, so across files an id can name a different
  parameter — a wire moved to the wrong pin. Cross-file matching
  (`network_validator::cross_file_parameter_mapping`) distrusts ids on a
  contradiction — an id pointing at an old parameter of another name while an
  old parameter of its own name exists — and then matches the whole list by
  name. **Closed (P3 follow-up):** ids are now unique across a file's whole
  history. The registry keeps a file-level floor for parameter ids and field
  ids (`NodeTypeRegistry::param_id_floor` / `field_id_floor`, top-level
  `next_param_id` / `next_field_id` in the `.cnnd`, written only when above
  every saved counter — i.e. after a deletion). Every point that *creates* a
  network or def claims ids above it (`claim_param_ids` /
  `claim_field_ids`): new network, duplicate, factor into subnetwork,
  closure → network, copy import, new record def. Allocation inside a
  network stays per-network. The floor is materialized at load and at save;
  ids handed out after the last save by a network then deleted may be reused,
  which is safe — no linking file can have recorded them. A re-created
  network therefore never shares an id with its predecessor, and the host
  falls back to names (a reported drop when the names differ too). Cost: a
  network created after others starts above their ids, so a new design's
  networks after the first may save a `next_param_id`; existing files do not
  change. The contradiction rule stays as a backstop. Still not covered: a
  retarget to an *unrelated* library whose same-named network happens to use
  overlapping ids (two files' id histories are independent).
- **Found by the harness: existing repair passes drop wires after a
  retype.** `repair_zone_body` disconnects a body wire whose zone-input type
  no longer converts, which the report called merely "flagged". Instead of
  teaching each pass to report, the refresh and the open take a
  positional image of every wire of the reconciled networks right after
  reconciliation, and report whatever the repair and validation passes remove
  afterwards (`report_wires_removed_by_repair`): the report is complete by
  construction. Regression test `a_retype_that_breaks_a_body_wire_reports_it_dropped`.
- A network instance's output pins can carry ids (inherited from a
  `record_destructure` inside the network) but match by position in v1
  (§7.3); only a `record_destructure`'s own outputs follow field ids.
- **Refresh granularity is the direct mount.** A changed nested library or a
  library's data file re-mounts the direct mount that contains it (the host's
  `uses` are per direct import).
- A held change marks the mount `ChangedOnDisk` also when it has no content
  yet (a missing library that reappeared while redo history exists).
- `UndoCommand` lost its `Send + Sync` bound: the refresh command keeps live
  network copies, because restoring a serialized one re-runs the node-data
  loaders, which re-read data files — an undo of a data-file refresh would
  read the new file.
- Data files: `NodeData::file_paths` (the load-time readers `import_xyz`,
  `import_cif`, `import_cube`); a refresh re-reads a node through its own
  saver → loader round trip, so every user-set field is kept. The watched
  list (`watched_data_files`) moved forward from P5; the Save As parts did
  not.
- `repair_call_sites_for_network` repairs only callers in the network's own
  file (the §7.1 parent filter) and moves `function_pin_roles` with the
  arguments.
- The open report is completed after validation
  (`StructureDesigner::finish_library_load`): frozen host nodes, libraries
  not `Loaded`, `changed_since_saved`; the design is dirty iff the open
  reconciled a node.

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
- A missed Flutter gate must not corrupt state (§5), so the Rust side is
  tested *without* the gates: every API mutation called on a linked target
  returns an error (the P2 alphabet, through the API layer this time).
- **Rust-side interactions.** First establish which interactions Rust itself
  knows are open (a gadget drag, an `atom_edit` drag — whatever the undo
  coalescing treats as one open step); this design has not checked. For each
  one Rust knows about, `check_dependencies` holds the refresh (like the redo
  hold of D9) instead of splitting the step, and applies it when the
  interaction ends — with a test per interaction. Any interaction only Flutter
  knows about is recorded in `lib/structure_designer/AGENTS.md` as guarded
  by Flutter alone.
- **Manual walkthrough (maintainer)** — thin editor UI is verified by hand:
  link, browse, try every forbidden gesture on a linked canvas, edit the library
  in its own file with an internal change and return (transient snackbar), then
  with a parameter removed and return (persistent snackbar, *Details*, *Undo*,
  "older than disk" marker, *Refresh*), undo two edits and then change the
  library (held snackbar, redo still works, the next edit applies the
  refresh), change a library while atomCAD keeps focus (poll picks it up),
  retarget v2→v3, unlink refused/allowed, missing file badge. The Flutter
  smoke test (`flutter test integration_test/`) is also run by the maintainer,
  not by an agent.

**Status (2026-09-29): implemented; the manual walkthrough and the smoke test
are the maintainer's.** Files: `rust/src/api/structure_designer/library_links_api.rs`
(+ `view_builders.rs` for the Dart shapes), `lib/structure_designer/library_link_actions.dart`,
the panel / canvas / model / menu edits; tests
`library_links_interaction_test.rs`, `tests/structure_designer_api/library_links_api_test.rs`,
`test/library_links_test.dart`. Deviations and findings:

- **Rust-side interactions, established.** The undo coalescing treats six
  things as one open step: `pending_move` (node drag), `pending_atom_edit_drag`,
  `pending_gadget_drag`, `pending_node_data_drag` (property sliders),
  `pending_zone_resize`, `pending_comment_edit`
  (`StructureDesigner::open_interaction`). `mechanosynth_edit`'s keystroke
  coalescing is *not* one: it is keyed on the undo push count, so any command
  pushed in between ends the run by itself. While one is open,
  `check_dependencies` **skips the check entirely** rather than marking a hold
  — nothing is observed, `last_seen` does not advance, and the first check
  after the interaction ends finds the change and applies it as its own
  command (a test per interaction). An explicit refresh / retarget during one
  is refused with an error. Guarded by Flutter alone (recorded in
  `lib/structure_designer/AGENTS.md`): a pointer held down anywhere (camera
  drags included), a focused text field, a dialog or menu on top, a wire drag.
- **The API-layer mutation alphabet was not run through the FFI.** Every
  wrapper needs the global `CADInstance`, whose `Renderer` needs a GPU adapter,
  and no Rust test constructs one. What was done instead: the API-layer sites
  that mutate a network *without* going through a guarded `StructureDesigner`
  method were audited (`set_active_network_canvas_viewport` — view state, never
  dirties a linked host; `resize_comment_node` / `update_comment_node` and the
  Text tab — guarded in P2; the `facet_shell` / `import_*` node-data accessors
  — `get_node_network_data_mut_scoped` returns `None` on a linked network), so
  the P2 alphabet on `StructureDesigner` still covers every path. The API tests
  cover the view fields (`read_only`, the add-node lists without transitive
  mounts — also in the drag-aware popup — and the mount / report shapes).
- **The report crosses the bridge with its labels resolved** (`node_label`,
  `source_label`, by the Find Usages helpers), and a refused refresh or
  retarget crosses as a report whose only content is `errors` (Flutter shows
  it as an error). One API function beyond §5.2: `check_library_alias`, so the
  link dialog validates the alias against local names and mounts as the user
  types.
- **After Save As the check runs, but it cannot yet see a moved folder.**
  Re-resolving the watched absolute paths against the new design folder is the
  Save As work of P5; until then a Save As into another folder keeps the
  mounts on their old absolute paths for the rest of the session (the next
  open resolves them afresh).
- The menu item is *Import copy…*; the link / retarget file dialogs remember
  their own folder (`FileDialogPurpose::LibraryLink`, key `library_link`).
  Linked rows' Dart twin of the prefix test is `mountFor` in
  `namespace_utils.dart`, tested against the Rust table `MOUNT_FOR_CASES`.
- The canvas gates edit gestures in the **model** (every edit entry point
  checks `activeNetworkReadOnly` first), so the gestures are inert rather than
  refused after a visible change; the node context menu keeps navigation,
  copy and Execute.
- A new shared snackbar, `showActionSnackBar` (`lib/common/error_display.dart`),
  carries *Details* / *Undo* / *Refresh*.

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

Tests (`file_dependencies_test.rs`). **Every test arms O3** with the allowed
list equal to the plan's *will copy* targets (plus *conflict* targets only when
`overwrite_conflicts = true`) and the new host path — Save As is where a bug
would overwrite a file of the user's that is not part of this design at all.
- Never overwrite what is not in the plan: a dependency whose target path is
  the **new host path itself**, the **original host**, or the **source** of
  another dependency → the plan refuses (error names both paths); two
  dependencies mapping to one target → refused; a target that is a directory
  or a symlink/junction pointing elsewhere → refused.
- Conflicts are never overwritten silently: `overwrite_conflicts = false`
  leaves every conflicting file byte-identical; the plan's statuses are
  recomputed at copy time, so a file that appeared at a *will copy* target
  after the dialog was shown is treated as a conflict, not overwritten.
- Save As onto an existing host file: a failure anywhere leaves the old file at
  the destination byte-identical (temp file + rename, §11.1).
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
- Copy failure: make one copy fail (read-only target file) → host not written,
  error names the file, already-copied files left in place and reported (D11).
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

**Status (2026-09-29): implemented; the manual walkthrough and the smoke test
are the maintainer's.** Files: `file_dependencies.rs` (plan, copy, bundle,
`link_library_copying`), `library_refresh::relocate_after_save`,
`library_links::relocate_mounts`, the API functions in `library_links_api.rs`,
`lib/structure_designer/save_as_dependencies.dart`, the link dialog's copy-in;
tests `file_dependencies_test.rs` (25) and four view-shape tests in
`library_links_api_test.rs`. The reference guide's `library_linking.md` gained
"Moving a design". Deviations and findings:

- **The overwrite choice crosses as a list, not a flag.**
  `save_as_with_dependencies(path, copy, overwrite_targets)`: Flutter passes
  the conflict targets the user saw and chose to overwrite. With a bare
  `overwrite_conflicts = true`, a file that appeared at a *will copy* target
  after the dialog would have been overwritten unseen, which the "plan is
  recomputed at copy time" test forbids.
- **One status beyond §5.2: `SourceMissing`** — a dependency that does not
  exist now either (a `Missing` mount, a deleted data file). Nothing to copy;
  it never makes the dialog appear.
- **Found: an undo could point the mounts back at the old folder.** A refresh
  command restores whole `LibraryMount` records, whose `abs_path` was resolved
  against the design's folder *at the time*; after a Save As into another
  folder, undoing an earlier refresh re-pointed the mounts (and the data-file
  watches) at the old location. The command's install now re-resolves every
  mount from its `rel_path` chain (`relocate_mounts`) and rebuilds the data
  watches. Test `undoing_an_earlier_refresh_keeps_the_mounts_at_the_new_folder`.
- **Relocation after Save As keeps the loaded stamps and forgets the mtime** of
  the last sighting, so the next check hashes the file now pointed at: a copy
  is merely touched (no command), a kept different file refreshes as one
  command with a report, an absent one goes `Missing`. It runs inside
  `save_node_networks_as` for every save to another path — plain Save As, *Save
  without dependencies*, and the CLI's `/save` alike.
- **`file_paths` now covers `ops_library`, `build_script` and `mechanosynth`'s
  deprecated properties too** — they parse their file at load, exactly like
  the importers — so those files are also *watched* from now on, not only
  copied. One consequence to know: an `export_build_script` writing a file that
  a `build_script` node reads triggers one refresh command after a write that
  changed the content (not a loop — an unchanged rewrite is only *touched*).
  `NodeData::file_path_pins` names the pin a wired path arrives through
  (`has_wired_paths`).
- **The bundle carries the design as it is in memory** (unsaved edits
  included — the point is "send me what I am looking at"); libraries and data
  files come from disk. The zip is written by a ~100-line ZIP 2.0 writer over
  `flate2` (already in the lockfile through `png`) rather than the `zip` crate;
  `read_zip` reads it back for the tests, and the output was checked against
  Python's `zipfile` and PowerShell's `Expand-Archive`.
- **`link_library_copying` never overwrites.** A different file at any target
  (the library's or a dependency's) refuses the whole operation before anything
  is copied; identical files are reused. The copy is linked as one undo step;
  the copied files stay on undo. Flutter offers it when
  `library_needs_copy(path)` — no relative path from the design's folder — and
  names the copy after the picked file. The cross-drive case itself cannot be
  set up in a test; the function is tested with a second temp root.
- The "copy failure" test injects a failing write through `LinkFs` instead of
  a read-only target file (renaming over a read-only file succeeds on Unix).
  `LinkFs` gained `entry_kind` (default: `symlink_metadata`) for the
  folder / link refusal.
- FRB escapes the Dart keywords: `APIDependencyKind.library_`,
  `APIDependencyGroup.external_`, `APIDependencyStatus.external_`,
  `APISaveAsResult.external_`, `APIBundleResult.external_`.
- Not done here (P6): the CLI / HTTP `/save` route still writes the design
  alone, without a dependency copy.

### Phase 6 — CLI, vendoring, documentation

Work: CLI `libraries` subcommand (`list`, `link <path> <alias>`, `refresh
[<mount>]`, `unlink <alias>`) over new HTTP routes in
`lib/ai_assistant/http_server.dart`; `query` shows a `# linked from …`
header for a linked network; *Make local copy* (vendoring, §10).

Docs (part of "done", per `AGENTS.md`):
- Reference guide: new page `doc/reference_guide/library_linking.md` linked
  from the hub; update `ui.md` (File menu, panel, read-only canvas) and
  `headless_cli.md`.
- `AGENTS.md` updates: `serialization/AGENTS.md` (the `imports` field and its
  `uses` table, v9, save filtering, "never write under a mount", and in "Load
  pipeline & derived state": interface reconciliation runs after mounting and
  **before** any argument-count repair); `atomcad-structure-designer/src/AGENTS.md`
  (library links: mount-by-prefix, `ensure_editable` is required for every new
  content-mutating entry point, `base_dir_for_eval` is the only way to resolve a
  relative path at eval time, **every repair pass that drops or realigns a wire
  must honour the frozen-node rule of §8** — a new such pass joins the
  frozen-node matrix — and new library-linking tests use the §11.1 oracles
  rather than ad-hoc assertions); `lib/structure_designer/AGENTS.md` (read-only
  rendering, `mountFor`).
- `doc/cnnd_versioning.md`: note v9.

Tests: CLI routes covered by Rust tests of the underlying `StructureDesigner`
methods (already in P1–P3). Vendoring and alias rename are the two operations
that turn linked content into saved local content or rewrite host references,
so they get the full treatment:
- Vendoring: the mount's `mount_fingerprint` before equals the fingerprint of
  the same names as local content after (nothing lost in the conversion,
  including nested mounts, record defs, folders and frozen nodes' targets);
  save → reopen with the library file deleted still evaluates identically;
  the file contains no `imports` entry for it; O2 and O5.
- *Rename alias…*: every host reference kind of the frozen-node matrix is
  rewritten (instances, record schemas, `DataType`s in node data, `uses`
  keys); O2, O5, O4.
- The randomized harness gains vendor and rename-alias steps.

**Status (2026-09-29): implemented; the manual walkthrough is the
maintainer's.** Files: `library_link_ops.rs` (`make_library_local`,
`rename_library_alias`, `query_active_network_text`), `library_links.rs`
(`rename_prefix`, `unresolved_refs_under`, `reinstall_recorded_layouts`),
`undo/commands/link_library.rs` (`VendorLibraryCommand`,
`RenameLibraryAliasCommand`), `NodeData::rebase_file_paths` on the eight nodes
that store a path, the two API functions, the HTTP routes and `atomcad-cli
libraries`, the two mount-folder menu items; tests
`library_links_vendor_test.rs` (9) and `zone_body_load_order_test.rs`, and the
harness's two new steps (500 seeds clean). Deviations and findings:

- **Found: a zone body lost wires on every reopen — any file, not only
  linking ones.** A load repairs networks one by one in name order, so a HOF
  body instance of a network whose name sorts later has no type yet, and
  `repair_zone_body` compared its zone-input wires against `DataType::None` and
  dropped them. Vendoring exposed it (`a.*` sorts after `Main`), but it also hit
  every *mount* of a library whose `bar` body calls its own `foo` — including
  the `lib_a` fixture — and every plain design with such a body, since zones
  Phase 6. The pass now keeps a wire whose destination type does not resolve,
  as the top-level pass always did; stage-2 validation sees the real type.
  Regression test `zone_body_load_order_test.rs`; the pitfall is recorded in
  `serialization/AGENTS.md`.
- **Vendoring rebases data-file paths.** The section above said vendoring is
  "just removing the mount record"; it is not, for a library outside the
  design's folder: its nodes' relative paths resolve against the *library's*
  folder (D8) and would resolve against the design's afterwards. Every node
  that stores a path — the six readers and the two exporters — implements
  `NodeData::rebase_file_paths`; a path with no relative spelling becomes
  absolute. Watches carry over by canonical path, so a held change stays held.
- **Undo of vendoring puts back the file's id floors.** A save while the
  content is local raises `param_id_floor` / `field_id_floor` over its ids;
  once it is linked again those ids are the library's history, and O5 caught
  the floor surviving the undo.
- **Rename alias refuses what would not be a bijection:** a new alias that
  contains or lies inside the old one (rename through another name), and one
  under which something already refers to a name (a dangling reference would
  start resolving into the library). Everything else is `rename_prefix` one way
  and back; frozen nodes keep their recorded layouts, whose types are
  re-prefixed with the `uses` tables.
- **`query`'s header** is built in the crate (`query_active_network_text`), not
  in the api wrapper, so it is tested: `# linked from <rel_path> (library
  `<mount>`[, through `<parent>`])` under `# Network:`.
- **The CLI's `save <path>` does the D11 dependency copy** (P5 left it
  writing the design alone). The CLI cannot show the dialog, so when the plan
  would copy anything the route refuses and lists the files unless given
  `--copy-deps` (conflicts are kept, never overwritten) or `--no-deps`.
- Beyond the Phase 6 list: `libraries rename` and `libraries make-local`
  subcommands, and `networks` marks linked networks `(linked, read-only)`.
- Docs: the reference guide's `library_linking.md` (*Renaming an alias*,
  *Making a library local*, *From the command line*) and `headless_cli.md`;
  `ui.md`, `cnnd_versioning.md` and the `serialization/` / Flutter `AGENTS.md`
  sections had already landed with P4/P5. The `atomcad` skill documents the
  new commands.

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
   changed is refreshed automatically (on focus, by poll, after Save As), as one
   undoable step with a report, and an open reconciles against the recorded
   interfaces (D13); manual *Refresh* per mount and *Refresh all
   dependencies*. The earlier safe/unsafe classification was dropped as too
   complex for implementers and users alike (D7, D9, D10). One exception
   (added 2026-09-29): while redo history exists, an automatic refresh is held
   rather than truncating it (D9).
4. **Paths and moving files (2026-09-29)** — relative paths may use `..` and
   are never rewritten; Save As copies dependencies (libraries and data files)
   into the same relative layout around the new location, via a dialog; absolute
   paths are refused for libraries and treated as external for data files;
   *Export project bundle* zips the file with its dependencies (D6, D11).
5. **Wire identity across a closed host (2026-09-29)** — wires are positional
   and the old interface of a linked network is not in the host file, so a
   library changed while the host was closed would shift wires on open. The
   host records the interfaces it is wired against per import (`uses`), from
   memory, and opening reconciles against them with the refresh's own repair
   (D13). A `param_id` on every argument was rejected as the broad refactor
   `doc/design_parameter_wire_stability.md` §5 parked.
6. **Frozen nodes in the text format (2026-09-29, found in P1)** — the text
   format writes wires by pin name, and a frozen node has no layout on load,
   so its wires were omitted from `query` and a `--replace` deleted them
   silently. Decision, two parts, both in **P2** (done 2026-09-29):
   - **Recorded layout.** When a node is frozen and has no cached layout (a
     fresh load), install one built from the recorded interface of the name it
     refers to — the `uses` entry, which D13 carries over for every unresolved
     name: an instance gets the entry's `params` (with their ids) and
     `outputs`; a `record_construct` / `record_destructure` / `product` gets
     its `fields` (ids as the `Parameter` / output-pin ids, exactly as
     `build_node_type_for_schema_with_defs` would stamp them). It replaces the
     `populate_custom_node_type_cache(.., false)` call the frozen branch of
     `repair_node_network` makes today, is installed with `refresh_args =
     false`, and — like any frozen layout — is never rebuilt until the name
     resolves. Types in a nested library's `uses` are written in the library's
     namespace and must be prefixed like everything else at mount time. This
     one mechanism gives frozen nodes their pins on the canvas (§8), their
     names in the text format, and a function-pin signature from which an
     `apply` fed by `@frozen` derives its `arg` pins — so
     `protected_node_ids`' second clause stops mattering in practice (keep it
     as the safety net).
   - **Positional spelling as the fallback.** A wire into a pin index the
     node's layout does not have (no `uses` entry: a hand-edited file, or a
     name that was never recorded) is written `@<index>: source`, and an
     output of such a node is referenced `node.@<index>`. `@` followed by
     digits is not a valid identifier, so it cannot collide with a pin name,
     and in key position it is currently a parse error, so nothing existing
     changes meaning. The editor accepts `@i` **only** on a node that is
     protected (§8) and places the wire at argument `i`, growing `arguments`
     as needed, without a type check; on any other node it is an error
     ("positional pin only on a node whose type is unavailable"). Named pins
     are still used wherever the layout has a name.
   Rejected: refusing `--replace` on a network with frozen nodes (the AI
   could no longer edit a design whose library is missing — the case where
   help is most needed); and positional spelling alone (the canvas would still
   draw frozen nodes pin-less, and every AI edit would see `@0` instead of
   `x`).
7. **Stable record-field and id-counter identity (2026-09-29, found in P1)**
   — record `FieldId`s are re-assigned in authored order on every load, and
   `next_param_id` / `next_field_id` are re-derived as `max + 1`, so an id
   deleted from the end of a list is handed out again after a save. Across
   library versions both break D13's matching: a field inserted at the front
   re-numbers every field, and a parameter deleted and replaced by a new one
   gives the new one the old one's id — so a host wire recorded against the
   old one would *move* to it, the one outcome D13 promises never happens.
   Decision, first step of **P3**:
   - `RecordTypeDef` serializes each field's `id` and the def's
     `next_field_id`; `SerializableNodeNetwork` serializes `next_param_id`.
     All additive (`#[serde(default)]`), no version bump: an older build
     ignores them and re-derives ids as today, losing identity but no data.
   - Load: a field with an `id` keeps it; fields without one get ids in
     authored order starting above every present id (so a pre-P3 file gets
     exactly the ids it gets today — which is also what the P1-written `uses`
     tables recorded, so those stay valid); duplicates are healed like
     `dedupe_param_ids_in_network` (first keeps it, later ones get fresh ids,
     reported). Counters load as `max(stored, max id + 1)` — the existing
     floor never lowers.
   - `collect_used_interfaces` reads parameter ids from the network's
     **parameter nodes** (`ParameterData.param_id`), not from
     `node_type.parameters`: a network restored by an undo snapshot has no ids
     in its interface until it is next validated (found in P1), and non-active
     networks are not revalidated after an undo.
   - ~~This rewrites the `.cnnd` of every file with record defs once (the ids
     appear); fixtures and snapshots that pin record defs are updated with it.~~
     Done in P3, differently: the ids and counters are written only where a
     load could not re-derive them, so no existing file changes.
   Rejected: matching record fields by name only — a renamed field would
   become a dropped wire, and it leaves the parameter-recycling hole open.
8. **Load order (2026-09-29, found in P1)** — imports are mounted *before* the
   local networks are inserted, not after (§5.1): each local network is
   repaired as it is inserted, and that repair must already know which names
   belong to a mount. Done in P1.
