# Node Network Thumbnails

Issue: [#84 — Add thumbnail preview for node networks](https://github.com/atomCAD/atomCAD/issues/84)

## Motivation

A `.cnnd` file is a whole library of node networks, and the only way to tell
them apart today is by name. A small picture of what each network *builds*
makes finding things quicker, especially in a file somebody else wrote.

mechadense lists three places a thumbnail would be useful, which do not
exclude each other:

1. next to each network in the node network list (the original request);
2. on custom nodes in the node canvas;
3. as icons for future point-and-click tool buttons for user-defined networks.

This document covers how thumbnails are produced, stored and refreshed, and
the UI for (1) and (2). (3) depends on a feature that does not exist yet; the
stored images will be ready for it.

### Scope

- **In:** one thumbnail per node network, showing its **3D output** (not its
  node graph — a design is recognised by what it builds, not by its wiring).
  Automatic capture, a manual *Set current view as thumbnail* command, storage
  in the `.cnnd`, display in the network list and tree, and a hover preview on
  custom nodes in the canvas.
- **Out:** a thumbnail for the whole file shown by the operating system's file
  browser (the issue's "overall thumbnail"). That needs a Windows shell
  thumbnail handler, which is a separate project. See *Future work*.
- **Out:** evaluating networks the user has not opened just to make their
  pictures. See D2.

## Current state (facts, with references)

- **One renderer, one scene.** `CADInstance` (`rust/src/api/api_common.rs`)
  holds the active `structure_designer`, the parked documents and a single
  `renderer`. The renderer has one colour target, one depth target, one
  readback buffer, one camera and one set of GPU meshes
  (`crates/atomcad-renderer/src/renderer.rs`).
- **The scene always belongs to the active network.**
  `StructureDesigner::refresh` rebuilds
  `last_generated_structure_designer_scene` from
  `active_node_network_name`. `refresh_structure_designer` (`api_common.rs`)
  evaluates, tessellates with `tessellate_scene_content` and uploads with
  `renderer.update_all_gpu_meshes`. Evaluating a network that is not active
  would mean un-hard-wiring `refresh`, gadget creation and error collection
  from the active network, plus validating the network first (an unvalidated
  network can have arguments that lag its parameters).
- **The active network changes from many places**, not only from the network
  list: selecting a network (`set_active_node_network`), back/forward
  (`show_navigated_network`), adding a network (`add_new_node_network`,
  `add_new_node_network_in_namespace`), duplicating one (the copy is
  activated), *Open in library file* (`DocumentSet::activate_at`), the
  fallbacks in `library_link_ops.rs` and `library_refresh_ops.rs`, the
  fallback after a delete, switching tabs (`DocumentSet::swap_in`), and
  File > New / Open replacing a document's content in place.
- **The GPU keeps the previous scene until the upload.**
  `refresh_structure_designer` first rebuilds the CPU scene
  (`structure_designer.refresh`), then tessellates, then calls
  `update_all_gpu_meshes` — the single place where the meshes on the GPU are
  replaced. Every network or document switch reaches it through a full
  refresh.
- **Tessellated content does not depend on the camera.**
  `tessellate_scene_content` takes the camera only for the pivot-point cube in
  the lightweight content (`scene_tessellator.rs`). Impostors are expanded in
  the shaders. The meshes already on the GPU can therefore be drawn from any
  camera without re-tessellating.
- **Every frame is already an offscreen render with readback.** There is no
  swapchain: `provide_texture` (`common_api.rs`) calls `renderer.render(bg)`,
  which returns BGRA bytes. `render` re-sorts transparent impostors for the
  camera it is drawing with, and the next live frame re-sorts again because
  its sort cache is keyed on the view matrix.
- **The existing screenshot** (`rust/src/api/screenshot_api.rs`,
  `capture_screenshot`) resizes the one render target to the requested size,
  renders, restores the size and writes a PNG file with the `image` crate.
  Each resize reallocates the live viewport's textures and readback buffer.
  It draws everything, gadgets and grid included.
- **Each network already stores its own camera:**
  `NodeNetwork.camera_settings: Option<CameraSettings>`, serialized as an
  optional field. `sync_camera_to_active_network` writes it (and marks the
  document dirty, except for linked networks), and `apply_camera_settings`
  reads it back on activation.
- **There is no zoom-to-fit** or scene-bounds function anywhere.
- **`.cnnd` is pretty-printed JSON** (`serde_json::to_string_pretty`,
  `node_networks_serialization.rs`). `SerializableNodeNetwork` already has
  additive optional fields (`camera_settings`, `canvas_viewport`) using
  `#[serde(default, skip_serializing_if = "Option::is_none")]` without a
  version bump. `duplicate_node_network` copies a network by
  snapshot-and-deserialize, so any serialized field travels with a duplicate.
- **Linked library content is never written** by the file that links it.
- **Save requires a dirty document:** `canSave => isDirty && filePath != null`
  (`structure_designer_model.dart`).
- **Undo snapshots whole networks.** Text edits, inline, convert-to-closure,
  factor selection, delete/duplicate network, zone-body edits and others store
  a serialized `SerializableNodeNetwork` before and after
  (`undo/AGENTS.md`), and restoring one deserializes a fresh `NodeNetwork`.
- **The workspace has `image` (PNG feature) but no base64 crate.**
- **The network list** is `node_network_list_view.dart` (a `ListTile` with an
  `Icon` as `leading`) and `node_network_tree_view.dart`, both under
  `lib/structure_designer/node_networks_list/`. Both are fed by
  `getNodeNetworksWithValidation()` → `APINetworkWithValidationErrors`.
- **Node heights are mirrored in Rust** (`node_layout.rs`, "must match
  Flutter"), so making a canvas node taller is a two-sided change.

## Decisions

### D1 — The thumbnail shows the 3D output

It is a render of the network's displayed nodes, as the viewport would show
them, minus the editing aids (D5).

### D2 — Capture happens when the GPU scene is replaced by another network's, and before saving

Thumbnails are captured only from the scene **already on the GPU**, which is
evaluated and tessellated. Capturing costs one extra small draw plus a
readback, a few milliseconds.

The capture runs at two points:

- **When the meshes on the GPU are about to be replaced by another network's.**
  The renderer remembers which network its content meshes belong to:
  `content_owner: Option<(DocumentId, String)>`, set by every
  `update_all_gpu_meshes` call to the active document and network (`None`
  only when no network is active; an empty scene still has an owner, and D3
  skips it). In `refresh_structure_designer`, just before
  `update_all_gpu_meshes`, if the network being uploaded is not
  `content_owner`, the old content is captured first and written to
  `content_owner`'s network.
- **Before every save** (*Save* and *Save As*), for the active network — so the
  network the user worked on last is in the file even if they never leave it.

Hooking the mesh replacement rather than the switch commands means every path
that changes the active network — the list, back/forward, adding or
duplicating a network, *Open in library file*, library-refresh fallbacks, tab
switches, File > Open (see *Current state*) — captures with no per-path code,
and so will any path added later.

How the capture finds its target and camera:

- **The target may no longer be active.** After a tab switch, the old network
  belongs to a parked document, so the API looks it up by `DocumentId` in the
  document set. If the document or the network no longer exists (deleted,
  renamed, document closed or its content replaced by File > Open/New), the
  capture is skipped: a deleted network needs no picture, and a renamed one is
  captured under its new name at the next switch or save.
- **The camera** is the old network's saved `camera_settings`, framed as in
  D4. The live camera has already moved to the new network by then, which
  does not matter.
- **Linked networks** are skipped (D8).

No thumbnail is ever produced by evaluating a network that is not active.

Consequence: a network that has never been opened in any session has no
thumbnail, and the UI shows the existing icon for it. An explicit *Generate
missing thumbnails* command is listed under *Future work* if that turns out to
matter. Closing a tab or the application does not capture: the last network's
picture is only as fresh as the last save, which is when it would reach the
file anyway.

The renderer lives in the root crate and the structure designer does not know
about it, so the capture lives in the API layer: one helper,
`capture_gpu_content_thumbnail(cad_instance, owner)`, called from
`refresh_structure_designer` and from the save functions. The
structure-designer crate stores opaque PNG bytes and knows nothing about
rendering.

### D3 — Replace the stored image only when the picture really changed

On each capture, the new render is compared with the stored image: decode the
stored PNG and count pixels whose largest channel difference exceeds a small
threshold. If too few pixels differ, the stored image is kept as it is.

This one rule covers every way a picture can go stale — edits to the network,
edits to a custom node it uses, a different display style, a changed viewing
direction — with no bookkeeping about *why* it changed.

What the tolerance protects against is **renderer noise**, not small edits.
Renders on another GPU or driver differ by a few levels in a few pixels;
without a tolerance, opening a file on a collaborator's machine and browsing
it would rewrite every image and fill git diffs with noise. Revisiting a
network without changing it is likewise a no-op. An edit, on the other hand,
usually rewrites the image even when it is small: the automatic framing (D4)
refits to the content bounds, so anything that moves the bounding box — one
atom added at the edge, a displayed helper node — rescales the whole picture
and changes nearly every pixel. That is accepted: the network changed in the
same commit, so the changed image line sits next to a real change. The
threshold is not meant to hide small edits and must not be tuned up to do so.

Starting constants (to tune against real files): a pixel counts as different
when a channel differs by more than 24 out of 255; the image counts as changed
when more than 0.5% of its pixels differ.

If the scene being captured displays nothing (an empty scene, or every
displayed node failed to evaluate — `content_bounds` is `None`), no capture
happens and the stored image is kept. An empty picture is never useful.

A thumbnail captured while the network has no thumbnail yet is always stored.

### D4 — Automatic framing: the network's viewing direction, fitted to the content

The automatic thumbnail uses the **direction** of the network's own saved
camera, but sets the **distance** so the whole displayed content fits. The
live view is often zoomed in on a detail, which makes a poor thumbnail.

- **Bounds:** the axis-aligned bounding box of the content meshes, computed
  when they are uploaded (`update_all_gpu_meshes` already receives the CPU
  meshes, and also records `content_owner`, D2): triangle and wireframe
  vertices, atom impostor centres grown by their radius, bond impostor
  endpoints, transparent impostors and isosurface vertices. Gadget, lightweight, label and background meshes are excluded. The
  renderer keeps it as `content_bounds: Option<(DVec3, DVec3)>`.
- **Fit:** take the bounding sphere (centre `c`, radius `r`) of the box. The
  view direction is `normalize(eye - target)` of the network's camera, or of
  the default camera if it has none. Perspective:
  `eye = c + dir · (1.1 · r / sin(fovy / 2))`, `target = c`. Orthographic:
  `ortho_half_height = 1.1 · r`. `up` is the network camera's `up`,
  orthonormalized. `znear`/`zfar` must cover `[dist - r, dist + r]` — check
  this against very large structures (the 1M-atom showcase).
- The fit is a pure function `fit_camera_to_bounds(&Camera, bounds) -> Camera`
  with unit tests.

### D5 — Thumbnail render mode

The thumbnail draws only the content: main, wireframe, atom and bond
impostors, transparent impostors and transparent isosurfaces. It leaves out
gadgets, the lightweight content (pivot cube), the grid/background lines and
atom labels (unreadable at thumbnail size). Selection highlighting is part of
the content meshes; whether it shows in the thumbnail needs checking during
implementation, and if it does, the thumbnail must suppress it the same way
the network-image export does (`hideSelection`), never by clearing the
selection.

The background is a **fixed** colour — the default viewport background, not the
user's preference — so the stored image does not depend on who saved the file.

The image is rendered at 256×256 and box-downsampled 2×2 to the stored
128×128. Impostors are not antialiased by MSAA, so downsampling is what
smooths their edges.

**Renderer change:** factor the body of `render` into a pass that takes a
**render target** (colour + depth texture views, readback buffer, size), a
camera bind group and a set of pass flags. The live viewport keeps its own
target and camera buffer. The thumbnail gets its own small target and its own
camera uniform buffer and bind group, created once and reused, so a capture
never resizes or reallocates the live target and never writes the live camera
buffer. `render_thumbnail(camera, bg) -> Vec<u8>` (RGBA, 128×128) is the public
entry point; PNG encoding and the comparison of D3 live next to it in the
renderer crate, which already depends on `image`.

### D6 — *Set current view as thumbnail* stores pixels, nothing else

The network's right-click menu (list and tree) gets **Set current view as
thumbnail**. It renders the live camera exactly as the user sees it — square,
keeping the vertical field of view, so it is the centre of the viewport — in
thumbnail render mode (D5), and stores the image. No camera is stored: when
the design later changes a lot, nothing re-renders from a remembered
position.

The command is enabled only for the active network, which is the only one
with a scene to render, and only for a local (not linked) network.

The stored thumbnail is then marked **user-set**, and automatic capture (D2,
D3) leaves a user-set thumbnail alone. A user-set image can go stale, but only
because the user chose it, and they can see that and fix it: run the command
again, or choose **Reset to automatic thumbnail** (shown only when the
thumbnail is user-set), which clears the flag and immediately captures an
automatic image (if the scene is empty, D3 keeps the current image, now
automatic).

Both commands change saved data on the user's request, so both are undoable
(`SetNetworkThumbnailCommand { network_name, before, after }`, where each side
is the PNG bytes plus the flag) and both mark the document dirty.

### D7 — Storage: base64 PNG inside each network in the `.cnnd`

```json
{
  "next_node_id": 12,
  "node_type": { "...": "..." },
  "nodes": [ "..." ],
  "camera_settings": { "...": "..." },
  "thumbnail": {
    "png": "iVBORw0KGgoAAAANSUhEUgAA...",
    "user_set": true
  }
}
```

- `SerializableNodeNetwork` gets a last field
  `thumbnail: Option<SerializableThumbnail>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. It is last so
  the network's real content stays at the top of its object. `user_set` is
  omitted when false. No version bump: older files load with no thumbnails,
  and older builds ignore the field (they drop it when they save).
- In memory: `NodeNetwork.thumbnail: Option<NetworkThumbnail { png: Vec<u8>,
  user_set: bool }>`, plus a session-only `thumbnail_revision: u64` for the UI
  cache (D9). Revisions are drawn from **one process-wide counter that never
  goes backwards** (a static `AtomicU64`), and a network takes a fresh value
  whenever its thumbnail is set or the network is created or deserialized (file
  load, library load, undo restore, duplicate). A per-network counter would
  restart on every rebuild, so two different images could share a revision —
  for example after an undo restore followed by a capture, or after File > Open
  loads another file's "Main" into the same tab — and the UI would show the
  stale one.
- Base64 needs a new workspace dependency, the `base64` crate, used by the
  structure-designer crate's serialization only.
- **Size:** a 128×128 PNG of a render on a flat background is roughly
  15–25 KB, 20–35 KB as base64, so about 1–1.5 MB for a 50-network file.
  Pretty-printing keeps each image on one line, so a changed thumbnail is one
  changed line in a diff.
- Because the image is part of the network, rename and delete need no extra
  code, duplicate needs only to serialize with the thumbnail (D8), and a
  linked library's thumbnails arrive with the library.

Rejected alternatives:

- **Sidecar files** (`foo.cnnd.thumbs/`) are lost when a file is moved,
  renamed or emailed, library linking would have to find them, and a save
  would no longer be one write.
- **A zip container** (JSON + PNGs) would break plain-text diffs and every
  tool that reads `.cnnd`. Too big a change for this feature.
- **A local app cache only:** collaborators would never see the images, which
  defeats the purpose.
- **One top-level `thumbnails` map:** keeps the blobs out of the network
  objects, but rename, duplicate and delete would have to keep its keys in
  sync. Putting the field last in each network gives nearly the same
  readability.

### D8 — Dirty flag, undo and linked networks

- **Automatic capture never marks the document dirty.** Just browsing networks
  must not produce an "unsaved changes" prompt. A capture that follows an edit
  rides along with that edit's dirty flag.
- **But automatic capture does make Save available.** Save is gated on
  `isDirty` today, so without this a file that was only browsed — every
  network now has a picture — could never be saved, and the pictures would be
  lost on close. `StructureDesigner` gets a session-only
  `has_unsaved_thumbnails: bool`, set when an automatic capture stores or
  replaces an image (in whichever document owns the network, parked or not)
  and cleared by save, load and new. Flutter's gate becomes
  `canSave => (isDirty || hasUnsavedThumbnails) && filePath != null`. The
  unsaved-changes prompt on close and the tab's dirty marker keep looking at
  `isDirty` only: unsaved thumbnails are worth saving when the user chooses
  to, never worth a prompt. A capture that is not saved is simply made again
  next session.
- **Automatic capture is not an undo step**, just as camera moves are not.
- **Thumbnails are not undo-snapshot content.** The whole-network snapshots
  (*Current state*) are written without the `thumbnail` field: an AI editing
  session pushes many of them, and each would otherwise carry about 30 KB of
  base64 per side, and undoing an unrelated text edit could put back an old
  image or flip `user_set`. The rule for a restore:
  - a command that **replaces a network that exists** (text edit, inline,
    convert-to-closure, factor selection, zone-body edits, …) keeps the live
    network's current `thumbnail` on the restored network;
  - a command that **brings back a network that does not exist** at restore
    time (undo of delete network or delete namespace, redo of duplicate)
    snapshots *with* the thumbnail, so the image comes back with the network.

  The serializer takes a flag for this (`include_thumbnail`). File save,
  `duplicate_node_network` and the snapshots of the delete-network,
  delete-namespace and duplicate-network commands pass `true`; every other
  undo snapshot passes `false`. The only command
  that changes a thumbnail is `SetNetworkThumbnailCommand` (D6). The
  "push only if changed" comparisons that some commands make between their
  before and after snapshots are unaffected, since neither side carries the
  image.
- **Linked (library) networks are never captured** and the D6 commands are
  disabled on them: their content is read-only and never saved by this file.
  They show the thumbnail their library file was saved with.
- **CLI:** `cli_runner` and the text-format tools neither read nor write
  thumbnails. A network replaced through `edit --replace` is just an edited
  network: its thumbnail updates the next time the GUI captures it. The text
  format round-trip corpus is unaffected because thumbnails are not part of the
  text format.

### D9 — UI

**Network list** (`node_network_list_view.dart`): the row's `leading` icon is
replaced by a 40×40 thumbnail with rounded corners, falling back to the
existing icon when the network has none.

**Network tree** (`node_network_tree_view.dart`): rows are denser, so the
network icon is replaced by a 24×24 thumbnail, again falling back to the icon.

**Hover preview:** in both views, hovering a thumbnail for a short delay shows
it at full 128×128 in a tooltip-style popup.

**Canvas:** custom nodes (detected with `isCustomNodeType`) show the same
128×128 popup when the title bar is hovered. The node itself does not change
size, so `node_layout.rs` is untouched. An inline image in the node header is
a possible later step (*Future work*).

**Context menu** (list and tree): *Set current view as thumbnail*, and *Reset
to automatic thumbnail* when the thumbnail is user-set (D6).

**Data flow:** `APINetworkWithValidationErrors` gains `has_thumbnail: bool` and
`thumbnail_revision: u64`. A sync API call,
`get_network_thumbnail_png(network_name) -> Option<Vec<u8>>`, returns the
bytes. Because revisions are process-wide and never reused (D7), Flutter's
cache is keyed by **revision alone** and holds a `MemoryImage` per revision:
a row looks up its network's current revision and fetches bytes only on a
miss. A rename keeps its revision and its cached image, which is correct
because the name is not part of the key; captures, undo restores and
File > Open bring new revisions, so they always fetch. Entries whose revision
no longer appears in the active document's network list are dropped when the
list is refreshed, so after a tab switch the other document's images are
fetched again, which is cheap. After a capture, the API caller already
refreshes the network list, which delivers the new revision. Pending
thumbnails also reach Flutter as `hasUnsavedThumbnails` next to `isDirty`
(D8).

**Reference guide:** the network-list section of
`doc/reference_guide/ui.md` (or wherever the network panel is documented)
describes the thumbnails, when they update and the two menu items.

## Open questions

1. **Default framing:** this document fits to the content (D4). The
   alternative is the user's exact last view. Trade-off: fitting gives a
   better picture but rewrites the image on most edits, because any change to
   the bounds rescales it (D3); the last view leaves the image alone through
   edits outside the view, but rewrites it whenever the camera moves, and
   often shows only a zoomed-in detail.
2. **Stored size:** this document uses 128 px (D7). 256 px would look sharper
   in a large hover preview but costs about four times the file size.
3. **Canvas:** hover preview only (D9), or also an inline image in custom
   node headers?
4. **Hiding thumbnails:** should the network panel have a *Show thumbnails*
   toggle for people who prefer the compact list?
5. **Selection in the picture** (found in Phase 1): selection highlighting
   *is* part of the content meshes — the atomic tessellator colours selected
   atoms and bonds (`atomic_tessellator.rs`, `is_selected` /
   `is_bond_selected`). D5 asks to suppress it the way the image export does,
   but a capture draws the meshes already on the GPU, built with the
   selection, and by the time a switch triggers the capture the CPU scene is
   already the incoming network's. Suppressing it would mean tessellating the
   outgoing scene a second time without selection before the switch — the
   cost D2 set out to avoid. Phase 1 therefore shows the selection; D3's
   tolerance hides a few selected atoms but not a large selection.

## Phases

### Phase 1 — Rust: render, capture, store

- Renderer: factor `render` over a render target; thumbnail target, camera
  buffer and pass flags; `content_bounds` and `content_owner`;
  `fit_camera_to_bounds`; `render_thumbnail`; PNG encode/decode and the D3
  comparison.
- Structure designer: `NodeNetwork.thumbnail` and `thumbnail_revision` (the
  process-wide counter); serialization with the `include_thumbnail` flag (plus
  the `base64` workspace dependency) and the snapshot rule of D8;
  `has_unsaved_thumbnails`; `SetNetworkThumbnailCommand`.
- API: `capture_gpu_content_thumbnail` called from
  `refresh_structure_designer` when `content_owner` changes and before saves,
  resolving parked documents through the document set;
  `set_current_view_as_thumbnail`, `reset_network_thumbnail`,
  `get_network_thumbnail_png`; the two new fields on
  `APINetworkWithValidationErrors` and `has_unsaved_thumbnails`; FRB codegen.
- Tests (in each crate's `tests/`): serialization round-trip with and without
  a thumbnail, old files load; duplicate carries the thumbnail; the fit math
  (perspective and orthographic); the comparison (identical, slightly
  different, really different); capture leaves the dirty flag alone but sets
  `has_unsaved_thumbnails`, and save clears it; a capture fires for each
  switch path (list, back/forward, add, duplicate, tab switch) and lands on
  the outgoing network, including in a parked document; no capture when the
  outgoing network was deleted or renamed, or its scene is empty; a user-set
  thumbnail is never
  replaced by automatic capture; the set/reset commands undo and redo;
  undoing a text edit keeps the current thumbnail, undoing a delete brings
  the image back; revisions never repeat across an undo restore or a reload;
  linked networks are never captured.

**Phase 1 implementation notes** (where the code differs from, or adds to,
the text above):

- `APINetworkWithValidationErrors` also carries `thumbnail_user_set`, which
  the context menu needs to decide whether to offer *Reset* (D9).
- *Set current view as thumbnail* is refused when nothing is displayed, for
  the reason D3 gives for automatic capture.
- An undo restore that keeps the live thumbnail keeps its revision too: the
  image is unchanged, so the cache entry stays valid, and D7's guarantee —
  two different images never share a revision — still holds.
- The GPU-free half of the hand-over rule (`thumbnail_ops::ContentOwner`,
  `outgoing_content_owner`, `DocumentSet::designer_mut`) lives in the
  structure-designer crate so the switch paths are tested without a GPU; the
  empty-scene skip is in the API layer and is covered by the manual
  walkthrough only.

### Phase 2 — Flutter: list and tree

- Thumbnails in list and tree rows, the image cache, the hover preview, and the
  two context-menu items.
- Reference-guide update.
- Flutter `canSave` also honours `hasUnsavedThumbnails` (D8).
- Manual walkthrough: open an old file and browse its networks (thumbnails
  appear, no dirty marker, no prompt on close, but Save is enabled); save and
  reopen (thumbnails survive); edit, leave, check the image updates; edit a
  custom node and check that a network using it updates when it is next
  visited; set and reset a user thumbnail and undo both; switch tabs and back
  (the outgoing network's image updates, the list shows the right images);
  open the saved file on a second machine and browse the networks that have
  thumbnails: no image changes and Save stays disabled.

**Phase 2 implementation notes:**

- The cache and the thumbnail widget are `lib/structure_designer/network_thumbnails.dart`
  (`NetworkThumbnailCache`, owned by the model as `networkThumbnails`, and
  `NetworkThumbnail`); the menu items are
  `node_networks_list/network_thumbnail_menu.dart`. The hover preview is a
  `Tooltip` with a `richMessage` image.
- *Set current view as thumbnail* is shown on every local network row but
  enabled only on the active one; a refusal (nothing displayed) is an error
  snackbar.
- Ctrl+S's "No changes to save" check uses the same rule as `canSave`
  (`hasSomethingToSave`).
- In the list view every row's leading slot is 40 px wide, so names stay
  aligned whether a row has a thumbnail or the fallback icon; rows with a
  thumbnail are taller.

### Phase 3 — Canvas

- Hover preview on custom nodes.
- Manual walkthrough.

**Phase 3 implementation notes:**

- The preview lives in the title bar's **existing** tooltip rather than a
  second one: for a custom node whose network has a thumbnail, the tooltip
  becomes the usual header text with the 128×128 image under it
  (`NetworkThumbnailPreview`, shared with the list and tree). Two tooltips on
  one title would compete for the same hover.
- The image is an **input** of `NodeWidget` (`thumbnail`), threaded through
  `canvasNodeWidget` like `titleMode`, so the widget stays a pure function of
  its inputs. The live canvas looks it up with
  `model.networkThumbnailImage(node.nodeTypeName)` — only networks are in the
  list, so built-in nodes get null without an `isCustomNodeType` call — and the
  image export passes none (a still image has no hover).
- The zoomed-out node levels, which show only the title, get the same tooltip
  on that title; there the picture helps most.
- Linked library networks preview too: they are in the network list with the
  thumbnail their library was saved with.
- Manual walkthrough: hover a custom node's title at each zoom level (preview
  appears; built-in nodes and networks without a thumbnail show the plain
  tooltip); capture a new image for the network (leave and come back, or *Set
  current view as thumbnail*) and check the canvas preview follows; export the
  node network image and check nothing changed in it.

## Future work

- **Generate missing thumbnails:** a command that activates each network
  without a thumbnail in turn, with progress and Cancel, then returns to the
  original network.
- **Inline thumbnails in custom node headers:** needs the node height changed
  on both sides (`node_layout.rs` and the Flutter node widget).
- **Tool button icons** for user-defined networks (mechadense's third use).
- **A thumbnail for the whole file:** store the main network's image at the top
  level of the `.cnnd`, which a Windows shell thumbnail handler could later
  read.
