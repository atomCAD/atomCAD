# Node Networks

← Back to [Reference Guide hub](../atomCAD_reference_guide.md)

A **node network** is a collection of nodes. A node may be either a built-in node or a custom node.

## Anatomy of a node

![](../atomCAD_images/node_anatomy.png)

A **node** may have zero or more *named input pins* (also called the node’s *parameters*) on the left side, and one or more *named output pins* on the right side. Most nodes have exactly one output pin (the "result"); a few nodes are **multi-output** — they expose more than one named output pin, each independently connectable and displayable. The clearest example is `atom_edit`, which exposes both a `result` pin (the applied edit) and a `diff` pin (the raw diff structure).

Each node also has one *function pin* in the upper-right corner (the function pin is described in the functional programming section).

Each pin has a data type. Hovering over a pin shows its type; the pin color also indicates the type. A wire may only connect an output pin to an input pin, and the two pins must either have the same data type or the output type must be implicitly convertible to the input type. (We will discuss implicit conversion soon.)

### Output pins, eye icon, and display

Every output pin has its own **eye icon** next to it that toggles whether that pin's value is rendered in the 3D viewport. This is true for single-output and multi-output nodes alike — the eye icon lives on the pin row, not in the title bar. Multiple pins of the same node can be displayed simultaneously, and they are independent of wiring (you can display a pin whether or not it is wired to anything downstream).

For multi-output nodes the pin name (`result`, `diff`, …) is shown next to each output pin. For single-output nodes the pin name is omitted (there is nothing to disambiguate); hovering still shows the name as a tooltip.

When more than one output pin of a node is displayed, only the lowest-indexed displayed pin is **interactive** — i.e. only it receives clicks, hover, and selection from viewport tools. The other displayed pins are visual-only. For example, when both pins of `atom_edit` are displayed, atom selection happens against the `result` pin (with provenance mapping back to the diff under the hood).

## Data types

Supported basic data types include:

- `Bool`
- `String`
- `Int`
- `Float`
- `Vec2` — 2D vector
- `Vec3` — 3D vector
- `IVec2` — 2D integer vector
- `IVec3` — 3D integer vector
- `Mat3` — 3×3 floating-point matrix (row-major)
- `IMat3` — 3×3 integer matrix (row-major)
- `LatticeVecs` — three lattice basis vectors `(a, b, c, α, β, γ)`
- `Structure` — a crystal structure: lattice basis + motif + motif offset
- `Geometry2D`
- `Blueprint` — a 3D **blueprint**: a `Structure` paired with a bounded geometry shape that acts as a "cookie cutter" in the infinite crystal field. Atoms are *latent*: they exist where the cutter overlaps the structure but have not been carved out yet.
- `Crystal` — a materialized atomic structure that *retains* its `Structure` (lattice information). Produced by carving a `Blueprint` (e.g. via `atom_fill` / `materialize`). The atoms and the optional geometry shell move together under any transform.
- `Molecule` — a free-floating atomic structure with **no** `Structure` association. Produced by importing an XYZ file or by stripping the structure off a `Crystal` (`exit_structure`). Can be moved arbitrarily.
- `Motif`

### The three phases

The data types `Blueprint`, `Crystal`, and `Molecule` together form a **three-phase model**: the same designed object passes through these phases as it moves from design through construction to deployment.

| Phase | Has structure | Has atoms | Role |
|---|---|---|---|
| **Blueprint** | yes | no (latent) | *Design.* Position the cookie cutter inside an infinite crystal; design boolean ops, surface cuts, alignment. |
| **Crystal** | yes | yes | *Construction.* Atoms have been carved; the structure association is retained, so structure-aligned operations remain available. |
| **Molecule** | no | yes | *Deployment.* Free-floating atoms, no longer tied to a structure. |

Phase transitions are explicit nodes (`materialize`, `exit_structure`, and their inverses) — see the [Atomic structure nodes](./nodes/atomic.md) reference.

Array types are supported. The type `[Int]` means an array of `Int` values.

Function types are written `A -> B`: a function that takes a parameter of type `A` and returns a value of type `B` has type `A -> B`.

Input pins can be array-typed. An array input pin is visually indicated with a small dot. Node networks provide a convenience that you can connect multiple wires into an array-typed input pin: the connected values will be concatenated into a single array. Also, a value of type `T` is implicitly convertible to an array of `T` (`T` → `[T]`).

### Implicit type conversion rules

- `Int` and `Float` can be implicitly converted to each other in both directions. When converting a `Float` to an `Int` it is rounded to the nearest integer.
- Similarly there is implicit conversion between `IVec2` and `Vec2`, and also between `IVec3` and `Vec3`.
- `IMat3` and `Mat3` are interconvertible (the `Mat3 → IMat3` direction truncates).
- A concrete phase type (`Blueprint`, `Crystal`, `Molecule`) implicitly converts to any abstract type that contains it (see *Abstract types* below).
- If `T` is implicitly convertible to `S` then `T` is also implicitly convertible to `[S]`. Element-wise array conversion `[T] → [S]` follows the same rule.
- An essential feature for higher order functions is this: Function type `F` can be converted to function type `G` if:
  - `F` and `G` have the same return type
  - `F` contains all parameters of `G` as its first parameters. (`F` can have additional parameters)

### Abstract types

