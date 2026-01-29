# atomCAD Data Types Reference

Complete documentation of atomCAD's type system.

## Primitive Types

### Bool
Boolean value: `true` or `false`.

### String
Text string, written with double quotes: `"hello world"`.

### Int
32-bit signed integer: `42`, `-10`, `0`.

### Float
64-bit floating point: `3.14`, `-1.5`, `1e-3`, `.5`.

## Vector Types

### Vec2
2D floating-point vector. Components: `x`, `y`.

Text format: `(1.0, 2.0)` or `(1, 2)` (integers auto-convert).

### Vec3
3D floating-point vector. Components: `x`, `y`, `z`.

Text format: `(1.0, 2.0, 3.0)`.

### IVec2
2D integer vector. Components: `x`, `y`.

Text format: `(1, 2)`.

Used for: 2D lattice coordinates.

### IVec3
3D integer vector. Components: `x`, `y`, `z`.

Text format: `(1, 2, 3)`.

Used for: 3D lattice coordinates, Miller indices.

## Domain Types

### Geometry2D
2D shape defined on the XY plane. Used as input to `extrude` to create 3D geometry.

Created by: `rect`, `circle`, `polygon`, `reg_poly`, `half_plane`, `union_2d`, `intersect_2d`, `diff_2d`.

### Geometry
3D geometry represented as a Signed Distance Field (SDF). Coordinates are in lattice units (integers), enabling lattice-aligned atomic structures.

Created by: `cuboid`, `sphere`, `half_space`, `facet_shell`, `extrude`, `union`, `intersect`, `diff`, `lattice_move`, `lattice_rot`.

### Atomic
Atomic structure containing atoms and bonds. The final output type for molecular designs.

Created by: `atom_fill`, `atom_trans`, `edit_atom`, `import_xyz`.

### UnitCell
Crystal lattice parameters defining the unit cell: `(a, b, c, alpha, beta, gamma)`.

- `a`, `b`, `c`: Lattice vector lengths in angstroms
- `alpha`, `beta`, `gamma`: Angles between lattice vectors in degrees

Created by: `unit_cell` node.

Geometry nodes accept a `unit_cell` input to specify the lattice. If not provided, cubic diamond is used.

### Motif
Crystal motif definition specifying atomic sites and bonds within a unit cell.

Created by: `motif` node with motif definition language.

Used by: `atom_fill` to populate geometry with atoms.

**Motif Definition Language:**

```
# Define parameter elements (can be overridden in atom_fill)
PARAM PRIMARY C
PARAM SECONDARY C

# Define atomic sites: SITE <id> <element> <frac_x> <frac_y> <frac_z>
SITE CORNER PRIMARY 0 0 0
SITE FACE_X PRIMARY 0 0.5 0.5
SITE INTERIOR1 SECONDARY 0.25 0.25 0.25

# Define bonds: BOND <site1> <relative_cell_prefix><site2>
# Prefix: 3 chars for (x,y,z) direction: '.' = same cell, '+' = next, '-' = previous
BOND INTERIOR1 ...CORNER      # same cell
BOND INTERIOR2 .++CORNER      # y+1, z+1 cell
BOND INTERIOR3 +..FACE_X      # x+1 cell
```

- Comments: lines starting with `#`
- Fractional coordinates: 0 to 1, relative to unit cell
- First site in BOND must be in current cell (prefix `...` or omitted)

## Compound Types

### Array Types `[T]`

An ordered collection of values of type `T`.

Examples:
- `[Int]` - array of integers
- `[Geometry]` - array of 3D geometries
- `[[Int]]` - array of integer arrays

Text format: `[value1, value2, value3]`

**Array Input Pins:**
- Visually marked with a small dot
- Accept multiple wire connections (values concatenated)
- Single value `T` auto-converts to `[T]`

### Function Types `A -> B`

A function taking parameter of type `A` and returning type `B`.

Examples:
- `Int -> Geometry` - function from integer to geometry
- `Float -> Float` - function from float to float

**Usage with higher-order functions:**

The `map` node applies a function to each element of an array:
```
map { xs: [1, 2, 3], f: my_transform }
```
Where `my_transform` is a custom node of type `Int -> Geometry`.

## Implicit Type Conversions

atomCAD automatically converts types when safe and unambiguous.

### Numeric Conversions

| From | To | Behavior |
|------|----|----------|
| `Int` | `Float` | Exact conversion |
| `Float` | `Int` | Rounds to nearest integer |
| `IVec2` | `Vec2` | Component-wise conversion |
| `Vec2` | `IVec2` | Component-wise rounding |
| `IVec3` | `Vec3` | Component-wise conversion |
| `Vec3` | `IVec3` | Component-wise rounding |

### Array Conversions

| From | To | Behavior |
|------|----|----------|
| `T` | `[T]` | Wraps single value in array |
| `[T]` | `[S]` | Element-wise conversion (if `T` converts to `S`) |

### Function Conversions (Partial Application)

A function with extra parameters can convert to a function with fewer parameters:

If `F` has parameters `(a, b, c)` and `G` needs only `(a)`:
- `F` can be used where `G` is expected
- Parameters `b` and `c` are bound at conversion time

This enables partial application:
```
# pattern has inputs: index (Int), gap (Int)
# map expects: Int -> Geometry
# gap is bound when wired to map's f input
map { xs: range1, f: pattern }
```

## Pin Compatibility

A wire can connect output pin of type `T` to input pin of type `S` if:
1. `T` equals `S`, or
2. `T` is implicitly convertible to `S`

**Wire validation happens at edit time** - incompatible connections are rejected.

## Type Inference

The text format infers types from literal values:

| Literal | Inferred Type |
|---------|---------------|
| `42` | `Int` |
| `3.14` | `Float` |
| `true`/`false` | `Bool` |
| `"text"` | `String` |
| `(1, 2)` | `IVec2` (if integers) or `Vec2` (if floats) |
| `(1, 2, 3)` | `IVec3` (if integers) or `Vec3` (if floats) |
| `[...]` | Array of element types |
| `node_id` | Output type of referenced node |

## Type Discovery

Use the CLI to discover types for any node:

```bash
# See inputs and output type
atomcad-cli describe sphere

# Output:
# Node: sphere
# Category: Geometry3D
# Description: Creates a sphere...
#
# Inputs:
#   center    : IVec3     [default: (0, 0, 0)]
#   radius    : Int       [default: 1]
#   unit_cell : UnitCell  [default: cubic diamond, wire-only]
#
# Output: Geometry
```

**Input markers in describe output:**
- `[required]` — Must be provided (wired or as literal)
- `[default: value]` — Has a default; optional to specify
- `[wire-only]` — Can only be connected via wire (no text literal for this type)
- `[literal-only]` — Can only be set as a text literal (no input pin)
