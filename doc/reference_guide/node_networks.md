# Node Networks

← Back to [Reference Guide hub](../atomCAD_reference_guide.md)

A **node network** is a collection of nodes. A node may be either a built-in node or a custom node.

## Anatomy of a node

![](../atomCAD_images/node_anatomy.png)

A **node** may have zero or more *named input pins* (also called the node’s *parameters*) on the left side, and one or more *named output pins* on the right side. Most nodes have exactly one output pin (the "result"); a few nodes are **multi-output** — they expose more than one named output pin, each independently connectable and displayable. The clearest example is `atom_edit`, which exposes both a `result` pin (the applied edit) and a `diff` pin (the raw diff structure).

Most nodes also have one *function pin* in the upper-right corner — a legacy artifact that lets a node be used as a first-class function value. It is suppressed on the higher-order-function nodes themselves (`map`, `filter`, `fold`, `foreach`), which author their per-element computation as an *inline body region* by default and accept a reusable function value through an ordinary `f` *input* pin (see [Function values: closures and the `f` pin](#function-values-closures-and-the-f-pin)) rather than through this legacy corner pin. See the [Higher-order functions](#higher-order-functions-and-inline-bodies) section.

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
- `Crystal` — a materialized atomic structure that *retains* its `Structure` (lattice information). Produced by carving a `Blueprint` (via `materialize`). The atoms and the optional geometry shell move together under any transform.
- `Molecule` — a free-floating atomic structure with **no** `Structure` association. Produced by importing an XYZ file or by stripping the structure off a `Crystal` (`exit_structure`). Can be moved arbitrarily.
- `Motif`
- `Record(Name)` — a user-defined record type bundling a fixed set of named, heterogeneously-typed fields into a single value. Defined from the **User Types** panel and consumed by the `record_construct`, `record_destructure`, and `product` nodes — see [Record types](./nodes/math_programming.md#record-types) for details. Records are structurally subtyped (compatibility is decided by field shape, not by name) with width subtyping (extra fields ride through unchanged).
- `Unit` — the type with exactly one value, used as the return type of *effect nodes* (`export_xyz`, `foreach`, …) — i.e. nodes that exist for their side effect rather than to produce a value. A wire of type `Unit` is never displayable (the eye icon is hidden) and any value can be implicitly *discarded* into `Unit` (the `T → Unit` widening), which is why a body-internal chain ending in `print` (whose output is `String`) can still feed `foreach`'s `Unit`-typed `out` zone-output pin. The reverse — `Unit → T` — is **not** allowed: a unit value carries no information. See the [Execute action](./ui.md#execute-action-side-effect-nodes) for how unit-returning nodes are gated to fire only on demand.

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

Internally, polymorphic output pins are declared with a *same as input* rule that points back at one of the node's input pins (visible in pin tooltips as e.g. `SameAsInput(molecule)`). The editor resolves that rule against the actually-wired source: the output pin then renders with the resolved concrete type's color, and any wire leaving it picks up the same color. If the input is unwired, the output falls back to its declared (possibly abstract) type for coloring purposes.

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
| Record types | neutral grey (single color regardless of def name — visual reflects structural compatibility, not identity) |

Array pins use the same color as their element type and are marked with a small dot.

**Abstract-type input pins** are rendered as a pie-sliced circle, one slice per concrete type contained in the abstract type, each slice colored with that concrete type's color. So a `HasAtoms` input pin appears solid green (Crystal + Molecule are both green); a `HasStructure` input pin appears half purple (Blueprint) and half green (Crystal); a `HasFreeLinOps` input pin is half purple (Blueprint) and half green (Molecule). Output pins are always concrete and render single-colored. Wires take the color of their source's concrete type.

![TODO(image): a node with HasStructure and HasFreeLinOps input pins shown as pie-sliced circles next to a node with concrete (single-colored) input pins for visual comparison](TODO)

## Blueprint alignment

A `Blueprint` (or a `Crystal` derived from one) is meaningful only insofar as its geometry is registered to the infinite crystal field of its `Structure`. Some operations break that registration — for example, `free_move` on a Blueprint translates the cookie cutter without moving the underlying lattice, and a `structure_rot` around an axis that is not a motif symmetry rotates atoms onto sites where the motif no longer maps to itself. Boolean CSG (`union`, `intersect`, `diff`), `materialize`, and `atom_edit` all assume their inputs share a common lattice registration; combining mis-registered values silently produces garbage atoms.

atomCAD does **not** prevent these operations — they are useful for strained structures, defect studies, or carrying a molecule as a pseudo-Blueprint — but it surfaces the risk so you can see it in the editor. Every `Blueprint` and `Crystal` value carries an **alignment** flag with three levels:

| Alignment | Meaning |
|---|---|
| `aligned` | Lattice and motif registration both preserved. Safe to combine with other `aligned` values of the same `Structure`. |
| `motif-unaligned` | Lattice translational symmetry still holds, but the motif may not map to itself under the applied operations. Boolean combinations with other values are still safe *as long as the atoms are not yet materialized*; after materialization the atoms may not all sit on motif sites. |
| `lattice-unaligned` | The value is no longer registered to any integer translation of the structure's lattice. This is a superset — anything lattice-unaligned is motif-unaligned by construction. |

### How alignment propagates

Alignment is a *derived* property — every node computes it from its inputs and operation, so values that flow through the network always carry an up-to-date flag. The propagation rules:

| Operation | Alignment effect |
|---|---|
| Construction (any shape primitive, `import_cif`, `materialize`'s output) | `aligned` |
| `structure_move`, when each `translation` component is divisible by `subdivision` | pass-through |
| `structure_move`, when components are not divisible | promotes to at least `lattice-unaligned` |
| `structure_rot`, when the rotation is also a motif symmetry | pass-through |
| `structure_rot`, when the rotation is not a motif symmetry | promotes to at least `motif-unaligned` |
| `free_move`, `free_rot` | promotes to at least `lattice-unaligned` |
| `union`, `intersect`, `diff`, `atom_union` | the most-degraded input wins (max over inputs) |
| `materialize`, `dematerialize` | pass-through |
| `exit_structure` | dropped (Molecules have no alignment) |
| `enter_structure` | always `lattice-unaligned` (atoms may not lie on motif sites) |
| `atom_edit` and other atomic ops (`relax`, `add_hydrogen`, `atom_replace`, …) | pass-through |

The `subdivision` parameter on geometry primitives (`half_space`, `extrude`, `drawing_plane`, `half_plane`, `facet_shell`) does **not** affect alignment — it controls where the cut sits, not where atoms end up. Only `structure_move`'s `subdivision` can break lattice alignment, because there it subdivides a translation.

Some nodes record a short *reason* string when they degrade alignment (e.g. *"non-motif rotation"*, *"fractional translation by (1, 0, 0)/2"*); the reason appears in the pin tooltip below the alignment line.

### Visual indicators in the editor

- **Wire dashes.** Wires carrying a value with `motif-unaligned` alignment are drawn with **long dashes**; wires carrying `lattice-unaligned` values are drawn with **short dashes**. Aligned wires (and wires of types that have no alignment, e.g. `Int` or `Motif`) are solid. The mnemonic: more broken up = more broken.
- **Output pin shape.** An output pin whose value is `motif-unaligned` or `lattice-unaligned` is rendered as a **filled warning triangle** instead of the usual filled circle. The triangle keeps the data-type color of the would-be circle, so the type-color channel is preserved. The two unaligned states share one shape — the wire dash style distinguishes them.
- **Pin tooltip.** Hovering an unaligned output pin adds a colored *Alignment: motif-unaligned* or *Alignment: lattice-unaligned* line to the tooltip, optionally followed by the reason string in parentheses.

These indicators are **information, not warnings** — workflows that deliberately want unaligned blueprints (e.g. strained-layer heterostructures, defect dynamics) are perfectly valid. The dashes and triangles tell you *why* a downstream consumer might misbehave when it expects aligned inputs.

![TODO(image): a small node network with one solid wire, one long-dashed wire, and one short-dashed wire feeding into a `union` node, with the union output pin rendered as a warning triangle and a tooltip showing "Alignment: lattice-unaligned"](TODO)

## Node properties vs. input pins

- Most placed node is the node network has associated data. This data consists of properties of the node which are editable in the node properties panel.
- Often a node has both a property and input pin for the same concept. For example the cuboid node has a Min corner property and also has a min_corner input pin. In these cases you can both manually (property) and programmatically (input pin) control this aspect. The input pin always takes precedence.
- Custom nodes follow the same model: their auto-generated property panel edits a per-parameter value, and a wired parameter pin overrides the value set inline. See [Subnetworks](#subnetworks).

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

### Editing custom node parameters

When you select a custom node instance, the **Node Properties** panel auto-generates an editor with one field per parameter pin whose type is a simple editable type (`Bool`, `Int`, `Float`, `String`, the `Vec`/`IVec` vector types, and the `Mat3`/`IMat3` matrices). You can set a value for each such parameter inline. Parameters of other types (`Blueprint`, `Crystal`, arrays, records, …) stay wire-only and do not appear in the panel.

As with built-in nodes, a value wired into a parameter pin takes precedence over the value set inline (see [Node properties vs. input pins](#node-properties-vs-input-pins)). A parameter that is neither wired nor set inline falls back to the `default` input pin of its `parameter` node inside the subnetwork.

## Higher-order functions and inline bodies

One of the key nodes to make an atomCAD node network more dynamic is the `expr` node. The `expr` node can represent arbitrary mathematical operations and even supports branching with the `if then else` construct. (See the description of the `expr` node in the nodes reference.)

To go beyond a single expression and write **per-element computations** that run across a stream of values, atomCAD provides four **higher-order function** nodes — `map`, `filter`, `fold`, and `foreach`. Each one takes an input stream (`xs: Iter[T]`) and applies the same per-element computation to every element.

The default way you supply that per-element computation is the inline-body model:

- Each higher-order-function node carries an **inline body region** inside the node — a small editable canvas of its own. You add nodes and wires *inside* the HOF the same way you do at the top level.
- The body region has **zone-input pins** on its inner-left edge (sources that supply per-iteration values to the body — `element`, `acc`) and a **zone-output pin** on its inner-right edge (the body's per-iteration return value — `result`, `new_acc`, `out`).
- Wires from outside the body into a body-internal pin are **captures** — they carry an outer-scope value into the per-iteration evaluation. Captures are how you parameterize a body without pre-binding function arguments: drag a wire from any outer-scope output straight into a body node's input.

Concretely, a `map` body that doubles each element looks like one `expr` node inside the body, with a parameter `x: Int` wired from the body's `element` source pin and `2 * x` wired into the body's `result` destination pin.

```
┌── map ──────────────────────────────────┐
│ xs●──┐                          ┌── ●   │  ← Iter[Int] in, Iter[Int] out
│      │                          │       │
│      ▼                          ▲       │
│   ┌─────────────────────────────┐       │
│   │ element●─→ [ 2 * x ] ●→ result      │   ← body
│   └─────────────────────────────┘       │
└─────────────────────────────────────────┘
```

To parameterize the body — say, a `gap` value that the body uses — drop a node in the outer scope (e.g. an `int` or `float` literal) and drag a capture wire from it into a body-internal pin.

The four HOF nodes differ in which zone-input pins they expose, what type the zone-output expects, and how they consume the per-iteration output. See the [HOF nodes reference section](./nodes/math_programming.md#higher-order-function-nodes-map-filter-fold-foreach) for details and the full list of pins.

The `range` node produces a stream of integers (`Iter[Int]`) — the typical input to `map` / `filter` / `fold` / `foreach`. Other stream sources include `product` (cartesian product of N input streams as a record-typed stream) and any `Array[T]` value (which auto-converts to `Iter[T]` at wire time).

### Nested HOFs

HOFs can be nested: a `map` placed inside another `map`'s body renders its own inline body region; a wire from the outermost scope into the innermost body is a capture that crosses two body boundaries. Each crossing is marked with a small dot on the wire. There is no fixed nesting limit — depth 2 or 3 is typical.

### The active body

Keyboard shortcuts (Delete, Ctrl+C / X / V / D) operate on whichever body you most recently clicked into — the **active body**. Clicking on the top-level canvas (outside any HOF) makes the top level active again. Each body has its own selection set; selection in one body doesn't affect another.

### Body sizing

Body regions grow automatically to fit their content and can be dragged larger from the bottom-right corner handle. The stored size is the minimum size; live content additions and node drags grow the body in real time. Bodies don't shrink below their content.

### Function values: closures and the `f` pin

The inline body is the *default* way to author an HOF's per-element computation, but on its own it fuses that computation to a single call site. To **reuse** one computation across several HOFs — or to compute and pass around a function — atomCAD provides function values, built with the `closure` node and consumed through an HOF's optional `f` pin or the `apply` node:

- A **`closure`** node owns an inline body exactly like an HOF, but instead of consuming the body inline it exposes it on a `Function`-typed output pin (rendered amber). You pick its shape — a *kind*: the four HOF body shapes (`(T) -> U`, `(T) -> Bool`, `(A, T) -> A`, `(T) -> Unit`), or **`Custom`** for an arbitrary parameter list (including 0 parameters, a thunk) with user-chosen names and types — from the Node Properties panel.
- Each HOF has an optional **`f` input pin**. Wire a `closure` output of the matching shape into it and that function drives the HOF instead of its own inline body — the body is hidden in the editor while `f` is connected, and reappears when you disconnect it. `map.f` additionally accepts higher-arity functions via auto-partialization (its extra parameters ride along in the output stream's element type).
- The **`apply`** node calls a function value either to completion or partially — wire all the function's argument pins for a full call, or leave some unwired to receive a new function value with the remainder still to fill. Argument pins materialize from the wired `f`'s signature; no shape is set up-front. This is what makes a `Function` a genuinely callable value — for example, calling a function-factory subnetwork's `Function` output, or chaining several `apply` nodes to supply arguments one at a time.

A subnetwork can also *return* a `closure` as its `Function` output, giving you a **function factory**: a subnetwork whose result is a function configured by its inputs.

See the [`closure`](./nodes/math_programming.md#closure), [`apply`](./nodes/math_programming.md#apply), and [Function values and closures](./nodes/math_programming.md#function-values-and-closures) sections of the nodes reference for the full details.

To see higher-order functions in atomCAD in action please check out the *Pattern* demo [in the demos document](../../samples/demo_description.md).
