# Parts of the UI

← Back to [Reference Guide hub](../atomCAD_reference_guide.md)

This is how the full window looks like:
![](../atomCAD_images/full_window.png)

---

We will discuss the different parts of the UI in detail. The parts are:

- 3D Viewport
- Node Networks List Panel
- Node Network Editor Panel
- Node Properties Panel
- Display Preferences Panel
- Camera Control Panel
- Preferences Dialog (Edit > Preferences)

## 3D Viewport

The node network results are displayed here.

![](../atomCAD_images/3d_viewport.png)

You can navigate the viewport with the mouse or touchpad. Although it is possible to use atomCAD with a touchpad we **strongly recommend using it with a mouse**. You can choose between multiple control mechanisms depending on your preference and constraints. (For example some mice do not have a middle mouse button or a mouse wheel).

- **Pan (move camera):**
  - Option 1: **Middle mouse button drag**
  - Option 2: SHIFT right mouse button drag
  - Option 3: SHIFT *touch-pan*  (for Magic Mouse or touchpad)

- **Orbit:** **Right mouse button drag**
- **Zoom:** 
  - Option 1: **Mouse scroll wheel**
  - Option 2: Vertical component of *touch-pan* (for Magic Mouse and touchpad)
  - Option 3: Pinch zoom (for touchpad)


All three operations use a *pivot point*. The pivot is the point where you click when you start dragging: if you click an object, the pivot is the hit point on that object; otherwise the pivot is the point on the XY plane under the cursor. You can visualize the pivot as a small red cube in **Edit → Preferences** (`Display camera pivot point`). For example, orbiting rotates the camera around the pivot point, and zooming moves the camera toward (or away from) the pivot point.

Orbiting is constrained so the camera never rolls (no tilt). This prevents users from getting disoriented. If you need complete freedom, a 6-degree-of-freedom (6DoF) camera mode will be developed soon.

