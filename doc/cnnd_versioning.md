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

Safe to do if the field is annotated with #[serde(default)].

- Old code reads struct saved with new code safely.
- New code reads struct saved with old code if #[serde(default)] is used.

### Use case: make an existing required property optional (`T` -> `Option<T>`)

Changing a node data field from `T` to `Option<T>` (so it can carry an explicit
"unset" state) needs no version bump, **provided two conditions hold**:

1. `Some(v)` serializes **byte-identically** to the old bare `T`. Then every old
   file (which always has a concrete value) deserializes to `Some(value)` — the
   correct semantics — with no migration. `None` serializes to JSON `null`.
2. The field is annotated `#[serde(default)]`, so an absent field reads back as
   `None`.

For plain `#[serde(default)]`-able types, serde's built-in `Option<T>` already
satisfies condition 1. For fields that use a custom `#[serde(with = "...")]`
serializer, pair it with an `Option`-aware variant that delegates to the original
for the `Some` case — `util/serialization_utils.rs` already provides
`option_ivec3_serializer` / `option_dvec3_serializer` (they emit the same bare
array as `ivec3_serializer` / `dvec3_serializer` for `Some`, and `null` for
`None`):

```rust
#[serde(with = "option_ivec3_serializer", default)]
pub miller_index: Option<IVec3>,
```

Note this only covers the **backward** direction (new code reading old files),
which is what migrations are for. A file that actually uses the new `null` state
cannot be read by *older* atomCAD — but that file uses a feature old atomCAD does
not have, so that forward-incompatibility is inherent and not something a
migration would address.

## New file format version and migartion

