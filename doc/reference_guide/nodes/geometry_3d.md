# 3D Geometry nodes

← Back to [Reference Guide hub](../../atomCAD_reference_guide.md)

These nodes output a `Blueprint` — a 3D shape paired with a `Structure`, which can later be used as an input to a `materialize` node to carve an atomic structure out of the lattice.
Positions and sizes are usually discrete integer numbers meant in crystal lattice coordinates.


## extrude

Extrudes a 2D geometry to a 3D geometry.

![](../../atomCAD_images/extrude.png)



You can create a finite or infinite extrusion. Infinite extrusion is unbounded both in the positive and negative extrusion direction. Finite extrusions start from the plane and is also finite in the (positive) extrusion direction.

By default the extrusion is **perpendicular to the drawing plane** (the *Extrude perpendicular to plane* checkbox is on). In this mode the direction is recomputed from the plane on every evaluation, so re-orienting the drawing plane later keeps the extrusion perpendicular instead of leaving it slanted along a stale direction.

Uncheck *Extrude perpendicular to plane* to specify the direction explicitly as Miller indices (this is the legacy behavior). A wired `dir` input pin overrides both — the precedence is wired pin > perpendicular mode > stored direction.


## cuboid

Outputs a cuboid with integer minimum corner coordinates and integer extent coordinates. Please note that if the unit cell is not cubic, the shape will not necessarily be a cuboid: in the most general case it will be a parallelepiped. 

![](../../atomCAD_images/cuboid_node.png)

![](../../atomCAD_images/cuboid_props.png)

![](../../atomCAD_images/cuboid_viewport.png)

## sphere

Outputs a sphere with integer center coordinates and integer radius.

![](../../atomCAD_images/sphere_node.png)

![](../../atomCAD_images/sphere_props.png)

![](../../atomCAD_images/sphere_viewport.png)

## free_sphere

The non-lattice-aligned analog of `sphere`: its center and radius are given directly in **real-space Ångström coordinates** (floating-point), rather than in whole lattice steps. Use it when you need a sphere positioned or sized *between* lattice points — the workaround of composing `sphere` with `free_move` can only reach lattice-quantized centers, and offers no way to get a non-whole-cell radius.

**Input pins**

- `center: Vec3` — the center in real-space Å (default `(0, 0, 0)`).
- `radius: Float` — the radius in Å (default `5.0`).
- `structure: Structure` (optional) — the lattice the resulting `Blueprint` carries for a downstream `materialize` (default: diamond). The sphere geometry is independent of this structure; it only supplies the lattice that atoms are later placed on.

Because the geometry is authored in real space there is no lattice quantization: the sphere is always perfectly round, even on a non-cubic unit cell (unlike `sphere`, whose radius scales with the `a` lattice vector). The output is a `Blueprint`.

