# Structure Designer - Agent Instructions

The bulk of atomCAD's Rust backend. Contains the node network system, built-in nodes, evaluator, and application logic. If anything can be factored out into an independent lower-level module, it should be (Stable Dependencies Principle).

## Subdirectory Instructions

- Working in `nodes/` → Read `nodes/AGENTS.md`
- Working in `nodes/atom_edit/` → Also read `nodes/atom_edit/AGENTS.md`
- Working in `evaluator/` → Read `evaluator/AGENTS.md`
- Working in `text_format/` → Read `text_format/AGENTS.md`
- Working in `serialization/` → Read `serialization/AGENTS.md`
- Working in `layout/` → Read `layout/AGENTS.md`
- Working in `implicit_eval/` → Read `implicit_eval/AGENTS.md`
- Working in `undo/` → Read `undo/AGENTS.md`

## Directory Structure

```
structure_designer/
├── structure_designer.rs      # StructureDesigner: main application state
├── structure_designer_changes.rs  # Change tracking for incremental refresh
├── structure_designer_scene.rs    # Scene graph for rendering output
├── node_network.rs            # NodeNetwork + Node: the core DAG
├── node_type.rs               # NodeType: node signature definition
├── node_data.rs               # NodeData trait: per-node behavior
├── data_type.rs               # DataType enum: type system for pins
├── node_type_registry.rs      # Central registry of all node types
├── network_validator.rs       # Validates and repairs networks
├── node_dependency_analysis.rs    # Computes downstream dependents
├── node_display_policy_resolver.rs # Controls node visibility
├── selection_factoring.rs     # Extracts selection into subnetwork
├── node_network_gadget.rs     # Gadget trait for interactive editing
├── node_layout.rs             # Node size estimation (matches Flutter)
├── navigation_history.rs      # Back/forward network navigation
├── common_constants.rs        # Shared constants
├── preferences.rs             # User preferences persistence
├── cli_runner.rs              # CLI batch execution mode
├── node_networks_import_manager.rs # Import networks from .cnnd libraries
├── undo/                      # Undo/redo system (command pattern)
├── nodes/                     # Built-in node implementations (47+)
├── evaluator/                 # Network evaluation engine
├── text_format/               # Human-readable text format (AI integration)
├── serialization/             # .cnnd JSON file I/O
├── layout/                    # Automatic node layout algorithms
├── implicit_eval/             # SDF evaluation and visualization
└── utils/                     # Utility helpers (half-space, XYZ gadget)
```

## Key Types

| Type | File | Purpose |
|------|------|---------|
| `StructureDesigner` | `structure_designer.rs` | Top-level application state, orchestrates everything |
| `NodeNetwork` | `node_network.rs` | DAG of nodes with connections, selection, display state |
| `Node` | `node_network.rs` | Single node: type, position, arguments, data |
| `NodeType` | `node_type.rs` | Node signature: parameters, output pins, serialization fns |
| `OutputPinDefinition` | `node_type.rs` | Output pin name + `PinOutputType` (Fixed / SameAsInput / SameAsArrayElements) |
| `PinOutputType` | `node_type.rs` | `Fixed(DataType)` for static types; `SameAsInput(name)` mirrors a named input pin's resolved concrete type (used for abstract-input polymorphic nodes) |
| `EvalOutput` | `node_data.rs` | Multi-output eval result (Vec of NetworkResult) |
| `NodeDisplayState` | `node_network.rs` | Per-node display type + displayed pins set |
| `NodeData` (trait) | `node_data.rs` | Per-node behavior: evaluation, gadgets, properties |
| `DataType` | `data_type.rs` | Pin type system: primitives (incl. `IMat3`/`Mat3` 3x3 matrices), `LatticeVecs`, `Structure`, the three phase types (`Blueprint`, `Crystal`, `Molecule`) and their abstract supertypes (`HasAtoms`, `HasStructure`, `HasFreeLinOps`), `Record(RecordType)` where `RecordType` is either `Named(String)` (registry reference) or `Anonymous(Vec<(String, DataType)>)` (inline schema, sorted by field name), `Array(Box<DataType>)` and `Iterator(Box<DataType>)` (`Iter[T]`, lazily-evaluated stream — see `evaluator/AGENTS.md` for the runtime walker), and `Unit` (the type with exactly one value — the return type of effect nodes; supports a universal `T → Unit` discard widening at field level, and `Unit → T` is rejected) |
| `RecordTypeDef` | `node_type_registry.rs` | Named record schema (user-declared *or* built-in). Fields are stored in **authored order** (drives pin layouts on `record_construct` / `record_destructure` / `product`); subtyping/equality canonicalize on demand |
| `NodeTypeRegistry` | `node_type_registry.rs` | Registry of built-in + custom (user-defined) node types, `record_type_defs` (user-declared schemas), and `built_in_record_type_defs` (application-supplied schemas like `ElementMapping`). Networks and record defs share one user-type namespace |
| `NetworkResult` | `evaluator/network_result.rs` | Evaluated node output value |

