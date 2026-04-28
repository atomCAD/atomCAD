# Atomic structure nodes

← Back to [Reference Guide hub](../../atomCAD_reference_guide.md)

## import_xyz

Imports an atomic structure from an XYZ file. Outputs a `Molecule` — XYZ files carry no crystal-lattice information, so the result has no `Structure` association.

![](../../atomCAD_images/import_xyz.png)

It converts file paths to relative paths whenever possible (if the file is in the same directory as the node or in a subdirectory) so that when you copy your whole project to another location or machine the XYZ file references will remain valid.

## export_xyz

Exports atomic structure on its `molecule` input into an XYZ file.

![](../../atomCAD_images/export_xyz.png)

The XYZ file will be exported when the node is evaluated. You can re-export by making the node invisible and visible again.

This node will be most useful once we will support node network evaluation from the command line so that you will be able to create automated workflows ending in XYZ files. Just to export something manually you can use the *File > Export visible* menu item.

## atom_fill

Converts a `Blueprint` into a `Crystal` by carving atoms out of the infinite crystal field using the blueprint's geometry as a cookie cutter. The output retains the `Structure`, so further structure-aligned operations remain available downstream.

![](../../atomCAD_images/atom_fill_node.png)

![](../../atomCAD_images/atom_fill_props.png)

![](../../atomCAD_images/atom_fill_viewport.png)

The motif passed into the `motif` input pin is the motif used to fill the geometry. If no motif is passed in the cubic zincblende motif is used. (See also: `motif` node).

In the Parameter *Element Value Definition* text area you can specify (override) the values of the parameter elements defined in the motif. If for the default (cubic zincblende motif) you specify the following:

```
PRIMARY Si
SECONDARY C
```

The primary element changes from carbon to silicon:

![](../../atomCAD_images/silicon_carbide.png)



You can also change the fractional motif offset vector. (Each component should be between 0 and 1). This can be useful to finetune where the cuts should be made on the crystal to avoid unwanted features like the methyl groups.

If the geometry cut is done such a way that an atom has no bonds that is removed automatically. (Lone atom removal.)

You can switch on or off the following checkboxes:

- *Remove single-bond atoms:* If turned on, atoms which only have one bond after the cut are removed. This is done recursively until there is no such atom in the atomic structure.
- *Surface reconstruction:* Real crystalline surfaces are rarely ideal bulk terminations; instead, they typically undergo *surface reconstructions* that lower their surface energy. atomCAD will support several reconstruction types depending on the crystal structure. At present, reconstruction is implemented only for **cubic diamond** crystals (carbon and silicon) and only for the most important one: the **(100) 2×1 dimer reconstruction**.
  If reconstruction is enabled for any other crystal type, the setting has no effect.
  The (100) 2×1 reconstruction automatically removes single-bond (dangling) atoms even if the *Remove single-bond atoms* option is not enabled. Surface reconstruction can be used together with hydrogen passivation or on its own.
- *Invert phase*: Determines whether the phase of the dimer pattern should be inverted. 
- *Hydrogen passivation:* Hydrogen atoms are added to passivate dangling bonds created by the cut.

## atom_move

Translates an atomic structure by a vector in world space. Unlike `lattice_move` which operates in discrete lattice coordinates, `atom_move` works in continuous Cartesian coordinates where one unit is one angstrom.

![](../../atomCAD_images/atom_move.png)

**Properties**

- `Translation` — 3D vector specifying the translation in angstroms.

**Gadget controls**

Drag the gadget axes to adjust the translation vector interactively.

## atom_rot

Rotates an atomic structure around an axis in world space by a specified angle.

![](../../atomCAD_images/atom_rot.png)

**Properties**

- `Angle` — Rotation angle in radians.
- `Rotation Axis` — 3D vector defining the axis of rotation (will be normalized).
- `Pivot Point` — The point around which the rotation occurs, in angstroms.

**Gadget controls**

The gadget displays the pivot point and rotation axis. Drag the rotation axis to adjust the angle interactively.

## atom_union

Merges multiple atomic structures into one. The `structures` input accepts an array of atomic structures (array-typed input; you can connect multiple wires and they will be concatenated). All elements of the array must be the same concrete type — either all `Crystal` or all `Molecule` — and the output preserves that type. Mixed `Crystal` + `Molecule` arrays are a validation error; insert an explicit `exit_structure` node first if you want a `Molecule` result.

![](../../atomCAD_images/atom_union.png)

## atom_lmove

Translates an atomic structure by a discrete vector in **lattice space** (integer lattice coordinates). This is the atomic-structure counterpart of the `lattice_move` geometry node.

**Properties**

- `Translation` — 3D integer vector specifying the translation in lattice coordinates.

## atom_lrot

Rotates an atomic structure in **lattice space** using discrete symmetry rotations. This is the atomic-structure counterpart of the `lattice_rot` geometry node. Only rotations that are symmetries of the unit cell are allowed.

**Properties**

