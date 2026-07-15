# Direct Editing Mode

← Back to [Reference Guide hub](../atomCAD_reference_guide.md)

atomCAD offers two modes of operation:

- **Direct Editing Mode** — A streamlined, beginner-friendly interface focused entirely on atomic structure editing.
- **Node Network Mode** — The full-featured parametric editor with node networks, described in the rest of this guide.

When you first launch atomCAD, you start in Direct Editing Mode. This mode hides node-network concepts entirely, presenting a simplified UI with just the 3D viewport, a camera control panel, a display settings panel, and the atom editor.

![](../atomCAD_images/direct_editing_mode.png)

## The Atom Editor

The atom editor is the central tool for building and modifying atomic structures in atomCAD. In Direct Editing Mode the atom editor occupies the left sidebar; in Node Network Mode the same editor appears in the Node Properties panel when an `atom_edit` node is selected. The tools and features described below work identically in both modes (with minor simplifications in Direct Editing Mode, such as the hidden `diff` output pin and other node-network-specific affordances).

The editor is based on **tools** — one tool can be active at a time. The active tool determines how you interact with the atomic structure in the viewport. You can switch tools using keyboard shortcuts: `F2` (Default tool), `F3` (Add atom tool), `F4` or `J` (Add bond tool), `F5` (Guideline tool).

### Default tool

![](../atomCAD_images/default_tool.png)

The Default tool is the primary editing tool in the atom editor. Most editing features are available only when this tool is active.

**Selection and editing:**
- **Select** atoms and bonds using the left mouse button. Simple click replaces the selection, Shift+click adds to the selection, and Ctrl+click toggles the selection of the clicked object. Rectangle (marquee) selection is also supported and selects both atoms and bonds: every atom whose projection lands inside the rectangle is selected, and every bond whose two endpoints both fall inside the rectangle is selected as well.
- **Delete selected** atoms and bonds (also available via the `Delete` or `Backspace` key).
- **Replace** all selected atoms with a specific element.
- **Quick element selection:** Type an element symbol (e.g., `C`, `N`, `O`, `Si`) on the keyboard to set the active element. The typed symbol is shown as a cursor overlay. This also works in the Add atom tool (for setting the element of the next atom to be placed).
- **Transform** (move and rotate) selected atoms by dragging. Frozen atoms cannot be dragged.
- **Bond info:** When one or more bonds are selected, the UI shows bond order information and lets you change the order. Use keyboard shortcuts `1`–`7` to set bond order: `1` single, `2` double, `3` triple, `4` quadruple, `5` aromatic, `6` dative, `7` metallic.

**Freeze atoms:**

Atoms can be marked as **frozen** to prevent them from being moved during dragging and energy minimization. Frozen atoms are displayed with an ice-blue rim highlight so they are easy to identify.

The Default tool provides four freeze-related buttons:
- **Freeze selected** — Marks all selected atoms as frozen.
- **Unfreeze selected** — Removes the frozen flag from all selected atoms.
- **Select frozen** — Replaces the current selection with all frozen atoms.
- **Clear frozen** — Removes the frozen flag from all atoms.

**Tag atoms:**

