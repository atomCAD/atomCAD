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

## import_cif

Imports a crystal structure from a CIF (Crystallographic Information File) file — the standard exchange format for crystallographic data, carrying unit-cell parameters, space-group symmetry, and fractional atom positions. Unlike `import_xyz`, a CIF file describes a periodic crystal, so this node reconstructs the full conventional unit cell and emits the lattice vectors and a fractional motif alongside the atomic structure.

![TODO(image): the `import_cif` node selected with its properties panel showing the file name, block name, and bond options](TODO)

**Input pins** (all optional; can also be set as properties)

- `file_name: String` — path to the CIF file. Like `import_xyz`, paths are converted to relative paths whenever possible so projects remain portable when copied to another machine.
- `block_name: String` — when a CIF file contains multiple data blocks, selects which one to import. Empty / unconnected uses the first block.
- `use_cif_bonds: Bool` — when `true` (default), bond information present in the CIF (`_geom_bond_*` records) is used directly.
- `infer_bonds: Bool` — when `true` (default), bonds are inferred from interatomic distances if the CIF carries no explicit bonds (or as a fallback when `use_cif_bonds` is off).
- `bond_tolerance: Float` — multiplier applied to covalent radii when inferring bonds (default `1.15`).

**Output pins**

- `unit_cell: LatticeVecs` — the conventional unit cell read from the CIF.
- `atoms: Molecule` — the expanded conventional unit cell as an atomic structure, in Cartesian coordinates.
- `motif: Motif` — the same atom set expressed as a fractional `Motif` so it can be fed directly into `atom_fill` (typically together with `unit_cell`).

**Typical pipelines**

- *Direct fill:* wire `motif` and `unit_cell` into an `atom_fill` (or `materialize`) node to use the imported crystal as a template for filling geometry.
- *Edit then fill:* wire `atoms` and `unit_cell` into a `motif_edit` node, edit interactively in 3D, then feed the edited motif into `atom_fill`.

## atom_fill

Converts a `Blueprint` into a `Crystal` by carving atoms out of the infinite crystal field using the blueprint's geometry as a cookie cutter. The output retains the `Structure`, so further structure-aligned operations remain available downstream.

![](../../atomCAD_images/atom_fill_node.png)

![](../../atomCAD_images/atom_fill_props.png)

![](../../atomCAD_images/atom_fill_viewport.png)

The motif passed into the `motif` input pin is the motif used to fill the geometry. If no motif is passed in the cubic zincblende motif is used. (See also: `motif` node).

### Parameter element overrides

Motifs declare *parameter elements* — placeholder slots like `PRIMARY` or `SECONDARY` that the `atom_fill` node substitutes with concrete elements when it materializes the crystal. The properties panel shows a *Parameter Element Overrides* table, populated automatically from the connected motif: one row per parameter, with the parameter's name on the left and an element dropdown on the right. Choose an element to override the parameter's default; leave a row at *Default (X)* to keep the motif's own default. For example, with the default cubic zincblende motif, switching `PRIMARY` from carbon to silicon yields the same silicon carbide as before — there is no longer a free-form text area to edit.

![](../../atomCAD_images/silicon_carbide.png)

When a motif is edited inside `motif_edit`, parameter atoms (which carry non-physical atomic numbers) **simulate as their default element** for the purpose of force-field minimization, guided placement, and hydrogen passivation — a `PRIMARY` atom whose default is carbon will be treated as carbon for bond-length and hybridization calculations. This keeps the motif geometry realistic during interactive editing even before any concrete substitutions are chosen in `atom_fill`. Hovering over such an atom in the viewport shows an extra *Effective element: …* line in the tooltip whenever the displayed atomic number differs from the simulated one.

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

## atom_composediff

Composes multiple atomic diffs into a single equivalent diff. Applying the composed diff to a base structure produces the same result as applying each input diff in sequence, but in one step.

**Input pins**

- `diffs: [HasAtoms]` — an array of diff structures to compose, in the order they would be applied. The array-typed input accepts multiple wires which are concatenated. All elements must be the same concrete variant (all `Crystal` or all `Molecule`); the output preserves that variant. Each element must itself be a diff (typically the `diff` output pin of an `atom_edit` or `motif_edit` node) — passing a non-diff atomic structure is an error.
- `tolerance: Float` (optional) — positional matching tolerance used when composing (default `0.1` Å). Can also be set as a property.

