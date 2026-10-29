# atomCAD Data Types Reference

Complete documentation of atomCAD's type system. Type names are written exactly
as `atomcad-cli describe` prints them and as type-valued properties
(`parameter.data_type`, `map.input_type`, …) accept them.

## Primitive Types

### Bool
Boolean value: `true` or `false`.

### String
Text string, written with double quotes: `"hello world"`, or triple quotes for
multi-line text: `"""line 1\nline 2"""`.

### Int
32-bit signed integer: `42`, `-10`, `0`.

### Float
64-bit floating point. A literal with a decimal point or an exponent is a
Float: `3.14`, `-1.5`, `1.0`, `1e-3`, `2.5E+2`. The integer part is required —
`.5` is a parse error; write `0.5`.

### Unit
The type with exactly one value, `()`. It is the output of effect nodes
(`export_atoms`, `export_build_script`, `foreach`), which do their work only
when the user runs right-click → Execute. Any type converts to `Unit`; nothing
converts from it.

## Vector and Matrix Types

### Vec2, Vec3
2D / 3D floating-point vectors. Components: `x`, `y` (, `z`).

### IVec2, IVec3
2D / 3D integer vectors. Components: `x`, `y` (, `z`). Used for lattice
coordinates and Miller indices.

**Vector literals** are tuples: `(1, 2)`, `(1.5, 2.0, -3.0)`. A tuple whose
components are all integers is an `IVec`; one with any float component is a
`Vec`. The literal is then converted to the pin's type, so `(1, 2, 3)` works on
a `Vec3` pin, and `(1.7, 2.0, 3.0)` on an `IVec3` pin truncates to `(1, 2, 3)`.
Two or three components only.

The vector constructor nodes take separate components instead:
`vec3 { x: 1.5, y: 2.5, z: 3.5 }`, `ivec3 { x: 1, y: 2, z: 3 }`.

### IMat2, IMat3, Mat3
2x2 integer, 3x3 integer and 3x3 float matrices. Literals are nested tuples,
**row-major**: `((1, 0, 0), (0, 1, 0), (0, 0, 1))`. A 3x3 literal with all
integer entries is an `IMat3`, otherwise a `Mat3`; a 2x2 literal is always an
`IMat2` (there is no float 2x2 type).

Created by: `imat2_rows`, `imat2_cols`, `imat2_diag`, `imat3_rows`,
`imat3_cols`, `imat3_diag`, `mat3_rows`, `mat3_cols`, `mat3_diag`.

Used by: `supercell.matrix` (`IMat3`), `plane_tiling_vectors.superlattice`
(`IMat2`).

## Lattice Types

### LatticeVecs
The three lattice (unit cell) basis vectors.

Created by: `lattice_vecs` (from `cell_length_a/b/c` and
`cell_angle_alpha/beta/gamma`, in Å and degrees; default cubic diamond,
a = 3.567 Å, all angles 90°), `import_cif`, `structure_unpack`.

Used by: `structure.lattice_vecs`, `motif_edit.unit_cell`,
`lattice_vecs_params`, `lattice_vecs_unpack`.

### Motif
The atom sites and bonds repeated in every unit cell, with optional parameter
elements that can be substituted later.

Created by: `motif` (from its `definition` text, below), `motif_edit`,
`motif_sub`.

Used by: `structure.motif`, `motif_sub.motif`.

### Structure
A complete crystal description: `LatticeVecs` + `Motif` + a motif offset.

Created by: `structure`, `get_structure` (reads it off a Blueprint or Crystal),
`supercell`.

Used by: the `structure` input of the lattice geometry primitives (`cuboid`,
`sphere`, `free_sphere`, `half_space`, `facet_shell`, `extrude`, `empty`) and of
`drawing_plane`, plus `enter_structure`, `with_structure`, `structure_unpack`,
`supercell`. Wherever the `structure` input is optional and unconnected, it
defaults to cubic diamond.

