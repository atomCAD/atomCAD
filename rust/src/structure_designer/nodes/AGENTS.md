# Nodes - Agent Instructions

Built-in node type implementations. Each file defines one node type's behavior via the `NodeData` trait.

## Node Categories

- **Primitives:** `bool`, `int`, `float`, `string`, `vec2`, `vec3`, `ivec2`, `ivec3`
- **Math/Programming:** `expr`, `value`, `parameter`, `sequence`, `if` (see below), `array_at`, `array_len`, `array_concat`, `array_append`, `imat3_rows`, `imat3_cols`, `imat3_diag`, `mat3_rows`, `mat3_cols`, `mat3_diag` (3x3 matrix constructors; `_rows`/`_cols` take three vectors, `_diag` takes one). Stored matrix defaults to identity. Wired input pin overrides the corresponding row/column/diagonal at eval. See `doc/design_matrix_types.md`.

  **`if`** (`if_else.rs` — the module is named `if_else` because `if` is a reserved Rust identifier; the node type name string is `"if"`) and **`switch`** (`switch.rs`, its n-way generalization keyed by a value) are the two **lazy structural selectors**: unlike an `expr` conditional they carry structural values (Crystal/Molecule/Blueprint/Geometry/Function…) and evaluate *only* the taken branch, so an error in an untaken branch never poisons the output. Both expand a stored `value_type` onto their pins via `calculate_custom_node_type` (the `parameter`/`array_at` idiom). `switch`'s variadic case list needs the same wire-stability machinery as `zip_with`'s lanes — hidden stable ids on `Parameter.id`, never the derived pin name. Details in each file's module doc; design doc `doc/design_switch_node.md`.