**Behavior**

The composition uses position-based matching to merge the diffs: a `diff_1` modification followed by a `diff_2` modification of the same atom collapses into one entry, an addition in `diff_1` cancelled by a deletion in `diff_2` drops out entirely, and so on. The resulting diff carries anchors back to the original base atoms, so it can be re-applied to the same base (via `apply_diff`) for the same final result, or shared across different bases that share enough atom positions.

A typical use is collapsing a long edit history into a single distributable patch: chain several `atom_edit` nodes, take their `diff` outputs, feed them through `atom_composediff`, and the result is one `Molecule` value that encodes the entire edit sequence.

## relax

Performs UFF (Universal Force Field) energy minimization on an atomic structure. Takes a `Crystal` or `Molecule` input and outputs the minimized structure, preserving the concrete input type.

This node is useful in node-network workflows where you want to relax a structure non-destructively as part of a parametric pipeline. For interactive minimization during atom editing, use the energy minimization feature built into the `atom_edit` node instead.

## add_hydrogen

Adds hydrogen atoms to satisfy valence requirements of undersaturated atoms. Takes a `Crystal` or `Molecule` input and outputs a hydrogen-passivated structure, preserving the concrete input type.

The algorithm detects hybridization (sp3, sp2, sp1) automatically and places hydrogen atoms at the correct bond lengths and angles. This is the node-network counterpart of the one-click hydrogen passivation in the `atom_edit` node.

## remove_hydrogen

Removes all hydrogen atoms from an atomic structure. Takes a `Crystal` or `Molecule` input and outputs the bare framework without hydrogens, preserving the concrete input type.

Useful in workflows like: `remove_hydrogen` → transform/edit → `add_hydrogen`, allowing you to work with the bare framework and re-passivate afterward.

## infer_bonds

Recomputes bonds in an atomic structure based on interatomic distances and covalent radii. Takes a `Crystal` or `Molecule` and outputs the same structure with a refreshed bond list, preserving the concrete input type. Useful after importing files that lack bond information (e.g. some XYZ sources) or after operations that move atoms enough to invalidate the existing bonds.

**Input pins**

- `molecule: HasAtoms` — the input structure.
- `additive: Bool` (optional) — when `false` (default), the existing bonds are discarded and rebuilt from scratch. When `true`, existing bonds are preserved and only inferred bonds that are not already present are added.
- `bond_tolerance: Float` (optional) — multiplier applied to the sum of covalent radii when deciding whether two atoms should be bonded (default `1.15`).

**Properties**

The same `additive` and `bond_tolerance` values are also available as node properties for cases where you want a fixed setting without an extra wire.

## atom_replace

Substitutes atoms of one element for another (or removes them) in bulk, according to a list of replacement rules. The output preserves the concrete input type — a `Crystal` in produces a `Crystal` out, a `Molecule` in produces a `Molecule` out.

![TODO(image): the `atom_replace` node selected with its properties panel showing two replacement rows (e.g. C→Si and H→Delete)](TODO)

**Input pins**

- `molecule` — the atomic structure to transform (`Crystal` or `Molecule`).

**Properties**

The replacement rules live as node properties, not wired inputs. The property panel shows a list of rows, each with `[source element] → [target element]` and a delete button, plus an *Add Replacement* button at the bottom.

The **target dropdown** has an extra entry — *Delete* — at the top of the list. Choosing *Delete* removes every atom of the source element from the structure (and cleans up their bonds) instead of substituting them.

**Behavior**

- Each rule maps a source element to a target element (or to *Delete*).
- Atoms whose element is not listed in any rule pass through unchanged.
- Rules apply independently — each atom is matched against the rule list once.
- If multiple rules name the same source element, the last rule wins.
- Bond connectivity is preserved when an element is substituted; bonds attached to deleted atoms are removed.

The node subtitle summarizes the active rules (e.g. `C→Si, O→S`, or `H→(del)` for a deletion rule), with a `… (+N more)` suffix when the list is longer than three entries.

**Text format**

The rule list serializes as an array of `(from_atomic_number, to_atomic_number)` pairs, with `0` representing *Delete*:

```
replace1 = atom_replace {
    replacements: [(6, 14), (8, 16)]
}
```

This replaces C→Si and O→S.

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

### Tolerance

