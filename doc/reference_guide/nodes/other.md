# Other nodes

← Back to [Reference Guide hub](../../atomCAD_reference_guide.md)

## lattice_vecs

Produces a `LatticeVecs` value representing the three lattice basis vectors defined by the lattice parameters `(a, b, c, α, β, γ)`.

**Usage**

- `LatticeVecs` values are part of a `Structure` and (together with the motif and motif offset) determine how geometry nodes interpret coordinates.
- The `structure` node exposes a `lattice_vecs` input pin so you can supply a `LatticeVecs` value when constructing or modifying a structure.
- Boolean and other topology operations inherit the structure from their input blueprints. A Boolean operation will error if its inputs carry incompatible lattice vectors.

**Behavior / examples**

- When a non-orthogonal unit cell is used, primitives adapt accordingly — e.g., `cuboid` produces a parallelepiped rather than an axis-aligned box.
- If no lattice vectors are supplied, the defaults (cubic diamond) are used.

**Notes**

- The node automatically detects and displays the crystal system (cubic, tetragonal, orthorhombic, hexagonal, trigonal, monoclinic, triclinic) based on the provided parameters.



![](../../atomCAD_images/unit_cell_node.png)

![](../../atomCAD_images/unit_cell_props.png)

## structure

Constructs or modifies a `Structure` value — the bundle of lattice vectors, motif, and motif offset that defines the infinite crystal field used by every blueprint and crystal. All four input pins are optional.

![TODO(image): the `structure` node selected in the network with its properties panel showing the lattice_vecs, motif, and motif_offset slots](TODO)

**Input pins** (all optional)

- `structure` — base `Structure` to modify. When connected, every other unconnected pin passes through unchanged from the base.
- `lattice_vecs` — overrides the lattice vectors.
- `motif` — overrides the motif.
- `motif_offset` — overrides the fractional motif offset (`Vec3`, each component in `[0, 1]`).

**Output**

A single `Structure` value. When the `structure` input is unconnected and a particular pin is also unconnected, the diamond default is used for that field — so an empty `structure` node is the diamond structure.

**Typical uses**

- Build a `Structure` from scratch: wire `lattice_vecs` and `motif` and leave `structure` unconnected.
- Override one field: wire the upstream `Structure` into `structure` and only the field you want to change. Untouched fields pass through.

A `Structure` value is what the geometry primitives (`cuboid`, `sphere`, `extrude`, …) consume implicitly to interpret their integer coordinates. To swap the structure carried by a `Blueprint` further down the chain, use `with_structure`.

## get_structure

Reads the `Structure` (lattice vectors + motif + motif offset) carried by a `Blueprint` or `Crystal` and emits it as a standalone `Structure` value. Useful when you need to feed the structure of one shape into a `with_structure` node further down the chain without losing the geometry on the original wire.

**Input pin**

- `input: HasStructure` — a `Blueprint` or `Crystal`.

## with_structure

Replaces the `Structure` carried by a `Blueprint` with a different `Structure`, preserving the blueprint's geometry. `Crystal` inputs are not accepted — a crystal's atoms are already materialized against a specific structure, so swapping it out would not be meaningful.

**Input pins**

- `shape: Blueprint` — the blueprint whose structure should be replaced.
- `structure: Structure` — the replacement structure.