- **Function values (closures):** `closure` (zone-bearing node that exposes its inline body as a `NetworkResult::Function` value on an output pin), `apply` (calls a `Function`, either fully or partially, on a single argument frame). `closure` carries a `{ kind, type_args, param_names }` model (`ClosureKind` ∈ {Map, Filter, Fold, Foreach, Custom} — the four HOF body shapes plus a fully-flexible `Custom` kind allowing arbitrary parameter names/types and 0-arity thunks) and expands the kind *inward* (zone pins + `Function` output). `apply` keeps a `{ kind, type_args, param_names }` data record for back-compat but its actual pin layout is **derived from the wired `f`** (`f: AnyFunction*`, arg pins materialize from the source's canonical-flat signature; partial application = wire a contiguous prefix). See *Closures (function values)* below, `doc/design_closures.md`, `doc/design_currying.md`, and `doc/design_function_pin_unification.md`.
- **Iterators (lazy stream pipeline):** `range` (`Iter[Int]`), `map` (`Iter[T] → Iter[U]`), `zip_with` (n-ary element-wise map / `zipWith` / multimap, issue #382: `(Iter[T_1], .., Iter[T_N]) → Iter[R]`, lazy `WalkerKind::ZipZone`, shortest-input truncation), `filter` (`Iter[T] → Iter[T]`), `fold` (terminal consumer, `Iter[T] → Acc`), `collect` (terminal consumer, `Iter[T] → Array[T]`, with optional `limit: Option<i32>` field + optional `limit: Int` input pin overriding it — see `doc/design_iter_display_via_collect.md`), `foreach` (terminal **side-effect** consumer, `Iter[T] → Unit` — see Effect nodes below).

  Iterator outputs are **lazy walkers** carried as `NetworkResult::Iterator(Walker)`. `Array[T] → Iter[T]` is an implicit wire conversion (eager wrap); `Iter[T] → Array[T]` is **disallowed** at validation and requires an explicit `collect`. A node whose *displayed* pin output is `Iter[T]` renders no viewport output — wire `collect` and display that to inspect elements.

  **`zip_with`** (`zip_with.rs`, `doc/design_zip_with.md`) is the variadic generalization of `map`: the same zone / `f`-pin machinery over a user-configurable list of N lanes. Its lane-identity, lane-removal remapping, and undo model are the reference implementation for variadic pin lists (`switch` cases follow it) — see its module doc.

  The HOF nodes (`map`, `filter`, `fold`, `foreach`) carry their per-element computation as an **inline body** (`Node.zone`) and *also* accept an optional **`f: Function` input pin** — at `eval` they call `zone_closure::obtain_closure`, which prefers the wired-in closure and otherwise builds one from the inline body. The body model (pin sets, wire shapes, captures) is in `../AGENTS.md` §Zones; the per-element runtime is in `evaluator/AGENTS.md`. What matters when **registering** one of these types is that it is not a regular multi-output node — see `map.rs` / `fold.rs` for the pattern (`custom_node_type.zone_input_pins = …`, `custom_node_type.zone_output_pins = …`, plus the `f` parameter). Note that `f` is a **real** `DataType::Function` *input* pin, distinct from the title-bar `-1` function pin, which is a function-value **source**.
- **Effect nodes (`Unit`-returning, gated by Execute):** `export_atoms` (writes an atomic-structure file as a side effect — format chosen by the extension, `.xyz` or `.mol`, dispatched through `crystolecule::io::atom_export::AtomExportFormat`; previously the XYZ-only `export_xyz`, passthrough Molecule, now `Unit`), `foreach` (drains an iterator and runs a body per element for the side effect, body return discarded). All effect nodes have output type `Unit`, so the **central skip rule** (`evaluator/AGENTS.md`) short-circuits them on display passes. Two authoring consequences: effect-node `eval` arms call their effect logic **unconditionally**, with no `if context.execute` guard; and light per-eval input validation that used to surface as a node error during display now defers to Execute, so recover the eager UX feedback via `get_subtitle` (see `export_atoms.rs` for the pattern). Design doc: `doc/design_node_execution.md`.
- **Debug:** `print` (passthrough `String` with a side effect that appends an entry to `context.print_buffer`). Output is `String`, not `Unit`, so the central skip rule does **not** apply — `eval` runs on every pass that reaches `print`. The `execute_only: bool` property gates the buffer push (when `true` the side effect fires only under Execute; when `false`, the default, it fires on every evaluation including normal display passes). The buffer is drained into `StructureDesigner.print_log` by the central `with_eval_context` helper; the Flutter Console panel polls via `take_print_log()`. Design doc: `doc/design_node_execution.md` (Phase 4).
- **Geometry 2D:** `rect`, `circle`, `free_circle`, `reg_poly`, `polygon`, `union_2d`, `intersect_2d`, `diff_2d`, `half_plane`
- **Geometry 3D (Blueprint outputs):** `cuboid`, `sphere`, `free_sphere`, `extrude`, `half_space`, `drawing_plane`, `facet_shell`, `union`, `intersect`, `diff`, `geo_trans`. Primitives take an optional `Structure` input (defaulting to diamond) instead of the old `LatticeVecs`/unit-cell input. `drawing_plane`'s three orientation inputs (`m_index` pin 1, plus `u` pin 5 / `v` pin 6 — in-plane lattice *directions* `[u v w]`) are all **optional** and resolved wired-pin > stored-field > unset, then dispatched through `DrawingPlane::from_spec` (cases A–D: auto axes / pin `u` / pin `u`+`v` / derive Miller from `u × v`). Bad combinations surface as a localized `NetworkResult::Error` — **no validator rule** (the `eval` error is the surfacing). `DrawingPlaneEvalCache` carries the *resolved* orientation so the gadget/editor reflect the effective plane (derived `m`, auto-picked second axis). Design doc: `doc/design_drawing_plane_explicit_axes.md`.

  Two families here have semantics worth knowing before you wire them: `sphere` / `circle` are **lattice-covariant** (integer center/radius in lattice cells; an ellipsoid/ellipse on a non-cubic cell), while `free_sphere` / `free_circle` are their **real-space (Å) float** analogs and stay Euclidean by design. Each file's module doc has the details; design docs are `doc/design_lattice_covariant_sphere_circle.md` and `doc/design_free_sphere_circle.md` (issue #381).
- **Structure construction:** `lattice_vecs`, `motif`, `motif_sub`, `structure` (unified constructor/modifier — all four inputs optional, defaults to diamond)
- **Structure destructuring (unpack nodes):** `structure_unpack` (`Structure` → `lattice_vecs`/`motif`/`motif_offset`), `lattice_vecs_unpack` (`LatticeVecs` → basis vectors `a`/`b`/`c` as `Vec3`), `lattice_vecs_params` (`LatticeVecs` → cell params `a`/`b`/`c`/`alpha`/`beta`/`gamma` as `Float` + packed `lengths`/`angles` as `Vec3`). Stateless, fixed-pin inverses of the `structure` / `lattice_vecs` constructors — the built-in-type analogue of `record_destructure`. Follow the empty-data-struct pattern (`StructureData {}` + `generic_node_data_saver`/`loader`), `calculate_custom_node_type` returns `None`, `eval` returns `EvalOutput::multi(...)`. They opt into `default_display_all_output_pins() == true` (see below). Design doc: `doc/design_structure_lattice_unpack_nodes.md`.
- **Phase transitions:** `materialize` (Blueprint → Crystal), `dematerialize` (Crystal → Blueprint), `exit_structure` (Crystal → Molecule), `enter_structure` (Molecule + Structure → Crystal)
- **Atomic ops (HasAtoms-polymorphic):** `edit_atom/`, `atom_edit/` (plus `motif_edit` sibling node type defined in the same module), `atom_union`, `atom_cut`, `relax`, `passivate`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, `freeze`, `unfreeze`, `apply_diff`, `atom_composediff`
- **Region-gated atom ops (`doc/design_blueprint_region_atom_edits.md` Part A):** `passivate`, `remove_hydrogen`, `infer_bonds`, `atom_replace`, and the metadata-edit pair `freeze`/`unfreeze` (`freeze.rs`) each carry an **optional `region: Blueprint` input pin as their last pin**, gating the op to atoms inside that volume. The shared seam and the rules for adding another such op are in **`evaluator/atom_op.rs`**'s module doc.
- **Movement (polymorphic over abstract inputs):** `structure_move`, `structure_rot` on `HasStructure`; `free_move`, `free_rot` on `HasFreeLinOps`; `lattice_symop`. The four `structure_*`/`free_*` movement nodes keep a `same_as_input("result","input")` pin-0 (concrete type flows through) but are now **two-output** (`[result, diff]`, see the diff-output bullet below); `lattice_symop` is unchanged single-output.
- **Diff output pins (issue #295, `doc/design_diff_outputs_for_atom_ops.md`):** `relax`, the four movement nodes (`free_move`/`free_rot`/`structure_move`/`structure_rot`), `atom_replace`, and `atom_cut` each carry a **second `diff` output pin** (pin 1) alongside `result` (pin 0) — the same two-pin shape as `atom_edit`. The snapshot→`extract_diff`→`multi` pattern, the shared helpers, and the traps (every error path must return **two**-pin errors; do not override `default_display_all_output_pins`) are in **`evaluator/atom_op.rs`**'s module doc.

**Angle convention (issue #384).** Every angle exposed on a node pin, node property, or the text format is in **degrees**; the internal math unit stays radians (convert at the eval boundary via `.to_radians()`). Stored angle fields are suffixed `_degrees` (e.g. `free_rot`'s `angle_degrees`, `lattice_symop`'s `rotation_angle_degrees`) so a stale radian-era snippet fails loudly instead of being silently reinterpreted. Pin names may keep a short form (`free_rot`'s pin is still `angle` while the property is `angle_degrees` — same pin/property split as `extrude`'s `dir`/`extrude_direction`); when they diverge, add a `get_parameter_metadata` entry so the text-format introspection doesn't wrongly mark the pin required. The expr language keeps radian trig (`sin`/`cos`/…) and adds a `deg`-suffixed degree family plus `degrees(x)`/`radians(x)`. `.cnnd` files migrate via `serialization/migrate_v5_to_v6.rs`.
- **Surface reconstruction patches:** `patch_build` (authored slab + `cut_volume` → built-in `Patch` record `{tile: Molecule, tiling_vectors: Array[IVec3], cut_volume: Blueprint}` via the node-free `extract_patch_tile`), `patch_latticefill` (tiles a `Patch` over a region, welds it in via the node-free `apply_patch`, → `Crystal`). The weld model needs no motif/diff: periodic and tile↔bulk bonds emerge from coincidence (`crystolecule::weld::weld_coincident_atoms` + the patch-ghost flag, `Atom` bit 6). Tiling vectors are usually built by `plane_tiling_vectors` (a `MathAndProgramming` node, takes `DrawingPlane` + optional `IMat2` superlattice). The coordinate frame, cell-selection rule, debug flags, and the "re-run FRB codegen after editing `APIPatchLatticeFillData`" note are in `patch_latticefill.rs`'s module doc. Design docs: `doc/design_surface_patches.md`, `doc/design_patch_cell_selection.md`.
- **Records:** `record_construct` (one parameter pin per field → `Record(schema)`), `record_destructure` (multi-output, one pin per field), `product` (cartesian product of `Iter[T_i]` inputs → `Iter[Record(target)]`, rightmost field varies fastest — lazy odometer in `Walker::product`). All three take a `schema` / `target` `String` property naming a `RecordTypeDef`. Pin layout follows the def's **authored** field order; emitted `NetworkResult::Record` values are stored in **canonical** (sorted-by-name) order — the conversion is local to each node. Pin layouts re-derive via `repair_node_network` when the def changes. See `doc/design_record_types.md` (and `doc/design_iterators.md` for `product`'s lazy semantics).
- **I/O:** `import_xyz` (Molecule), `import_cif` (Blueprint). (`export_atoms` is listed under *Effect nodes* — it writes a `.xyz`/`.mol` file as its side effect and gates on Execute.)
- **Annotation:** `comment`

Nodes **deleted** by the lattice-space refactoring: `atom_fill` (→ `materialize`), `atom_lmove`/`atom_lrot` (→ `structure_move`/`structure_rot`), `atom_move`/`atom_rot` (→ `free_move`/`free_rot`), `lattice_move`/`lattice_rot` (→ `structure_move`/`structure_rot`), `atom_trans`. Old `.cnnd` files are up-converted at load time by `serialization/migrate_v2_to_v3.rs`.

## Closures (function values)

`closure.rs` and `apply.rs` are the function-value node pair (`doc/design_closures.md`, `doc/design_currying.md`, `doc/design_function_pin_unification.md`). They share a stored data model — `{ kind: ClosureKind, type_args: Vec<DataType>, param_names: Vec<String> }` (`ClosureData` / `ApplyData`, identical shape) — and differ in how `calculate_custom_node_type` expands it.

- **`ClosureKind`** (`Map` / `Filter` / `Fold` / `Foreach` / `Custom`) is a *shape template* fixing arity and which pin types are **free** (user-picked, filled from `type_args`) vs. **fixed/derived** (`Bool`, `Unit`, or `= acc`). The four preset kinds equal the four HOF body shapes, so a closure of a given preset kind drops into the matching HOF's `f` pin by construction. `Custom` allows arbitrary parameter count (**including 0** — a thunk), each with a user-chosen name (`param_names`) and a user-chosen type (`type_args[0..N]`); `type_args[N]` is the return type. **`ClosureData::default` is `Custom` with zero parameters** — the 0-ary `() -> Float` shape (issue #418), matching the `closure` `NodeType`'s declared `zone_input_pins: vec![]` / `Function(() -> Float)` output; the Flutter Kind dropdown surfaces it as its own top-of-list "0-ary function" entry rather than as a sixth `ClosureKind` variant. `ClosureKind` and its helpers (`num_type_args`, `param_types`, `return_type`, `param_names`, `result_name`, `function_type`) live in `closure.rs` and are reused by `apply.rs`. See `doc/design_custom_closure_kind.md` for the rationale.
- **`closure`** is **zone-bearing**, so `ensure_zone_init`, CoW body cloning, copy/paste, undo, and `walk_all_nodes` recursion all work with **no new lifecycle code**, and body rendering is inherited from the generic zone UI. The function type written into its output pin is **canonicalized** by `FunctionType::new` (currying-equivalent forms collapse to the flat multi-arg form).
- **`apply`** is bodyless: its `f` pin is declared `AnyFunction { leading_params: vec![] }` (accepts any function value) and its **actual** arg-pin layout and output type are installed by the post-pass `update_apply_pin_layouts_for_network` from the wired `f`'s canonical-flat signature — `R` when all arg pins are wired, `Function(<unwired tail>, R)` for a partial application. Arg pins are **optional** but must be wired as a **contiguous prefix**; non-prefix wiring is a validation error. The eval loop and the load-time hazard that layout creates are in `apply.rs`'s module doc.

The optional `f` pin lives on the four HOFs too (added in `map.rs` etc.); see the Iterators bullet above and `evaluator/AGENTS.md`. `map.f`'s declared type is `AnyFunction { leading_params: vec![element_type] }` — accepts any function value whose first parameter matches the input element type, with extra parameters absorbed via auto-partialization (`update_map_pin_layouts_for_network` derives the output pin type from the wired source's tail). `filter.f` / `fold.f` / `foreach.f` are exact-arity `Function(...)` declared types. Validation rules (f-pin suspends the "zone-output wire required" rule; `apply` requires `f`; `apply` arg pins must be wired as a contiguous prefix) are in `network_validator.rs` — see `function_input_pin_connected` plus the inline `apply` checks in the main `validate_network` pass.

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
    fn default_display_all_output_pins(&self) -> bool;  // default false
}
```

### Default output-pin display

The global default is **pin-0-only**: a freshly added node shows just its first output pin (`NodeDisplayState::normal`/`with_type` set `displayed_pins = {0}`). This is deliberate — a multi-output node like `atom_edit` should not draw both `result` and `diff` at once. Override `default_display_all_output_pins() -> true` for a node whose every output is worth showing on creation; the three unpack nodes do this so users can hover-inspect every unpacked value immediately. It is **safe only when the outputs draw no viewport geometry** — `LatticeVecs`/`Motif`/`Vec3`/`Float`/`Structure` all fall through `convert_result_to_node_output` to `NodeOutput::None`, so the eye toggle merely gates the per-pin hover readout, with no clutter. `StructureDesigner::apply_default_all_pin_display` consults this hook in both add paths (top-level + scoped) *after* the display-policy pass; both display setters preserve an existing `displayed_pins` set, so the policy never clobbers it. (It augments only a node the policy is already showing — it does not force-show a hidden node.) Backend-only; the Flutter side already renders each pin's eye from `NodeView.displayed_pins`.

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

### Errors: never re-wrap, never replace an upstream cause

An error a node receives on an input is **already localized and already
chained** — `evaluate_arg` is the chaining hub (`error in {pin} input (from
{type} #{id}): {inner}`, `network_result::error_in_input_chained`) and it also
records the D7 **origin link** that makes "Go to root cause" work
(`doc/design_error_management.md`). Two rules follow, and both are about not
destroying information the evaluator already produced:

- **Do not re-wrap.** Forward an errored input verbatim
  (`if let NetworkResult::Error(_) = v { return EvalOutput::single(v); }`, the
  near-universal early-return guard). Adding another `error in … input:` layer
  on top duplicates the hub's own wrap and makes the message a nesting doll.
  Multi-output nodes forward it on **every** pin (`multi(vec![err.clone(),
  err])`), never `single(err)` — see the diff-output bullet above.
- **Never replace an error with a type complaint.** The recurring bug shape is
  a `match` that dispatches on the expected variant and falls through to an
  ad-hoc `"all inputs must be X"` arm — which an `Error` value also lands in,
  so the root cause is swapped for a sentence that is simply false. This bites
  hardest on **array inputs**, whose elements can individually be errors
  (`sequence` stores each wired input verbatim; `collect` can drain an erroring
  stream). Right after unwrapping `NetworkResult::Array(..)`, and **before** any
  per-element dispatch, call the shared scanner:

  ```rust
  if let Some(err) = first_array_element_error("shapes", &shape_results) {
      return EvalOutput::single(err);
  }
  ```

  Current callers: `union` / `union_2d` / `intersect` / `intersect_2d` /
  `diff` / `diff_2d` (through `helper_union`'s `HelperUnionError::Upstream`
  variant), `atom_union`, `atom_cut`, `atom_composediff`. `patch_build` does
  the same inline with an explicit per-element `Error` arm. A loop that
  *silently drops* unexpected elements (as `atom_cut` did) is the worst
  variant — the node then produces a quietly wrong result with no error at all.

Errors are runtime-only (`NetworkResult` has no `Serialize`/`PartialEq`), and
`convert_to` is a no-op on `Error`, so forwarding is always type-safe.

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