## Data Flow

```
User Action → StructureDesigner method
  → Capture before-state, perform mutation, push UndoCommand
  → Track changes in StructureDesignerChanges
  → NetworkEvaluator generates StructureDesignerScene
  → Scene sent to renderer/Flutter UI
```

## Type System

`DataType` governs pin compatibility. Conversion rules:
- Int ↔ Float, IVec2 ↔ Vec2, IVec3 ↔ Vec3, IMat3 ↔ Mat3 (float→int direction truncates)
- Single value → Array (broadcasting)
- Function structural match (same arity; parameters and return type pairwise convertible). The old partial-application "prefix" rule was dropped in closures Phase 2 — partial application is now expressed by closure captures, not the type rule.
- LatticeVecs → DrawingPlane (legacy)
- Concrete phase type → its abstract supertypes (Crystal/Molecule → HasAtoms; Blueprint/Crystal → HasStructure; Blueprint/Molecule → HasFreeLinOps). No abstract → concrete downcasts, no cross-abstract edges.
- **Iterator rules** (`Iter[T]`): `Array[S] → Iter[T]` (eager element conversion at wrap time, wraps as `Walker::from_array`), `S → Iter[T]` (single-element broadcast), `Iter[T] → Iter[T]` (identity passthrough only — `Iter[S] → Iter[T]` with `S ≠ T` is **disallowed in v1**). The reverse `Iter[T] → Array[T]` is **deliberately not** an implicit conversion: turning a fused stream back into a materialized array is exactly the operation iterators avoid, so it's rejected at validation and users must insert a `collect` node. Iterator-typed values cannot be captured into closures (the walker would alias across invocations) and `Iter[T]` is not allowed as a record field type. Design doc: `doc/design_iterators.md`.

Note: IVec3 does **not** auto-promote to a diagonal IMat3 — wire through an `imat3_diag` node when you want axis-aligned matrix semantics. See `doc/design_matrix_types.md` D4.

