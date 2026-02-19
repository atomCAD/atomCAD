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
| `NodeType` | `node_type.rs` | Node signature: parameters, output type, serialization fns |
| `NodeData` (trait) | `node_data.rs` | Per-node behavior: evaluation, gadgets, properties |
| `DataType` | `data_type.rs` | Pin type system (Bool, Float, Vec3, Geometry, Atomic, etc.) |
| `NodeTypeRegistry` | `node_type_registry.rs` | Registry of built-in + custom (user-defined) node types |
| `NetworkResult` | `evaluator/network_result.rs` | Evaluated node output value |

## Data Flow

```
User Action → StructureDesigner method
  → Modify NodeNetwork (add/connect/delete nodes)
  → Track changes in StructureDesignerChanges
  → NetworkEvaluator generates StructureDesignerScene
  → Scene sent to renderer/Flutter UI
```

## Type System

`DataType` governs pin compatibility. Conversion rules:
- Int ↔ Float, IVec2 ↔ Vec2, IVec3 ↔ Vec3
- Single value → Array (broadcasting)
- Function partial application
- UnitCell → DrawingPlane (legacy)

Check `DataType::can_be_converted_to()` for the complete rules.

## Node Networks as Custom Types

A `NodeNetwork` can itself become a node type usable in other networks. The `NodeTypeRegistry` manages both built-in node types and user-defined network-as-node types. Parameter nodes in a network become the custom type's input pins.

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

Run: `cd rust && cargo test --test structure_designer`