**Alignment.** A `free_sphere` is marked `aligned`, *not* `lattice_unaligned` — a fractionally-positioned cutter does not taint alignment, because atoms are always placed on motif sites during materialization and the cutter merely decides which of them survive. See [Blueprint alignment](../node_networks.md#blueprint-alignment). (This is the deliberate opposite of `free_move`, which taints conservatively because it acts on arbitrary already-built objects.)

Like `sphere`, `free_sphere` has no viewport gadget; edit its properties in the panel.

## half_space

Outputs a half-space (the region on one side of an infinite plane).

![](../../atomCAD_images/half_space_node.png)

![](../../atomCAD_images/half_space_props.png)

![](../../atomCAD_images/half_space_viewport.png)

**Properties**

- `Center` — 3D integer vector; shown as a red sphere in the gadget.
- `Miller Index` — 3D integer vector that defines the plane normal. Enter it manually or pick it from the *earth-like* map. The number of selectable indices on the map is controlled by `Max Miller Index`.
- `Shift` — integer offset along the Miller Index direction. Measured in the smallest lattice increments (each step moves the plane through lattice points).

**Visualization**
The half-space boundary is an infinite plane. In the editor it is shown as a striped grid (even in Solid mode) so you can see its placement; otherwise the whole view would be uniformly filled. After any Boolean operation involving a half-space, the result is rendered normally.

**Gadget controls**

- Drag the light-blue cylinder to change `Shift`.
- Click the red `Center` sphere to show circular discs (one per Miller index) on a selection sphere; drag to a disc and release to choose that Miller index. The number of discs depends on `Max Miller Index`.

![](../../atomCAD_images/half_space_viewport_select_miller_index.png)

**Notes**
Striped rendering is only a visualization aid; it does not affect Boolean results.

## facet_shell

Builds a finite polyhedral **shell** by clipping an infinite lattice with a user‑supplied set of half‑spaces.

> WARNING: **facet_shell** currently only works correctly with cubic unit cells. We intend to add proper generic unit cell support to the **facet_shell** node in the future.

Internally it is implemented as the intersection of a set of half spaces: the reason for having this as a separate
built-in node is a set of convenience features.
Ideal for generating octahedra, dodecahedra, truncated polyhedra, Wulff shapes.

![](../../atomCAD_images/facet_shell_node.png)

![](../../atomCAD_images/facet_shell_props.png)

![](../../atomCAD_images/facet_shell_viewport.png)

This node generally offers the same features as the half_space node, but some additional features are also available:

- clicking on a facet selects it.
- when a facet is selected you can manipulate it the same way as a half space.
- if you turn on the **symmetrize** boolean property for a facet, the facet will be symmetrized using the natural point group symmetry according to the miller index family. Basically a symmetrized facet is replaced with a set of facets according to the following table:

```
Miller family | Num. of planes | Equivalents generated
{100}         | 6              | (±1, 0, 0), (0, ±1, 0), (0, 0, ±1) — the six cube faces
{110}         | 12             | All permutations of (±1, ±1, 0) — normals pointing to the mid‑edges of the cube
{111}         | 8              | All sign combinations of (±1, ±1, ±1) — normals pointing to the eight corners of the cube
{hhl} (h≠l)   | 24             | All permutations of (±h, ±h, ±l) — “mixed” families where two indices are equal, one distinct
General (hkl) | 48             | All permutations of (±h, ±k, ±l) — the full 48‑member orbit under O<sub>h</sub>
```

- The 'Split symmetry members' button creates individual facets from the symmetry variants of a facet.

## union

Computes the Boolean union of any number of 3D blueprints. The `shapes` input accepts an array of `Blueprint` values (array-typed input; you can connect multiple wires and they will be concatenated).


![](../../atomCAD_images/union_node.png)

![](../../atomCAD_images/union_viewport.png)

## intersect

Computes the Boolean intersection of any number of 3D blueprints. The `shapes` input accepts an array of `Blueprint` values.

![](../../atomCAD_images/intersect_node.png)

![](../../atomCAD_images/intersect_viewport.png)

## diff

Computes the Boolean difference of two 3D geometries.

![](../../atomCAD_images/diff_node.png)

![](../../atomCAD_images/diff_viewport.png)

We could have designed this node to have two single `Blueprint` inputs but for convenience reasons (to avoid needing to use too many nodes) both of its input pins accept an array of `Blueprint` values and first a union operation is done on the individual input pins before the diff operation.
The node expression is the following:

```
diff(base, sub) = diff(union(...each base input...), union(...each sub input...))
```

## structure_move

Translates a structure-bound object — a `Blueprint` or a `Crystal` — by a relative vector in **discrete lattice space**. The input pin accepts the abstract `HasStructure` type, and the concrete variant flows through unchanged: a `Blueprint` in produces a `Blueprint` out, a `Crystal` in produces a `Crystal` out. `Molecule` inputs are rejected — use `free_move` for free-space translation.

![](../../atomCAD_images/lattice_move.png)

**Input pins**

- `input: HasStructure` — the object to translate.
- `translation: IVec3` — the translation vector in lattice coordinates.
- `subdivision: Int` (optional) — divides the lattice spacing for finer-than-cell translations. The effective translation is `translation / subdivision`. Setting `subdivision = 1` (the default) gives whole-lattice-vector steps; larger values give fractional steps.

The component-wise divisibility of `translation` by `subdivision` decides whether the result remains lattice-aligned (see [Blueprint alignment](../node_networks.md#blueprint-alignment)). When the translation is not divisible, the output is flagged `lattice_unaligned`.

For a `Blueprint`, only the geometry (the cookie cutter) moves; latent atoms remain anchored to the structure. For a `Crystal`, atoms and geometry move together rigidly.

`structure_move` also exposes a `diff` output pin capturing the atom motion only (the geometry/structure component is not representable in a diff; a `Blueprint` input yields an empty diff) — see [Diff output pins on atom-manipulating nodes](atomic.md#diff-output-pins-on-atom-manipulating-nodes).

You can directly enter the translation vector or drag the axes of the gadget. *Continuous* transformation in lattice space is not supported (use `free_move` for that).

## structure_rot

Rotates a structure-bound object — a `Blueprint` or a `Crystal` — in lattice space. Like `structure_move`, the input pin is `HasStructure` and the concrete variant flows through unchanged. `Molecule` inputs are rejected.

![](../../atomCAD_images/lattice_rot.png)

**Input pins**

- `input: HasStructure` — the object to rotate.
- `axis_index: Int` — index into the input structure's symmetry axes (only rotations that are symmetries of the unit cell are allowed; the node exposes those valid lattice-symmetry rotations).
- `step: Int` — number of *n*-fold rotation steps to apply. For example, with a 4-fold axis, `step = 1` is a 90° rotation, `step = 2` is 180°.
- `pivot_point: IVec3` — pivot in lattice coordinates. Defaults to the origin `(0, 0, 0)`.

Lattice alignment is always preserved by the rotation itself, but the rotation may or may not be a symmetry of the motif (or, with a non-zero `motif_offset`, of the motif placement). When the rotation maps every motif site and bond to itself, the output stays `aligned`; otherwise it is flagged `motif_unaligned`. See [Blueprint alignment](../node_networks.md#blueprint-alignment).

For a `Blueprint`, only the geometry rotates. For a `Crystal`, atoms and geometry rotate together.

`structure_rot` also exposes a `diff` output pin capturing the atom motion only (a `Blueprint` input yields an empty diff) — see [Diff output pins on atom-manipulating nodes](atomic.md#diff-output-pins-on-atom-manipulating-nodes).
