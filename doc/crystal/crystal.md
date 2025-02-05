# atomCAD Model Representation and Kernel Architecture 

## Design philosophy

- We would like to use non-destructive editing where possible
- We would like to enable structure reuse as much as possible. The user should be able to distill useful parts of the design into a reusable representation. In the long run the possibility of an ecosystem of part libraries should be achievable.
- Balance power and simple UX as much as possible. (This is very hard, some tradeoff is needed here.)  

- We want the user to be able to create geometry separately and create an atomic representation from a geometry separately in a non-destructive way.
- The most important way users create geometry is CSG (constructive Solid Geometry). 
- We would like to enable the user to do as much as possible by creating geometry and creating atoms from the geometry in a non-destructive way, but sometimes editing atom by atom is unavoidable and we would like to support this as well.

## Model representation

I created a separate, little more theoretical document about the model representation:  [Shape Algebra](./shapre_algebra.md) Another document with a little different focus, and even more mathematically precise:  [Math](./math.md)

## Node networks

I find that a very elegant way to implement a non-destructive editing system is to support a node network. Lots of successful non-destructive editing software use node networks. Some examples:

- Houdini (the whole system is based on different node networks)

- Unreal engine (Material Editor, Niagara, Blueprint Scripting)
- Blender geometry nodes, shader editor, compositor
- Davinci resolve

To get a feel on how a node network works in a geometry editing context you can watch into this video:

https://www.youtube.com/watch?v=2oC9TOgQ3KE

The 'Introduction to Houdini' page is also a good start:

https://www.sidefx.com/docs/houdini/basics/intro.html 

## Node network in atomCAD

A **node** can have any number of (including zero) parameters. These can be also called **input pins**. A node has exactly one **output pin**.

Each input and the output pin of the node has a **data type**. You can connect an output pin with an input pin with a directed edge if their type match. We call these directed edges, which always go from an output pin to an input pin a **wire**. The network should be a DAG: should not contain a circle.

Currently we develop an MVP, in which there are only 2 data types:

- **geometry** (2D shape)
- **atomic**: atoms and bonds

Additionally nodes can have any amount of node data. Node data can be filled only on the UI. Node data usually consist of so called node **properties**. There are properties in the node data that may be promoted to input pins (a.k.a parameters) in later versions when we might plan more procedural nodes that can be utilized to fill these pins.

On the User interface there is always a **displayed node**. The output of that node is displayed in the editor viewport. The displayed node is not necessarily the same as the **selected node**. The properties of the selected node is displayed on the UI, and if there are gizmos associated with the selected node they are displayed and available for interaction. This means that for example if you select a cutter plane node, you can move that intuitively in the viewport while the displayed node might be their parent diff node which shows the geometry after the cut.

![Node Network](./node_network.png)

In the above example a plane cut away from a cuboid minus a sphere is displayed. There is also an unused experiment cutter plane on the picture which is currently not connected. In this case the user experiments with different cutter planes while not changing other parts of the node network.

## Supported nodes

I use the following syntax to document the node types here:

`node_type_name<properties of the node>(parameters of the node) -> output pin data type`

If there are no properties of a node we omit the angle brackets.

In some cases a parameter can receive a set of values instead of just one value. In this case we denote the data type with an asterisk (see union node for example).   

There will be several nodes that create geometry from scratch based on some parameters. I list only the Cuboid node in this category now.

### cuboid

`cuboid<extent: Vec3>() -> geometry`

Creates a cuboid geometry.

### half_space

`half_space<miller indices, offset>() -> geometry`

A half space defined by a plane which is usually used to cut away parts from a geometry with the `diff` node.

### union

`union(shapes: geometry*) -> geometry`

Creates a CSG union from the input shapes.

### intersect

`intersect(shapes: geometry*) -> geometry`

Creates a CSG intersection from its input shapes.

### negate

`negate(shape: geometry) -> geometry`

Creates a CSG negation from its input shape.

### diff

`diff(base: geometry*, sub: geometry*) -> geometry

Creates a CSG diff from its input shapes. For convenience multiple shapes can be given as a **base**, these are unioned. Multiple shapes can be given as **sub**, all these shapes are subtracted. 

### geo_transform

`geo_transform<translation, rotation>(shape: geometry) -> geometry`

Transforms a geometry in lattice space.

### geo_to_atomic

`geo_to_atomic<crystal related parameters, e.g. passivation fixing>(shape: geometry) -> atomic`

Creates an atomic entity from a Geometry.

### edit_atomic

`edit_atomic(atomic: atomic)-> atomic`

Edits an atomic entity atom by atom. Encapsulates multiple edits which can be inspected one-by-one. 

### SDF Function

*This node will be implemented only after the MVP. Please note that while other nodes are independent of the underlying implementation discussed in the shape algebra document, this node only works if implicits are used.*

Creates an SDF programmatically in a scripting language.

This can be used to create advanced highly procedural parametric SDFs.

Bret Victor made a demonstration of a system in which meaningful parameters could be intuitively set despite the textual programming language. It is in the first 10 minutes of his talk called "Inventing on Principle":

https://www.youtube.com/watch?v=EGqwXt90ZqA

It is clear that edit-to-run times should be very small, especially when only changing constant values.

When choosing the embedded scripting language, overall we need an easily embeddable language (into Rust) with reasonable sandboxing, and very quick edit-to-run numbers. We need to know the dependency tree between functions. If the language provides operator overloading (to help automatic differentiation) it is a big plus, but probably not a necessity, as we can live with numeric differentiation.

A promising choice seem to be https://rhai.rs/

Embeddable into Rust, supports operator overloading. It is interpreted, so there is no compilation: edit to run delay is probably minimal. On the other hand it might be a bit slow due to being interpreted.

*Inputs:*-

*Params:*

**function name**: string

parameters (or maybe even inputs) are automatically created based on the function parameters.

*output type*: Geometry

## Version control

This is mostly covered in [Math](./math.md).