If the replacement structure differs from the original in `lattice_vecs` or `motif_offset`, the geometry is no longer registered to integer translations of the lattice and the result is flagged as *lattice-unaligned*. If only the motif differs, the result is flagged as *motif-unaligned*. Both flags propagate downstream so later nodes (and `materialize`) can warn or refuse to operate on misaligned blueprints. See [Blueprint alignment](../../atomCAD_reference_guide.md#blueprint-alignment) for details.

## supercell

Rewrites a `Structure` so that its unit cell is a larger one defined by a 3×3 integer matrix. The physical infinite crystal is unchanged — only the way it is factored into `(cell + motif)` changes. Used to enlarge the working cell before motif-level edits or to model superstructures.

![TODO(image): the `supercell` node selected with its properties panel visible, showing the equation-style rows for new_a / new_b / new_c and the determinant readout](TODO)

**Input pins**

- `structure: Structure` (optional) — defaults to the diamond structure when unconnected.
- `matrix: IMat3` (optional) — when connected, overrides the stored matrix. For the common axis-aligned case (e.g. 2×2×2) wire an `imat3_diag` node.

**Properties**

The properties panel shows the matrix as three equation-style rows, one per new basis vector:

```
new_a = [m00]·a + [m01]·b + [m02]·c
new_b = [m10]·a + [m11]·b + [m12]·c
new_c = [m20]·a + [m21]·b + [m22]·c
```

Each `[n]` is an editable integer field; `a`, `b`, `c` refer to the original basis. Below the rows a live readout shows `det = N` (the volume scaling factor); a determinant of 0 (singular matrix) or a negative determinant (left-handed basis) is highlighted in red, and the node will fail to evaluate.

When the `matrix` input pin is connected, the stored matrix grays out and the readout reflects the wired matrix instead. Disconnecting the pin restores the stored values.

**Behavior**

The new motif contains `|det(matrix)| × old_sites_count` sites — every old cell that fits inside the new cell contributes a tiled copy of the old motif. A common workflow is to feed the supercell output into `motif_edit` to make local modifications (vacancies, substitutions, dopants) inside the larger cell.

## motif

The `motif` node produces a `Motif` value which can be an input to an `atom_fill` node and determines the content which fills the provided geometry.

![](../../atomCAD_images/motif_node.png)

![](../../atomCAD_images/motif_props.png)

The motif is defined textually using atomCAD's motif definition language.

The features of the language are basically parameterized fractional atom sites, explicit & periodic bond definitions.

There are 3 commands in the language for now: `param`, `site` and `bond`

**param**

The `param` command simply defines a *parameter element*. The name of the parameter element needs to be specified followed optionally by the default element name. (If the default element is not provided, it is carbon.) As an example, these are the parameter elements in the cubic zincblende motif:

```
PARAM PRIMARY C
PARAM SECONDARY C
```

Parameter elements are the ones that are replaced by concrete elements which the user chooses in the `atom_fill` node.

**site**

The `site` command defines an atomic site. You need to specify the site id, an element name, (which can be a regular element name like `C` or a parameter element). Then the 3 fractional lattice coordinates need to be specified. (Fractional coordinates are always 0 to 1. The unit cell basis vectors will be used to convert these to real cartesian coordinates.)
These are the sites in the cubic zincblende motif:

```
SITE CORNER PRIMARY 0 0 0

SITE FACE_Z PRIMARY 0.5 0.5 0
SITE FACE_Y PRIMARY 0.5 0 0.5
SITE FACE_X PRIMARY 0 0.5 0.5

SITE INTERIOR1 SECONDARY 0.25 0.25 0.25
SITE INTERIOR2 SECONDARY 0.25 0.75 0.75
SITE INTERIOR3 SECONDARY 0.75 0.25 0.75
SITE INTERIOR4 SECONDARY 0.75 0.75 0.25
```



**bond**

Finally the bond command defines a bond. Its two parameters are *site specifiers*. A site specifier is a site id optionally prefixed by a 3 character relative cell specifier. The relative cell specifier's three characters are for the three lattice directions: '-' means shift backwards in the specific direction, '+' means shift forward, '.' means no shift in the given direction.

It is important that the first site specifier in the bond must always have to have the … (meaning 0,0,0) relative cell specifier (which is the default, so it need not be specified)

These are the bonds in the cubic zincblende motif:

```
BOND INTERIOR1 ...CORNER
BOND INTERIOR1 ...FACE_Z
BOND INTERIOR1 ...FACE_Y
BOND INTERIOR1 ...FACE_X

BOND INTERIOR2 .++CORNER
BOND INTERIOR2 ..+FACE_Z
BOND INTERIOR2 .+.FACE_Y
BOND INTERIOR2 ...FACE_X

BOND INTERIOR3 +.+CORNER
BOND INTERIOR3 ..+FACE_Z
BOND INTERIOR3 ...FACE_Y
BOND INTERIOR3 +..FACE_X

BOND INTERIOR4 ++.CORNER
BOND INTERIOR4 ...FACE_Z
BOND INTERIOR4 .+.FACE_Y
BOND INTERIOR4 +..FACE_X
```

Please note that the format allows empty lines and lines started with the `#` character are treated as comment.

Here is the complete cubic zincblende motif:

```
# cubic zincblende motif

PARAM PRIMARY C
PARAM SECONDARY C

SITE CORNER PRIMARY 0 0 0

SITE FACE_Z PRIMARY 0.5 0.5 0
SITE FACE_Y PRIMARY 0.5 0 0.5
SITE FACE_X PRIMARY 0 0.5 0.5

SITE INTERIOR1 SECONDARY 0.25 0.25 0.25
SITE INTERIOR2 SECONDARY 0.25 0.75 0.75
SITE INTERIOR3 SECONDARY 0.75 0.25 0.75
SITE INTERIOR4 SECONDARY 0.75 0.75 0.25

BOND INTERIOR1 ...CORNER
BOND INTERIOR1 ...FACE_Z
BOND INTERIOR1 ...FACE_Y
BOND INTERIOR1 ...FACE_X

BOND INTERIOR2 .++CORNER
BOND INTERIOR2 ..+FACE_Z
BOND INTERIOR2 .+.FACE_Y
BOND INTERIOR2 ...FACE_X

BOND INTERIOR3 +.+CORNER
BOND INTERIOR3 ..+FACE_Z
BOND INTERIOR3 ...FACE_Y
BOND INTERIOR3 +..FACE_X

BOND INTERIOR4 ++.CORNER
BOND INTERIOR4 ...FACE_Z
BOND INTERIOR4 .+.FACE_Y
BOND INTERIOR4 +..FACE_X
```
