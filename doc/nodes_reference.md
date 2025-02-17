# atomCAD Nodes Reference 

I use the following syntax to document the node types here:

`node_type_name<properties of the node>(parameters of the node) -> output pin data type`

If there are no properties of a node we omit the angle brackets.

In some cases a parameter can receive a set of values instead of just one value. In this case we denote the data type with an asterisk (see union node for example).   

There will be several nodes that create geometry from scratch based on some parameters. I list only the Cuboid node in this category now.

## cuboid

`cuboid<extent: Vec3>() -> geometry`

Creates a cuboid geometry.

## half_space

`half_space<miller indices, offset>() -> geometry`

A half space defined by a plane which is usually used to cut away parts from a geometry with the `diff` node.

## union

`union(shapes: geometry*) -> geometry`

Creates a CSG union from the input shapes.

## intersect

`intersect(shapes: geometry*) -> geometry`

Creates a CSG intersection from its input shapes.

## negate

`negate(shape: geometry) -> geometry`

Creates a CSG negation from its input shape.

## diff

`diff(base: geometry*, sub: geometry*) -> geometry

Creates a CSG diff from its input shapes. For convenience multiple shapes can be given as a **base**, these are unioned. Multiple shapes can be given as **sub**, all these shapes are subtracted. 

## geo_transform

`geo_transform<translation, rotation>(shape: geometry) -> geometry`

Transforms a geometry in lattice space.

## geo_to_atomic

`geo_to_atomic<crystal related parameters, e.g. passivation fixing>(shape: geometry) -> atomic`

Creates an atomic entity from a Geometry.

## edit_atomic

`edit_atomic(molecule: atomic)-> atomic`

Edits an atomic entity atom by atom. Encapsulates multiple edits which can be inspected one-by-one.

## merge_atomic

`merge_atomic(molecules: atomic*)-> atomic`

This node serves 2 purposes. First it merges any number of atomic representations into one.

Second, it is possible to specify bonding rules: if certain conditions match bonds are created. The simplest such bonding rules are based on atomic distances. It can be specified if a rule is inter-molecule only, or does it apply even inside one molecule too. With such intra-molecule rules a `merge_atomic` node can even be meaningfully applied with only one input molecule.

Later when knowing user needs better we can design complex bonding rules too. An inspiration for such a rule language can be SMARTS:

https://www.daylight.com/dayhtml/doc/theory/theory.smarts.html 

## atomic_transform

`atomic_transform<translation, rotation>(molecule: atomic) -> atomic`

Transforms an atomic representation.

## SDF Function

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