Records are **structurally** typed (names don't gate compatibility) with **width + structural depth subtyping**. At leaf field positions only **tag-only widenings** (identity + concrete-to-abstract phase upcasts, factored into `is_tag_only_widening`) are accepted — value-converting widenings like `Int → Float` are rejected at field level so destructure pins can pass the runtime payload through unchanged. Subtyping requires `&NodeTypeRegistry` to resolve `Named` references; threaded through `can_be_converted_to`. See `doc/design_record_types.md`.

Check `DataType::can_be_converted_to()` for the complete rules. `DataType::is_abstract()` identifies the three abstract supertypes.

### Three-Phase Model (lattice-space refactoring)

Objects in the node network flow through three concrete phases:

| Phase | Ingredients | Role |
|---|---|---|
| **Blueprint** | Structure + Geometry | *Design.* Geometry is a "cookie cutter" positioned in an infinite crystal field. |
| **Crystal** | Structure + Geometry (opt) + Atoms | *Construction.* Atoms have been carved out of the structure; atoms + geometry are rigidly coupled. |
| **Molecule** | Geometry (opt) + Atoms | *Deployment.* No structure association; free-floating. |

Three **abstract** supertypes name two-out-of-three combinations (each used only as an input-pin type):

| Abstract | Members | Property |
|---|---|---|
| `HasAtoms` | Crystal, Molecule | has materialized atoms (atom ops) |
| `HasStructure` | Blueprint, Crystal | has a structure (structure_move, structure_rot) |
| `HasFreeLinOps` | Blueprint, Molecule | free movement is legal (free_move, free_rot) |

Polymorphic nodes that accept an abstract input use `OutputPinDefinition::single_same_as("input")` (or `same_as_input(...)` for named pins) so the concrete variant flows through unchanged: a Crystal into `atom_edit` comes out as a Crystal, a Molecule comes out as a Molecule. `NodeTypeRegistry::resolve_output_type` resolves polymorphic pins against the connected source type at validation time; at runtime nothing special happens — the node receives a concrete `NetworkResult::Crystal(..)` / `Molecule(..)` / `Blueprint(..)` and returns the same variant.

Payload structs (in `evaluator/network_result.rs`): `BlueprintData { structure, geo_tree_root }`, `CrystalData { structure, atoms, geo_tree_root: Option<_> }`, `MoleculeData { atoms, geo_tree_root: Option<_> }`. The legacy `frame_transform` field is gone — movement nodes bake transforms directly into atom positions and `geo_tree` transforms. (`GeometrySummary2D` still carries one; it is 2D-only and unaffected.)

Design docs: `doc/design_lattice_space_refactoring.md` (master), `doc/design_crystal_molecule_split.md` (phase 6), `doc/design_phase_transitions_and_movement.md` (phase 7).

## Multi-Output Pins

Nodes can have multiple named output pins. Key types and conventions:

- **`NodeType.output_pins: Vec<OutputPinDefinition>`** — replaces the old single `output_type` field. Use `output_type()` accessor for pin 0's type. Use `OutputPinDefinition::single(DataType::X)` for single-output nodes.
- **`NodeData::eval()` returns `EvalOutput`** — use `EvalOutput::single(result)` for single-output nodes, `EvalOutput::multi(vec![...])` for multi-output.
- **`NodeDisplayState`** — replaces `displayed_node_ids`. Bundles `display_type: NodeDisplayType` + `displayed_pins: HashSet<i32>`. The map is `displayed_nodes: HashMap<u64, NodeDisplayState>`.
- **Display is per output pin**, not per node. Display policy operates at node level; pin-level display is always explicit/manual.
- **Interactive pin** = lowest-indexed displayed output pin (for hit testing). See `NodeSceneData::interactive_pin_index()`.
- **Pin indexing:** -1 = function pin, 0 = primary result, 1+ = additional outputs.

Design doc: `doc/design_multi_output_pins.md`.

## Node Networks as Custom Types

A `NodeNetwork` can itself become a node type usable in other networks. The `NodeTypeRegistry` manages both built-in node types and user-defined network-as-node types. Parameter nodes in a network become the custom type's input pins. The return node's full `output_pins` are propagated to the custom node type (multi-output passthrough).

## Record Type Defs

User-declared `RecordTypeDef`s live alongside custom networks in `NodeTypeRegistry::record_type_defs` and share one user-type namespace with networks (and built-ins). `RecordType::Named(N)` references resolve through the registry on every lookup, so field-level edits to a def are visible everywhere immediately — only renames need a `DataType` walk (see `rename_record_type_def`, modeled on `rename_node_network`). The `record_type_def` dependency graph must stay acyclic; the cycle check runs on add/update. Schema or deletion changes trigger `repair_node_network` to disconnect now-incompatible wires and refresh `record_construct` / `record_destructure` / `product` pin layouts. Design doc: `doc/design_record_types.md`.

**Built-in record defs** (`NodeTypeRegistry::built_in_record_type_defs`) are application-supplied schemas like `ElementMapping = {from: Int, to: Int}` (consumed by `atom_replace.rules`). They share the user-type namespace with user defs and networks — `name_is_taken` consults this map, and `add_record_type_def` / `rename_record_type_def` reject collisions with built-in names. **Always look up named record defs through the unified accessor `NodeTypeRegistry::lookup_record_type_def(name)`** — it tries `record_type_defs` first, then falls back to `built_in_record_type_defs`. Direct indexing into `record_type_defs` silently misses built-ins. The same pattern applies to the `populate_custom_node_type_cache_with_types` helpers, which take both maps as parameters. Design doc: `doc/design_atom_replace_rules_input.md` (Phase A).

## Zones (inline HOF bodies)

The higher-order-function nodes (`map`, `filter`, `fold`, `foreach`) own an **inline body** — a `NodeNetwork` held on the HOF's `Node.zone: Option<Arc<NodeNetwork>>`. Body nodes' positions live in the body's own coordinate frame; `next_node_id` is per-body, so the same numeric id can appear in nested bodies.

**Pin sets.** A zone-owning `NodeType` declares both `zone_input_pins` (inside-facing source pins on the body's inner-left edge — `element`, `acc`) and `zone_output_pins` (inside-facing destination pins on the body's inner-right edge — `result`, `new_acc`, `out`). The four external pin sets (regular input/output) coexist on the same HOF node. Test `NodeType::has_zone()` to detect HOF types.

**Wire shapes.** A wire stored on a body node's `arguments` can have `source_scope_depth ≥ 0`:
- `depth = 0` — regular intra-body wire (source in the same network).
- `depth ≥ 1` with `source_pin = NodeOutput {..}` — **capture** from an ancestor scope's node output.
- `depth ≥ 1` with `source_pin = ZoneInput { pin_index }` — **iteration-value reference** from an enclosing HOF's zone-input pin (`element`, `acc`).

Body-return wires live on the HOF's separate `zone_output_arguments` list (one `Argument` per declared zone-output pin) — they read a body-internal source and feed the HOF's per-iteration return. The discriminator is `ArgumentKind::ZoneOutput`; everything else is `External`.

**Evaluation.** Each HOF obtains a `ZoneClosure` (`evaluator/zone_closure.rs` — body + frozen captures + `zone_output_wires` + `owner_node_id` + type metadata) via `obtain_closure`: if the HOF's optional `f: Function` pin is wired, it takes the wired-in closure; otherwise it builds one from its own inline body (`build_inline_closure`). It then runs that closure one element at a time through the shared `zone_closure::run_closure_once`. `Walker::MapZone` / `FilterZone` carry the closure and call `run_closure_once` (with a body-only stack) lazily per `next`; `fold` and `foreach` are eager — they drain the upstream walker in `eval()` and call `run_closure_once` (with their real network stack, so nested deep captures resolve) per step against a freshly built inner context. Captures resolve via `evaluate_arg` walking up the scope-stack `ancestors` chain by `source_scope_depth`.

**Closures (function values).** The same `ZoneClosure` bundle is the payload of `NetworkResult::Function`. The **`closure`** node (`nodes/closure.rs`) *produces* one — it is a zone-bearing node whose `eval` wraps `build_inline_closure` as a value instead of feeding it to a walker — and the HOF `f` pins / the **`apply`** node (`nodes/apply.rs`) *consume* one. Closures Phase 2 deleted the legacy `evaluate_zone_output`, `FunctionEvaluator`, and the `output_pin_index == -1` Closure-construction branch; the `f` pin is a real `DataType::Function` value pin, not that old `-1` convention. See `nodes/AGENTS.md` (Closures section), `evaluator/AGENTS.md` (Walker section), and `doc/design_closures.md`.

**Validation** (`network_validator.rs::validate_zones_recursive`) enforces three rules across the recursive zone tree:
1. Every zone-output pin has at least one incoming wire (error attributed to the HOF in its parent network).
2. Capture wires reference an existing node in the ancestor at the named depth (error attributed to the body-internal destination).
3. `ZoneInput { pin_index }` references point to a real zone-input pin index of an actual ancestor HOF (error attributed to the body-internal destination).

Closures (Phase 5) layer two more rules on the same pass: rule 1 is **suspended** for an HOF whose `f` (Function) pin is connected (the wired-in closure drives evaluation, so an empty inline body is fine — `function_input_pin_connected` gates this); and the `apply` node, which has no inline body to fall back on, is flagged when its **required** `f` pin is disconnected. Function-typed `f`-source compatibility falls out of ordinary wire type-checking against the declared `Function` pin type (no special-case code).

Body errors land on `body.validation_errors` with `node_id == Some(body_internal_id)`; the API's `build_node_view` filters by `node_id` and surfaces them on the body node's `NodeView.error`. The HOF in the parent network also gets a generic "Zone body is invalid" marker so it lights up red even when only a deep body node is at fault.

**Repair.** When an HOF's zone-input pin type changes (e.g. `map.input_type` flipped `Int → Crystal`), `repair_node_network::repair_zone_body` walks the body and disconnects any wire whose source/destination types are no longer compatible — same shape as the existing `arguments` repair, just scoped to one body. Uses the borrow-split pattern (snapshot `zone_output_wires`, then `.zone.take()` to repair, then re-insert).

**Walking a network's nodes — `walk_all_nodes` / `walk_all_nodes_mut`.** When a function needs to do per-node work over an entire `NodeNetwork` — populate per-node caches, look up references to named types/networks, rewrite `node_type_name` or per-node `DataType` fields on a rename, count or collect references for a dependency closure — use the recursive helpers in `node_network.rs`:

```rust
walk_all_nodes(network, &mut |node| { ... });
walk_all_nodes_mut(network, &mut |node| { ... });
```

instead of a bare `for node in network.nodes.values()` loop. The helpers descend into every `Node.zone` body at every depth, so body-internal nodes get the same treatment as top-level ones. Mutable access goes through `Node::zone_mut`, which CoW-clones the `Arc<NodeNetwork>` on first mutation.

A bare `network.nodes.values()` walk silently skips every node inside every HOF body. The recurring bug shape it produces: after a `.cnnd` round-trip (or another state-refresh path) the body's nodes are missing whatever derived state the walk was supposed to produce, and the first downstream consumer panics or misbehaves. `initialize_custom_node_types_for_network` (body `expr` had no `custom_node_type`, parameter access panicked at load) was the precipitating bug; the post-fix sweep also routed dependency walks, rename/import cascades, delete-safety checks, and parameter-interface repair through the recursive helpers.

The exceptions — places where a single-frame walk is intentional — are selection state, layout/sugiyama positioning, per-network camera, text-format editing of the active network, and similar UI-frame bookkeeping. When in doubt, prefer the helper.

Design docs: `doc/design_zones.md` (Rust side, phases 1–6) and `doc/design_zones_ui.md` (Flutter side, phases U1–U7).

## Execute action & effect nodes

A small set of nodes (`export_xyz`, `foreach`, future effects) exist for their **side effects** rather than to produce a value. These nodes return `DataType::Unit` so the graph passes them through cleanly without misrepresenting them as data sources, and they fire only when the user explicitly invokes the right-click → Execute action.

The mechanism is one flag, one rule, one helper:

- **`NetworkEvaluationContext.execute: bool`** (in `evaluator/network_evaluator.rs`). Default `false` (display passes); set to `true` for one evaluation pass by `StructureDesigner::execute_node`. Flows into inner-body evaluations — the lazy zone walkers run bodies against the same context, the eager HOFs (`fold`/`foreach`) copy it into a `fresh_inner_for_eager_body` context — so effects nested inside `map` / `filter` / `fold` / `foreach` chains fire correctly under Execute.
- **Central skip rule** (in `evaluate_all_outputs`). Before dispatching to `NodeData::eval`, if `!context.execute` AND every resolved output pin of the node is `DataType::Unit`, the call is skipped and an `EvalOutput` of `NetworkResult::Unit` per pin is synthesised directly. This gates *every* `Unit`-returning node in one place — no per-node guards, no risk of forgetting one. The check uses the **resolved** output type via `resolve_output_type`, not the declared one, so a future `SameAsInput` pin that resolves to `Unit` is also covered.
- **`StructureDesigner::with_eval_context(execute, |evaluator, registry, prefs, context| { … })`** is the one `NetworkEvaluationContext::new()` caller in the `structure_designer` crate. (The eager HOFs build their body context via `fresh_inner_for_eager_body` — a struct literal, outside the `::new()` audit; the old `FunctionEvaluator::evaluate` inner-body `::new()` site is gone as of closures Phase 2.) The helper sets `execute`, runs the closure, then drains `context.print_buffer` into `self.print_log`. Reviewers grepping for `NetworkEvaluationContext::new(` outside this site and outside test crates have a one-shot audit.

`execute_node` records `pass_start = self.print_log.len()` *before* the pass and slices `self.print_log[pass_start..]` *after* to populate `APIExecuteResult.logs` — this returns only the prints from the pass while leaving any pre-existing display-pass entries in `print_log` for the Console panel's regular `take_print_log` polling cadence to drain. Without this slicing the panel would re-receive prior entries via `APIExecuteResult.logs` and double-display them.

`StructureDesigner.print_log: Vec<PrintLogEntry>` accumulates entries pushed by the `print` node (and any future node that wants to surface text to the in-app Console panel). `take_print_log()` drains and returns; `clear_print_log()` empties without returning.

Authoring guidance for effect-node `eval` arms: call effect logic unconditionally — the central rule guarantees `eval` is only invoked under `context.execute == true`. **Do not** add `if context.execute` guards inside individual effect nodes' `eval`. Light per-eval input validation that used to surface during display now defers to Execute; recover eager UX feedback via `get_subtitle` (see `nodes/export_xyz.rs::get_subtitle` for the `(no file name)` pattern). Design doc: `doc/design_node_execution.md`.

## Change Tracking & Refresh

`StructureDesignerChanges` tracks per-node visibility/data/selection changes. `RefreshMode` controls evaluation scope:
- `Lightweight` - UI-only changes (selection, camera)
- `Partial` - Re-evaluate only changed nodes (default)
- `Full` - Re-evaluate entire network

## Testing

Tests go in `rust/tests/structure_designer/`. Key test files:
- `structure_designer_test.rs` - Core operations
- `text_format_test.rs` - Text format parsing/serialization
- `cnnd_roundtrip_test.rs` - File format roundtrips
- `node_snapshot_test.rs` - Node type snapshots (insta)
- `undo_test.rs` - Global undo/redo tests
- `atom_edit_undo_test.rs` - atom_edit undo/redo tests

Run: `cd rust && cargo test --test structure_designer`