Some operations are naturally polymorphic over multiple phase types — e.g. `add_hydrogen` works on any atomic structure (`Crystal` or `Molecule`), while `structure_move` works on anything carrying a structure (`Blueprint` or `Crystal`). To express this without duplicating nodes, atomCAD has three **abstract types**, each naming a "two-out-of-three" combination of the concrete phases:

| Abstract type | Members | Used by |
|---|---|---|
| `HasAtoms` | `Crystal`, `Molecule` | atom operations: `atom_edit`, `apply_diff`, `relax`, `add_hydrogen`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, `atom_union`, `atom_composediff` |
| `HasStructure` | `Blueprint`, `Crystal` | structure-aligned operations: `structure_move`, `structure_rot`, `get_structure` |
| `HasFreeLinOps` | `Blueprint`, `Molecule` | free movement: `free_move`, `free_rot` |

Abstract types appear **only** as input-pin types on built-in polymorphic nodes. Every actual value flowing through a wire is concrete — a `Crystal`, a `Molecule`, a `Blueprint` — never an abstract type. Each concrete type implicitly converts to any abstract type that contains it; there is no implicit conversion in the other direction.

**Type preservation.** When a value flows through a polymorphic node, the *concrete* type is preserved on the output. A `Crystal` fed into `add_hydrogen` comes out as a `Crystal`; a `Molecule` comes out as a `Molecule`. A chain like `Crystal → add_hydrogen → structure_move` therefore stays well-typed end to end — the `structure_move` (which needs `HasStructure`) still accepts the post-`add_hydrogen` result.

### Pin coloring

Pins are colored by their data type:

| Type family | Color |
|---|---|
| `Bool`, `Int`, `Float` | warm orange |
| `Vec2`, `Vec3`, `IVec2`, `IVec3`, `Mat3`, `IMat3` | cool blue |
| `Blueprint`, `Geometry2D` | purple |
| `Crystal`, `Molecule` | green |
| `LatticeVecs`, `Structure`, `Motif` | teal / cyan |
| Function types | amber |

Array pins use the same color as their element type and are marked with a small dot.

**Abstract-type input pins** are rendered as a pie-sliced circle, one slice per concrete type contained in the abstract type, each slice colored with that concrete type's color. So a `HasAtoms` input pin appears solid green (Crystal + Molecule are both green); a `HasStructure` input pin appears half purple (Blueprint) and half green (Crystal); a `HasFreeLinOps` input pin is half purple (Blueprint) and half green (Molecule). Output pins are always concrete and render single-colored. Wires take the color of their source's concrete type.

![TODO(image): a node with HasStructure and HasFreeLinOps input pins shown as pie-sliced circles next to a node with concrete (single-colored) input pins for visual comparison](TODO)

## Node properties vs. input pins

- Most placed node is the node network has associated data. This data consists of properties of the node which are editable in the node properties panel.
- Often a node has both a property and input pin for the same concept. For example the cuboid node has a Min corner property and also has a min_corner input pin. In these cases you can both manually (property) and programmatically (input pin) control this aspect. The input pin always takes precedence.

As an example see the input pins and the properties of the `cuboid` node:

![](../atomCAD_images/cuboid_node.png)

![](../atomCAD_images/cuboid_props.png)

## Subnetworks

You create a custom node by adding a node network whose name matches the custom node’s name — that node network becomes the implementation of the custom node. In other words, node networks act like functions: when node `B` is used inside node network `A`, the network `B` serves as a subnetwork of `A`.

As built-in nodes, custom nodes also can have input pins (a.k.a parameters) and an output pin.

To set up an input pin (parameter) of your custom node you need to use a `parameter` node in your subnetwork.

![](../atomCAD_images/parameter.png)

The above image shows a subnetwork named `cube` which has an integer parameter defined name `size`.

The *sort order* property of a parameter determines the order of the parameters in the resulting custom node.

To make a subnetwork 'return a value' you need to set its *output node*. The output node will supply the output value of the custom node we are defining with our subnetwork. It is similar to a return statement in a programming language. You can set a node as an output node of its node network by right clicking on it and selecting the *Set as return node* menu item. 

![](../atomCAD_images/return_node.png)

Now that we created the `cube` subnetwork when adding a node in a different node network the `cube` custom node will be available: 

![](../atomCAD_images/add_cube.png)

The cube node will have the `Int` typed `size` input pin and a `Blueprint` typed output pin:

![](../atomCAD_images/cube_node.png)

## Functional programming in atomCAD

One of the key nodes to make an atomCAD node network more dynamic is the `expr` node. And `expr` node can represent arbitrary mathematical operations and even supports branching with the `if then else` construct. (See the description of the `expr` node in the nodes reference below.)

To create complex programming logic in atomCAD the expr node is not enough: you need to use nodes which represent higher order functions. Currently only the `map` higher order function node is supported, but we plan to add more (e.g. filter, reduce).

To use a higher order function in any language effectively a language feature to be able to dynamically create a *function value* depending on parameters is needed: in some languages these are *closures*, in other languages it is *partial function application*. In an atomCAD node network is it achieved in a very simple way: as we mentioned at the implicit conversion rules: you can supply a function into a function typed input pin that has extra parameters. These extra parameters are bound at the time the function's real time value is created, and this dynamic function value is supplied to the higher order function. (See the description of the `map` node below where we discuss this with a concrete example.)

Another important node for functional programming is the `range` node which creates an array of integers that can be supplied to nodes like the `map` node.

To see functional programming in atomCAD in action please check out the *Pattern* demo [in the demos document](../../samples/demo_description.md).