- `Rotation` — A valid lattice symmetry rotation.
- `Pivot` — 3D integer vector for the rotation pivot point.

## apply_diff

Applies an atomic diff structure to a base atomic structure. This node is used in advanced parametric workflows where defect patches are created separately (e.g. by taking the `diff` output pin of an `atom_edit` node) and then applied to different base structures or at different positions.

**Input pins**

- `base` — The base atomic structure (`Crystal` or `Molecule`).
- `diff` — The diff atomic structure to apply (`Crystal` or `Molecule`).

The output preserves the concrete type of `base` — a `Crystal` base produces a `Crystal` output, a `Molecule` base produces a `Molecule`.

The diff structure encodes additions, deletions, and modifications of atoms. The node uses position-based matching to apply the diff to the base structure.

## relax

Performs UFF (Universal Force Field) energy minimization on an atomic structure. Takes a `Crystal` or `Molecule` input and outputs the minimized structure, preserving the concrete input type.

This node is useful in node-network workflows where you want to relax a structure non-destructively as part of a parametric pipeline. For interactive minimization during atom editing, use the energy minimization feature built into the `atom_edit` node instead.

## add_hydrogen

Adds hydrogen atoms to satisfy valence requirements of undersaturated atoms. Takes a `Crystal` or `Molecule` input and outputs a hydrogen-passivated structure, preserving the concrete input type.

The algorithm detects hybridization (sp3, sp2, sp1) automatically and places hydrogen atoms at the correct bond lengths and angles. This is the node-network counterpart of the one-click hydrogen passivation in the `atom_edit` node.

## remove_hydrogen

Removes all hydrogen atoms from an atomic structure. Takes a `Crystal` or `Molecule` input and outputs the bare framework without hydrogens, preserving the concrete input type.

Useful in workflows like: `remove_hydrogen` → transform/edit → `add_hydrogen`, allowing you to work with the bare framework and re-passivate afterward.

## atom_cut

Cuts an atomic structure using cutter geometries. Unlike `atom_fill` which creates atoms from geometry, `atom_cut` removes atoms that lie outside the cutter shapes — effectively performing a Boolean intersection between an existing atomic structure and one or more 3D geometries.

**Input pins**

- `molecule` — The atomic structure to be cut (`Crystal` or `Molecule`). The output preserves the concrete input type.
- `cutters` — An array of `Blueprint` values defining the region to keep (array-typed input; you can connect multiple wires).

**Properties**

- `Cut SDF Value` — The SDF threshold for the cut boundary (default 0.0). Atoms with SDF values greater than this threshold are removed.
- `Unit Cell Size` — The unit cell size in Ångströms used to normalize atom positions when evaluating against the cutter geometry.

Bonds connected to removed atoms are automatically deleted.

## atom_edit

The `atom_edit` node provides the same atom editing tools described in the [Direct Editing Mode](../direct_editing.md#the-atom-editor) section above — all tools, keyboard shortcuts, hydrogen passivation, energy minimization, freeze, and measurements work identically. When an `atom_edit` node is selected in the node network, the atom editor appears in the Node Properties panel.

This section covers the additional aspects of `atom_edit` that are specific to node-network workflows.

![](../../atomCAD_images/atom_edit.png)

### How atom_edit stores edits

Internally, an `atom_edit` node stores a **diff** — an atomic structure that encodes additions, deletions, and modifications relative to the input (base) structure. When the node is evaluated, the diff is applied to the base to produce the output. This means the `atom_edit` node is non-destructive: the base structure flows in untouched, and the diff layer captures all your edits (added atoms, deleted atoms, moved atoms, element replacements). Multiple `atom_edit` nodes can be chained, each applying its own diff to the previous result.

### Output pins: result and diff

`atom_edit` is a **multi-output** node. It exposes two output pins:

- **`result`** (pin 0) — the applied result: the base structure with the diff applied. This is the primary output for normal editing workflows.
- **`diff`** (pin 1) — the raw diff structure (additions, deletions, modifications relative to the base). The diff is itself an atomic structure, so it can be repositioned (via movement nodes) and re-applied to different base structures using the `apply_diff` node.

Each pin has its own eye icon — display either or both in the 3D viewport. When both are displayed, atom selection and other tool interactions act on `result` (the lower-indexed displayed pin); the `diff` rendering is visual-only. Display only `diff` to interact directly with diff atoms (this replaces the legacy "Output diff" checkbox; old `.cnnd` files with `output_diff: true` are auto-migrated to display the `diff` pin instead).

The `result` pin preserves the concrete input type — Crystal in / Crystal out, Molecule in / Molecule out. The `diff` pin is always a `Molecule` (a raw diff has no inherent lattice identity).

In the text format, refer to a non-default output pin with `.pinname` after the source node, e.g. `apply_diff { base: input, diff: my_edit.diff }` to take the diff from `my_edit` rather than the default `result`. See the [Node Network Text Format](../../node_network_text_format.md) document for the full syntax.
