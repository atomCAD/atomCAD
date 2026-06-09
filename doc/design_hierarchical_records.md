# Design: Hierarchical Record Type Defs (Move / Rename Parity with Networks)

## Problem

Record type defs are displayed **flat at the root** of the user-types tree view,
even though node networks enjoy a full dot-delimited namespace hierarchy with
inline rename, a "Move / rename…" dialog, and batch namespace move/delete. The
asymmetry is purely historical: the tree-view rename/move feature
(`doc/design_tree_view_rename.md`) was designed before records existed, so
records were bolted onto the existing tree as flat leaves
(`node_network_tree_view.dart` lines 95–105, with the explicit comment "Record
defs do not participate in the namespace hierarchy in v1").

We want records to be **first-class citizens of the same hierarchy** as
networks:

- A record def can live at any namespace depth (`Physics.ElementMapping`,
  `Chemistry.Bonds.Pair`).
- Inline rename, the Move/rename dialog, and namespace-level batch move/delete
  all work on record leaves and on folders containing records.
- A single folder can mix networks and records freely.

## Why this is cheap (key insight)

The hierarchy is **not a data structure** — it is a naming convention. Networks
live flat in `NodeTypeRegistry::node_networks: HashMap<String, NodeNetwork>`,
keyed by the dot-separated qualified name; folders are synthesized on the
Flutter side by splitting names on `.`. Records live identically flat in
`record_type_defs: HashMap<String, RecordTypeDef>`. Three facts make records
"already most of the way there":

1. **The name validator already permits dots.** `identifier::is_valid_user_name`
   (`identifier.rs`) blacklists only backticks, control chars, and edge
   whitespace. A record named `Physics.Point` is already legal storage — nothing
   in the backend rejects it.
2. **Records already share one namespace with networks.**
   `NodeTypeRegistry::name_is_taken` (node_type_registry.rs:1070) consults
   `record_type_defs`, `built_in_record_type_defs`, `node_networks`, and
   `built_in_node_types`. So `Physics.Point` cannot be both a network and a
   record; collisions are already rejected on add/rename. The
   `compute_namespace_rename` conflict check already uses `name_is_taken`, so it
   already detects collisions against records.
3. **Records already have the full rename-rewrite machinery.**
   `rename_record_type_def` → `rewrite_record_name_in_registry`
   (node_type_registry.rs:2314) walks every `RecordType::Named(old)` reference in
   the registry — parameter/pin types, the `schema`/`target` strings on
   `record_construct` / `record_destructure` / `product`, embedded `DataType`
   fields on `expr` / `map` / `filter` / `fold` / `foreach` / `sequence` /
   `array_*` nodes, nested record-def fields, and HOF zone bodies (via
   `walk_all_nodes_mut`). This is the exact analog of `apply_rename_core` for
   networks, and it is keyed on the **full** name, so renaming/moving a record
   across namespaces is already a correct exact-name rewrite. **No new
   reference-tracking logic is needed for rename.**

What is missing is purely *plumbing*: the namespace-level batch operations and
the single-leaf move operation are hard-coded to sweep `node_networks` only, and
the Flutter tree builder appends records flat. The only genuinely new backend
logic is the **record-reference check for namespace delete** (see the policy
decision below).

## Current Architecture

### Backend (network-only namespace operations)

All in `rust/src/structure_designer/structure_designer.rs`:

- `compute_namespace_rename(old_prefix, new_prefix) -> NamespaceRenamePlan`
  (1419) — collects names under `old_prefix.` from **`node_networks` only**;
  conflict check via `name_is_taken` (already record-aware).
- `compute_network_rename(old_name, new_name) -> NamespaceRenamePlan` (1479) —
  single-leaf preview; guards on `node_networks.contains_key`.
- `rename_namespace(old_prefix, new_prefix) -> bool` (1502) — applies the plan by
  calling `apply_rename_core` per affected network; updates
  `navigation_history`, clipboard `node_type_name` refs; pushes
  `RenameNamespaceCommand`.
- `rename_node_network(old, new) -> bool` (~1360) — single-network rename via
  `apply_rename_core`; pushes `RenameNetworkCommand`.
- `delete_namespace(prefix) -> Result<(), String>` (1664) — collects networks
  under `prefix.`; blocks via `check_delete_references` (network→network only);
  snapshots networks; removes; pushes `DeleteNamespaceCommand`.

Record-side equivalents (all kind-specific, all already correct for flat names):

- `NodeTypeRegistry::rename_record_type_def(old, new)` (1138) — exact-name rename
  + `rewrite_record_name_in_registry`. Rejects built-ins, collisions, missing.
- `NodeTypeRegistry::delete_record_type_def(name)` (1125) — removes; leaves
  dangling `Named` refs as validation errors (**does not block**).
- `StructureDesigner::rename_record_type_def` (1819) / `add_record_type_def`
  (1746) / `delete_record_type_def` — push the per-record undo commands.

### Undo commands (`rust/src/structure_designer/undo/commands/`)

- `rename_namespace.rs` — `RenameNamespaceCommand { renames: Vec<(String, String)> }`,
  all treated as networks (`apply_rename_core` per pair).
- `delete_namespace.rs` — `DeleteNamespaceCommand { network_snapshots:
  Vec<(String, SerializableNodeNetwork)>, active_network_before/after }`.
- `rename_record_type_def.rs`, `delete_record_type_def.rs`,
  `add_record_type_def.rs`, `update_record_type_def.rs` — per-record commands.

### API surface (`rust/src/api/structure_designer/structure_designer_api.rs`)

- `rename_namespace`, `delete_namespace`, `preview_namespace_rename`,
  `preview_network_rename`, `rename_node_network`, `delete_node_network`.
- `rename_record_type_def`, `delete_record_type_def`, `add_record_type_def`,
  `update_record_type_def`.
- `get_record_type_def_names()` (1248) — **user defs only** (no built-ins); this
  is what the tree panel lists, so built-in records like `ElementMapping` never
  appear in the tree. (Dropdowns use `get_all_record_type_def_names`.)

### Flutter (`lib/structure_designer/node_networks_list/`)

- `node_network_tree_view.dart` — `_buildTreeFromNames(networkQualifiedNames,
  recordDefNames)` splits **network** names into folder segments (lines 47–93)
  but appends **records flat at root** (95–105) with `leafKind = recordDef`. The
  context menu gates "Move / rename…" to `!isLeaf || leafKind == network` (618).
  Inline rename special-cases records to a flat `renameRecordTypeDef` ignoring
  namespace (334–340). Namespace delete confirmation text says "networks" (537).
- `move_namespace_dialog.dart` — `showMoveNamespaceDialog` /
  `showMoveNetworkDialog`; previews via `previewNamespaceRename` /
  `previewNetworkRename`.
- `structure_designer_model.dart` — `renameNamespace`, `deleteNamespace`,
  `renameNodeNetwork`, plus `renameRecordTypeDef`, `deleteRecordTypeDef`,
  `activeRecordDefName`.

## Design Decision: Delete semantics for mixed namespaces

There is an **intentional asymmetry** between the two kinds' standalone delete
behavior, and a namespace can now contain both:

- Standalone **network** delete is *blocked* if any network outside the deleted
  set references it (`check_delete_references`).
- Standalone **record** delete is *allowed* and leaves dangling `Named` refs as
  validation errors (`delete_record_type_def`).

**Chosen policy** (agreed): the **namespace-delete path blocks on any external
reference of either kind** — it is the conservative, batch-destructive
operation, so it should never silently dangle a user's references. Standalone
single-leaf deletes keep their existing per-kind behavior (network leaf delete
still blocks; record leaf delete still dangles). This means:

- `delete_namespace` must additionally detect, for every **record** in the
  deleted set, whether any **surviving** entity (a network not being deleted, or
  a record def not being deleted) references it via `RecordType::Named`. If so,
  the whole operation is rejected with a listing.
- A deleted network referencing a deleted record is fine (both go away). A
  surviving network referencing a deleted record blocks. A surviving record
  field referencing a deleted record blocks.

This is the **only new reference-analysis logic** in the project.

## Design

### Three correctness fixes (referenced as #1/#2/#3)

Beyond the move/rename parity plumbing, this design folds in three correctness
fixes the parity work exposed. They are referenced by these numbers throughout
the doc (Helpers, §8, Edge Cases, Testing):

- **#1 — Backend-owned active record def.** The active record def must survive
  undo/redo of a rename/move; today it is Flutter-only state that gets silently
  cleared. Fixed in **§8**.
- **#2 — Infallible / atomic batch rename.** A mixed network+record batch must
  apply all-or-nothing with no swallowed errors. Fixed by **Helper 1**
  (`rename_record_type_def_unchecked`).
- **#3 — Undo/redo pin-layout repair.** Record-node pin layouts must be repaired
  after a record rename/delete is applied via undo/redo (the `Full` refresh does
  not do this). Fixed by **Helper 2** (`NodeTypeRegistry::repair_all_networks`).

### A kind-aware layer over the flat maps

Introduce a small backend enum to disambiguate a qualified name's kind:

```rust
// In structure_designer.rs (or node_type_registry.rs)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UserTypeKind {
    Network,
    Record,
}
```

with a registry helper:

```rust
impl NodeTypeRegistry {
    /// Kind of an existing *user-defined* type (network or user record def).
    /// Built-in record defs and built-in node types return None (immutable,
    /// not part of the movable hierarchy).
    pub fn user_type_kind(&self, name: &str) -> Option<UserTypeKind> {
        if self.node_networks.contains_key(name) {
            Some(UserTypeKind::Network)
        } else if self.record_type_defs.contains_key(name) {
            Some(UserTypeKind::Record)
        } else {
            None
        }
    }
}
```

All batch operations dispatch per-leaf on this kind. The per-leaf rewrites
already exist — `apply_rename_core` for networks, `rename_record_type_def` for
records — so the generalization is "iterate both maps, tag each affected name
with its kind, and call the right rewrite."

### Two shared backend helpers (parity glue)

Before the per-section changes, two small helpers close gaps that would
otherwise break batch atomicity and undo/redo refresh. Both stem from an
asymmetry between the network and record rewrite primitives that only surfaces
once records ride the batch/undo paths.

#### Helper 1 — `rename_record_type_def_unchecked` (infallibility parity, fix #2)

`apply_rename_core` (networks) **cannot fail** — it does a map move + a
`node_type_name` rewrite with no collision/missing/built-in guards.
`rename_record_type_def` (records) **validates** and returns
`Result<(), RecordTypeDefError>`. Routing records through the *checked* call
inside a batch is wrong on two counts:

- **Non-atomic batch.** In a mixed-folder move, networks rename
  unconditionally while one record's checked rename could return `Err`, leaving
  the batch half-applied (networks moved, one record stranded, earlier
  reference rewrites now dangling) with no rollback.
- **Swallowed errors.** The natural dispatch is `let _ = rename_record_type_def(…)`,
  which silently discards that `Err` (and the existing per-record undo command
  `RenameRecordTypeDefCommand` already does exactly this).

Fix: add the infallible analog of `apply_rename_core`:

```rust
impl NodeTypeRegistry {
    /// Infallible record rename for batch/undo paths where validity is already
    /// established (the preview's `name_is_taken` conflict check gates the user
    /// action; on undo/redo the target name was just vacated by the symmetric
    /// rename of the same batch). Mirrors `apply_rename_core`: map move + name
    /// field update + `rewrite_record_name_in_registry`, with NO
    /// built-in/collision/missing guards. The user-facing standalone
    /// `rename_record_type_def` keeps its checks (it is the validating entry
    /// point); only the batch namespace path and the undo commands call this.
    pub fn rename_record_type_def_unchecked(&mut self, old_name: &str, new_name: &str) {
        if old_name == new_name { return; }
        if let Some(mut def) = self.record_type_defs.remove(old_name) {
            def.name = new_name.to_string();
            self.record_type_defs.insert(new_name.to_string(), def);
            rewrite_record_name_in_registry(self, old_name, new_name);
        }
    }
}
```

With this, the batch record arm and **both** undo directions are infallible,
exactly like the network arm — no partial application, no `let _ =`.

#### Helper 2 — `NodeTypeRegistry::repair_all_networks` (pin-layout refresh parity, fix #3)

The forward record methods (`StructureDesigner::rename_record_type_def` /
`delete_record_type_def` / `update_record_type_def`,
`structure_designer.rs:1793-1798, 1833-1838, 1886-1891`) all run
`repair_node_network` on **every** network after the registry mutation, because
`rewrite_record_name_in_registry` clears `custom_node_type` on
`record_construct` / `record_destructure` / `product` nodes and only
`repair_node_network` repopulates their pin layouts. But the undo refresh path
does **not**: `apply_undo_refresh_mode(Full)` (`structure_designer.rs:649-656`)
runs only `mark_full_refresh` + `apply_node_display_policy(None)` +
`validate_active_network` — none of which repairs record-node pin layouts (and
`validate_active_network` only touches the active network + its parents, not all
networks). So a record rename/delete/restore applied via undo/redo leaves those
nodes with stale pins.

Fix: factor the forward methods' "repair every network" sweep into a method on
`NodeTypeRegistry` (so both `structure_designer.rs` and the undo-commands module
can call it through the registry they each already hold — symmetric with
Helper 1):

```rust
impl NodeTypeRegistry {
    /// Run `repair_node_network` on every stored network. Required after any
    /// record def add/rename/delete/restore so `record_construct` /
    /// `record_destructure` / `product` pin layouts (and now-incompatible wires)
    /// are refreshed — the `Full` undo refresh does NOT do this. Replaces the
    /// per-network loop currently inlined in the forward record methods.
    pub fn repair_all_networks(&mut self) {
        let names: Vec<String> = self.node_networks.keys().cloned().collect();
        for n in names {
            if let Some(mut network) = self.node_networks.remove(&n) {
                self.repair_node_network(&mut network);
                self.node_networks.insert(n, network);
            }
        }
    }
}
```

The forward record methods are refactored to call `self.node_type_registry
.repair_all_networks()` instead of inlining the loop. The generalized undo
commands (§6) call `ctx.node_type_registry.repair_all_networks()` whenever the
batch touched **any** record. (Pure-network namespace renames/deletes don't need
it — `apply_rename_core` already rewrites `node_type_name` and no record-node pin
cache is involved.) The existing per-record commands
(`RenameRecordTypeDefCommand`, etc.) share the same latent gap and are migrated
onto this method in the same change.

### Rust changes

#### 1. `compute_namespace_rename` — sweep both maps

Collect affected names from `node_networks` **and** `record_type_defs` under
`old_prefix.`. Tag each `NamespaceRenameItem` with its `UserTypeKind`. The
conflict check is unchanged (`name_is_taken` is already record-aware). The
`affected_set` used to exclude self-collisions must contain both kinds' old
names.

```rust
pub struct NamespaceRenameItem {
    pub old_name: String,
    pub new_name: String,
    pub conflict: bool,
    pub kind: UserTypeKind,   // NEW
}
```

`is_applicable()` / `valid_names` semantics are unchanged.

#### 2. `compute_network_rename` → `compute_leaf_rename`

Generalize the single-leaf preview to detect kind via `user_type_kind(old_name)`
and produce a one-item plan tagged with the kind (empty plan if the name is
unknown or a built-in). Same `NamespaceRenamePlan` return shape. Keep
`compute_network_rename` as a thin wrapper if any caller still needs the
network-specific name, or rename the API (see §7).

#### 3. `rename_namespace` — dispatch per kind

For each `(old, new, kind)` in the plan:

- `Network` → `apply_rename_core(registry, active_name, old, new)`, plus the
  existing `navigation_history.rename_network` and clipboard `node_type_name`
  cascade.
- `Record` → `registry.rename_record_type_def_unchecked(old, new)` (Helper 1 —
  infallible; internally runs `rewrite_record_name_in_registry`). The plan's
  `name_is_taken` conflict check has already gated the whole batch as
  applicable, and a prefix-substitution rename to a fresh prefix cannot
  self-collide against another affected old name, so the unchecked call is safe
  and keeps a mixed batch atomic (no half-applied state, no swallowed `Err`).
  No navigation-history entry (records are not navigated to). Clipboard
  record-ref rewrite is **out of scope** — it matches the existing standalone
  `rename_record_type_def` behavior, which does not touch the clipboard (noted
  as a pre-existing limitation in Edge Cases).

After dispatching all leaves, if the batch contained **any** record, call
`registry.repair_all_networks()` (Helper 2) so record-node pin layouts are
refreshed (the forward path already does this per-network). Push a single
generalized `RenameNamespaceCommand` (defined in §6).

#### 4. New single-leaf move path

`rename_node_network` stays as-is for networks. Add the record path by routing a
record-leaf move through the existing `StructureDesigner::rename_record_type_def`
(which already pushes `RenameRecordTypeDefCommand` and works for any dotted
name). The Flutter move dialog calls the kind-appropriate model method (see §7);
no new backend method is required beyond `compute_leaf_rename` for the preview.

#### 5. Generalize `delete_namespace` + reference check

- Collect affected **networks** (under `prefix.`) and affected **records**
  (under `prefix.`). Error only if **both** are empty (today's "No networks
  found" message becomes "No items found under namespace '…'").
- Keep `check_delete_references` for networks (network→network instances).
- Add a new helper:

```rust
/// Returns Err listing blockers if any entity NOT in the deleted set references
/// a record in `target_records` via RecordType::Named. "Entity not in the
/// deleted set" = a network whose name is not in `deleted_networks`, or a user
/// record def whose name is not in `target_records`. Walks network signatures +
/// all nodes (incl. zone bodies via walk_all_nodes) for embedded DataTypes, and
/// every surviving record def's field types.
fn check_record_delete_references(
    registry: &NodeTypeRegistry,
    target_records: &HashSet<&str>,
    deleted_networks: &HashSet<&str>,
) -> Result<(), String>
```

  Detection reuses the same `DataType`-walking surface that
  `rewrite_record_name_in_registry` already enumerates (factor the
  node-data/`DataType` traversal into a shared read-only visitor if convenient,
  or mirror its match arms). Run both checks before mutating; surface a combined
  error if either blocks.
- Snapshot affected networks (`SerializableNodeNetwork`, existing) **and**
  affected records (`RecordTypeDef`, which is `Clone`). Remove both from their
  maps. If any record was removed, call `registry.repair_all_networks()`
  (Helper 2) so wires that now resolve through a dangling `Named` ref are
  disconnected and record-node pin layouts refresh — exactly what the forward
  `delete_record_type_def` does. Active-record clearing is now backend-tracked
  and handled in the command + §8 (not only model-side).
- Push the generalized `DeleteNamespaceCommand` (defined in §6).

#### 6. Generalized undo commands

`RenameNamespaceCommand` carries kind-tagged renames and dispatches in
undo/redo:

```rust
pub struct NamespaceRename {
    pub old_name: String,
    pub new_name: String,
    pub kind: UserTypeKind,
}

pub struct RenameNamespaceCommand {
    pub renames: Vec<NamespaceRename>,
}

impl UndoCommand for RenameNamespaceCommand {
    fn undo(&self, ctx: &mut UndoContext) {
        let mut touched_record = false;
        for r in &self.renames {
            match r.kind {
                UserTypeKind::Network => apply_rename_core(
                    ctx.node_type_registry, ctx.active_network_name,
                    &r.new_name, &r.old_name),
                UserTypeKind::Record => {
                    // Helper 1 — infallible. The target name was just vacated by
                    // the symmetric rename of this same batch, so no validation
                    // is needed and no `Err` can be silently dropped.
                    ctx.node_type_registry
                        .rename_record_type_def_unchecked(&r.new_name, &r.old_name);
                    // Remap the active record def so undo restores the selection
                    // (parity with `apply_rename_core` remapping active_network).
                    if ctx.active_record_def_name.as_deref() == Some(&r.new_name) {
                        *ctx.active_record_def_name = Some(r.old_name.clone());
                    }
                    touched_record = true;
                }
            }
        }
        // Helper 2 — the `Full` refresh does NOT repair record-node pin layouts.
        if touched_record { ctx.node_type_registry.repair_all_networks(); }
    }
    // redo: forward direction (old→new), same active-record remap + repair;
    // refresh_mode: Full
}
```

Note the new `ctx.active_record_def_name: &mut Option<String>` on `UndoContext`
— the record-side analog of the existing `active_network_name: &mut Option<String>`.
See §8 for why the active record def must move into backend state.

`DeleteNamespaceCommand` gains a record list (no manual `Debug` needed for the
record part — `RecordTypeDef` derives `Debug`; networks still use the existing
manual impl):

```rust
pub struct DeleteNamespaceCommand {
    pub network_snapshots: Vec<(String, SerializableNodeNetwork)>,
    pub record_snapshots: Vec<(String, RecordTypeDef)>,   // NEW
    pub active_network_before: Option<String>,
    pub active_network_after: Option<String>,
}
```

undo re-inserts records (`record_type_defs.insert`) and networks; redo removes
both. Order is irrelevant for storage. **Both directions must call
`repair_all_networks` (Helper 2) whenever `record_snapshots` is non-empty** —
the `Full` refresh re-validates the active network but does not repair
record-node pin layouts or disconnect wires that now dangle (undo of a delete
re-introduces a `Named` target; redo removes it again). Also remap/clear
`ctx.active_record_def_name`: redo clears it if the active def was under the
deleted prefix, undo restores it if it was. (Built-in record defs are never in
these snapshots: they are excluded by `user_type_kind`/`delete_record_type_def`
guards.)

**Undo collision-check caveat (resolved).** Both undo directions now use
`rename_record_type_def_unchecked` (Helper 1), which performs no collision
check, so there is no `Err` to reject or swallow — the swap is unconditional.
This is safe because the target name was just vacated by the symmetric rename of
the same batch. Apply records and networks in the recorded order; both rewrites
are exact-name and order-independent across kinds (a network never aliases a
record name — single namespace).

#### 7. API + FRB

- `preview_namespace_rename` already returns `APINamespaceRenamePreview`; the
  items can optionally carry a `kind` (string/enum) so the dialog could show an
  icon — **optional**, not required for correctness. Renaming
  `preview_network_rename` → `preview_leaf_rename` (or keeping the name and just
  making it kind-agnostic internally) keeps the FFI surface small.
- `delete_namespace` signature is unchanged (`prefix -> APIResult`); only its
  behavior broadens.
- Add a kind-agnostic move entry the dialog can call, or have the model pick
  `renameNodeNetwork` vs `renameRecordTypeDef` based on the leaf kind it already
  knows (`_LeafKind`). The latter needs **no new API** — preferred.
- Run `flutter_rust_bridge_codegen generate`.

#### 8. Active record def → backend state (undo/redo parity, fix #1)

Today the active record def lives **only** on the Flutter side
(`StructureDesignerModel.activeRecordDefName`). The forward model methods remap
it (`renameRecordTypeDef`: old→new at `structure_designer_model.dart:801-802`)
or clear it (`deleteRecordTypeDef`: 787-788), and `refreshFromKernel`
(2493-2495) **only drops a stale name**, never remaps. Undo/redo go through the
generic `sd_api.undo()`/`redo()` + `refreshFromKernel`, which never invokes
those forward methods — so undoing/redoing a record folder-move (or a standalone
record rename) finds the old name gone and **silently clears the user's active
record def** instead of following it to the new name.

Networks don't have this bug: the active network is **backend state**
(`StructureDesigner::active_node_network_name`), and `apply_rename_core` remaps
that pointer *inside the undo command* via `UndoContext.active_network_name`.
The symmetric fix is to give the active record def the same treatment:

- Add `StructureDesigner::active_record_def_name: Option<String>` as the single
  source of truth. The existing record add/rename/delete/`setActive` paths set
  it; expose it to Flutter (a `get_active_record_def_name` accessor, or fold it
  into the structure-designer view that `refreshFromKernel` already reads).
- Thread `active_record_def_name: &mut Option<String>` onto `UndoContext`
  (alongside `active_network_name`). Wire it through the same `std::mem::take`
  dance `undo()`/`redo()` use.
- `RenameNamespaceCommand` / `RenameRecordTypeDefCommand` remap it on
  undo/redo; `DeleteNamespaceCommand` / `DeleteRecordTypeDefCommand` clear it
  when the active def is removed and restore it on undo (§6).
- Flutter `activeRecordDefName` becomes a **mirror** read in
  `refreshFromKernel` from the backend value. The forward model methods no
  longer need their hand-rolled remap/clear (the backend now owns it), and the
  "drop if missing" reconciliation becomes a plain assignment from the backend
  field — correct for both forward edits and undo/redo.

This is the only part of #1's fix that adds API/FRB surface; it is what makes
the active-record selection survive undo/redo the way the active network already
does.

### Flutter changes

#### `node_network_tree_view.dart`

- **`_buildTreeFromNames`**: stop appending records flat. Feed record names
  through the same segment-splitting loop used for networks, building shared
  intermediate namespace nodes, with `leafKind = recordDef` on the record leaves.
  A folder node is kind-neutral; a folder may now contain both kinds. (Simplest:
  generalize the loop to take a list of `(qualifiedName, _LeafKind)` and run once
  over the union.)
- **Inline rename** (lines 331–348): drop the record special-case. Treat a record
  leaf exactly like a network leaf — preserve its namespace via
  `combineQualifiedName(getNamespace(old), newSegment)` — and route the commit to
  `model.renameRecordTypeDef(oldFull, newFull)`. (`rename_record_type_def` already
  accepts dotted names; nothing else changes.)
- **Context menu** (line 618): allow "Move / rename…" for record leaves too —
  remove the `leafKind == network` gate.
- **`_handleMove`**: for a record leaf, open the move dialog in record mode (it
  previews via `previewLeafRename` and commits via `renameRecordTypeDef`). For a
  folder, the existing namespace path now covers mixed contents unchanged.
- **Namespace delete** (`_showNamespaceDeleteConfirmation`, line 524): reword
  "networks" → "items"; the listing via `_collectLeafNames` already gathers all
  leaves regardless of kind, so it works as-is.

#### `move_namespace_dialog.dart`

- Generalize `showMoveNetworkDialog` to accept a leaf kind (or add a thin
  `showMoveRecordDialog`) so it previews with the kind-agnostic
  `previewLeafRename` and commits via the right model method. The namespace
  dialog is unchanged (the backend now sweeps both maps).

#### `structure_designer_model.dart`

- `renameNamespace` / `deleteNamespace` need no signature change.
- With the active record def now backend-owned (§8), `activeRecordDefName`
  becomes a mirror refreshed from the kernel in `refreshFromKernel`. Drop the
  hand-rolled forward remap/clear in `renameRecordTypeDef` (801-802) /
  `deleteRecordTypeDef` (787-788) and the "drop if missing" branch (2493-2495)
  in favor of a single read from the backend value — this is what makes the
  selection follow a folder move and survive undo/redo, not just the forward
  edit.

## Edge Cases

- **Mixed folder.** `Physics/` containing `Physics.Spring` (network) and
  `Physics.ElementMapping` (record) renames/deletes both atomically in one undo
  step.
- **Folder of records only.** `delete_namespace` must not error "no networks";
  the empty check counts both maps.
- **Record referenced from outside on namespace delete.** Blocked with a listing
  (chosen policy). Standalone record-leaf delete still dangles (unchanged).
- **Collision on move.** `name_is_taken` already rejects moving a record onto an
  existing network/record/built-in name; the preview reports `conflict`, Apply
  stays disabled (existing dialog behavior).
- **Built-in records.** `ElementMapping` etc. never appear in the tree (panel
  uses user-defs-only `get_record_type_def_names`) and are guarded out of every
  rename/delete path by `user_type_kind` / `is_built_in_record_type_def`.
- **Clipboard record refs (pre-existing limitation).** Neither standalone nor
  batch record rename rewrites `RecordType::Named` references inside the
  clipboard, so a copied `record_construct`/`parameter` node pasted after a rename
  can dangle. This matches today's standalone `rename_record_type_def` behavior
  and is left out of scope; flagged here so it is a known, not a surprise.
- **Active record def under rename/delete.** Now backend-owned (§8) and remapped
  (rename) / cleared (delete) **inside the undo command**, mirroring
  active-network handling — so it follows a folder move and survives undo/redo,
  not just the forward edit. The earlier model-only handling silently dropped it
  on undo/redo.
- **Dots typed into a record's inline rename.** Adds a hierarchy level, exactly
  as for networks.

## Implementation Phases

### Phase 1 — Backend kind-aware operations + reference check + undo
- Add `UserTypeKind` + `NodeTypeRegistry::user_type_kind`.
- Add the two shared helpers: `rename_record_type_def_unchecked` (Helper 1) and
  `repair_all_networks` (Helper 2). Migrate the existing per-record commands
  (`RenameRecordTypeDefCommand` etc.) onto them too (drops the `let _ =`
  swallow; adds the missing repair sweep on undo/redo).
- Add `StructureDesigner::active_record_def_name` + thread
  `active_record_def_name: &mut Option<String>` onto `UndoContext` (§8).
- Extend `compute_namespace_rename` to sweep both maps and tag items; add
  `kind` to `NamespaceRenameItem`.
- Generalize `compute_network_rename` → `compute_leaf_rename` (kind dispatch).
- Make `rename_namespace` dispatch per kind (record arm → Helper 1; repair sweep
  if any record touched).
- Add `check_record_delete_references`; broaden `delete_namespace` to records
  (empty check, both reference checks, record snapshots, removal, repair sweep).
- Generalize `RenameNamespaceCommand` (kind-tagged) and `DeleteNamespaceCommand`
  (record snapshots); both call the repair sweep + remap/clear the active record
  def on undo/redo when records are involved.
- **Tests (land this phase):** see [Testing → Phase 1](#phase-1--backend-rust) —
  the `check_record_delete_references` matrix, namespace ops, infallibility,
  undo/redo round trips (incl. the #1/#3 regressions), and the dotted-record
  `.cnnd` roundtrip.

### Phase 2 — API + FRB + model
- Make `preview_network_rename` kind-agnostic (or add `preview_leaf_rename`).
  Optionally surface `kind` on `APINamespaceRenameItem`.
- Expose the backend active record def (a `get_active_record_def_name` accessor
  or fold it into the structure-designer view; §8).
- `flutter_rust_bridge_codegen generate`.
- Model: make `activeRecordDefName` a mirror of the backend value read in
  `refreshFromKernel`; drop the hand-rolled forward remap/clear.
- **Tests:** none standalone — see [Testing → Phase 2](#phase-2--apifrbmodel)
  (intentional gap; covered by Phase 1 backend + Phase 3 UI).

### Phase 3 — Flutter tree + dialogs
- `_buildTreeFromNames`: records through the segment splitter (union loop).
- Inline rename: drop record special-case; namespace-preserving + route to
  `renameRecordTypeDef`.
- Context menu: enable "Move / rename…" for record leaves.
- `_handleMove` + move dialog: record-leaf mode.
- Namespace delete confirmation wording → "items".
- **Tests (land this phase):** see [Testing → Phase 3](#phase-3--flutter-integration_test)
  — tree builder, namespace-preserving inline rename, move dialog, and the
  end-to-end "#1 active def survives undo/redo via the UI".

### Phase 4 — Polish + manual acceptance
- SnackBar feedback parity for record move/rename failures.
- The automated suites in Phases 1–3 are the primary coverage; this phase is the
  manual `flutter run` belt-and-suspenders pass — see
  [Testing → Phase 4](#phase-4--manual-acceptance).

## Testing

Tests land **in the phase that introduces the code they exercise**, so a
failure is fixed against fresh code in the same phase (no deferred "test phase").
Conventions: backend undo tests assert **state round-trips** via `normalize_json()`
byte-equality (do → undo ⇒ equals initial; redo ⇒ equals post-do) and guard the
`next_node_id` / `next_param_id` / HashMap-ordering pitfalls from `undo/AGENTS.md`.
Register new Rust test modules in the parent crate file (e.g. `tests/structure_designer.rs`).

### Phase 1 — backend (Rust)

**`check_record_delete_references` matrix** (the riskiest piece — one test per
walked surface; `tests/structure_designer/structure_designer_test.rs`):
- Surviving **network signature** (parameter type or output-pin `Fixed` type)
  references a deleted record → **block**.
- Surviving network **node** embeds the deleted record in a `DataType` field —
  a case per arm of the walk: `expr`, `map`, `filter`, `fold`, `foreach`,
  `sequence`, `array_at` / `array_len` / `array_concat` / `array_append`,
  `record_construct` / `record_destructure` / `product`.
- Reference inside a surviving network's **zone body** (HOF) → **block** (guards
  the `walk_all_nodes` recursion).
- Surviving **record-def field** references a deleted record → **block**.
- **Deleted** network references a **deleted** record (both in the set) →
  **allowed** (no false block).
- **Built-in** record def present in the registry → never spuriously blocks.
- Combined network + record blockers → one combined error listing.

**Namespace ops** (`structure_designer_test.rs`):
- Record-only namespace rename and delete (empty check counts both maps;
  reworded "No items found …" message).
- Mixed folder (`Physics.Spring` network + `Physics.ElementMapping` record)
  rename and delete in one step.
- Folder move where a moved record references **another moved record** — the
  referrer's field follows to the new name (exercises the simultaneous-rewrite
  case the "Undo collision-check caveat" reasons about).

**#2 — infallible / atomic batch rename** (`structure_designer_test.rs`):
- `rename_record_type_def_unchecked`: renames + rewrites refs; no-op on
  `old == new`; no-op (no panic) when `old` is missing.
- A mixed batch applies **every** leaf (no half-application).
- A batch whose preview reports `conflict` is `!is_applicable()` → apply is
  refused with no partial mutation.

**Undo/redo** (`tests/structure_designer/undo_test.rs`; round-trip + `normalize_json`):
- `RenameNamespaceCommand` (record-only and mixed): do → undo ⇒ initial;
  redo ⇒ post-do.
- `DeleteNamespaceCommand` (non-empty `record_snapshots`): undo re-inserts records
  + networks; redo removes both.
- **#1 active record def:** with a def active, undo/redo of a folder move that
  renamed it leaves `active_record_def_name` **remapped** (not cleared);
  namespace delete clears it on redo and **restores** it on undo.
- **#3 pin-layout repair:** after undo *and* redo of a namespace rename/delete
  touching a record, that record's `record_construct` / `record_destructure` /
  `product` instances have the **correct pin layout** (assert pins, not just node
  presence).
- **#3 migrated per-record commands:** undo of a *standalone*
  `RenameRecordTypeDefCommand` / `DeleteRecordTypeDefCommand` now also repairs
  pin layouts and remaps the active record def (guards the latent gap the
  migration closes).

**Serialization roundtrip** (`tests/integration/cnnd_roundtrip_test.rs` —
greenfield: no record-def roundtrip exists there today):
- A record def at a **dotted** name (`Physics.ElementMapping`) referenced by a
  `record_construct` survives save → load (through `canonicalize_network`), with
  pins and the `Named` reference intact.

### Phase 2 — API/FRB/model

No standalone automated test, **by design**: the leaf-rename preview and the
active-record accessor are thin FRB wrappers (`rust/AGENTS.md`: test the core,
not the wrapper), and the Dart model change is glue with no behavior of its own
until Phase 3's UI consumes it. Its correctness is pinned by the Phase 1 backend
guard (`active_record_def_name` remap) and the Phase 3 end-to-end UI test (the
`refreshFromKernel` mirror read). Called out so the absence is intentional, not
an oversight.

### Phase 3 — Flutter (`integration_test/`)

- **`_buildTreeFromNames`** (pure `names → tree` logic — widget/unit test):
  networks-only, **records-only** folder, **mixed** folder, and **dotted-record
  nesting** (`Chemistry.Bonds.Pair`) each produce the expected tree with
  `leafKind` set correctly. Add to `integration_test/node_network/network_list_test.dart`
  (or a sibling `tree_view_test.dart`).
- **Inline rename** of a record leaf preserves its namespace
  (`getNamespace(old)` + new segment → `renameRecordTypeDef(oldFull, newFull)`);
  a typed dot adds a level.
- **Context menu** "Move / rename…" is enabled on a record leaf.
- **Move dialog** record-leaf mode previews via `previewLeafRename` and commits
  via `renameRecordTypeDef`.
- **End-to-end #1:** activate a record def, move its folder, `Ctrl+Z` / `Ctrl+Y`
  — the schema editor stays on the (re)named def (exercises the Phase 2 model
  mirror through the UI). Place beside the panel tests in `integration_test/panels/`.

### Phase 4 — manual acceptance

The suites above are the primary coverage. The manual `flutter run` walkthrough
is the final belt-and-suspenders pass: create a nested record, move a folder
containing a network + a record, undo/redo, confirm an external-ref-blocked
delete, and save/reload a `.cnnd` with dotted record names.

## Files to Modify

### Phase 1 (Rust)
| File | Change |
|------|--------|
| `rust/src/structure_designer/node_type_registry.rs` | `UserTypeKind`, `user_type_kind`, `rename_record_type_def_unchecked` (Helper 1), `repair_all_networks` (Helper 2 — method; forward record methods refactored to call it) |
| `rust/src/structure_designer/structure_designer.rs` | `compute_namespace_rename` (both maps + kind), `compute_leaf_rename`, `rename_namespace` (dispatch + repair sweep), `delete_namespace` (records + repair sweep), `check_record_delete_references`, `active_record_def_name` field; forward record methods call `repair_all_networks()` |
| `rust/src/structure_designer/undo/mod.rs` | `UndoContext.active_record_def_name`; thread through `undo()`/`redo()` |
| `rust/src/structure_designer/undo/commands/rename_namespace.rs` | Kind-tagged `RenameNamespaceCommand`; Helper 1 + repair sweep + active-record remap |
| `rust/src/structure_designer/undo/commands/delete_namespace.rs` | `record_snapshots`; repair sweep + active-record clear/restore |
| `rust/src/structure_designer/undo/commands/{rename,delete,update}_record_type_def.rs` | Migrate onto Helper 1 / Helper 2 + active-record remap (closes the same latent gap) |
| `rust/tests/structure_designer/structure_designer_test.rs` | Reference-check matrix, namespace ops, #2 infallibility tests (Testing → Phase 1) |
| `rust/tests/structure_designer/undo_test.rs` | `RenameNamespaceCommand`/`DeleteNamespaceCommand` round trips + #1/#3 regressions |
| `rust/tests/integration/cnnd_roundtrip_test.rs` | Dotted-record-name roundtrip (greenfield) |

### Phase 2 (API/FRB/model)
| File | Change |
|------|--------|
| `rust/src/api/structure_designer/structure_designer_api.rs` | Kind-agnostic leaf-rename preview; (optional) `kind` on preview item; expose active record def (§8) |
| `rust/src/api/structure_designer/structure_designer_api_types.rs` | (optional) `APINamespaceRenameItem.kind`; active-record-def on the view if folded in |
| `lib/src/rust/` | Regenerated bindings |
| `lib/structure_designer/structure_designer_model.dart` | `activeRecordDefName` becomes a backend mirror read in `refreshFromKernel`; drop forward remap/clear |

### Phase 3 (Flutter)
| File | Change |
|------|--------|
| `lib/structure_designer/node_networks_list/node_network_tree_view.dart` | Record leaves through segment splitter; inline rename; context-menu gate; `_handleMove`; delete wording |
| `lib/structure_designer/node_networks_list/move_namespace_dialog.dart` | Record-leaf move mode |
| `integration_test/node_network/network_list_test.dart` (or new `tree_view_test.dart`) | `_buildTreeFromNames` (mixed / records-only / dotted nesting), inline rename, context menu, move dialog |
| `integration_test/panels/` | End-to-end #1: active record def survives undo/redo via the UI |

## Estimated Scope
- **Phase 1**: ~300–400 lines Rust (the reference check + the two generalizations
  + the two shared helpers + the `active_record_def_name` plumbing + undo),
  ~250 lines tests. Largest single piece is `check_record_delete_references`; the
  two helpers and the active-record field are small but touch several files.
- **Phase 2**: ~40 lines Rust (incl. the active-record accessor) + regen + ~20
  lines Dart (model now mirrors the backend value; net deletion of forward
  remap/clear).
- **Phase 3**: ~80 lines Dart (mostly deleting the flat-records special-cases)
  + ~120 lines integration tests (tree builder + inline rename + move dialog +
  the end-to-end #1 walkthrough).

Total: ~650–750 lines, lower-risk than the original tree-rename feature because
the per-leaf reference-rewrite layer for records already exists and is tested.
The three correctness fixes (infallible batch rename, undo/redo pin-layout
repair, backend-owned active record def) are each small and mirror an existing
network mechanism rather than inventing a new one.

## Related Docs
- `doc/design_tree_view_rename.md` — the original networks-only hierarchy feature.
- `doc/design_record_types.md` — record type system, `RecordType::Named`,
  structural subtyping, the rename-rewrite rationale.
- `doc/design_atom_replace_rules_input.md` (Phase A) — built-in record defs and
  the unified `lookup_record_type_def` accessor.
