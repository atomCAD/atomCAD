# atomCAD Model Representation and Kernel Architecture 

## Design philosophy

The design philosophy behind the concept described in this document is the following:

- We would like to use non-destructive editing where possible
- We would like to enable structure/code reuse as much as possible. The user should be able to distill useful parts of the design into a reusable representation. In the long run the possibility of an ecosystem of part libraries should be achievable.
- Balance power and simple UX as much as possible. (This is very hard, some tradeoff is needed here.) 

## Supported design paradigms

Before thinking about the architecture and UX of our software, let's first think about what design paradigms we support. By design paradigm I mean the way the user primarily thinks about their design and would like to interact with their design, and also how the design is fundamentally represented. We might support more paradigms later but let's review what we would like to support in the near future: 

- We want the user to be able to create geometry separately and create an atomic representation from a geometry separately in a non-destructive way.
- There are some ways to represent geometry that fundamentally changes what can be done with that geometry or have fundamental restrictions on that geometry. BRep models, polygonal models and implicit surfaces a 3 very different ways which require a radically different set of operations with a radically different effort needed to create kernels for. We assume in the short term that we support only implicit surfaces. Supporting polygonal models in addition to implicit models would require some plus effort and would make our software internally more complicated, but this can be considered even in the short term. Supporting BReps would make the software much more complicated and should be done only if really needed.
- We would like to enable the user to do as much as possible by creating geometry and creating atoms from the geometry in a non-destructive way, but sometimes editing atom by atom is unavoidable and we would like to support this as well.

## Implicit surfaces

We plan to use implicit surfaces, more specifically SDF (Signed Distance Field) to express solid geometry in atomCAD. SDF functions is a very elegant and composable way to achieve interesting solid geometries. In atomCAD we have a strong need for supporting CSG (Constructive Solid Geometry) from primitives like cutter planes with specified Miller indices along crystal lattice points. Fortunately CSG is just a special use-case for SDFs: unions and intersections being min and max functions in an SDF.

It is a hard question whether to enable the creation of complex parametrized procedural SDFs procedurally in a general purpose programming language or concentrate on providing a simple workflow to create simpler SDFs. Fortunately it is not hard to support both so I decided to do that. As we will see SDFs are supported through SDF function libraries and through simple nodes like cutter plane nodes, boolean union, diff and intersection nodes.

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

I considered somewhat simpler UX paradigms, but gaining a little simplicity we would lose too much power. I also think that the usage of node networks is quite common among technically savvy users. Some of the above software is complex not necessarily because of the node network paradigm but because of the complexity of the domain they operate in.

## Node network in atomCAD

A Node can have any number of input pins (including zero) and a positive number of output pins (usually one).

Each input and output pin has a type. You can connect an output pin with an input pin with a directed edge if their type match. The network should be a DAG: should not contain a circle.

Most important pin types:

- integer
- iVec3
- float
- Vec3
- sdf: SDF geometry
- atomic: atoms and bonds
- string

Input pins values for certain types (integer, iVec3, float, Vec3) can be set on the UI without connecting a wire into the pin.

Additionally nodes can have any number of parameters. Parameters can be filled only on the UI.

On the User interface there is always a 'displayed node'. The output of that node is displayed in the editor viewport. The displayed node is not necessarily the same as the selected node. The parameters of the selected node is displayed on the UI, and if there are gizmos associated with the selected node they are displayed and available for interaction. This means that for example if you select a cutter plane node, you can mode that intuitively in the viewport while the displayed node might be their parent diff node which shows the geometry after the cut.

![Node Network](./node_network.png)

In the above example a plane cut away from a cuboid minus a sphere is displayed. There is also an unused experiment cutter plane on the picture which is currently not connected. In this case the user experiments with different cutter planes while not changing other parts of the node network.

## Supported nodes

There will be several nodes that create geometry from scratch based on some parameters. I list only the Cuboid node in this category now.

### Cuboid

Creates a cuboid geometry.

*Inputs:*

**extent**: Vec3

*output type*: SDF

### Cutter plane

A half space defined by a plane which is usually used to cut away parts from a geometry with the Diff node.

*Parameters:* Miller index and offset of the plane

*output type*: SDF

### Union

Creates a CSG union from its inputs. More precisely it performs a **min** operation on its SDF inputs.

*Inputs:*

**geo1**: SDF

**geo2**: SDF

*output type*: SDF 

### Intersection

Creates a CSG intersection from its inputs. More precisely it performs a **max** operation on its SDF inputs.

*Inputs:*

**geo1**: SDF

**geo2**: SDF

*output type*: SDF

### Negation

Creates a CSG negation from its input. More precisely it performs a **multiply by (-1)** operation on its SDF input.

*Inputs:*

**geo**: SDF

*output type*: SDF

### Diff

Creates a CSG diff from its inputs. This is equivalent of creating an intersection of geo1 with negated geo2.

*Inputs:*

**geo1**: SDF

**geo2**: SDF

*output type*: SDF

### SDF Function

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

*output type*: SDF

### Atomic from SDF

Creates an atomic entity from an SDF.

Please note that at SDF evaluation time there is no explicit notion of planes and edges: We should work from whatever evaluations we do in the SDF (value evaluations and gradient evaluations). Plane and edge fixing algorithms can use SDF values and gradients at certain atoms and other crystal points for their heuristics. 

*Inputs:*

**geo1**: SDF

*Params*: Several parameters related to fixing faces and edges.

*output type*: atomic

### Edit atomic

Edits an atomic entity atom by atom. Encapsulates multiple edits which can be inspected one-by-one. 

*Inputs:*

**atoms**: atomic

*Params*: Several parameters related to fixing faces and edges.

*output type*: atomic

## Undo-redo

What would be an undo history in a direct editing program is already embedded into the node network itself, so changes in our representation (like SDF function source code changes, creating nodes, rewiring nodes, changing parameters) are 'meta changes' in this sense, so any version control on a representation which itself contains 'operation history' in itself seem to be a bit redundant, but still a necessity mainly because we want the users not to lose previous versions of their model. We will simply use a full global undo stack for anything the user does on the whole node network.

## Version control

Besides having a fully non-destructive base model + applying a global undo on it, we still have an almost independent problem: how different designers can cooperate, how merge conflicts are resolved. The global undo stack should not mess with this, so the task is to do version control for the fully non-destructive representation. I think it would be just too much detail to be planned up-front. I like to think of this task as trying to serialize down the representation into well mergeable text files, and do the version control in git. It is still possible to create a custom version control later, but it should not be planned up-front, it is enough to conceptually know that it can be done reasonably well.
