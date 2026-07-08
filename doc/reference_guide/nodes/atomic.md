# Atomic structure nodes

← Back to [Reference Guide hub](../../atomCAD_reference_guide.md)

## import_xyz

Imports an atomic structure from an XYZ file. Outputs a `Molecule` — XYZ files carry no crystal-lattice information, so the result has no `Structure` association.

![](../../atomCAD_images/import_xyz.png)

It converts file paths to relative paths whenever possible (if the file is in the same directory as the node or in a subdirectory) so that when you copy your whole project to another location or machine the XYZ file references will remain valid.

## export_xyz

Saves the atomic structure on its `molecule` input as an XYZ file. This is an **effect node**: its output type is `Unit`, and the file write only happens when the node is invoked through the right-click **Execute** action (or transitively from a `foreach` upstream of it). Display passes — including normal scene refreshes triggered by editing — never write a file. See [Execute action (side-effect nodes)](../ui.md#execute-action-side-effect-nodes).

![](../../atomCAD_images/export_xyz.png)

**Input pins**

- `molecule: HasAtoms` — the `Crystal` or `Molecule` to write.
- `file_name: String` — the file path. May be wired in (typical when batch-exporting) or set as a stored property. Relative paths are resolved against the design's directory; absolute paths are stored relative when the file lives under the design tree, so projects remain portable when copied.

**Output (single pin)**

- `Unit`. The pin is not displayable in the 3D viewport; its only purpose is to be wired into a `foreach` body (or to be the target of an explicit Execute) so the side effect fires when intended.

> **Note on the signature change.** `export_xyz` previously passed the molecule through unchanged on its output pin and wrote the file as a side effect of any evaluation that reached it (which made editing upstream parameters silently overwrite files). It now returns `Unit` and writes only on Execute. If you want both the export side effect *and* the molecule downstream, wire the molecule directly into the downstream consumer and treat `export_xyz` as a sibling sink.

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
- `motif: Motif` — the same atom set expressed as a fractional `Motif` so it can be fed directly into a `structure` node and downstream `materialize` (typically together with `unit_cell`).

**Typical pipelines**

- *Direct fill:* wire `motif` and `unit_cell` into a `materialize` node (via a `structure` node) to use the imported crystal as a template for filling geometry.
- *Edit then fill:* wire `atoms` and `unit_cell` into a `motif_edit` node, edit interactively in 3D, then feed the edited motif into `materialize`.

## materialize

Converts a `Blueprint` into a `Crystal` by carving atoms out of the infinite crystal field using the blueprint's geometry as a cookie cutter. The output retains the `Structure`, so further structure-aligned operations remain available downstream.

![](../../atomCAD_images/atom_fill_node.png)

![](../../atomCAD_images/atom_fill_props.png)

![](../../atomCAD_images/atom_fill_viewport.png)

The motif and motif offset used for filling come from the input Blueprint's `Structure` (which is built up by the `structure` / `motif` / `lattice_vecs` nodes upstream). If no upstream structure has been chosen, the default cubic zincblende motif is used. (See also: the `motif` and `structure` nodes.)

**Input pins**

- `shape: Blueprint` — the cookie-cutter geometry plus the structure that supplies the lattice and motif.
- `passivate: Bool` (optional) — see *Hydrogen passivation* below.
- `rm_single: Bool` (optional) — see *Remove single-bond atoms* below.
- `surf_recon: Bool` (optional) — see *Surface reconstruction* below.
- `invert_phase: Bool` (optional) — see *Invert phase* below.
- `rm_unbonded: Bool` (optional) — see *Remove unbonded atoms* below.
- `regions: Array[Record(MaterializeRegion)]` (optional) — per-region setting overrides; see *Per-region settings* below.

The boolean inputs default to the values set on the node properties; wiring an input overrides the property.

> **Note on the rename:** `materialize` was previously called `atom_fill`. The old `motif` and `m_offset` input pins are gone — both come from the input Blueprint's structure now. Older `.cnnd` files that still reference `atom_fill` will be migrated automatically.

### Parameter element overrides

Motifs declare *parameter elements* — placeholder slots like `PRIMARY` or `SECONDARY` that the `materialize` node substitutes with concrete elements when it materializes the crystal. The properties panel shows a *Parameter Element Overrides* table, populated automatically from the connected motif: one row per parameter, with the parameter's name on the left and an element dropdown on the right. Choose an element to override the parameter's default; leave a row at *Default (X)* to keep the motif's own default. For example, with the default cubic zincblende motif, switching `PRIMARY` from carbon to silicon yields the same silicon carbide as before — there is no longer a free-form text area to edit.

![](../../atomCAD_images/silicon_carbide.png)

When a motif is edited inside `motif_edit`, parameter atoms (which carry non-physical atomic numbers) **simulate as their default element** for the purpose of force-field minimization, guided placement, and hydrogen passivation — a `PRIMARY` atom whose default is carbon will be treated as carbon for bond-length and hybridization calculations. This keeps the motif geometry realistic during interactive editing even before any concrete substitutions are chosen in `materialize`. Hovering over such an atom in the viewport shows an extra *Effective element: …* line in the tooltip whenever the displayed atomic number differs from the simulated one.

You can switch on or off the following checkboxes:

- *Remove unbonded atoms:* If turned on (the default), atoms left with no bonds after the cut are removed automatically (lone atom removal). Turn it off to keep unbonded atoms — useful for debugging what is actually being cut, dumping atoms for repackaging into a new structure or patch, or materializing salts such as NaCl whose ions are not covalently bonded.
- *Remove single-bond atoms:* If turned on, atoms which only have one bond after the cut are removed. This is done recursively until there is no such atom in the atomic structure. Note that this recursive cleanup **also removes unbonded (zero-bond) atoms** — both atoms that are already unbonded and atoms that become unbonded as the recursion peels away their neighbors. In other words, *Remove single-bond atoms* implies *Remove unbonded atoms*: enabling it removes lone atoms regardless of the *Remove unbonded atoms* setting.
- *Surface reconstruction:* Real crystalline surfaces are rarely ideal bulk terminations; instead, they typically undergo *surface reconstructions* that lower their surface energy. atomCAD will support several reconstruction types depending on the crystal structure. At present, reconstruction is implemented only for **cubic diamond** crystals (carbon and silicon) and only for the most important one: the **(100) 2×1 dimer reconstruction**.
  If reconstruction is enabled for any other crystal type, the setting has no effect.
  The (100) 2×1 reconstruction automatically removes single-bond (dangling) atoms even if the *Remove single-bond atoms* option is not enabled. Surface reconstruction can be used together with hydrogen passivation or on its own.
- *Invert phase*: Determines whether the phase of the dimer pattern should be inverted. 
- *Hydrogen passivation:* Hydrogen atoms are added to passivate dangling bonds created by the cut.

### Per-region settings (`regions`)

The five booleans above (`passivate`, `rm_single`, `surf_recon`, `invert_phase`, `rm_unbonded`) normally apply to the **entire** structure. The optional `regions` input lets you override them inside one or more volumes you draw — for example, depassivating or reconstructing only the top surface of a slab while the rest keeps the node's default treatment.

**Building a region spec.** A region is a `MaterializeRegion` record built with an ordinary `record_construct` node (select the built-in `MaterializeRegion` type from its dropdown). Its fields are:

- `volume: Blueprint` (required) — the region's shape. Build it from the same geometry nodes you already use (`half_space`, `cuboid`, `sphere`, CSG combinations), in the **same real space** as the Blueprint being materialized. Only the volume's geometry is used; any `Structure` it carries is ignored. The typical region is a single `half_space` whose plane cuts through the surface you want to treat differently.
- `margin: Float` (optional) — membership tolerance in Å (see *Margin* below). Leave unset to use the default of 0.1 Å.
- `passivate`, `rm_single`, `surf_recon`, `invert_phase`, `rm_unbonded` (all optional `Bool`) — the per-region overrides. Each field has three states: **force on** (set to `true`), **force off** (set to `false`), and **inherit** (leave unset). An unset field is transparently inherited — a region that sets only `surf_recon: true` changes nothing else.

Wire one or more region records into an `array` node and feed that into `materialize.regions`. A typical chain is: `half_space` → `record_construct(MaterializeRegion)` (with `surf_recon: true`) → `array` → `materialize.regions`.

**Root + painter's model.** The node's own checkboxes are the **root** — they apply to all of space and stay editable even when `regions` is connected (the side panel notes *"Regions override these settings inside their volumes."*). Regions layer on top of the root: for any point and any setting, the regions are resolved **last → first** in array order, and the first (latest-in-array) region that contains the point *and* sets that field wins; if none does, the root supplies the value. Resolution is per field, so overlapping regions compose field-by-field rather than wholesale. A disconnected `regions` pin, an empty array, or a region with every field unset all reproduce today's behavior exactly.

**Margin.** A point belongs to a region when the region geometry's signed distance at that point is ≤ the region's `margin`. The default of 0.1 Å matters because surface atoms produced by the cut sit numerically *on* the boundary of any region you build by reusing the cutting geometry — the small positive margin robustly captures those surface atoms without grabbing the layer below. A **negative** margin shrinks the region (e.g. to deliberately exclude the boundary layer). Where two regions' margins overlap, the array order decides the result inside the overlap band, deterministically.

## dematerialize

Converts a `Crystal` back to a `Blueprint` by discarding its carved atoms. The geometry shell is preserved as the Blueprint's geometry. Useful when you want to roll back from a materialized state and reuse the cookie-cutter shape upstream of further structure-aligned operations.

**Input pins**

- `input: Crystal` — the Crystal to dematerialize.

The operation is destructive: any atom edits applied to the Crystal (e.g. via `atom_edit`) are lost when the atoms are dropped. The Crystal must carry a geometry shell — Crystals that have lost their geometry (e.g. created via `enter_structure` from a free Molecule) cannot be dematerialized and produce an error.

Alignment is propagated through unchanged.

## exit_structure

Converts a `Crystal` to a `Molecule` by dropping its structure association. Atoms and any geometry shell pass through unchanged; the resulting `Molecule` is free-floating and can be moved with `free_move` or `free_rot`.

**Input pins**

- `input: Crystal` — the Crystal whose structure should be discarded.

This is the canonical step for breaking a Crystal out of its lattice context — for example, before exporting an unconstrained molecule, or before using free-space movement nodes that reject `Crystal` inputs.

## enter_structure

Converts a `Molecule` into a `Crystal` by re-associating it with a `Structure`. Pure packaging — atoms are not snapped to lattice positions; they stay exactly where they were.

**Input pins**

- `input: Molecule` — the free-floating atoms (and optional geometry shell) to wrap.
- `structure: Structure` — the structure (lattice vectors + motif) to attach.

Because the Molecule's atoms generally do not lie on the target structure's motif sites, the output Crystal is conservatively flagged `lattice_unaligned` (see [Blueprint alignment](../node_networks.md#blueprint-alignment)). Use this when you have arbitrary atoms (e.g. relaxed or imported) and want to bring them back into a structure-aware pipeline. Snapping atoms to the nearest lattice positions is a separate operation, not done by this node.

## free_move

Translates an unanchored object — a `Blueprint` or a `Molecule` — by a vector in world space (Cartesian coordinates). The input pin accepts the abstract `HasFreeLinOps` type; the concrete variant flows through unchanged. `Crystal` inputs are rejected — use `exit_structure` first to drop the lattice association, or use `structure_move` to stay in lattice space.

![](../../atomCAD_images/atom_move.png)

**Input pins**

- `input: HasFreeLinOps` — the Blueprint or Molecule to translate.
- `translation: Vec3` (optional) — the translation vector in ångströms.

For a `Blueprint`, only the geometry (the cookie cutter) moves; the structure stays fixed. The cutter typically drifts off-lattice as a result, so the output is flagged `lattice_unaligned`. For a `Molecule`, atoms and geometry move together freely.

**Gadget controls**

Drag the gadget axes to adjust the translation vector interactively.

## free_rot

Rotates an unanchored object — a `Blueprint` or a `Molecule` — around an axis in world space. Like `free_move`, the input is `HasFreeLinOps`; `Crystal` inputs are rejected.

![](../../atomCAD_images/atom_rot.png)

**Input pins**

- `input: HasFreeLinOps` — the Blueprint or Molecule to rotate.
- `angle: Float` (optional) — rotation angle in degrees. (Stored on the node as `angle_degrees`; the pin keeps the short name `angle`.)
- `rot_axis: Vec3` (optional) — axis of rotation (will be normalized).
- `pivot_point: Vec3` (optional) — pivot point, in ångströms. Defaults to the origin.

For a `Blueprint`, only the geometry rotates; the structure stays fixed, so the output is flagged `lattice_unaligned`. For a `Molecule`, atoms and geometry rotate together.

**Gadget controls**

The gadget displays the pivot point and rotation axis. Drag the rotation axis to adjust the angle interactively.

## atom_union

Merges multiple atomic structures into one. The `structures` input accepts an array of atomic structures (array-typed input; you can connect multiple wires and they will be concatenated). All elements of the array must be the same concrete type — either all `Crystal` or all `Molecule` — and the output preserves that type. Mixed `Crystal` + `Molecule` arrays are a validation error; insert an explicit `exit_structure` node first if you want a `Molecule` result.

![](../../atomCAD_images/atom_union.png)

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

## Restricting an atom operation to a region

Several atom operations — `add_hydrogen`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, `freeze`, and `unfreeze` — accept an optional **`region: Blueprint`** input pin (always the last pin) that confines their effect to a volume you draw. With `region` disconnected, the operation applies to **all** atoms (its original behavior). With `region` connected, the operation only touches atoms **inside** the region volume; atoms outside pass through untouched.

- **Membership.** An atom is in-region when the region geometry's signed distance at the atom's position is ≤ a small margin (default 0.1 Å — the same default `materialize`'s per-region margin uses). The margin reliably captures surface atoms that sit numerically *on* a boundary you built by reusing the cutting geometry, without grabbing the layer below.
- **Build the region** from the same geometry nodes you already use (`half_space`, `cuboid`, `sphere`, CSG combinations), in the same real space as the atoms. Only the region Blueprint's geometry is used; any `Structure` it carries is ignored. The typical region is a single `half_space` whose plane cuts through the surface you want to treat. A region disjoint from the structure is a well-defined no-op.
- **Which atom counts.** Each operation tests the position of the **existing (host) atom** it acts on: `add_hydrogen` tests the dangling-bond atom (the new H is placed wherever the bond template puts it, even if that lands just outside the region); `remove_hydrogen` tests the heavy atom an H is bonded to (an H sitting just outside the boundary is still stripped if its host is in-region); `infer_bonds` (re)infers a bond when **at least one** endpoint is in-region; `atom_replace` / `freeze` / `unfreeze` test the atom being edited. Newly created atoms are never themselves membership-tested.
- **Multiple regions = chained nodes.** Because each of these operations returns the same kind of structure it received, you apply several regional treatments by placing several nodes in sequence, each with its own region — there is no multi-region pin on these nodes. (That painter's-algorithm pattern is unique to `materialize`, whose settings are consumed together in a single fill pass; see its *Per-region settings*.)

## relax

Performs UFF (Universal Force Field) energy minimization on an atomic structure. Takes a `Crystal` or `Molecule` input and outputs the minimized structure, preserving the concrete input type.

This node is useful in node-network workflows where you want to relax a structure non-destructively as part of a parametric pipeline. For interactive minimization during atom editing, use the energy minimization feature built into the `atom_edit` node instead.

**Frozen atoms.** `relax` honors the per-atom *frozen* flag: atoms marked frozen (by an upstream `freeze` node) are held fixed during minimization while their mobile neighbors move and settle. A frozen atom still participates in the force field — it pulls on its neighbors — it just doesn't move itself. This is how you relax only a sub-volume of a structure: freeze everything you want to hold, then `relax`. `relax` itself has no `region` pin; compose it with `freeze` / `unfreeze` to scope which atoms move.

## freeze

Marks atoms as **frozen** so a downstream `relax` node holds them fixed. Takes a `Crystal` or `Molecule` and outputs the same structure with the frozen flag set on the selected atoms, preserving the concrete input type.

**Input pins**

- `molecule: HasAtoms` — the input structure.
- `region: Blueprint` (optional) — restrict freezing to atoms inside this volume. Disconnected → **all** atoms are frozen. See *Restricting an atom operation to a region* above.

Freezing is a pure metadata edit — atom positions and bonds are unchanged. Chaining `freeze` nodes with different regions accumulates: `freeze(region A) → freeze(region B)` leaves the union of A and B frozen. Pair `freeze` with `relax` to constrain which atoms move (see `relax`).

## unfreeze

The inverse of `freeze`: clears the frozen flag so `relax` can move the atoms again. Takes a `Crystal` or `Molecule` and outputs the same structure with the frozen flag cleared on the selected atoms, preserving the concrete input type.

**Input pins**

- `molecule: HasAtoms` — the input structure.
- `region: Blueprint` (optional) — restrict unfreezing to atoms inside this volume. Disconnected → **all** atoms are unfrozen. See *Restricting an atom operation to a region* above.

## add_hydrogen

Adds hydrogen atoms to satisfy valence requirements of undersaturated atoms. Takes a `Crystal` or `Molecule` input and outputs a hydrogen-passivated structure, preserving the concrete input type.

The algorithm detects hybridization (sp3, sp2, sp1) automatically and places hydrogen atoms at the correct bond lengths and angles. This is the node-network counterpart of the one-click hydrogen passivation in the `atom_edit` node.

An optional **`region: Blueprint`** input pin (last pin) restricts passivation to dangling bonds on in-region atoms; a passivating H whose host atom is in-region is still placed even if it lands just outside the region. Disconnected → all atoms are passivated. See *Restricting an atom operation to a region* above.

## remove_hydrogen

Removes all hydrogen atoms from an atomic structure. Takes a `Crystal` or `Molecule` input and outputs the bare framework without hydrogens, preserving the concrete input type.

Useful in workflows like: `remove_hydrogen` → transform/edit → `add_hydrogen`, allowing you to work with the bare framework and re-passivate afterward.

An optional **`region: Blueprint`** input pin (last pin) restricts removal to hydrogens whose heavy (host) atom is in-region — including an H whose own position is just outside the region. Disconnected → all hydrogens are removed. See *Restricting an atom operation to a region* above.

## infer_bonds

Recomputes bonds in an atomic structure based on interatomic distances and covalent radii. Takes a `Crystal` or `Molecule` and outputs the same structure with a refreshed bond list, preserving the concrete input type. Useful after importing files that lack bond information (e.g. some XYZ sources) or after operations that move atoms enough to invalidate the existing bonds.

**Input pins**

- `molecule: HasAtoms` — the input structure.
- `additive: Bool` (optional) — when `false` (default), the existing bonds are discarded and rebuilt from scratch. When `true`, existing bonds are preserved and only inferred bonds that are not already present are added.
- `bond_tolerance: Float` (optional) — multiplier applied to the sum of covalent radii when deciding whether two atoms should be bonded (default `1.15`).
- `region: Blueprint` (optional) — restrict bond inference to bonds with at least one endpoint inside this volume; a surface atom thus gets its bonds even to a neighbor just outside. Disconnected → bonds are inferred everywhere. See *Restricting an atom operation to a region* above.

**Properties**

The same `additive` and `bond_tolerance` values are also available as node properties for cases where you want a fixed setting without an extra wire.

## atom_replace

Substitutes atoms of one element for another (or removes them) in bulk, according to a list of replacement rules. The output preserves the concrete input type — a `Crystal` in produces a `Crystal` out, a `Molecule` in produces a `Molecule` out.

![TODO(image): the `atom_replace` node selected with its properties panel showing two replacement rows (e.g. C→Si and H→Delete)](TODO)

**Input pins**

- `molecule` — the atomic structure to transform (`Crystal` or `Molecule`).
- `rules: Array[Record(ElementMapping)]` (optional) — a programmatically-built list of replacement rules. `ElementMapping` is a built-in record def with two `Int` fields, `from` and `to` (atomic numbers; `0` on `to` means *Delete*).
- `region: Blueprint` (optional) — apply the replacement rules only to atoms inside this volume; out-of-region atoms pass through unchanged. Disconnected → rules apply to all atoms. See *Restricting an atom operation to a region* above.

**Properties**

When `rules` is unwired, the replacement rules live as node properties instead. The property panel shows a list of rows, each with `[source element] → [target element]` and a delete button, plus an *Add Replacement* button at the bottom.

The **target dropdown** has an extra entry — *Delete* — at the top of the list. Choosing *Delete* removes every atom of the source element from the structure (and cleans up their bonds) instead of substituting them.

When `rules` is wired, the wired array entirely replaces the property list — the editor renders disabled (existing rows stay visible at half opacity so you can read what would come back on disconnect), and the node subtitle is suppressed (the upstream source carries its own subtitle). The stored property values are not cleared by connecting the pin; disconnect to edit inline again.

**Behavior**

- Each rule maps a source element to a target element (or to *Delete*).
- Atoms whose element is not listed in any rule pass through unchanged.
- Rules apply independently — each atom is matched against the rule list once.
- If multiple rules name the same source element, the last rule wins.
- Bond connectivity is preserved when an element is substituted; bonds attached to deleted atoms are removed.
- For wired rules, `from` is validated to `-1..=118` (the `-1` and `0` sentinels are silently ignored, matching the property-driven path) and `to` to `0..=118`; out-of-range values produce an evaluation error rather than a silent skip.

The node subtitle summarizes the active rules (e.g. `C→Si, O→S`, or `H→(del)` for a deletion rule), with a `… (+N more)` suffix when the list is longer than three entries. Suppressed when `rules` is wired.

**Text format**

The rule list serializes as an array of `(from_atomic_number, to_atomic_number)` pairs, with `0` representing *Delete*:

```
replace1 = atom_replace {
    replacements: [(6, 14), (8, 16)]
}
```

This replaces C→Si and O→S.

## atom_cut

Cuts an atomic structure using cutter geometries. Unlike `materialize` which creates atoms from geometry, `atom_cut` removes atoms that lie outside the cutter shapes — effectively performing a Boolean intersection between an existing atomic structure and one or more 3D geometries.

**Input pins**

- `molecule` — The atomic structure to be cut (`Crystal` or `Molecule`). The output preserves the concrete input type.
- `cutters` — An array of `Blueprint` values defining the region to keep (array-typed input; you can connect multiple wires).

**Properties**

- `Cut SDF Value` — The SDF threshold for the cut boundary (default 0.0). Atoms with SDF values greater than this threshold are removed.
- `Unit Cell Size` — The unit cell size in Ångströms used to normalize atom positions when evaluating against the cutter geometry.

Bonds connected to removed atoms are automatically deleted.

## Surface reconstruction patches (`patch_build` + `patch_latticefill`)

A surface reconstruction is periodic: a small per-cell rearrangement (form a dimer, add an adatom, depassivate/repassivate, remove or substitute surface atoms) repeats across a crystal face. The `materialize` node has a *Surface reconstruction* checkbox for the one hard-coded case (cubic-diamond (100) 2×1), but the **patch** nodes let you author *any* reconstruction once and tile it across a region.

The two nodes form an author-then-apply pair:

- **`patch_build`** extracts a reusable *patch* from a hand-built example — you draw a slab of the reconstructed surface sitting on its bulk and one tile's volume, and the node figures out the tile automatically.
- **`patch_latticefill`** tiles that patch across a workpiece and welds it in.

The key idea is that **periodic bonds are never represented explicitly — they emerge from coincidence.** The tile is an ordinary atomic structure that *includes* the atoms it shares with its neighbours (and the bulk atoms it bonds down into). When tiles are laid out on the lattice, each shared atom lands on the identical position as the corresponding atom of the next tile; fusing those coincident atoms (a *weld*) turns every boundary-crossing bond into an ordinary bond. The same weld fuses the tile to the surrounding bulk. So a patch carries no motif, no fractional coordinates, and no diff — just a `Molecule`, a few integer tiling vectors, and a cut volume.

A patch is a **built-in record** — `Patch = { tile: Molecule, tiling_vectors: Array[IVec3], cut_volume: Blueprint }` — so you can inspect or assemble one with the ordinary `record_destructure` / `record_construct` nodes if you ever need to. See [Record types → built-in record defs](./math_programming.md#record-types).

> **Scope (v1).** One face at a time; multi-face stitching and edges/corners are left to passivation or manual cleanup. The patch and the region must share a lattice (the tiling vectors are integer combinations of the substrate lattice — incommensurate interfaces are out). Boundary bonds reaching more than one cell are supported but uncommon.

### patch_build

Extracts a tileable patch from an authored slab and a cut volume. The authoring model is **draw, don't assemble**: build an ordinary big slab of the reconstructed surface on its bulk (a `Crystal` or `Molecule`), draw **one tile's volume** as a normal `Blueprint`, and let the node extract the tile. You never mark individual atoms as interior / boundary / ghost — that is all settled by coincidence at weld time.

![TODO(image): the `patch_build` node with an authored reconstructed slab and a single-tile cut volume wired in, properties panel showing the build threshold ε](TODO)

**Input pins**

- `source: HasAtoms` — the whole authored slab (the reconstruction **on its bulk**). Only its atoms are read; the stored tile is *computed* from this, not equal to it. A `Crystal` or a `Molecule` both work.
- `lattice: HasStructure` — supplies the lattice vectors used to interpret and validate the integer tiling vectors.
- `tiling_vectors: Array[IVec3]` — 1–3 periodic directions, each an integer combination of `lattice`'s vectors (1 = chain/edge, 2 = surface, 3 = bulk twin). Typically produced by a [`plane_tiling_vectors`](./math_programming.md#plane_tiling_vectors) node rather than typed by hand. Must be linearly independent.
- `cut_volume: Blueprint` — the geometry of **one tile**. It does double duty: at build time it separates the slab into interior (kept as real tile atoms) and the outward-bonded ghosts; the same volume is stored in the patch and drives substrate removal at apply time.

**Output (single pin)**

- `Patch` — the tileable patch record.

**Property**

- `Build threshold ε (Å)` (default `0.1`) — a slab atom counts as *interior* when its cut-volume membership SDF ≤ ε. Keep it above any on-surface jitter so atoms drawn right on the cut face are caught, but well below the interplanar spacing so it never grabs the layer below.

**How extraction works.** Interior atoms (inside the cut) become real tile atoms. Slab atoms *outside* the cut that are bonded to an interior atom are copied as **patch-ghosts** — these are exactly the two kinds of atom the weld needs: neighbour-tile atoms (across a tile boundary → realize the periodic bond) and bulk collar atoms (one step into the substrate → realize the tile↔bulk bond and inherit the bulk's bonds). Bonds with at least one interior endpoint are kept; ghost–ghost bonds are dropped. The extracted atoms and the cut volume are kept **in the coordinates you drew them in** — they came straight off the authored slab, so they are already on the lattice. Because every placement `patch_latticefill` makes is a whole-lattice-vector translation (the tiling steps plus the optional `origin` offset), every atom stays on the lattice and the welds line up; and at the default offset nothing is moved, so the patch reappears exactly where it was authored.

### patch_latticefill

Tiles a patch across a region and welds it in, producing the reconstructed `Crystal`.

![TODO(image): the `patch_latticefill` node with a target crystal and a patch wired in, properties panel showing passivate, tolerance, and the green "Compatible" badge](TODO)

**Input pins**

- `target: HasAtoms` — the structure being reconstructed.
- `region: HasStructure` (optional) — where to tile; supplies the substrate lattice vectors and the fill extent. Defaults to `target`'s extent (in which case `target` must be a `Crystal`, so it carries a structure). `target` and `region` are separate pins because in 3D the fill volume need not match the workpiece volume.
- `patch: Patch` — from `patch_build`.
- `origin: IVec3` (optional, default `(0,0,0)`) — a whole-cell **offset** applied to the entire reconstruction. The default `(0,0,0)` places it exactly where it was authored (same lattice registration) — what you want whenever `target` is the crystal the patch was built from, or an equivalent one. Set it to slide the reconstruction by whole unit cells, or to pick a different phase (e.g. which sites pair into dimers). It does **not** change *how much* of the region is filled — tiling always covers every cell whose footprint fits; `origin` only shifts their common phase.
- `passivate: Bool` (optional, default `true`) — hydrogen-passivate the danglers left after welding and dropping unwelded ghosts. Set `false` to keep edge danglers exposed — e.g. when a later `patch_latticefill` on an adjacent face is meant to bond to them — and passivate once at the end. (Matches `materialize`'s passivate.)
- `tolerance: Float` (optional, default `0.1` Å) — weld tolerance. Atoms within this distance fuse into one. Keep it below the smallest interatomic spacing so distinct lattice sites never over-merge.

**Output (single pin)**

- `Crystal` — the reconstructed crystal.

**What it does, in order.** Starting from the authored registration shifted by `origin`, select the cells whose tile fits the region. A cell is selected when **all of the tile's interior atoms, placed at that cell and projected onto the surface plane, land inside the region** — whole-cell containment in the periodic directions (no partial lateral tiles), free along the surface normal (so the cut volume may legitimately stick out to reach passivation hydrogens above the face). Cut the displaced substrate in those cells, place a copy of the tile in each — at `origin = (0,0,0)` and no tiling step the copy lands exactly where it was drawn — weld all coincident atoms (fusing tile↔tile periodic bonds and tile↔bulk collar bonds in one pass), drop any patch-ghost that found no real twin (a true reconstruction edge), then passivate the residual danglers. Cut and place share the same cell set, so substrate is never removed where it is not also reconstructed.

**Property — Test height at lattice origin** (default **off**). The "surface plane" the containment test projects onto needs a height that lies inside the target slab. Off (the default) derives it from the **target** slab's own extent, so it works wherever the target sits — including a thin slab offset from the lattice origin (the common case, since a surface is authored at the height where it sits). On projects onto the plane through the **lattice origin** instead — simpler, but it **selects nothing** when the target does not straddle the origin (the badge then reads "No tiles placed"). Leave it off unless your workpiece is deliberately built through the origin.

**Compatibility badge.** After each evaluation the properties panel shows a compatibility badge summarizing the weld outcome:

- **Tiles placed** — how many cells received a tile. **Zero is a failure, not a success**: no cell was selected, so the patch added nothing (usually the test plane missed the target — see *Test height at lattice origin*).
- **Welded joins (neighbour + bulk)** — shared/collar atoms (the tile's outward "ghost" atoms) that landed on a real atom and fused: each one realizes a bond either to a neighbouring tile (a periodic bond) or down into the bulk (a **collar** bond — the ring of atoms where the patch attaches to the substrate beneath it). A healthy result has many.
- **Orphaned edge ghosts (dropped)** — ghost atoms that found no real twin: they point outward across the patch's outer **edge** (no neighbour tile there) or at bulk that isn't present. They are dropped and hydrogen-passivated. **This is normal** — every finite patch has a perimeter of these; it is *not* a defect on its own.
- **Over-coordinated atoms** — real atoms left with more bonds than chemically allowed after welding. This *is* a defect — usually the patch sits **too low / into the sub-surface**.

The badge reads red **No tiles placed** when nothing was tiled; amber **Check fit** on a real problem — over-coordinated atoms, or a patch that was placed but whose ghosts *all* failed to weld (it's floating / mis-registered); and otherwise green **Welded in** (a normal perimeter of orphaned edge ghosts does not turn it amber). It reads *not yet evaluated* until the node has been displayed at least once. For collars to weld, `target` must share the build lattice's full lattice **and** motif registration (a registration mismatch shows up as a patch that placed tiles but welded nothing — the floating "Check fit" case).

**Debug views (panel checkboxes, off by default).** Two non-physical toggles for understanding *why* cell selection chose the cells it did:

- **Project atoms to test plane** — outputs the patch atoms flattened onto the exact plane the containment test runs on (no weld). Lets you see each atom's test position against the region footprint and read off why a tile passed or failed.
- **Show frontier tiles** — also places the one-cell-wider ring of cells around the selection, flagging the *not-selected* ones as **frozen** (so they render distinctly) — you see the just-excluded neighbours next to the included ones. When nothing was selected at all, it shows the `−1…+1` block around the origin instead, so you can still see where the rejected tiles would have gone.

## atom_edit

The `atom_edit` node provides the same atom editing tools described in the [Direct Editing Mode](../direct_editing.md#the-atom-editor) section above — all tools, keyboard shortcuts, hydrogen passivation, energy minimization, freeze, measurements, and the [guideline tool](../direct_editing.md#guideline-tool) work identically. When an `atom_edit` node is selected in the node network, the atom editor appears in the Node Properties panel.

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

- `result: Motif` (pin 0) — the constructed motif in fractional coordinates, ready to feed into a `structure` node and downstream `materialize`. While the wire carries a `Motif`, the viewport renders the corresponding 3D atomic structure (with ghost atoms and wireframe box) so the editing experience is fully visual.
- `diff: Molecule` (pin 1) — the raw diff structure (additions, deletions, modifications relative to the base) for inspection or for routing through `apply_diff` / `atom_composediff`.

### Working in Cartesian, exporting fractional

Atoms inside the editor are placed and dragged in **Cartesian** coordinates (one unit = one ångström) so that all the existing atom-editor tools — guided placement, drag, rotate, minimize — behave exactly as they do in `atom_edit`. The conversion to fractional motif coordinates happens at the output boundary using the connected `unit_cell`.

### Unit-cell wireframe and ghost atoms

The viewport shows the primary unit cell as a wireframe parallelepiped, plus **ghost atoms** — faded copies of motif atoms in the 26 neighboring cells. Ghost atoms make the periodic structure visible and serve as bond targets when you want to express a bond that crosses a cell boundary.

A `Neighbor depth` property (`0.0`–`1.0`, default `0.3`) controls how far into neighboring cells ghosts are shown. The default value covers diamond-family bonding geometries with minimal visual clutter; raise it to see deeper neighbors, lower it to declutter.

### Cross-cell bonds

To create a bond that crosses a cell boundary, use the **Add Bond** tool to draw from a primary-cell atom to a ghost atom. The node records the corresponding `relative_cell` offset and renders the bond's symmetric counterpart on the other side of the cell automatically, so the bond is visible from any direction. Internally only one canonical entry is stored; the symmetric rendering is generated on the fly.

### Parameter elements

Motifs use *parameter elements* — placeholder slots like `PRIMARY` or `SECONDARY` that get substituted with concrete elements by the `materialize` node. `motif_edit` exposes parameter elements directly: define them in the node's properties (a list of `(name, default element)` pairs) and place them as atoms in the editor. Hover tooltips show the parameter name (e.g. *PRIMARY*) instead of *Unknown*, and minimization, guided placement, and hydrogen passivation use the parameter's default element so the geometry is realistic while editing.

### Typical workflows

- *Build a motif from scratch:* leave `molecule` unconnected, wire a `lattice_vecs` into `unit_cell`, then place atoms and bonds in 3D.
- *Edit an imported crystal:* wire `import_cif`'s `atoms` output into `molecule` and its `unit_cell` output into `unit_cell`. The full conventional cell shows up as the base; edit on top of it non-destructively.
- *Modify a supercell:* feed a `supercell` node's output through `materialize` / `import_xyz` (or any path that produces atoms) into a `motif_edit` to introduce vacancies, substitutions, or dopants inside an enlarged cell.
