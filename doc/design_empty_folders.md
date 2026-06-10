# Design: Empty Folders

## Problem

Folders in the user-types tree (networks + record defs) are **implicit** — a folder
exists only as the shared dot-delimited prefix of the entities under it. There is no
way to create an empty folder, so the natural "make a folder, then fill it" workflow
is impossible: you can only organize *after* creating, by giving items dotted names.

This pairs badly with the "Add node network / Add record" folder context-menu actions
— you can only add into a folder that already exists because something is already in it.

## Goal

Let the user create an empty folder (primarily via a folder's right-click menu;
secondarily via an action-bar button for root-level folders), with the lightest
backend support that is still consistent.

## Semantics (decided)

**A folder is removed the moment it becomes empty.** An empty folder persists *only*
if it was deliberately created and has never had anything put into it. The instant any
child (an entity *or* a subfolder) appears under a folder, that folder stops being a
tracked empty folder and behaves exactly like every folder does today — including
vanishing when its last child is removed.

Rationale (user's call): (1) consistent with current behavior — emptying a folder
removes it, as now; (2) technically simple; (3) **no user-visible distinction between
"deliberate" and "incidental" folders** — that distinction (the rejected "hybrid"
model) is invisible state the user would have to predict, which is confusing.

## Storage

`NodeTypeRegistry` gains:

```rust
pub folders: BTreeSet<String>,   // currently-empty, leaf-most folder paths
```

- **One marker = one full path** (`"A.B.C"`); intermediate folders (`A`, `A.B`) are
  *derived* from the path by the tree builder, exactly as they are derived from an
  entity name like `A.B.Spring`. No separate entries for intermediates.
- The set only ever holds **leaf-most, currently-empty** folders. Invariant maintained
  by **prune-on-add** (below).
- `BTreeSet` (not `HashSet`) for deterministic serialization order — avoids the
  HashMap-ordering snapshot/undo normalization pain documented elsewhere.

## The one rule: prune-on-add

> Adding any child (an entity or a subfolder marker) under a path removes that path —
> and all ancestor prefixes of it — from `folders`. Removing children never touches
> `folders`.

This is the entire mechanism. Because removal never touches the set, "vanishes when
emptied" falls out for free (there is nothing left to clean up). Concretely:

- `prune_ancestor_folders(child_path)` removes every strict-ancestor prefix of
  `child_path` from `folders`. Called by the two low-level inserts
  (`NodeTypeRegistry::add_node_network`, `add_record_type_def`) and by `add_folder`.
- `add_node_network` returns the markers it pruned so undo can restore them (see Undo).

`prune_redundant_folders()` is a one-shot reconcile run after `.cnnd` load: removes any
marker that is an ancestor-or-equal of an existing entity or another marker. Insurance
against a hand-edited / out-of-order file; in normal operation the saved set is already
clean.

## Collision

`name_is_taken(name)` also consults `folders`. A folder path and an entity full-path
can't be identical (filesystem-like). This does **not** retroactively fix the
pre-existing latent overlap where a network `A` can coexist with network `A.B` (so `A`
is both a leaf and an implied folder) — out of scope.

## Namespace operations become folder-aware

Entities under namespace `P` are named `P.something` (strict dotted prefix). An empty
folder's marker is named **exactly** `P`. So the existing entity-only sweeps miss empty
folders. Both ops additionally sweep markers (`marker == P` **or** `marker` starts with
`P.`):

- **`rename_namespace(old, new)`** — also remaps matching markers (`old` → `new`,
  `old.sub` → `new.sub`). Promote-to-root (`new` empty) on a bare marker `old` just
  removes it. The operation is **applicable** when entities *or* markers are affected;
  `compute_namespace_rename` counts marker remaps so the move dialog's preview enables
  Apply and reports folder conflicts. Inline folder rename and drag both funnel through
  here, so they work too.
- **`delete_namespace(prefix)`** — also removes matching markers; applicable when
  entities *or* markers exist under the prefix. "Delete" on an empty folder thus works.

## Undo

- **`AddFolderCommand { path, pruned_ancestors }`** — do: prune ancestors + insert
  `path`; undo: remove `path` + restore `pruned_ancestors`.
- **`AddNetworkCommand` / `AddRecordTypeDefCommand`** gain `pruned_folders: Vec<String>`
  (the ancestor markers the create absorbed). Undo restores them so undoing a create
  brings back the empty folder it filled.
- **`RenameNamespaceCommand`** gains the folder-marker remaps; **`DeleteNamespaceCommand`**
  gains the removed markers. Both restore on undo.

**Known limitation (documented):** prune-on-add is unconditional in the registry insert,
but only the *interactive* create commands above capture the pruned markers. Rare
entity-creation paths (paste / import / factor-into-subnetwork) that create directly
into a freshly-made empty folder will prune its marker without restoring it on undo —
i.e. that empty folder won't reappear when the create is undone. No real content is
lost; only an empty-folder marker. Acceptable for v1.

## Serialization

`SerializableNodeTypeRegistryNetworks` gains, on the same rail as `record_type_defs`:

```rust
#[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
pub folders: BTreeSet<String>,
```

Backward compatible (old files omit it → empty default); no migration pass needed.
Forward compatible (older app ignores the unknown field, silently dropping empty
folders). Save writes `registry.folders`; load sets it then runs
`prune_redundant_folders()`.

## API

- `add_folder(path: String) -> APIResult` — validate (valid name, not taken), prune,
  insert, push undo, refresh.
- `get_folder_names() -> Vec<String>` — sorted marker paths for the tree.
- The move-preview API already exposes `applicable`; folding markers into
  `is_applicable()` / conflict detection makes the existing dialog "just work".

## Flutter

- Model: `addFolder(path)`, `folderNames` getter (fetched in `refreshFromKernel`).
- `_buildTreeFromNames` takes an extra `folderPaths` list; each contributes its own
  folder node + derived intermediates, no leaves. Implicit and explicit folders dedup
  through the existing `namespaceNodes` map.
- **Folder context menu** gains **"New folder…"** (creates a subfolder inside the
  right-clicked folder) — the primary entry point.
- **Action bar** gains a **"New folder"** button (creates a root-level folder) — the
  only way to make a top-level folder.
- New-folder name dialog (prompts for the simple name; folders are about their name, so
  unlike networks/records they are not auto-named).

## Out of scope / deferred

- Retroactively forbidding the pre-existing leaf-vs-implied-folder name overlap.
- Restoring pruned markers on undo for paste/import/factor (see Known limitation).
