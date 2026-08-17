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
├── scene_tessellator.rs       # Scene graph → renderer meshes (calls atomcad-display)
├── node_network.rs            # NodeNetwork + Node: the core DAG
├── node_type.rs               # NodeType: node signature definition
├── node_data.rs               # NodeData trait: per-node behavior
├── data_type.rs               # DataType enum: type system for pins
├── node_type_registry.rs      # Central registry of all node types
├── network_usages.rs          # Find Usages: read-only collection of a network's instance nodes
├── network_validator.rs       # Validates and repairs networks
├── node_dependency_analysis.rs    # Computes downstream dependents
├── node_display_policy_resolver.rs # Controls node visibility
├── displayed_node_refs.rs     # Eligibility-gated collection of the scene's displayed nodes
├── selection_factoring.rs     # Extracts selection into subnetwork
├── node_inlining.rs           # Inlines a custom-node instance (inverse of factoring)
├── closure_network_conversion.rs # Converts closure ⇄ custom-network instance (function-value forms)
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
| `DataType` | `data_type.rs` | Pin type system: primitives (incl. `IMat3`/`Mat3` 3x3 matrices), `LatticeVecs`, `Structure`, the three phase types (`Blueprint`, `Crystal`, `Molecule`) and their abstract supertypes (`HasAtoms`, `HasStructure`, `HasFreeLinOps`), `Record(RecordType)` where `RecordType` is either `Named(String)` (registry reference) or `Anonymous(Vec<(String, DataType)>)` (inline schema, sorted by field name), `Array(Box<DataType>)` and `Iterator(Box<DataType>)` (`Iter[T]`, lazily-evaluated stream — see `evaluator/AGENTS.md` for the runtime walker), `Function(FunctionType)` (concrete function value type, stored in canonical **flat** form — currying-equivalent shapes are absorbed by `FunctionType::new`), `AnyFunction { leading_params: Vec<DataType> }` (input-only "any function whose params start with `leading_params`" — empty list = any function, used by `apply.f`; non-empty = used by `map.f` for the starts-with rule, see `doc/design_function_pin_unification.md`), and `Unit` (the type with exactly one value — the return type of effect nodes; supports a universal `T → Unit` discard widening at field level, and `Unit → T` is rejected) |
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

`DataType` governs pin compatibility. **The complete conversion-rule inventory is the doc comment on `DataType::can_be_converted_to`** (`data_type.rs`) — numeric/vector widenings, broadcasting, phase upcasts, the `Function`/`AnyFunction` structural match, and the iterator rules including the deliberately-absent `Iter[T] → Array[T]`. Read it before adding or relaxing a rule. `DataType::is_abstract()` identifies the three abstract supertypes.

Two properties of the system are worth stating outside that inventory because they constrain node *design*, not just wire checks:

- Records are **structurally** typed (names don't gate compatibility) with **width + structural depth subtyping**. At leaf field positions only **tag-only widenings** (identity + concrete-to-abstract phase upcasts, factored into `is_tag_only_widening`) are accepted — value-converting widenings like `Int → Float` are rejected at field level so destructure pins can pass the runtime payload through unchanged. Subtyping requires `&NodeTypeRegistry` to resolve `Named` references. See `doc/design_record_types.md`.
- Conversions are **implicit at the wire**, so anything deliberately excluded from them needs an explicit node instead: `collect` for `Iter[T] → Array[T]`, `imat3_diag` for `IVec3` → diagonal `IMat3`.

### Three-Phase Model (lattice-space refactoring)

Objects in the node network flow through three concrete phases:

| Phase | Ingredients | Role |
|---|---|---|
| **Blueprint** | Structure + Geometry | *Design.* Geometry is a "cookie cutter" positioned in an infinite crystal field. |
| **Crystal** | Structure + Geometry (opt) + Atoms | *Construction.* Atoms have been carved out of the structure; atoms + geometry are rigidly coupled. |
| **Molecule** | Geometry (opt) + Atoms | *Deployment.* No structure association; free-floating. |

Three **abstract** supertypes name two-out-of-three combinations. Built-in nodes use them only as input-pin types, but they can also appear as *statically declared* output types: a user-declared function type with an abstract return (e.g. a `closure` declared `(Float) -> HasAtoms`) puts the abstract type on the consuming `apply`'s output pin. `resolve_output_type` resolves such a `Fixed(abstract)` pin to the abstract type, so it wires into pins accepting the same abstract type (identity conversion); abstract → concrete downcasts and cross-abstract edges remain rejected. Runtime values are always a concrete phase variant regardless:

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

**Evaluation.** Each HOF obtains a `ZoneClosure` via `zone_closure::obtain_closure` (the wired `f` pin's closure when connected, else one built from its own inline body) and runs it per element through the shared `zone_closure::run_closure_once`. The bundle's fields, the lazy/eager split, and the load-bearing `network_stack` argument are documented in `evaluator/AGENTS.md` (§`run_closure_once`) and `evaluator/zone_closure.rs`. Captures resolve via `evaluate_arg` walking up the scope-stack `ancestors` chain by `source_scope_depth`.

**Closures (function values).** The `closure` node (`nodes/closure.rs`) is a zone-bearing node whose `eval` wraps its inline body as a `NetworkResult::Function` value; the HOF `f` pins and the `apply` node consume one. The body-model consequence is that a `closure` needs **no special lifecycle code** — `has_zone()`, CoW body cloning, copy/paste, undo, and `walk_all_nodes` recursion all apply to it unchanged. Everything else about closures — `ClosureKind`, `apply`'s consumption loop and `pre_supplied_args`, the `AnyFunction` pin declarations and their `update_*_pin_layouts_for_network` post-passes — lives in `nodes/AGENTS.md` (§Closures) and `evaluator/AGENTS.md`. Design docs: `doc/design_closures.md`, `doc/design_currying.md`, `doc/design_custom_closure_kind.md`, `doc/design_function_pin_unification.md`.

**`apply`'s pin layout is *derived*, unserialized state.** Any pass that touches an `apply` before that layout has been derived must preserve its `arguments` **positionally** (`update_apply_pin_layouts_for_network_preserving_args`, run **before** `repair_network_arguments`) — a by-name rebuild or a truncation against the under-derived `[f]` layout silently drops the `arg0…` wires. Full reasoning, the current call-site list, and the regression tests are in `nodes/apply.rs`'s module doc; the load-order half is in `serialization/AGENTS.md` ("Load pipeline & derived state").

**Function pins** (`doc/design_function_pins.md`) revived an `output_pin_index == -1` branch in `NetworkEvaluator::evaluate` with new semantics: `zone_closure::build_node_function_closure` (see its doc comment) synthesizes a `ZoneClosure` from "the whole node viewed as a function of its inputs", so the title-bar `-1` pin is a real `NetworkResult::Function` source consumed exactly like a `closure` output. `NodeNetwork::function_pin_consumed(node_id)` is the derived **function-mode** predicate (surfaced as `NodeView.function_pin_consumed`): it gates connection rules, the `Supplied`-required warning, and the `-1`-wire undo refresh mode, but **not** display (`doc/design_function_pin_roles.md` §"Display relaxation"). Wiring an input pin on such a node is **not** forbidden — the old "function pin and input pins are mutually exclusive" rule was **removed** (see the `// Function-mode mutual exclusion is gone` comment in `node_network.rs::can_connect_nodes`). A wired input freezes that pin as a *capture*: it drops out of the exposed parameter list and the arity re-derives on the next validate pass. That wired-input-as-capture idiom is what the closure ⇄ network conversions (`doc/design_closure_network_conversion.md`) rewrite between.

**Function pin roles** (`doc/design_function_pin_roles.md`, issue #408) let the user *override* that wiring-derived partition per pin. `Node.function_pin_roles: BTreeMap<usize, FunctionPinRole>` is sparse and index-keyed: `Delayed` forces a parameter (a wire on the pin becomes a **preview / type witness** — it feeds pin-0 display and type resolution but is dropped from the synthesized body, so it is invocation-inert), `Supplied` forces a capture (wired → capture the wire; unwired → leave the body argument empty so the node's own stored-data fallback supplies the gizmo-edited value at invocation). **The map never stores an explicit `Auto`** — absence *is* `Auto`, so "no overrides" is one canonical state and role-free files stay byte-identical; `StructureDesigner::set_function_pin_role` normalizes and the loader prunes hand-authored `Auto` entries.

**The partition lives in exactly one place: `node_network::function_pin_dispositions`** — see its doc comment for the two consumers and the type-unsoundness argument. The `-1` *types* likewise come from the one shared `NodeTypeRegistry::resolve_function_pin_signature[_scoped]`. Note that signature helper returns the **un-canonicalized** `(params, return)` pair: the resolver wraps it in `FunctionType::new` for the wire type, while the closure's `param_types` must stay the body's actual frame size (see `evaluator/AGENTS.md`).

The Flutter surface is the sidebar's generic "Function output" section (scoped API pair `get_function_pin_roles` / `set_function_pin_role`); the getter's per-pin `effective` field is built from the same `function_pin_dispositions` helper, so the UI renders the partition rather than re-deriving it — keep it that way.

One consequence that is easy to lose: a `-1` wire's connect/delete toggles the source node's `function_pin_consumed` state, and the `NodeDataChanged` undo arm's conditional revalidation **cannot** cover that (it tests consumption *after* the undo, so the leg that removes consumption always reads "not consumed" and skips) — `ConnectWireCommand` / `DeleteWiresCommand` therefore report `UndoRefreshMode::Full` for `-1` wires specifically.

**Validation** (`network_validator.rs::validate_zones_recursive`) enforces four rules across the recursive zone tree:
1. Every zone-output pin has at least one incoming wire (error attributed to the HOF in its parent network).
2. Capture wires reference an existing node in the ancestor at the named depth (error attributed to the body-internal destination).
3. `ZoneInput { pin_index }` references point to a real zone-input pin index of an actual ancestor HOF (error attributed to the body-internal destination).
4. **No `parameter` node lives in a body** (issue #417) — a `parameter` declares an input pin of the enclosing *network*, and a body has no interface. Single-sourced as `node_type_registry::allowed_in_zone_body(name)`; the authoring paths refuse it up front and `ParameterData::eval` guards it off `NetworkStackElement::is_zone_body` (**never** reconstruct that from stack shape or `eval_scope_path`). Rationale in `network_validator.rs`'s module doc.

Closures (Phase 5) layer two more rules on the same pass: rule 1 is **suspended** for an HOF whose `f` (Function) pin is connected (the wired-in closure drives evaluation, so an empty inline body is fine — `function_input_pin_connected` gates this); and the `apply` node, which has no inline body to fall back on, is flagged when its **required** `f` pin is disconnected. Function-typed `f`-source compatibility falls out of ordinary wire type-checking against the declared `Function` pin type (no special-case code).

Body errors land on `body.validation_errors` with `node_id == Some(body_internal_id)`; the API's `build_node_view` filters by `node_id` and surfaces them on the body node's `NodeView.error`. The HOF in the parent network also gets a generic "Zone body is invalid" marker so it lights up red even when only a deep body node is at fault.

**Repair.** When an HOF's zone-input pin type changes (e.g. `map.input_type` flipped `Int → Crystal`), `repair_node_network::repair_zone_body` walks the body and disconnects any wire whose source/destination types are no longer compatible — same shape as the existing `arguments` repair, just scoped to one body. Uses the borrow-split pattern (snapshot `zone_output_wires`, then `.zone.take()` to repair, then re-insert).

**Walking a network's nodes — `walk_all_nodes` / `walk_all_nodes_mut`.** When a function needs to do per-node work over an entire `NodeNetwork` — populate per-node caches, look up references to named types/networks, rewrite `node_type_name` or per-node `DataType` fields on a rename, count or collect references for a dependency closure — use the recursive helpers in `node_network.rs` instead of a bare `for node in network.nodes.values()` loop — a bare walk silently skips every node inside every HOF body. The recurring bug shape it produces: after a `.cnnd` round-trip (or another state-refresh path) the body's nodes are missing whatever derived state the walk was supposed to produce, and the first downstream consumer panics or misbehaves (`initialize_custom_node_types_for_network` was the precipitating case). See the two fn doc comments for traversal order and the `zone_mut` CoW detail.

The exceptions — places where a single-frame walk is intentional — are selection state, layout/sugiyama positioning, per-network camera, text-format editing of the active network, and similar UI-frame bookkeeping. When in doubt, prefer the helper.

Design docs: `doc/design_zones.md` (Rust side, phases 1–6) and `doc/design_zones_ui.md` (Flutter side, phases U1–U7).

## Validation errors: blocking vs non-blocking vs interface

`ValidationError` carries `blocking: bool` and `interface: bool` fields (`node_network.rs`). Since error-management Phase 3 (`doc/design_error_management.md` D3/D5) the blast radius of a blocking error is **the offending node's downstream cone, not the network**:

- **Blocking error attributed to a node** (`ValidationError::new(text, Some(id))`, the default) → the node is **cone-poisoned**: the central gates in `NetworkEvaluator::evaluate` / `evaluate_all_outputs` (skip-and-synthesize) see the error before dispatch and *skip the node's `eval` entirely*, synthesizing a `NetworkResult::Error` from the error text(s) (all of the node's blocking texts joined with newlines — `node_network::node_poison_message`) as the node's output and recording it under the node's `NodeRef`. Downstream consumers receive it through the ordinary chaining machinery; independent nodes evaluate untouched. Safety comes from *not evaluating* — the historical reason these rules blocked is that evaluating the broken node could panic or emit garbage (type mismatch → `extract_*().unwrap()`), and skip-and-synthesize never enters that code path. A poisoned node's `-1` function pin is refused too (the synthesized closure would smuggle the broken wiring into a body the poison lookup can't see).
- **Non-blocking error** (`ValidationError::warning(..)`) → advisory: surfaces as a badge, the node still evaluates. Use when the node's output remains at least partially useful (e.g. `Supplied`-but-unwired — pin 0 still displays; a `motif_sub` / `materialize` parse error, whose `eval` ignores the failure and still emits a usable value).
- **Interface error** (`ValidationError::interface_error(..)` — today only the `validate_parameters` rules) → whole-network refusal: a corrupted parameter interface desyncs call sites that map arguments by index (the known OOB-panic class), so no cone can contain it.

**`NodeNetwork::valid` means "free of the interface residue"**: no interface error and no blocking error without node attribution (`node_id == None` — nothing to poison). The predicate is `node_network::has_interface_residue`, computed in exactly one place — the end of `validate_network`. Node-attributed blocking errors do **not** flip `valid`. Every `.valid` reader asks "is this network usable at all?" and inherits the residue meaning through the flag: the scene gate (`generate_scene_scoped`), the custom-network eval refusal (`"{name} is invalid"`), the "References invalid node network" rule, the `execute_node`/CLI gates, the upward validity cascade. Cross-network consequences:

- Custom network B with only node-attributed blocking errors stays `valid`; instances evaluate B normally. A poisoned node inside B's return cone propagates out of an instance as an ordinary chained runtime error (exact per-call-site localization); outside the return cone, instances are completely unaffected.
- B with interface residue → the "References invalid node network" rule stamps a blocking error on each instance → the instance is a poisoned node in its parent; the localized `"{name} is invalid"` eval refusal remains as defense-in-depth. The parent renders everything else.

**The litmus test when you add a new validation rule** is no longer "is eval safe?" (skip-and-synthesize makes any blocking rule safe) — it is *"is the node's output still useful?"*

- Output unavailable/meaningless → `ValidationError::new` (blocking; costs one cone, not the network).
- Output still partially useful → `ValidationError::warning`.
- Corrupts the network's parameter interface → `ValidationError::interface_error` — a deliberate, rare choice, like the blocking/warning choice but rarer.

One rule of that litmus is easy to get wrong and expensive when you do: **a blocking rule inside a zone body must not also set `validate_zones_recursive`'s local `ok = false`** unless the *owner's* eval is genuinely broken — `ok` raises the `ZONE_BODY_INVALID_MARKER` on the enclosing HOF, i.e. it poisons the whole HOF, when one stray broken body node should only darken its own cone.

The rest of the validator's mechanics live in **`network_validator.rs`'s module doc**: why the passes accumulate (an unrecorded error is an unpoisoned node), the stored-data error channel (`NodeData::get_data_error`), wire-cycle detection and the `EvalFrameKey` re-entrancy guard, and the Phase 6 severity sweep with its "re-validate after poking the registry directly" consequence.

A user does not distinguish "validation error" from "runtime error" — both mean "this part is broken." The severity flags are purely about **blast radius**: warning = badge only, blocking = this node + its downstream cone, interface = the whole network and its instances.

**Interaction with re-validation heuristics.** Anything that re-validates *to clear stale local errors after a structural edit* must key off `!network.validation_errors.is_empty()`, **not** `!network.valid` — node-attributed errors (blocking or not) leave `valid == true` now, so a `!valid` check misses nearly everything (this generalizes the old `delete_selected` fix). By contrast, every gate that asks *"can this network be evaluated / executed / referenced at all?"* and the validity-**flip** dependency propagation in `validate_active_network` must stay on `valid` — switching those to `validation_errors` would re-block the network on a cone-scoped error and cascade the blanking across network boundaries.

## Execute action & effect nodes

A small set of nodes (`export_atoms`, `foreach`, future effects) exist for their **side effects** rather than to produce a value. These nodes return `DataType::Unit` so the graph passes them through cleanly without misrepresenting them as data sources, and they fire only when the user explicitly invokes the right-click → Execute action.

The flag (`NetworkEvaluationContext.execute`) and the **central skip rule** that gates every `Unit`-returning node in one place — including how it propagates into inner bodies — are documented in `evaluator/AGENTS.md` (§"Central skip rule"); the node-authoring side is in `nodes/AGENTS.md` (§Effect nodes). The `structure_designer`-level half is the orchestration:

- **`StructureDesigner::with_eval_context(execute, |evaluator, registry, prefs, context| { … })`** is the one `NetworkEvaluationContext::new()` caller in the `structure_designer` crate. (The eager HOFs build their body context via `fresh_inner_for_eager_body` — a struct literal, outside the `::new()` audit; the old `FunctionEvaluator::evaluate` inner-body `::new()` site is gone as of closures Phase 2.) The helper sets `execute`, runs the closure, then drains `context.print_buffer` into `self.print_log`. Reviewers grepping for `NetworkEvaluationContext::new(` outside this site and outside test crates have a one-shot audit.

`execute_node` records `pass_start = self.print_log.len()` *before* the pass and slices `self.print_log[pass_start..]` *after* to populate `APIExecuteResult.logs` — this returns only the prints from the pass while leaving any pre-existing display-pass entries in `print_log` for the Console panel's regular `take_print_log` polling cadence to drain. Without this slicing the panel would re-receive prior entries via `APIExecuteResult.logs` and double-display them.

`StructureDesigner.print_log: Vec<PrintLogEntry>` accumulates entries pushed by the `print` node (and any future node that wants to surface text to the in-app Console panel). `take_print_log()` drains and returns; `clear_print_log()` empties without returning.

Authoring guidance for effect-node `eval` arms: call effect logic unconditionally — the central rule guarantees `eval` is only invoked under `context.execute == true`. **Do not** add `if context.execute` guards inside individual effect nodes' `eval`. (The UX consequence — per-eval input validation deferring to Execute, recovered via `get_subtitle` — is in `nodes/AGENTS.md`.) Design doc: `doc/design_node_execution.md`.

## Reflow on Footprint Growth

When an edit grows a node's **rendered footprint in place** — without the user dragging anything — neighbours should be pushed out of the way so the grown node doesn't overlap them. Use the reusable primitive `StructureDesigner::reflow_for_footprint_change(scope_path, node_id, old_sizes) -> Vec<ScopedMoves>` rather than reinventing neighbour-pushing: it re-estimates the node's new size (`node_inlining::instance_size`), shifts the lower-right sweep band in that scope via `node_inlining::make_space_for_inline`, and **cascades up the scope chain** — a zone body that grew past its stored size grows its owning HOF in the parent network, repeating until a scope absorbs the growth (`delta == 0`). It only moves nodes and reports the moves; it does not push undo commands.

Pre-edit footprints **must be captured before mutating** (the bodies have already grown by the time reflow runs): `capture_footprint_chain(scope_path, node_id)` for a node growing in its own scope, `capture_body_owner_footprint_chain(scope_path)` for a body edit that grows the owning HOF one scope up (Case C). Triggers currently wired: HOF expand on `f`-disconnect (`delete_selected_scoped`), `set_collapse_mode`, in-body add·paste·duplicate·connect (`add_node_scoped` / `paste_at_position_scoped` / `duplicate_node_scoped` / `connect_nodes_scoped` / `connect_wire_scoped`), and `convert_instance_to_closure`. **Shrinks need no reflow** (pulling neighbours inward would be surprising — delta clamps to ≥ 0). The undo side bundles the moves into the same step via `CompositeCommand` — see `undo/AGENTS.md` ("Composite Commands & Reflow Bundling"). No Flutter change is needed: positions are authoritative in Rust and the `ScopeResolver` re-derives layout from them each frame. Design doc: `doc/design_reflow_on_footprint_change.md`.

## Change Tracking & Refresh

`StructureDesignerChanges` tracks per-node visibility/data/selection changes. `RefreshMode` controls evaluation scope:
- `Lightweight` - UI-only changes (selection, camera)
- `Partial` - Re-evaluate only changed nodes (default)
- `Full` - Re-evaluate entire network

**The scene is keyed by `NodeRef`, not by bare node id**, and the displayed set is **derived, not read from one map**. `StructureDesignerScene.node_data`, the invisible LRU cache, and `StructureDesignerChanges.visibility_changed` all key on `NodeRef { scope_path, node_id }` — per-body `next_node_id` counters let a body node and a top-level node share a numeric id, so a bare `u64` key silently clobbers. Both refresh paths get their work list from **`displayed_node_refs::collect_displayed_node_refs(network, registry)`**: the top-level `displayed_nodes` *plus*, recursively, the `displayed_nodes` of every body reachable through an **eligible chain** — one whose every zone-owning ancestor is a `closure` node with **zero** zone-input pins (`is_zero_ary_closure`). Everything else follows from that:

- Nodes inside a 0-ary closure body are **scene-evaluable**: `NetworkEvaluator::generate_scene_scoped` stands up the real network stack for the body so captures resolve by the ordinary stack walk and errors/hover strings key under the right `NodeRef`. The eligibility rule itself and the "display flags in an *ineligible* body are **dormant**, not cleared" invariant (never "revoke" them on an arity change) are stated in `displayed_node_refs.rs`'s module doc; the frameless evaluation and `try_current_zone_input` are in `evaluator/AGENTS.md`.
- `refresh_partial` therefore uses the collection (not `network.displayed_nodes` by bare id) as its "is this ref displayed?" oracle for cache moves, cache restores, and the data-change intersection; it invalidates cached entries for the **full scoped** affected set (a *hidden* body node's cache entry must still be dropped when a captured upstream source changes, else hide → edit → show restores stale geometry); and a `data_changed` zone-owner evicts its whole body subtree from the scene + cache (`StructureDesignerScene::remove_scope_subtree`) before re-collecting, because dependency analysis reaches *out of* a body, never *into* one.
- Click-to-activate (`viewport_pick`) filters its candidate set to **top-level** refs: the Flutter disambiguation overlay / `scrollToNode` / solo-hide are keyed by bare node id. Displayed body geometry is visible but not click-activatable.
- The eligibility rule has exactly **one** definition — `displayed_node_refs::is_body_scene_evaluable(parent_chain_eligible, node, registry)` — folded top-down by two callers that must never diverge: `collect_displayed_node_refs` (which bodies the scene evaluates) and `api::…::build_zone_view` (which bodies get eye icons, surfaced as `ZoneView::body_scene_evaluable` and consumed verbatim by Flutter). Divergence would show eyes for nodes that never render, or hide eyes for nodes that do.
- Both display mutators are scope-aware and undoable: `set_node_display_scoped` and `toggle_output_pin_display_scoped` (the bare-id variants delegate with an empty path). They mark visibility changed *at the node's own scope*, update the live/cached scene entry under the node's own `NodeRef`, and push a `scope_path`-carrying `SetNodeDisplayCommand` / `SetOutputPinDisplayCommand`. Body display flags are persisted (they serialize with the body network), so they must be undoable — and body state is also restored wholesale by `EditZoneBodyCommand`, which is only correct if every intervening body mutation is itself a command.

Design doc: `doc/design_zero_ary_closure_body_display.md` (issue #409).

## Testing

Tests go in `rust/tests/structure_designer/`. Key test files:
- `structure_designer_test.rs` - Core operations
- `text_format_test.rs` - Text format parsing/serialization
- `cnnd_roundtrip_test.rs` - File format roundtrips
- `node_snapshot_test.rs` - Node type snapshots (insta)
- `undo_test.rs` - Global undo/redo tests
- `atom_edit_undo_test.rs` - atom_edit undo/redo tests

Run: `cd rust && cargo test --test structure_designer`