`atom_edit` matches diff entries to base atoms by position. The match radius is controlled by a single `tolerance` value (in Ångströms), available both as a node property and as the optional `tolerance` input pin. A wired pin overrides the property; when the pin is unconnected, the property value is used. The current value is shown in the node subtitle as `tol=…` whenever the pin is not connected.

Lower values make matching more strict (good when atoms are densely packed); higher values let the diff still apply after the base structure has been deformed slightly. The default works for typical atom-scale geometry; reach for the property when re-applying a saved diff to a relaxed or otherwise perturbed base.

## motif_edit

A visual, interactive motif editor — the spatial counterpart of the textual `motif` node. Place atoms in 3D, see neighboring cells, draw cross-cell bonds, and the result is converted to a `Motif` (with fractional coordinates) at the output. Internally `motif_edit` uses the same diff-based architecture as `atom_edit`: all atom-editor tools, keyboard shortcuts, hydrogen passivation, energy minimization, freeze, and measurements work identically.

![TODO(image): the `motif_edit` node selected with the viewport showing the unit-cell wireframe, primary-cell atoms, faded ghost atoms in neighboring cells, and a cross-cell bond](TODO)

**Input pins**

- `molecule: HasAtoms` (optional) — base atomic structure used as the starting point. Often the `atoms` output of an `import_cif` node, an existing `Crystal` you want to convert into a motif, or unconnected to start from an empty motif.
- `unit_cell: LatticeVecs` (optional) — basis vectors used to convert between Cartesian editing space and fractional motif coordinates. Defaults to cubic diamond when unconnected.
- `tolerance: Float` (optional) — positional matching tolerance for the diff (default same as `atom_edit`).

**Output pins**

- `result: Motif` (pin 0) — the constructed motif in fractional coordinates, ready to feed into `atom_fill`. While the wire carries a `Motif`, the viewport renders the corresponding 3D atomic structure (with ghost atoms and wireframe box) so the editing experience is fully visual.
- `diff: Molecule` (pin 1) — the raw diff structure (additions, deletions, modifications relative to the base) for inspection or for routing through `apply_diff` / `atom_composediff`.

### Working in Cartesian, exporting fractional

Atoms inside the editor are placed and dragged in **Cartesian** coordinates (one unit = one ångström) so that all the existing atom-editor tools — guided placement, drag, rotate, minimize — behave exactly as they do in `atom_edit`. The conversion to fractional motif coordinates happens at the output boundary using the connected `unit_cell`.

### Unit-cell wireframe and ghost atoms

The viewport shows the primary unit cell as a wireframe parallelepiped, plus **ghost atoms** — faded copies of motif atoms in the 26 neighboring cells. Ghost atoms make the periodic structure visible and serve as bond targets when you want to express a bond that crosses a cell boundary.

A `Neighbor depth` property (`0.0`–`1.0`, default `0.3`) controls how far into neighboring cells ghosts are shown. The default value covers diamond-family bonding geometries with minimal visual clutter; raise it to see deeper neighbors, lower it to declutter.

### Cross-cell bonds

To create a bond that crosses a cell boundary, use the **Add Bond** tool to draw from a primary-cell atom to a ghost atom. The node records the corresponding `relative_cell` offset and renders the bond's symmetric counterpart on the other side of the cell automatically, so the bond is visible from any direction. Internally only one canonical entry is stored; the symmetric rendering is generated on the fly.

### Parameter elements

Motifs use *parameter elements* — placeholder slots like `PRIMARY` or `SECONDARY` that get substituted with concrete elements by the `atom_fill` node. `motif_edit` exposes parameter elements directly: define them in the node's properties (a list of `(name, default element)` pairs) and place them as atoms in the editor. Hover tooltips show the parameter name (e.g. *PRIMARY*) instead of *Unknown*, and minimization, guided placement, and hydrogen passivation use the parameter's default element so the geometry is realistic while editing.

### Typical workflows

- *Build a motif from scratch:* leave `molecule` unconnected, wire a `lattice_vecs` into `unit_cell`, then place atoms and bonds in 3D.
- *Edit an imported crystal:* wire `import_cif`'s `atoms` output into `molecule` and its `unit_cell` output into `unit_cell`. The full conventional cell shows up as the base; edit on top of it non-destructively.
- *Modify a supercell:* feed a `supercell` node's output through `materialize` / `import_xyz` (or any path that produces atoms) into a `motif_edit` to introduce vacancies, substitutions, or dopants inside an enlarged cell.
