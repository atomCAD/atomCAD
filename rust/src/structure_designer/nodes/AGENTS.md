# Nodes - Agent Instructions

Built-in node type implementations. Each file defines one node type's behavior via the `NodeData` trait.

## Node Categories

- **Primitives:** `bool`, `int`, `float`, `string`, `vec2`, `vec3`, `ivec2`, `ivec3`
- **Math/Programming:** `expr`, `value`, `map`, `range`, `parameter`
- **Geometry 2D:** `rect`, `circle`, `reg_poly`, `polygon`, `union_2d`, `intersect_2d`, `diff_2d`, `half_plane`
- **Geometry 3D:** `cuboid`, `sphere`, `extrude`, `half_space`, `drawing_plane`, `facet_shell`, `union`, `intersect`, `diff`, `geo_trans`
- **Atomic:** `unit_cell`, `motif`, `atom_fill`, `edit_atom/`, `atom_edit/`, `atom_move`, `atom_rot`, `atom_trans`, `atom_union`, `atom_cut`, `relax`
- **Lattice:** `lattice_symop`, `lattice_move`, `lattice_rot`
- **I/O:** `import_xyz`, `export_xyz`
- **Annotation:** `comment`

## Adding a New Node

1. **Create** `nodes/my_node.rs` implementing `NodeData`
2. **Add module** in `nodes/mod.rs`
3. **Register** in `node_type_registry.rs` → `create_built_in_node_types()`

### NodeData Trait (key methods)

```rust
pub trait NodeData: Send + Sync {
    fn eval(&self, evaluator: &Evaluator, registry: &NodeTypeRegistry,
            node: &Node) -> Result<NetworkResult>;
    fn clone_box(&self) -> Box<dyn NodeData>;

    // Optional overrides:
    fn provide_gadget(&self, ...) -> Option<Box<dyn NodeNetworkGadget>>;
    fn calculate_custom_node_type(&self, ...) -> Option<NodeType>;
    fn get_subtitle(&self) -> Option<String>;
    fn get_text_properties(&self) -> Option<Vec<(&str, TextValue)>>;
    fn set_text_properties(&mut self, props: &[(&str, TextValue)]) -> Result<()>;
    fn get_parameter_metadata(&self) -> Option<Vec<ParameterMetadata>>;
}
```

### NodeType Registration

```rust
NodeType {
    name: "MyNode".to_string(),
    description: "What this node does".to_string(),
    summary: Some("One-line summary".to_string()),
    category: NodeTypeCategory::Geometry3D,
    parameters: vec![
        Parameter::new("input_name", DataType::Float),
    ],
    output_type: DataType::Geometry,
    public: true,
    node_data_creator: || Box::new(NoData),
    node_data_saver: no_data_saver,
    node_data_loader: no_data_loader,
}
```

### Evaluation Pattern

Most nodes follow this pattern in `eval()`:
1. Extract inputs: `evaluator.evaluate_arg(node, 0)?` or `evaluate_arg_required(node, 0)?`
2. Convert types: `result.extract_float()`, `result.extract_geometry()`, etc.
3. Compute output and return `Ok(NetworkResult::Geometry(...))`

## edit_atom/ Subdirectory

Interactive atom editing node with command history (undo/redo):
- `edit_atom.rs` - Main `EditAtomData` implementing `NodeData`
- `edit_atom_command.rs` - Command trait and dispatcher
- `commands/` - Individual commands: add_atom, add_bond, delete, replace, select, transform

## Text Format Properties

Nodes that store editable state must implement `get_text_properties()` and `set_text_properties()` to support the AI text format. Use `TextValue` for typed property values.

## Conventions

- Use `NoData` struct when node has no internal state (purely wired inputs)
- Use `no_data_saver`/`no_data_loader` for stateless nodes
- Nodes with state need custom `NodeData` struct + custom saver/loader
- Always handle missing optional inputs gracefully (return defaults or error)
