# CNND format versioning best practices

This document contains the best practices to handle different .cnnd file versions.

It is possible to load and save the list of node networks in the atomCAD editor.
For this we use the serde Rust library, in particular we save to JSON format.

Runtime structures like `NodeNetwork`, `Node`, and `NodeType` are converted to serializable counterparts (`SerializableNodeNetwork`, `SerializableNode`, `SerializableNodeType`) that can be safely serialized to JSON.
Each node contains polymorphic `NodeData` (trait objects) which cannot be directly serialized. The system uses function pointers (`node_data_saver`/`node_data_loader`) registered in the `NodeType` to convert node data to/from JSON values.

This document is relevant when:

- changing the definition (for example input pins) of a node in node_type_registry.rs
- or when changing the node data struct for a node in the rust file named after the node.

Our strategy is to have a generic behaviour in the serialization system that in most cases either the change can be done without any additional effort or with a small overhead like a serde attribute.

Only in cases when this is not possible we create a new version and use a migration mechanism.



## Possible changes without new file format version and migration



### Use case:  built-in node input pin changes

Unfortunately input pins in a node network are referred by index and not pin name, so there are more problems than there would be if we would refer pins by name. (We might change this in a later version migration). The problem can be when loading a file saved with an older version of atomCAD. The following cases are automatically handled:

- Adding a new input pin (a.k.a parameter) to the end of the parameter list.
- Deleting pins from the end of the list. TODO: implement this

In case you need to insert an input pin not to the end of the input pins, or delete a pin not from the end, you need a new version and migration.

### Use case: add new property to built-in node data







## New file format version and migartion

