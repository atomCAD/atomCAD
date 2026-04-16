# Nodes - Agent Instructions

Built-in node type implementations. Each file defines one node type's behavior via the `NodeData` trait.

## Node Categories

- **Primitives:** `bool`, `int`, `float`, `string`, `vec2`, `vec3`, `ivec2`, `ivec3`
- **Math/Programming:** `expr`, `value`, `map`, `range`, `parameter`, `sequence`
- **Geometry 2D:** `rect`, `circle`, `reg_poly`, `polygon`, `union_2d`, `intersect_2d`, `diff_2d`, `half_plane`
- **Geometry 3D (Blueprint outputs):** `cuboid`, `sphere`, `extrude`, `half_space`, `drawing_plane`, `facet_shell`, `union`, `intersect`, `diff`, `geo_trans`. Primitives take an optional `Structure` input (defaulting to diamond) instead of the old `LatticeVecs`/unit-cell input.
- **Structure construction:** `lattice_vecs`, `motif`, `motif_sub`, `structure` (unified constructor/modifier — all four inputs optional, defaults to diamond)
- **Phase transitions:** `materialize` (Blueprint → Crystal), `dematerialize` (Crystal → Blueprint), `exit_structure` (Crystal → Molecule), `enter_structure` (Molecule + Structure → Crystal)
- **Atomic (Atomic-polymorphic):** `edit_atom/`, `atom_edit/` (plus `motif_edit` sibling node type defined in the same module), `atom_union`, `atom_cut`, `relax`, `add_hydrogen`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, `apply_diff`, `atom_composediff`
- **Movement (polymorphic over abstract inputs):** `structure_move`, `structure_rot` on `StructureBound`; `free_move`, `free_rot` on `Unanchored`; `lattice_symop`. All four movement nodes use `OutputPinDefinition::single_same_as("input")` so the concrete type flows through.
- **I/O:** `import_xyz` (Molecule), `import_cif` (Blueprint), `export_xyz`
- **Annotation:** `comment`

Nodes **deleted** by the lattice-space refactoring: `atom_fill` (→ `materialize`), `atom_lmove`/`atom_lrot` (→ `structure_move`/`structure_rot`), `atom_move`/`atom_rot` (→ `free_move`/`free_rot`), `lattice_move`/`lattice_rot` (→ `structure_move`/`structure_rot`), `atom_trans`. A migration script for old `.cnnd` files is pending (phase 8).

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

**IMPORTANT:** `output_type` field no longer exists on `NodeType`. Use `output_pins` with one of the helper constructors on `OutputPinDefinition`:

- `OutputPinDefinition::single_fixed(data_type)` — single output with a statically declared type.
- `OutputPinDefinition::single_same_as("input_pin_name")` — single polymorphic output that mirrors the resolved concrete type of the named input pin (used with abstract input types like `Atomic`/`StructureBound`/`Unanchored`).
- `OutputPinDefinition::single_same_as_array_elements("input_pin_name")` — mirrors the element type of an `Array[..]` input pin.
- For multi-output: build a `vec![OutputPinDefinition::fixed("result", ...), OutputPinDefinition::same_as_input("diff", "molecule"), ...]` manually.

```rust
NodeType {
    name: "MyNode".to_string(),
    description: "What this node does".to_string(),
    summary: Some("One-line summary".to_string()),
    category: NodeTypeCategory::Geometry3D,
    parameters: vec![
        Parameter { id: None, name: "input".to_string(), data_type: DataType::Float },
    ],
    output_pins: OutputPinDefinition::single_fixed(DataType::Blueprint),
    // Polymorphic example (atom-op style):
    //   parameters: vec![Parameter { id: None, name: "molecule".into(), data_type: DataType::Atomic }, ...],
    //   output_pins: OutputPinDefinition::single_same_as("molecule"),
    // Multi-output example (atom_edit style):
    //   output_pins: vec![
    //       OutputPinDefinition::same_as_input("result", "molecule"),
    //       OutputPinDefinition::fixed("diff", DataType::Molecule),
    //   ],
    public: true,
    node_data_creator: || Box::new(NoData),
    node_data_saver: no_data_saver,
    node_data_loader: no_data_loader,
}
```

Access pin 0's declared type via `node_type.output_type()` — returns `&DataType::None` sentinel for polymorphic pins. Use `NodeTypeRegistry::resolve_output_type` when you need the resolved concrete type against a specific node context.

### Evaluation Pattern

`eval()` returns `EvalOutput`, not `NetworkResult` directly:
1. Extract inputs: `evaluator.evaluate_arg(...)` or `evaluate_arg_required(...)`.
2. Convert types: `result.extract_float()`, `result.extract_crystal()`, `result.extract_molecule()`, `result.extract_atomic()` (accepts Crystal or Molecule), `result.extract_structure()`, etc.
3. For polymorphic nodes (abstract input), match on the concrete variant (`NetworkResult::Crystal(c) => ...`, `NetworkResult::Molecule(m) => ...`) and re-wrap in the same variant at the output so `SameAsInput` typing is preserved. See `structure_move.rs` / `atom_edit_data.rs` for reference.
4. Return `EvalOutput::single(NetworkResult::Blueprint(...))` for single-output, or `EvalOutput::multi(vec![...])` for multi-output.

## edit_atom/ Subdirectory

Interactive atom editing node with command history (undo/redo):
- `edit_atom.rs` - Main `EditAtomData` implementing `NodeData`
- `edit_atom_command.rs` - Command trait and dispatcher
- `commands/` - Individual commands: add_atom, add_bond, delete, replace, select, transform

## atom_edit/ Subdirectory

Non-destructive atom editing node with diff-based architecture. See `atom_edit/AGENTS.md` for full details.

Key files:
- `types.rs` - Shared type definitions (tool enums, selection, eval cache)
- `atom_edit_data.rs` - `AtomEditData` struct, `NodeData` impl, accessors
- `selection.rs` - Ray-based and marquee atom/bond selection
- `operations.rs` - Shared mutation operations (delete, replace, transform, drag)
- `default_tool.rs` - Default tool pointer event state machine
- `add_atom_tool.rs` - Add Atom tool interaction
- `add_bond_tool.rs` - Add Bond tool interaction
- `minimization.rs` - UFF energy minimization (batch + continuous during drag)
- `atom_edit_gadget.rs` - XYZ selection gadget
- `text_format.rs` - Human-readable diff text format

## Text Format Properties

Nodes that store editable state must implement `get_text_properties()` and `set_text_properties()` to support the AI text format. Use `TextValue` for typed property values.

## Conventions

- Use `NoData` struct when node has no internal state (purely wired inputs)
- Use `no_data_saver`/`no_data_loader` for stateless nodes
- Nodes with state need custom `NodeData` struct + custom saver/loader
- Always handle missing optional inputs gracefully (return defaults or error)
