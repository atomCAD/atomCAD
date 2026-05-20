# Nodes - Agent Instructions

Built-in node type implementations. Each file defines one node type's behavior via the `NodeData` trait.

## Node Categories

- **Primitives:** `bool`, `int`, `float`, `string`, `vec2`, `vec3`, `ivec2`, `ivec3`
- **Math/Programming:** `expr`, `value`, `parameter`, `sequence`, `array_at`, `array_len`, `array_concat`, `array_append`, `imat3_rows`, `imat3_cols`, `imat3_diag`, `mat3_rows`, `mat3_cols`, `mat3_diag` (3x3 matrix constructors; `_rows`/`_cols` take three vectors, `_diag` takes one). Stored matrix defaults to identity. Wired input pin overrides the corresponding row/column/diagonal at eval. See `doc/design_matrix_types.md`.
- **Function values (closures):** `closure` (zone-bearing node that exposes its inline body as a `NetworkResult::Function` value on an output pin), `apply` (calls a `Function` once on a single argument set). Both share a `{ kind, type_args }` model (`ClosureKind` ∈ {Map, Filter, Fold, Foreach} = the four HOF body shapes); `closure` expands the kind *inward* (zone pins + `Function` output), `apply` expands it *outward* (required `f` input + per-param arg pins + value output). See *Closures (function values)* below and `doc/design_closures.md`.
- **Iterators (lazy stream pipeline):** `range` (`Iter[Int]`), `map` (`Iter[T] → Iter[U]`), `filter` (`Iter[T] → Iter[T]`), `fold` (terminal consumer, `Iter[T] → Acc`), `collect` (terminal consumer, `Iter[T] → Array[T]`, with optional `limit: Option<i32>` field + optional `limit: Int` input pin overriding it — see `doc/design_iter_display_via_collect.md`), `foreach` (terminal **side-effect** consumer, `Iter[T] → Unit` — see Effect nodes below). Outputs are **lazy walkers** carried as `NetworkResult::Iterator(Walker)`; `Array[T] → Iter[T]` is an implicit wire conversion (eager wrap); `Iter[T] → Array[T]` is **disallowed** at validation, requiring an explicit `collect`. A node whose *displayed* pin output is `Iter[T]` renders no viewport output — wire `collect` and display that to inspect elements.

  The HOF nodes (`map`, `filter`, `fold`, `foreach`) carry their per-element computation as an **inline body** — a `Node.zone: Option<Arc<NodeNetwork>>` populated by the API's `set_zone` machinery — declaring `zone_input_pins` (`element`, `acc`) and `zone_output_pins` (`result`, `new_acc`, `out`). Captures (wires whose `source_scope_depth ≥ 1`) carry outer-scope values into body nodes; iteration values flow from the HOF's `ZoneInput` source pins to body destinations. Each HOF *also* has an optional **`f: Function` input pin**: at `eval` it calls `zone_closure::obtain_closure`, which uses the wired-in `ZoneClosure` from `f` when connected and otherwise falls back to building one from its own inline body (`build_inline_closure`). Either way the resulting closure runs per element through the shared `zone_closure::run_closure_once` — lazily inside `Walker::MapZone` / `FilterZone`, eagerly in `fold`/`foreach::eval`. (This `f` pin is a **real** `DataType::Function` value pin produced by the `closure` node — *not* the legacy `FunctionEvaluator` / `output_pin_index == -1` convention, which was deleted in closures Phase 2 along with the old `Closure` and `evaluate_zone_output`.) The HOF type registration is therefore subtly different from a regular multi-output node: see `map.rs` / `fold.rs` for the pattern (`custom_node_type.zone_input_pins = ...`, `custom_node_type.zone_output_pins = ...`, plus the `f` parameter). See `doc/design_iterators.md`, `doc/design_zones.md`, `doc/design_zones_ui.md`, `doc/design_closures.md`, and `evaluator/AGENTS.md` (Walker section) for the full design.