Atoms can carry named **tags** — inert, durable metadata that marks a group of atoms for downstream tools (the same tags the [`tag` node](nodes/atomic.md#tag) applies). The **Tags** section (a collapsible panel alongside *Energy Minimization* and *Add Hydrogen*, shown with the Default tool active) offers three actions on the current selection:

- **Tag selected…** — opens a dialog to type a tag name (or pick one of the structure's existing tags from the suggestion chips) and adds it to the selected atoms.
- **Untag selected…** — the same dialog; removes the entered/picked tag from the selected atoms.
- **Clear all tags on selection** — removes every tag from the selected atoms.

Tagging inside `atom_edit` records a normal, undoable edit and persists in the project file. Tags have **no visual effect** on their own — **hover an atom to see the tags it carries** (they appear on a `Tags:` line in the hover popup). A structure supports at most 32 distinct tag names; exceeding that reports an error and applies no change.

**Energy minimization:**

The Default tool integrates UFF (Universal Force Field) energy minimization:

- **Minimize unfrozen** (`Ctrl+M`): Runs energy minimization on all unfrozen atoms in the structure.
- **Minimize selected** (`Ctrl+Shift+M`): Runs energy minimization on only the selected atoms.
- **Minimize diff**: Runs energy minimization where only atoms you added or modified are allowed to move; the original base atoms stay fixed. This button is only enabled when the atom_edit node has pending diff changes.
- **Continuous minimization:** When enabled, the minimizer runs automatically after each editing action, helping the structure settle into favorable geometries as you build. The following parameters can be tuned in *Edit > Preferences* under the **Simulation** category:
  - *Steps per frame* — Number of minimization iterations per animation frame (1–50).
  - *Settle steps on release* — Extra minimization steps run when you release a drag (0–500), giving the structure time to relax after manipulation.
  - *Max displacement per step* — Maximum distance (in Ångströms) any atom can move in a single step (default 0.1 Å). Lower values produce more stable but slower convergence.

**Hydrogen passivation:**

The Default tool includes one-click hydrogen passivation and depassivation:

- **Add hydrogens** (`Ctrl+H`): Adds hydrogen atoms to all undersaturated atoms (or only selected atoms if any are selected). The algorithm auto-detects hybridization and places hydrogens at correct bond lengths and angles.
- **Remove hydrogens** (`Ctrl+Shift+H`): Removes hydrogen atoms from the structure (or only from selected atoms and their neighbors).

### Add atom tool

![](../atomCAD_images/add_atom_tool.png)

- **Free placement:** Click empty space to place an atom at the clicked position.
- **Guided placement:** Click an existing atom to enter guided placement mode. The system computes chemically valid candidate positions based on the atom's hybridization and displays them as interactive guide dots. Click a guide dot to place and bond the new atom in one action.
  - Supports sp3, sp2, and sp1 hybridization geometries.
  - A **Hybridization** dropdown (Auto / sp3 / sp2 / sp1) lets you override the auto-detected hybridization.
  - A **Bond Mode** toggle (Covalent / Dative) controls the saturation limit: Dative mode unlocks lone pair positions for coordinate bonding.
  - When an atom is placed near an existing atom, the atoms are merged automatically.
- **Add atom at position:** When you need an atom at exact coordinates rather than picked from the viewport, the Add Atom panel exposes a *Position (Å)* vec3 input and an *Add atom at position* button. Type the X/Y/Z values and click the button to drop one atom of the currently selected element at that location, using the chosen hybridization override. Useful for reproducible placement (e.g. lattice-aligned coordinates copied from another source).
- Press `Escape` or click empty space to cancel guided placement and return to idle.

### Add bond tool

![](../atomCAD_images/add_bond_tool.png)

- Add bonds by clicking two atoms in the viewport.
- **Bond order** can be configured. Common orders: single, double, triple. Specialized orders: quadruple, aromatic, dative, metallic.
- Use keyboard shortcuts `1`–`7` to select the bond order: `1` single, `2` double, `3` triple, `4` quadruple, `5` aromatic, `6` dative, `7` metallic.
- Clicking an existing bond cycles through the common orders (single → double → triple → single).

### Guideline tool

A **guideline** is a temporary line in 3D space that constrains atom placement to positions that are hard to hit by free clicking — for example the ad-atom site of a Si(111) √3×√3 R30° reconstruction, which sits equidistant from three surface atoms. The Guideline tool (`F5`) fully owns the viewport while active and walks through three states: **Define** the line, **Place** atoms on it, and **Move** an atom along it.

A guideline is **transient**: it is *not* saved to the project file and is *not* part of undo/redo (the atoms you place or move *are* undoable). It is a frozen snapshot — it does not move if the atoms it was derived from later move. The line is dropped when you switch to another tool, deselect the node, or press **Clear** / `Escape` (which returns you to Define so you can build another).

**Define — building the line.** Click 1–3 atoms in the viewport to pick the atoms that define the line (click an already-picked atom to drop it, or click empty space to start over). The panel's **Create** button is labeled by how many atoms you picked:

- **3 atoms → Equidistant line:** runs through the circumcenter of the triangle, perpendicular to it. Every point on the line is equidistant from all three atoms.
- **2 atoms → Center line:** passes through the midpoint, directed from the first to the second picked atom.
- **1 atom → Directional line:** originates at the atom; you enter the direction as a vector (a **Normalize** button rescales it to unit length).

Degenerate input (three near-collinear atoms, two coincident atoms, or a zero-length direction) is rejected with a notification and no line is created. After **Create**, the picked atoms no longer matter — the line is frozen.

**Place — adding atoms on the line.** Once the line exists, the panel shows a **position field** (the signed distance `t` in Å along the line from its origin) and an element selector. Drag the marker dot — or anywhere in empty space — to slide it along the line, then click **Place atom** to create a free atom (no bonds) of the selected element at that position. After placing, the tool automatically switches to **Move** with the new atom selected, so you can fine-tune it; to start another atom, click empty space (this returns to Place, with the marker where you left it).

**Move — adjusting an atom on the line.** Click an existing atom to pick it: it snaps onto the line and becomes the active point. Drag it to slide it along the line, or type an exact `t` in the position field. Click a different atom to pick that one instead, or click empty space to release it (returning to Place). Picking always snaps the atom onto the line — there is no off-line offset to manage; free 3D motion is the Default tool's job.

Placing atoms via a guideline never creates bonds — switch to the Add bond tool afterward if you need them. Viewport clicks never place an atom on their own (the **Place atom** button does), which keeps "click an atom to pick it" unambiguous.

### Measurements

When 2–4 atoms are selected the UI displays a measurement card. Measurements are available regardless of which tool is active.

- **2 atoms:** bond distance (in Ångströms)
- **3 atoms:** bond angle (in degrees)
- **4 atoms:** dihedral (torsion) angle (in degrees)

A **Modify** button on the measurement card opens a dialog where you can enter a precise target value. Atoms are moved along bond axes, rotated around vertices, or rotated around torsion axes to achieve the target value. A "move connected atoms" option (on by default) moves the fragment attached to the moving atom rather than just the single atom.

**Atom info on hover:** Hovering over an atom shows a tooltip with its element, position, and which node produced it. The tooltip also reports the atom's hybridization: when an explicit override is set the line reads e.g. *Hybridization: sp2 (override)*; when hybridization is left on auto the line shows the auto-inferred value in parentheses, e.g. *Hybridization: auto (sp3)*. For atoms whose displayed atomic number is a non-physical parameter element, an *Effective element: …* line shows the real element used for simulation, guided placement, and passivation (see the parameter element note in the [`materialize`](./nodes/atomic.md#materialize) section).

### Rim highlights

The atom editor uses colored rim highlights to convey atom state while preserving element colors:

- **Selected atoms** — magenta rim
- **Frozen atoms** — ice-blue rim
- **Delete markers** — red rim on neutral-colored sphere
- **Marked atoms** (for measurements) — yellow/blue rims

### Bond colors

Regular single, double, triple, and quadruple bonds are rendered in neutral grey. Specialized bond orders use distinguishing colors so they can be told apart at a glance: aromatic bonds are purple, dative bonds are teal, and metallic bonds are steel blue. Selected bonds keep their bond-order color but are tinted with the selection color (magenta), so the two channels do not collide.

### Import XYZ

In Direct Editing Mode, *File > Import XYZ* imports atoms from an XYZ file directly into the current structure. This is the quickest way to load an existing molecule and start editing.

## Other capabilities

- **Undo / Redo** (`Ctrl+Z` / `Ctrl+Shift+Z` or `Ctrl+Y`): All editing actions can be undone and redone.
- **Export** via *File > Export visible* to `.mol` or `.xyz` format.

## Switching between modes

- **Direct Editing → Node Network:** Use *View > Switch to Node Network Mode* or the mode radio buttons in the Display section. Always available.
- **Node Network → Direct Editing:** Use *View > Switch to Direct Editing Mode*. This requires that exactly one `atom_edit` node is displayed and currently selected. If the criteria are not met, the menu item is disabled with a tooltip explaining why.

Both modes use the same `.cnnd` file format — your work is preserved when switching between modes.

## File menu differences

The *File > Import from .cnnd library* menu item is available only in Node Network Mode (it is an advanced feature for importing node networks).
