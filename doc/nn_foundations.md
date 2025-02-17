# atomCAD: Node Network Algebraic Foundations

## Structure DAG and Node-Based Editing

An atomically precise CAD system requires a formalized representation of 3D structures. We use a Directed Acyclic Graph (DAG) to represent non-destructive edits, allowing substructures to be reused efficiently.

The structure DAG forms an algebraic system on 3D shapes, incorporating operations like CSG union, difference, and intersection. Unlike an axiomatic algebra, this is a concrete algebra defined on 3D geometric structures. The DAG extends a traditional tree of operations, common in compiler design, by allowing multiple roots and shared subexpressions.

A key extension includes atomic representations, enabling transformations from 3D shapes to atomic structures. 

## History DAG: Capturing Edits

In addition to the structure DAG, we maintain a history DAG that records user operations non-destructively. This DAG operates on the set of structure DAGs, with edits forming a persistent, append-only history. Common operations include:

- `connect(node_network_id, node_id1, node_id2)`
- `create_node(node_network_id, node_id, node_type)`
- `delete_node(node_network_id, node_id)`
- `update_node(node_network_id, node_id, {prop_name: value, ...})`

This approach mirrors version control systems, enabling reversible edits while keeping the primary structure clean and editable.

## Shape Algebra and Operator Set

The node network is fundamentally an algebra on 3D shapes, represented as infinite point sets. Operators include:

- `half_space<miller_index, offset>()` (cutting plane)
- `union(a, b)`
- `intersection(a, b)`
- `neg(a)` (negation)
- `diff(a, b)`

These operators remain abstract, independent of the underlying implementation. Three common models can realize this algebra:

1. **Implicit Functions:** Each operator is defined as a function over 3D space, allowing recursive evaluation to determine membership in a shape.
2. **Polygon Mesh Approximation:** Shapes are approximated as meshes.
3. **Voxel Approximation:** Shapes are discretized into voxels, allowing arbitrary precision with a memory tradeoff.

## Procedural Generation and Implementation Independence

Maintaining implementation independence is crucial to flexibility. Implementation dependent proceduralism can be easily incorporated (like procedural implicit functions), but some procedural extensions can also be  incorporated without tying the representation to a specific implementation. This can be achieved by:

- **Scripted Operator Trees:** Dynamically generating Structure DAG nodes programmatically.
- **Macro-Based Expansions:** New operators can be introduced as macro nodes that simply expand into regular nodes maintaining compatibility across implementation models. Macro nodes can be built-in macro nodes, like the Pattern node common in CAD tools, but we might introduce user defined macros too.

## User Interaction and Mental Model

The user interacts primarily with the structure DAG, which captures all explicit edits. The history DAG operates in the background, maintaining an append-only log of modifications. This separation mirrors the mental model used in version control systems, where day-to-day work happens within a living structure, with versioning considered only when necessary.

By structuring the CAD system this way, we ensure a balance between expressiveness, editability, and implementation flexibility, allowing for a robust atomically precise modeling environment.