- **Effect nodes (`Unit`-returning, gated by Execute):** `export_xyz` (writes an XYZ file as a side effect; previously passthrough Molecule, now `Unit`), `foreach` (drains an iterator and runs a body per element for the side effect, body return discarded). All effect nodes have output type `Unit` so the **central skip rule** in `evaluator/network_evaluator.rs::evaluate_all_outputs` short-circuits them on display passes — `eval` only runs when `context.execute == true`, set by `StructureDesigner::execute_node` (the right-click → Execute UI action). Effect-node `eval` arms therefore call their effect logic unconditionally, with no `if context.execute` guard. The cleaner of the two consequences: a million-element iterator upstream of a `foreach` costs zero work during normal editing — neither `xs` nor `f` is touched. The flagged consequence: light per-eval input validation that used to surface as a node error during display now defers to Execute; recover the eager UX feedback via `get_subtitle` (see `export_xyz.rs` for the pattern). Design doc: `doc/design_node_execution.md`.
- **Debug:** `print` (passthrough `String` with a side effect that appends an entry to `context.print_buffer`). Output is `String`, not `Unit`, so the central skip rule does **not** apply — `eval` runs on every pass that reaches `print`. The `execute_only: bool` property gates the buffer push (when `true` the side effect fires only under Execute; when `false`, the default, it fires on every evaluation including normal display passes). The buffer is drained into `StructureDesigner.print_log` by the central `with_eval_context` helper; the Flutter Console panel polls via `take_print_log()`. Design doc: `doc/design_node_execution.md` (Phase 4).
- **Geometry 2D:** `rect`, `circle`, `reg_poly`, `polygon`, `union_2d`, `intersect_2d`, `diff_2d`, `half_plane`
- **Geometry 3D (Blueprint outputs):** `cuboid`, `sphere`, `extrude`, `half_space`, `drawing_plane`, `facet_shell`, `union`, `intersect`, `diff`, `geo_trans`. Primitives take an optional `Structure` input (defaulting to diamond) instead of the old `LatticeVecs`/unit-cell input.
- **Structure construction:** `lattice_vecs`, `motif`, `motif_sub`, `structure` (unified constructor/modifier — all four inputs optional, defaults to diamond)
- **Phase transitions:** `materialize` (Blueprint → Crystal), `dematerialize` (Crystal → Blueprint), `exit_structure` (Crystal → Molecule), `enter_structure` (Molecule + Structure → Crystal)
- **Atomic ops (HasAtoms-polymorphic):** `edit_atom/`, `atom_edit/` (plus `motif_edit` sibling node type defined in the same module), `atom_union`, `atom_cut`, `relax`, `add_hydrogen`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, `apply_diff`, `atom_composediff`
- **Movement (polymorphic over abstract inputs):** `structure_move`, `structure_rot` on `HasStructure`; `free_move`, `free_rot` on `HasFreeLinOps`; `lattice_symop`. All four movement nodes use `OutputPinDefinition::single_same_as("input")` so the concrete type flows through.
- **Records:** `record_construct` (one parameter pin per field → `Record(schema)`), `record_destructure` (multi-output, one pin per field), `product` (cartesian product of `Iter[T_i]` inputs → `Iter[Record(target)]`, rightmost field varies fastest — lazy odometer in `Walker::product`). All three take a `schema` / `target` `String` property naming a `RecordTypeDef`. Pin layout follows the def's **authored** field order; emitted `NetworkResult::Record` values are stored in **canonical** (sorted-by-name) order — the conversion is local to each node. Pin layouts re-derive via `repair_node_network` when the def changes. See `doc/design_record_types.md` (and `doc/design_iterators.md` for `product`'s lazy semantics).
- **I/O:** `import_xyz` (Molecule), `import_cif` (Blueprint). (`export_xyz` is listed under *Effect nodes* — it writes a file as its side effect and gates on Execute.)
- **Annotation:** `comment`

Nodes **deleted** by the lattice-space refactoring: `atom_fill` (→ `materialize`), `atom_lmove`/`atom_lrot` (→ `structure_move`/`structure_rot`), `atom_move`/`atom_rot` (→ `free_move`/`free_rot`), `lattice_move`/`lattice_rot` (→ `structure_move`/`structure_rot`), `atom_trans`. Old `.cnnd` files are up-converted at load time by `serialization/migrate_v2_to_v3.rs`.