By default the axis kept vertical on screen while orbiting is the world **Z** axis. When you work on a crystal surface that is not aligned with Z — a (111) or (110) surface, for example — that surface never levels out on screen and orbiting around it feels awkward. You can pick a different **navigation up-axis** (typically the surface's plane normal) so the surface stays level and orbits naturally. Use the **Up:** button in the [Camera Control Panel](#camera-control-panel). Changing the axis is a view-only convenience: it does not move or re-orient your model, and the background grid keeps showing the true world/lattice orientation.

## Node network composability and Node Networks list panel

A structure design consists of node networks. The list of node networks in the current design is shown in the **Node Networks** panel. Select a network in the panel to open it in the node network editor. To create a new network, click the **Add Network** button.

![](../atomCAD_images/node_networks_list_panel.png)

Node networks in a design can be browsed in the **List** tab or in the **Tree** tab. Especially in larger designs or in reusable part libraries it is beneficial to organize your node networks in a namespace hierarchy. The hierarchy can be created by simply naming your node networks using the '.' character as a separator.

![](../atomCAD_images/node_networks_tree_panel.png)

> Terminology: a name like `dl.lib.basepoly.cube_centered` is the qualified name of the given node network, while the name `cube_centered` is the simple name of that same node network.

In the node network editor panel: Node titles show only the simple name, with the full qualified name available on hover.

### Navigating between node networks

When working with custom nodes (nodes defined by subnetworks), you can quickly navigate to their definitions:

- **Go to Definition:** Right-click a custom node and select *Go to Definition* to open the subnetwork that implements it.

The **Node Networks** panel includes browser-like navigation buttons at the top:

- **Back (←):** Returns to the previously viewed node network.
- **Forward (→):** Moves forward in the navigation history after going back.

These buttons are grayed out when navigation in that direction is unavailable.

Each node network stores its own camera settings (position, orientation, orthographic mode). When you switch between node networks, the camera automatically restores to the saved view for that network. Camera settings are saved as part of the `.cnnd` file.

## Node network editor panel

![](../atomCAD_images/node_network_editor_panel.png)

### Navigating in the node network editor panel

There will be a separate longer chapter in this document about node networks. Here we just discuss how to use the node network editor panel in general. If this UI chapter does not make sense yet to you, come back to it after reading the node networks chapter.

The node network editor canvas can be panned the following way:

- Option1: **Middle mouse button drag**
- Option 2: SHIFT right mouse button drag
- Option 3: SHIFT *touch-pan* (for Magic Mouse or touchpad)

If you get lost you can use the *View > Reset node network view* menu item.

The node network can be zoomed using the mouse scroll wheel.

### Manipulating nodes and wires

**Add nodes**
Right-click in the node editor to open the **Add Node** window and add a new node.

![](../atomCAD_images/add_node.png)

**Move nodes**
Left-click a node and drag to move it.

**Connect pins**
Left-click and drag from an output pin to an input pin to create a wire. To disconnect a wire, select it and press `Del` (see Selection below).

**Quick-add node from wire**
If you drag a wire from a pin and release it in empty space, the **Add Node** window opens filtered to show only nodes with compatible pins. After selecting a node type, the new node is created at the drop location and the wire is automatically connected. If multiple pins are compatible, a dialog lets you choose which one.

**Selection**
Multiple nodes and wires can be selected. Selection is used for:

- Deleting selected nodes or wires with the `Del` key.
- Editing the *active* node’s properties in the **Node Properties** panel.
- Enabling viewport interactions for the *active* node: many node types expose interactive *gadgets* in the viewport; the exact interactions depend on the node type (see the Nodes Reference section).

*Single selection:*
- Left-click a node or wire to select it (clears previous selection).

*Multi-selection:*
- **Ctrl+click** a node or wire to toggle it in/out of the current selection.
- **Shift+click** a node or wire to add it to the current selection.
- **Rectangle selection:** Left-click and drag on empty space to draw a selection rectangle. Any node or wire that overlaps the rectangle is selected. Modifier keys work with rectangle selection too: Ctrl toggles, Shift adds.

*Active vs selected:*
When multiple nodes are selected, the most recently selected/added node becomes the *active* node. The active node is shown with a different color and is the one whose properties appear in the Node Properties panel and whose gadget is shown in the viewport.

*Moving multiple nodes:*
When you drag any selected node, all selected nodes move together.

**Visibility vs selection**
Selecting a node does *not* make its output visible. Visibility is controlled independently by an **eye icon next to each output pin**: a node with one output pin has one eye icon; a multi-output node such as `atom_edit` has one eye icon per pin, each toggling that pin's display in the 3D viewport independently. The **Geometry Visualization** preferences panel also contains node display policies that may automatically change node visibility when selections change (see **Geometry Visualization** preferences). Display policies operate at node level — they make pin 0 visible; additional pins of a multi-output node are only displayed via explicit toggle. Nodes inside a higher-order function's or closure's body region have no eye icon — except inside a **parameter-less closure**, whose body nodes are viewable (see [Viewing the contents of a parameter-less closure](./node_networks.md#viewing-the-contents-of-a-parameter-less-closure)); display policies never touch body nodes either way.

**Copy, cut, paste, and duplicate**
Selected nodes can be copied, cut, and pasted:
- `Ctrl+C` to copy, `Ctrl+X` to cut, `Ctrl+V` to paste (also available via right-click context menu).
- `Ctrl+D` to duplicate selected nodes in place.
- Internal wires between copied nodes are preserved; external connections (wires to nodes outside the selection) are dropped.
- Pasted nodes are placed at the mouse cursor position.
- You can copy nodes in one network and paste into a different network.

**Factor selection into subnetwork**
You can convert a group of selected nodes into a reusable custom node type:
1. Select one or more connected nodes.
2. Right-click and choose **"Factor into Subnetwork..."**.
3. A dialog opens where you can set the subnetwork name and edit parameter names.
4. On confirmation, the selected nodes are moved into a new subnetwork and replaced with a single custom node instance.

The selection must be a "single-output subset" — at most one wire may exit the selection to nodes outside it. Parameter nodes cannot be included in the selection.

**Inline a custom node**
The inverse of factoring: you can replace a custom node instance with a copy of its subnetwork's contents, spliced into the current network in place.
1. Right-click a custom node instance and choose **"Inline"**. (The item appears only for custom nodes — built-in nodes, including the higher-order-function nodes, `closure`, and `apply`, cannot be inlined.)
2. The single instance is removed and the subnetwork's nodes are copied in where it stood. Each input pin's incoming wire is reconnected to whatever inside the subnetwork consumed the matching `parameter`, and consumers of the instance's output are reconnected to the subnetwork's return node. Surrounding nodes are pushed right and down to make room for the (usually larger) inlined content.

The named subnetwork definition in the user-types panel is **left untouched** — only this one instance is expanded, and any other instances of the same custom node keep working. Inlining works in any scope, including inside a higher-order-function body, and is undoable (`Ctrl+Z`).

**Convert between a closure and a named network**
A [`closure`](./nodes/math_programming.md#closure) and a custom node instance used as a [function value](./nodes/math_programming.md#function-values-and-closures) are two representations of the same thing — a function with some captured values — so you can convert freely between them. Both operations are exact inverses of each other, work in any scope (including inside a higher-order-function body), and are undoable (`Ctrl+Z`).

- **Convert to Closure** — right-click a custom node instance and choose **"Convert to Closure"**. The instance is replaced by a `closure` node whose inline body is a copy of the subnetwork's contents. The instance's **wired** input pins become **captures** in the body (the capture wires are reconnected to the same sources); its **unwired** input pins become the closure's **parameters**. The named subnetwork definition is left untouched. The item appears only for a custom node instance that is *used as a function* — i.e. nothing consumes its normal output, only its [function pin](./node_networks.md#anatomy-of-a-node) (or it is unconsumed) — and whose subnetwork has a return node. Use it when you want to edit one function's body in place, or reshape an instance into a reusable `closure`.

- **Extract to Network…** — the inverse: right-click a `closure` node and choose **"Extract to Network..."**. A dialog asks for a name; on confirmation a new custom node type is created from the closure's body and the `closure` is replaced by an instance of it, used through its function pin. The closure's **parameters** and its **captures** both become `parameter` nodes of the new network; the instance's capture pins are wired to the original capture sources. Use it to promote a one-off closure body into a named, reusable subnetwork that appears in the user-types panel.

**Click-to-activate from viewport**
When multiple nodes have their output visible in the 3D viewport, you can click on a rendered output to activate the node that produced it. The first click activates the node and scrolls the node network panel to reveal it; subsequent clicks on the same node’s output perform the normal action (e.g., atom selection). If outputs from multiple nodes overlap at the click position, a disambiguation popup appears letting you choose which node to activate. The active node’s geometry is rendered with a distinct color to help distinguish it from other visible outputs.

Clicks on the active node's interactive gadget (e.g. the XYZ translation gizmo's arrows) always go to the gadget — they are never treated as click-to-activate, even when another node's rendered output lies behind the gizmo. Gizmo handles also have a minimum grab size in screen pixels, so they stay easy to hit when zoomed out.

### Execute action (side-effect nodes)

Some nodes exist to *do something* rather than to produce a value: `export_atoms` writes an atomic-structure file (`.xyz` or `.mol`) to disk; `foreach` runs a body once per element of an upstream stream; future effect nodes will follow the same pattern. These nodes return the [`Unit` data type](./node_networks.md#data-types) (a value that carries no information) so the node graph can wire them around without misrepresenting them as data sources.

Effect nodes only fire when the user **explicitly invokes them**. To run an effect node, **right-click the node and choose Execute** from the context menu. This is the *only* way an effect node fires — display passes (the implicit re-evaluations triggered by editing parameters, moving nodes, panning the camera, etc.) skip over Unit-returning nodes entirely, even when the node is visible. That eliminates a whole class of footguns where editing an unrelated parameter would silently overwrite an exported file.

The Execute action is **one-shot**: invoking it runs a single evaluation pass with the side-effect flag set, then resets. There is no "armed" state — to re-fire, right-click and choose Execute again. The targeted node is evaluated independently of display state: whether the node is visible or not, and whether anything downstream is displayed, the action evaluates the node and its transitive inputs fresh.

While an Execute pass is running, a small modal **"Executing…"** dialog appears so you know the app is working and not frozen. (The Rust evaluator runs synchronously on the UI thread, so the dialog does not animate while the pass is in flight; it disappears as soon as the pass completes.) On success, a status snackbar confirms completion; on error, a snackbar surfaces the message and the targeted node lights up red in the graph.

The most common pattern is a `product → foreach( variant → export_atoms(...) )` pipeline: edit the product axes freely (no files written), then right-click the `foreach` node and choose Execute to write one file per variant. See the [`foreach`](./nodes/math_programming.md#foreach) and [`export_atoms`](./nodes/atomic.md#export_atoms) reference entries for full pipeline examples.

## Console panel

The **Console panel** is a docked, collapsible bottom strip that displays entries pushed by the [`print` node](./nodes/math_programming.md#print) — a debug-observation surface for the node graph. Hidden by default; toggle with **Ctrl+`** (backtick — same as VS Code / Chrome dev tools), via the *View > Show/Hide Console* menu entry, or by clicking the close `×` icon on the panel header.

When visible, the panel shows a chronological list of entries, newest at the bottom. Each row reads:

```
[HH:MM:SS]  [▶]  network_name / node_label    text
```

The `▶` marker appears only on entries produced by Execute passes, so you can tell at a glance which prints came from an explicit run versus a normal display refresh. Entries accumulate across passes; closing and reopening the panel does not lose them, but closing the application does (the log is in-memory only).

Header controls:

- **Autoscroll toggle** — when on (default), the view scrolls to the latest entry as new ones arrive; when off, the scrollbar stays where you parked it so a long log won't yank away while you are reading older entries.
- **Clear** (trash icon) — empties the buffer.
- **Close** (×) — collapses the panel; equivalent to *View > Hide Console* or **Ctrl+`**.

A small dot on the *View > Show Console* menu item (and on the toolbar toggle, when the panel is closed) signals that new entries arrived since the panel was last open. Opening the panel clears the dot.

See the [`print` node reference](./nodes/math_programming.md#print) for how to feed the Console panel from a node network.

## Node Properties Panel

The properties of the active node can be edited here.

![](../atomCAD_images/cuboid_properties_panel.png)

This is different for each node, we will discuss this in depth at the specific nodes. There are some general features though:

- When dragging the mouse on integer number editor fields the number can be
incremented or decremented using the mouse wheel. Shift + mouse wheel works in 10 increments.
- Selecting a **custom node** (a node defined by a subnetwork) shows an auto-generated panel with one editable field per simple-typed parameter pin — see [Editing custom node parameters](./node_networks.md#editing-custom-node-parameters).

In case no node is selected the description of the active node network can be edited in the node properties panel:

![](../atomCAD_images/network_description.png)

This description will be displayed beside the custom node in the *Add Node* window. 

## Display Preferences Panel

This panel contains common settings for how geometry and atomic structures are visualized.

![](../atomCAD_images/display_preferences_panel.png)

### Geometry Visualization mode

Choose how geometry node outputs are rendered:

- **Surface Splatting** — The surface is represented by many small discs sampled from the object’s Signed Distance Field (SDF). This mode renders true implicit geometry (no polygonal mesh is produced).
- **Wireframe (Explicit Mesh)** — The geometry is evaluated to a polygonal mesh and displayed as a wireframe (edges only). Use this mode when you need to inspect mesh topology or see precise polygon edges.
- **Solid (Explicit Mesh)** — The geometry is evaluated to a polygonal mesh and rendered as a solid. This is the default mode.

In **Surface Splatting** and **Solid** modes the outer surface is shown in green and the inner surface in red (inner = surface facing inward).

A separate **Show geometry shell on Crystal and Molecule** toggle (next to the three rendering-mode buttons) controls whether the geometry shell carried by a Crystal or Molecule is rendered alongside its atoms. Crystals always have a shell (it is the cookie-cutter geometry that produced them); Molecules can also carry a shell when they were built from a Blueprint via `exit_structure`. Turn the toggle off when the shell would obscure the atoms; turn it on to see how the shell aligns with the atomic structure. The toggle persists in preferences.

### Node display policy

Choose how node output visibility is managed:

- **Manual (User Selection)** — Visibility is controlled entirely by the eye icons on each output pin; selection changes do not affect visibility.
- **Prefer Selected Nodes** *(default)* — Visibility is resolved per *node island* (a node island is a connected component of the network):
  - If an island contains the currently selected node, that selected node's output is made visible.
  - If there is no selected node in the island, the output of the island’s frontier nodes are made visible.
- **Prefer Frontier Nodes** — In every island, the output of the frontier nodes are made visible. Frontier nodes are nodes whose output is not connected to any other node’s input — i.e., they represent the current “results” or outputs of that island.

Even when a non-Manual policy is active, you can still toggle a pin's visibility manually using its eye icon; that manual visibility will persist until the selection or policy changes it.

### Atomic visualization

- Ball and stick: atoms are represented with small balls (their radius is half the covalent radius) and bonds are represented as sticks.
- Space-filling: atoms are represented as big balls: their radius is exactly the van der Waals radius (we use data published by Santiago Alvarez in 2014)
- **Scene transparency** (opacity icon): a toggle that ghosts the whole scene so you can see internal features through their surroundings — a quick, non-destructive alternative to placing [`xray`](nodes/atomic.md#xray) nodes. It flips the *Make whole scene transparent* preference on or off; the alpha it uses (default 0.5) is set in **Edit → Preferences → Atomic Structure Visualization**. Impostor rendering only. It multiplies with any per-region `xray` transparency, so `xray`-ghosted atoms stay more transparent than the rest of the scene.

## Camera Control Panel

Contains common settings for the camera.

![](../atomCAD_images/camera_control_panel.png)

- **View dropdown:** Snaps the camera to a canonical orientation — Top, Bottom, Front, Back, Left, or Right — or shows *Custom* when the current orientation is none of these. The canonical views follow the navigation up-axis: with a (111) up-axis, *Top* faces the (111) surface.
- **Perspective / Orthographic buttons:** Switch between perspective and orthographic projection. The active mode is highlighted.
- **Up: ⟨axis⟩ button:** Sets the navigation up-axis — the axis kept vertical on screen while orbiting (see [3D Viewport](#3d-viewport)). The label shows the current axis (`Z` by default); a non-default axis is highlighted so a tilted turntable is never a mystery. Clicking it opens the **Navigation Up Axis** dialog:
  - Choose **Plane (hkl)** to use a crystal plane's normal, or **Direction [uvw]** to use a lattice direction. (These differ on non-cubic lattices — the plane normal is not the lattice direction of the same index.) Enter the index with the map or the numeric fields. The dialog shows which lattice the index is interpreted in (the active node's lattice, or a cubic-diamond fallback).
  - **From displayed plane** is a one-click shortcut: if the active node produces or is drawn on a construction plane (a `drawing_plane`, or a 2D shape such as `rect`/`circle`), it takes that plane's normal directly.
  - **Apply** sets the axis, **Reset (Z)** returns to the world-Z default, and **Close** dismisses the dialog. When you apply a new axis the image rolls until the new axis reads as vertical — that is the expected confirmation, not a glitch.

  The chosen axis is stored per node network (like the rest of the camera settings) and saved in the `.cnnd` file. A freshly created network starts from the default Z axis.

## Menu Bar

Used for loading and saving a design, exporting a design to .xyz or .mol, undo/redo, and for opening the preferences panel.

![](../atomCAD_images/menu_bar.png)

- *File > New*: Creates a new blank design.
- *File > Load Design*, *File > Save Design*, *File > Save Design As*: The native file format of an atomCAD design is the .cnnd file format. CNND stands for Crystal Node Network Design. It is a json based format. It contains a list of node networks. Can be used as a design file or as a design library file intended for reusing node networks from it as custom nodes in other designs.
- *File > Export visible*: You can export visible atomic structures into `.xyz` or `.mol` format. `.mol` is a better choice because in this case bonds are saved too. `.xyz` do not support bond information so when saving into `.xyz` bond information is lost. In case of `.mol` the newer `V3000` flavor is used instead of the old `V2000` flavor because `V3000` supports more than 999 atoms.
- *Edit > Undo* (`Ctrl+Z`) / *Edit > Redo* (`Ctrl+Shift+Z` or `Ctrl+Y`): Undo and redo all operations, including node edits, wire connections, atom editing, and more.
- *Edit > Validate active network*: Validates the active node network and reports any errors. Available in Node Network Mode only.
- *Edit > Auto-Layout Network*: Automatically arranges nodes in the current node network using the Sugiyama layout algorithm for a clean, readable layout.
- *View > Switch to Horizontal Layout* / *View > Switch to Vertical Layout*: Changes the orientation of the node network editor panel.
- *View > Show/Hide Console* (**Ctrl + backtick**): Toggles the [Console panel](#console-panel) docked at the bottom of the window.

## Preferences Dialog

The *Edit > Preferences* menu item opens the Preferences dialog, which contains advanced settings organized into categories. All preferences are persisted across sessions.

### Geometry Visualization

| Setting | Description |
|---------|-------------|
| Visualization method | *Surface Splatting*, *Solid*, or *Wireframe*. Controls how geometry node outputs are rendered. |
| Samples Per Unit Cell | Resolution for surface splatting tessellation. Higher values produce smoother surfaces. |
| Sharpness Angle Threshold | Angle (in degrees) used to detect sharp edges during mesh generation. |
| Mesh Rendering | Normal calculation method: *Smooth* (interpolated normals), *Sharp* (flat shading), or *Smart (detect sharp edges)* (smooth within groups, sharp at edges). |
| Show geometry shell on Crystal and Molecule | When enabled, Crystal and Molecule outputs render their geometry shell together with the atoms. Disable to hide the shell when it would obscure the atomic structure. Mirrors the toggle in the Display Preferences panel. |

### Atomic Structure Visualization

| Setting | Description |
|---------|-------------|
| Visualization method | *Ball and Stick* or *Space Filling*. |
| Rendering Method | *Impostors* (high-performance) or *Triangle Mesh* (traditional geometry). |
| Ball & Stick Cull Depth | Distance (in Ångströms) beyond which atoms are hidden in Ball and Stick mode. Set to 0 to disable culling. |
| Space Filling Cull Depth | Distance (in Ångströms) beyond which atoms are hidden in Space Filling mode. Set to 0 to disable culling. |
| Make whole scene transparent | Global "see through everything" viewing lens: when enabled, **every** atom and bond renders semi-transparent at the alpha below, without any [`xray`](nodes/atomic.md#xray) nodes. Impostor rendering only (no effect in *Triangle Mesh* mode). It composes with `xray` by **multiplication** — an atom an `xray` node ghosted to α = 0.3 becomes 0.3 × the scene alpha, so ghosted regions stay more transparent than their surroundings. The same toggle is available as a one-click button in the [Display Preferences panel](#atomic-visualization). |
| Scene transparency alpha | The global alpha (0 = fully transparent, 1 = fully opaque) used when *Make whole scene transparent* is on. Default 0.5. Editable with the slider or the numeric field; the value is kept even while the toggle is off. |
| Atom label size (Å) | Height of the text drawn by an [`apply_style`](nodes/atomic.md#apply_style) rule's `label` field. Default 0.7 Å — roughly a ball-and-stick carbon's diameter. This is a **world-space** size, so labels scale with zoom like the atoms they name; range 0.05–10. Applies to every label in the scene (there is no per-rule size). |

### Other Settings

| Setting | Description |
|---------|-------------|
| Display camera pivot point | Shows or hides the camera pivot point as a small red cube. |

### Layout

| Setting | Description |
|---------|-------------|
| Auto-layout algorithm | *Topological Grid* or *Sugiyama*. Controls which algorithm is used for automatic node layout. |
| Auto-layout after AI edit operations | When enabled, the node network is automatically re-laid out after edits made via the CLI or AI assistant. |

### Background

| Setting | Description |
|---------|-------------|
| Background Color | The scene background color. |
| Show Axes | Toggles visibility of the Cartesian axes. |
| Show Lattice Axes | Toggles dotted lines showing non-Cartesian lattice directions (nested under Show Axes). |
| Show Grid | Toggles visibility of the Cartesian grid. |
| Grid Size | Spacing between grid lines. |
| Grid Color / Grid Strong Color | Colors for regular and primary (axis-aligned) grid lines. |
| Show Lattice Grid | Toggles a secondary grid aligned to the lattice (useful for non-cubic unit cells). |
| Lattice Grid Color / Lattice Grid Strong Color | Colors for the lattice grid lines. |
| Drawing Plane Grid Color / Drawing Plane Grid Strong Color | Colors for the 2D drawing plane grid. |

### Simulation

| Setting | Description |
|---------|-------------|
| Use vdW distance cutoff | Uses a 6 Å distance cutoff for van der Waals interactions during energy minimization. Faster on large structures with negligible accuracy loss. |
| Steps per frame | Number of continuous minimization iterations per animation frame (1–50). |
| Settle steps on release | Extra minimization steps run when a drag is released (0–500). |
| Max displacement per step | Maximum atom displacement per minimization step in Ångströms (default 0.1 Å). |

## Import from library .cnnd files

The *File > Import from .cnnd library* menu item allows you import selected node networks from a library .cnnd file.

A library .cnnd file is just a regular .cnnd file containing node networks created to be reused in other files.

![](../atomCAD_images/import_from_lib.png)

- It is possible to select any number of node networks to import from a library .cnnd file
- Always imports with transitive dependencies
- It is possible to select (preview) those dependencies
- You can specify a prefix which will be prepended to all the network names to avoid naming conflicts or to be able to load a parallel version of networks under a different 'namespace' to be able to compare them.
- From time to time you might want to import a new version of the node networks with the same new from a file with a new version. It is possible to overwrite node network with the same name when importing but a proper 'Overwrite warning' message is displayed.
