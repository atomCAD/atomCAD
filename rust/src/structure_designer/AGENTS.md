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
| `DataType` | `data_type.rs` | Pin type system: primitives, `LatticeVecs`, `Structure`, the three phase types (`Blueprint`, `Crystal`, `Molecule`) and their abstract supertypes (`HasAtoms`, `HasStructure`, `HasFreeLinOps`) |
| `NodeTypeRegistry` | `node_type_registry.rs` | Registry of built-in + custom (user-defined) node types |
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
- Int ↔ Float, IVec2 ↔ Vec2, IVec3 ↔ Vec3
- Single value → Array (broadcasting)
- Function partial application
- LatticeVecs → DrawingPlane (legacy)
- Concrete phase type → its abstract supertypes (Crystal/Molecule → HasAtoms; Blueprint/Crystal → HasStructure; Blueprint/Molecule → HasFreeLinOps). No abstract → concrete downcasts, no cross-abstract edges.

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