### DrawingPlane
A Miller-indexed plane of the lattice on which 2D geometry is drawn.

Created by: `drawing_plane`.

Used by: the `d_plane` input of the 2D primitives (`rect`, `circle`,
`free_circle`, `polygon`, `reg_poly`, `half_plane`, `empty_2d`) and
`plane_tiling_vectors.plane`.

### Motif Definition Language

The `motif` node's `definition` property:

```
# Parameter elements: PARAM <name> [<default element>]  (default element: C)
PARAM PRIMARY Si
PARAM SECONDARY

# Sites: SITE <id> <element or parameter> <frac_x> <frac_y> <frac_z>
SITE CORNER PRIMARY 0 0 0
SITE FACE_X PRIMARY 0 0.5 0.5
SITE INTERIOR1 SECONDARY 0.25 0.25 0.25
SITE INTERIOR2 SECONDARY 0.25 0.75 0.75

# Bonds: BOND <site1> [<cell prefix>]<site2> [<multiplicity>]
BOND INTERIOR1 ...CORNER
BOND INTERIOR1 FACE_X
BOND INTERIOR2 .++CORNER
BOND INTERIOR2 ...FACE_X
```

- Commands are case-insensitive (`PARAM` / `param`).
- Comments are **whole lines** starting with `#`. A comment after a command on
  the same line is an error (its words are read as extra arguments).
- Site coordinates are fractional, relative to the unit cell.
- The cell prefix is three characters for the x, y, z cell offsets of the
  second site: `.` = same cell, `+` = next cell, `-` = previous cell. Omitting
  it means `...`. The first site of a bond must be in the current cell.
- The optional multiplicity is a positive integer (default 1).
- Site and parameter ids may contain letters, digits and `_`.

Parameter elements are overridden downstream with a
`parameter_element_value_definition` string — one `NAME Element` pair per line,
e.g. `"PRIMARY Si\nSECONDARY C"` — on `materialize` or `motif_sub`.

## Geometry and Atomic Types

Objects flow through three **phases**:

| Phase | Contains | Role |
|-------|----------|------|
| `Blueprint` | Structure + geometry | Design: a geometry "cookie cutter" positioned in an infinite crystal |
| `Crystal` | Structure + atoms (+ geometry) | Atoms carved out of the structure, still tied to the lattice |
| `Molecule` | Atoms (+ geometry) | Free-floating atoms, no structure |

### Geometry2D
2D shape on a drawing plane. Input to `extrude` to make 3D geometry.

Created by: `rect`, `circle`, `free_circle`, `polygon`, `reg_poly`,
`half_plane`, `empty_2d`, `union_2d`, `intersect_2d`, `diff_2d`.

### Blueprint
3D geometry (a signed distance field) together with the `Structure` it will be
filled from. The lattice primitives take integer lattice coordinates (`sphere`:
`center: IVec3`, `radius: Int`); `free_sphere` takes Cartesian Å
(`center: Vec3`, `radius: Float`).

Created by: `cuboid`, `sphere`, `free_sphere`, `half_space`, `facet_shell`,
`extrude`, `empty`, `union`, `intersect`, `diff`, `with_structure`,
`dematerialize`.

### Crystal
Atoms carved out of a Blueprint, still associated with its structure.

Created by: `materialize` (Blueprint → Crystal), `enter_structure`
(Molecule + Structure → Crystal), `patch_latticefill`.

### Molecule
Atoms with no structure association.

Created by: `exit_structure` (Crystal → Molecule), `import_xyz`.

### Abstract phase types: HasAtoms, HasStructure, HasFreeLinOps
Input-pin types that accept more than one phase:

| Type | Accepts | Used by (examples) |
|------|---------|--------------------|
| `HasAtoms` | Crystal, Molecule | `atom_edit`, `relax`, `passivate`, `atom_union`, `export_atoms` |
| `HasStructure` | Blueprint, Crystal | `structure_move`, `structure_rot`, `structure_invert`, `get_structure` |
| `HasFreeLinOps` | Blueprint, Molecule | `free_move`, `free_rot` |