## Closures (function values)

`closure.rs` and `apply.rs` are the function-value node pair (`doc/design_closures.md`). They share a stored data model — `{ kind: ClosureKind, type_args: Vec<DataType> }` (`ClosureData` / `ApplyData`, identical shape) — and differ only in how `calculate_custom_node_type` expands the kind:

- **`ClosureKind`** (`Map` / `Filter` / `Fold` / `Foreach`) is a *shape template* fixing arity and which pin types are **free** (user-picked, filled from `type_args`) vs. **fixed/derived** (`Bool`, `Unit`, or `= acc`). The four kinds equal the four HOF body shapes, so a closure of a given kind drops into the matching HOF's `f` pin by construction. `ClosureKind` and its helpers (`param_types`, `return_type`, `param_names`, `result_name`, `function_type`) live in `closure.rs` and are reused by `apply.rs`.
- **`closure`** is **zone-bearing** (`has_zone()` true via its declared `zone_input_pins` / `zone_output_pins`), so `ensure_zone_init`, CoW body cloning, copy/paste, undo, and `walk_all_nodes` recursion all work with **no new lifecycle code**. Its `eval` returns `NetworkResult::Function(build_inline_closure(...))` — the first half of an HOF eval, wrapped as a value. Body rendering is inherited from the generic zone UI.
- **`apply`** is bodyless. It declares a **required** `f: Function(...)` parameter plus one ordinary arg pin per parameter, reads `f` directly (no `obtain_closure` fallback), resolves the arg pins, and runs the closure once via `run_closure_once` against a `fresh_inner_for_eager_body` context (it is an eager consumer, so it passes its real `network_stack`). `get_parameter_metadata` marks `f` and every arg pin required.

The optional `f` pin lives on the four HOFs too (added in `map.rs` etc.); see the Iterators bullet above and `evaluator/AGENTS.md`. Validation rules (f-pin suspends the "zone-output wire required" rule; `apply` requires `f`) are in `network_validator.rs` (`function_input_pin_connected`).

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
    fn adapt_for_drag_source(&self, source: &DataType, dir: DragDirection,
                             registry: &NodeTypeRegistry) -> Option<Box<dyn NodeData>>;
}
```

### Drag-Aware Add Node

If your new node has user-configurable type properties that drive its pin types via `calculate_custom_node_type` (e.g. `MapData::input_type`, `ArrayAtData::element_type`, `ParameterData::data_type`), implement `adapt_for_drag_source`. The drag-aware add-node popup invokes it on each candidate node when the user drags a wire from a pin and drops on empty space — without it, the candidate is filtered using only the static (default) pin signature and won't surface for sources that *could* match after configuring its type properties.

The implementation pattern is short: clone `self`, overwrite the type properties to match the drag source (typically via `DataType::drag_element_type_from_output` to peel `Iter[T]`/`Array[T]` or broadcast a scalar), and return the adapted data. Return `None` for inputs that can't yield a valid configuration (abstract types, `Function(_)`, or — for nodes like `collect` — scalar broadcast that doesn't make semantic sense). The popup filter and `add_node_with_drag_source` both verify the adapter's claim by re-running the static-pin check against the resolved node type, so over-promising is silently dropped to default data rather than producing a mis-typed node — adapters can be loose. See `map.rs` / `array_at.rs` / `parameter.rs` for reference and `doc/design_drag_aware_add_node.md` for the full design.

### NodeType Registration

**IMPORTANT:** `output_type` field no longer exists on `NodeType`. Use `output_pins` with one of the helper constructors on `OutputPinDefinition`:

- `OutputPinDefinition::single_fixed(data_type)` — single output with a statically declared type.
- `OutputPinDefinition::single_same_as("input_pin_name")` — single polymorphic output that mirrors the resolved concrete type of the named input pin (used with abstract input types like `HasAtoms`/`HasStructure`/`HasFreeLinOps`).
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
    //   parameters: vec![Parameter { id: None, name: "molecule".into(), data_type: DataType::HasAtoms }, ...],
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
