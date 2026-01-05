# Rust Backend - Agent Instructions

## Module Architecture

Dependencies flow downward (no circular dependencies):

```
┌─────────────────────────────────────────────────────┐
│                  structure_designer                  │
│  (Node network, evaluator, application logic)       │
├─────────────────────────────────────────────────────┤
│        display           │          api             │
│   (Tessellation)         │   (Flutter API layer)    │
├──────────────────────────┴──────────────────────────┤
│  crystolecule  │  geo_tree   │  renderer  │  expr   │
│  (Atoms/bonds) │  (CSG/SDF)  │  (wgpu)    │ (Lang)  │
├─────────────────────────────────────────────────────┤
│                       util                           │
└─────────────────────────────────────────────────────┘
```

## Key Modules

- **structure_designer/** - Node network, evaluator, serialization (.cnnd)
- **crystolecule/** - Atomic structures, unit cells, motifs, lattice operations
- **geo_tree/** - CSG types, SDF evaluation, geometry caching
- **renderer/** - wgpu rendering, shaders (*.wgsl), mesh management
- **display/** - Tessellates domain objects (atoms, geometry) into meshes
- **expr/** - Expression language (lexer, parser, validation)
- **api/** - Flutter Rust Bridge API layer

## Adding a New Node Type

1. Create `src/structure_designer/nodes/my_node.rs`
2. Add to `src/structure_designer/nodes/mod.rs`
3. Register in `src/structure_designer/node_type_registry.rs`

## Code Conventions

- **Edition:** Rust 2024 (requires Rust 1.85+)
- **Toolchain:** Stable only (`rust-toolchain.toml`)
- **Error handling:** Use `thiserror` for error types
- **Math:** Use `glam` (DVec2, DVec3, DMat4)
- **Parallelism:** Use `rayon` for data parallelism

## Key Types

| Type | Purpose |
|------|---------|
| `StructureDesigner` | Main application state |
| `NodeNetwork` | Graph of connected nodes |
| `NodeType` | Definition of a node kind |
| `NetworkResult` | Node output value |
| `AtomicStructure` | Collection of atoms/bonds |
| `GeoNode` | CSG operation tree |
| `ImplicitGeometry3D` | SDF geometry |

## Debugging

- `println!()` output appears in Flutter console
- `dbg!()` macro for value inspection
