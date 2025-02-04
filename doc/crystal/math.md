## Mathemtaical definition of the Structure DAG

So the node network that the user edits (the structure DAG) can be mathematically defined this way:

We have an algebraic system on the underlying set of 3D shapes, with operations like CSG union, CSG diff, etc...

The word 'operation' is also used in a mathematical way: a function from a set to itself.

Our algebra is not an axiomatic algebra, but an algebra defined on a concrete model (set of 3D shapes). (a related axiomatic algebra would be boolean algebra) (see model theory)

Traditionally a tree of operations in an algebra is discussed, which is an expression in the algebra. We extend this to a DAG, where subtrees can be reused and there can be multiple roots, but it has no effect on the meaning of these operations. (The DAG representation is common in compiler design, see https://www.geeksforgeeks.org/directed-acyclic-graph-in-compiler-design-with-examples/) 

You can define a new operation (function from the underlying set to itself) by creating a separate DAG with special parameter elements and one root of the DAG designated as the 'return value of the function'.

We extend this model by extending our underlying set: instead of it being the set of 3D shapes, we add the set of possible atomic representations to it. This way our algebra can have operators that create an atomic structure based on a shape, or an operation that transforms an atomic structure to another in some way.

## Discussion

The structure DAG is very important because the user manipulates it directly. Where things get a little confusing at first is whether is this DAG the structure or the history of our model?  Any expression in any algebra could be seen as the edit history of the leaf values passed into the expression, but I suggest the term history at this level. The definition of a non-destructive editor in my opinion is that the document in the editor, which the user directly edits contains some operations that were traditionally considered history operations. In our case the document in the editor is basically an algebraic expression. (An extension of that: a set of functions which are all defined as algebraic expressions, and expressions are not trees but DAGs.)

## History

The history graph, which is also a DAG can be defined also as a DAG on an algebraic structure, but this algebraic structure's underlying set is the set of structure DAGs: our document, which the user edits.

Operations in this algebraic structure are the edit operations the user makes on the document. Using a DAG to represent history is common, version control systems a represented this way. Unary operations are simple changes, binary operations are merges.

As it happens that our underlying document is a DAG itself, we are very lucky because we can work with a very few simple unary operations in the history DAG.

Calling the defined functions in our document the non-mathematical way 'node networks', and operations as nodes, using this nomenclature here are the history DAG unary operations:

- connect(node_network_id, node_id1, node_id2)
- create_node(node_network_id, node_id, node_type)
- delete_node(node_network_id, node_id)
- update_node(node_network_id, node_id, {prop_name1: prop_value1, prop_name2: prop_value2, ...})