A node that takes an abstract type and passes its input through returns the
concrete phase it was given: a Crystal into `atom_edit` comes out a Crystal, a
Molecule comes out a Molecule. That is why `describe` shows such outputs as
`dynamic`. Runtime values are always a concrete phase.

### ScalarField
A scalar function of 3D space (e.g. an orbital or a density).

Created by: `import_cube`. Used by: `isosurface`, `sample_field`.

### Isosurface
A surface extracted from a `ScalarField` at a level, with its colouring.
Created by: `isosurface`; it is displayed, not consumed by other nodes.

### OpLibrary
A parsed mechanosynthesis operation library. Created by: `ops_library`. Used
by: `mechanosynth.ops`, `mechanosynth_edit.ops`.

## Compound Types

### Array Types `[T]`

An ordered collection of values of type `T`, e.g. `[Int]`, `[Blueprint]`,
`[[Int]]`.

**Array input pins** (e.g. `union.shapes`):
- Accept multiple wire connections; the values are concatenated in wire order.
- A single value `T` converts to `[T]`.
- Are **wire-only**: a list on an array pin names the wires,
  `shapes: [sphere1, box1]`; it cannot hold literal values. To write literal
  elements, use an `array` node (`array { element_type: Int, elements: [1, 2, 3] }`)
  and wire it; to build an array from other nodes' outputs, use `sequence`.

### Iterator Types `Iter[T]`

A lazily evaluated stream of `T`. Produced by `range`, `map`, `filter` and
`zip_with`. An array or a single value converts to an iterator, but an iterator
does **not** convert to an array: wire it through `collect` to get a `[T]`
(set `collect`'s `element_type` to `T`; it defaults to `Int`).

### Function Types `A -> B`

A function value. Spellings: `Int -> Blueprint` (one parameter),
`(Int, Float) -> Blueprint` (several), `() -> Float` (none). `=>` is also
accepted in place of `->` after a parameter list.

Function values come from a `closure` node or from a node's **function pin**,
referenced with `@`: `f: @sp` passes the node `sp` as a function. The node's
**unwired** inputs become the function's parameters, in pin order; wired inputs
are baked in. Per-pin overrides use `pin_roles` (`delayed` = always a
parameter, `supplied` = always baked in, using the stored value if unwired):

```
r  = range { start: 1, count: 3 }
c  = ivec3 { x: 0, y: 0, z: 0 }
sp = sphere { center: c, pin_roles: { structure: supplied } }   # (Int) -> Blueprint
m  = map { input_type: Int, output_type: Blueprint, xs: r, f: @sp }
```

Function pins on higher-order nodes accept any function whose parameter list
**starts with** the given types, shown as `Function(Int,*)` (`map.f` requires
the element type first); `Function*` (`apply.f`) accepts any function.

### Record Types

Structurally typed records with named fields:
- Named: `Record(Point)` — refers to a record type defined in the project
  (always written with `Record(...)`; a bare `Point` is read as a node name).
- Anonymous: `{x: Int, y: Float}`.

Two record types are compatible by their fields, not their names: a record with
extra fields is accepted where fewer are required. A field's type must match
exactly or be a phase type widening to its abstract type (no `Int` → `Float`
inside records). `Optional[T]` appears only in record field declarations,
marking a field that may be absent; it is never a pin type.

## Implicit Type Conversions

Conversions happen automatically on wires.

### Value Conversions

| From | To | Behavior |
|------|----|----------|
| `Int` | `Float` | Exact |
| `Float` | `Int` | Rounds to nearest |
| `IVec2` / `IVec3` | `Vec2` / `Vec3` | Component-wise, exact |
| `Vec2` / `Vec3` | `IVec2` / `IVec3` | Component-wise, rounds to nearest |
| `IMat3` | `Mat3` | Exact |
| `Mat3` | `IMat3` | Truncates each entry toward zero |
| `LatticeVecs` | `DrawingPlane` | The (001) plane through the origin (legacy) |
| `() -> T` | `T` | Calls the zero-parameter function (not inside arrays) |

There is no `IMat2` conversion, no `Bool` ↔ `Int` conversion, and no
`IVec3` → `IMat3` conversion (use `imat3_diag`).

**Literals are converted differently from wires:** a float literal written for
an `Int` or `IVec` property is **truncated**, not rounded
(`int { value: 2.7 }` stores `2`).

### Phase Conversions

`Crystal` → `HasAtoms`, `HasStructure`; `Molecule` → `HasAtoms`,
`HasFreeLinOps`; `Blueprint` → `HasStructure`, `HasFreeLinOps`. Never from an
abstract type back to a concrete one, and never between two abstract types.

### Collection Conversions

| From | To | Behavior |
|------|----|----------|
| `T` | `[T]` | Wraps the value in a one-element array |
| `[S]` | `[T]` | Element-wise (if `S` converts to `T`) |
| `T` | `Iter[T]` | One-element stream |
| `[S]` | `Iter[T]` | Streams the array's elements |
| `Iter[S]` | `Iter[T]` | Element-wise, lazily |
| any | `Unit` | Discards the value |

### Function Conversions

A function converts to another function type only with the **same number of
parameters**, each parameter and the return type convertible. There is no
implicit partial application: to fix some arguments, wire those pins of the
node whose function pin you pass (see Function Types), or use a `closure` or
`apply` node.

## Pin Compatibility

A wire can connect an output of type `T` to an input of type `S` if `T` equals
`S` or `T` converts to `S` implicitly. The GUI refuses to draw an incompatible
wire. A text-format edit still makes it, but validation reports a
`Data type mismatch` error on the node and the edit returns `success: false`.

## Literal Syntax in the Text Format

| Literal | Read as |
|---------|---------|
| `42`, `-10` | `Int` |
| `3.14`, `1e-3` | `Float` |
| `true` / `false` | `Bool` |
| `"text"`, `"""multi-line"""` | `String` |
| `(1, 2)` / `(1, 2, 3)` | `IVec2` / `IVec3` if all components are integers, else `Vec2` / `Vec3` |
| `((1, 0), (0, 1))` | `IMat2` |
| `((1,0,0), (0,1,0), (0,0,1))` | `IMat3` if all integers, else `Mat3` |
| `[a, b]` | A list: wires on an array pin, values on a list property |
| `node_id` | A wire from that node's primary output (pin 0) |
| `node_id.pin_name` | A wire from that node's named output pin |
| `@node_id` | A wire from that node's function pin |

A literal is converted to the property's declared type (Int → Float,
IVec → Vec, and the truncating Float → Int / Vec → IVec described above).

## Type Discovery

Use the CLI to discover types for any node:

```bash
atomcad-cli describe sphere

# Node: sphere
# Category: Geometry3D
# Description: Outputs the lattice image of a sphere: integer center and radius in lattice cells; an ellipsoid on non-cubic cells.
#
# Inputs:
#   center    : IVec3      [default: (0, 0, 0)]
#   radius    : Int        [default: 1]
#   structure : Structure  [default: diamond, wire-only]
#
# Output: Blueprint
```

`Output:` shows the primary output (pin 0) only. `dynamic` means the type
depends on the node's settings or on what is wired into it.

**Input markers in describe output:**
- `[required]` — Has no default; the node reports an error until it is provided
- `[default: value]` — Has a default; optional to specify
- `[has default]` — Has a default that `describe` cannot print; optional
- `[wire-only]` — Can only be connected via a wire (no text literal for it)
- `[literal-only]` — Can only be set as a text literal (no input pin)
