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

- **structure_designer/** - Node network, evaluator, serialization (.cnnd) (see `src/structure_designer/AGENTS.md`)
- **crystolecule/** - Atomic structures, unit cells, motifs, lattice operations (see `src/crystolecule/AGENTS.md`)
- **geo_tree/** - CSG types, SDF evaluation, geometry caching (see `src/geo_tree/AGENTS.md`)
- **renderer/** - wgpu rendering, shaders (*.wgsl), mesh management
- **display/** - Tessellates domain objects (atoms, geometry) into meshes
- **expr/** - Expression language (lexer, parser, validation)
- **api/** - Flutter Rust Bridge API layer

## Adding a New Node Type

1. Create `src/structure_designer/nodes/my_node.rs`
2. Add to `src/structure_designer/nodes/mod.rs`
3. Register in `src/structure_designer/node_type_registry.rs`

## Addressing Nodes Across Scopes (zones)

HOF zone bodies (`map` / `filter` / `fold` / `foreach` / `closure`) are nested `NodeNetwork`s with **per-body `next_node_id` counters**, so a node id is only unique *within one network* — a body node and a top-level node routinely share the same numeric id. Any lookup that resolves a node **by bare `u64` id is ambiguous**, and that was the source of the zones property-panel bug (clicking a body `expr` showed the outer one, or the panel spun forever because the id collided with a non-`expr` node).

Rules — do not regress these:

- **`StructureDesigner::get_node_network_data[_mut]` are TOP-LEVEL-ONLY** (they do not recurse into bodies). Use them only for interactive subsystems that act on the top-level *active* node — currently `atom_edit` / `edit_atom` (they resolve through the top-level network's `active_node_id`, so they cannot target a body node by design). **Never** reintroduce a "walk every body and return the first id match" lookup — it silently returns the wrong node on a collision.
- Anything that can target a node in **any** scope — every `get_*_data` / `set_*_data` property API, `execute_node`, comment ops, `facet_shell`/`import_xyz`/`import_cif` actions, etc. — **must take a `scope_path: Vec<u64>` parameter** and resolve through `StructureDesigner::get_scope_network(&scope_path)`: reads via `get_node_network_data_scoped(&scope_path, node_id)`, in-place mutations via `get_node_network_data_mut_scoped(&scope_path, node_id)`, whole-data replacement via `set_node_network_data_scoped(&scope_path, node_id, …)`. `scope_path` empty = top-level active network; non-empty = the chain of HOF node ids down to the body.
- **When you add a new node property getter/setter in `src/api/structure_designer/`, it must take `scope_path` like its siblings.** A bare-`node_id` getter is exactly the mistake to avoid; the Flutter property panel always has the selected node's scope and passes it.

See `src/structure_designer/AGENTS.md` (Zones) for the body model and `walk_all_nodes` (the parallel "bare iteration skips body nodes" lesson).

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

## Testing

**⚠️ IMPORTANT: Never use `#[cfg(test)]` inline test modules in source files.**

When adding new functionality to the Rust codebase:

1. **Write tests for new core logic** - especially for functions in `structure_designer/`, `crystolecule/`, `geo_tree/`, `expr/`, etc.
2. **Tests go in `rust/tests/`**, NOT inline in source files
3. **Mirror the source file hierarchy** in the test directory:
   - Source: `src/structure_designer/text_format/`
   - Test: `tests/structure_designer/text_format_test.rs`
4. **Register test modules** in the parent test file (e.g., add to `tests/structure_designer.rs`):
   ```rust
   #[path = "structure_designer/text_format_test.rs"]
   mod text_format_test;
   ```
5. Follow the existing folder structure:
   - `rust/tests/structure_designer/` - Structure designer tests
   - `rust/tests/crystolecule/` - Atomic structure tests
   - `rust/tests/geo_tree/` - Geometry tests
   - `rust/tests/expr/` - Expression language tests
   - `rust/tests/integration/` - Integration/roundtrip tests

**When tests may be skipped:**
- **API wrapper functions** (`rust/src/api/`) - these are thin wrappers; test the underlying core function instead
- **Renderer/GPU code** - difficult to test without a GPU context
- **Trivial getters/setters** - unless they contain logic

**Test file naming:** `<module>_test.rs` (e.g., `structure_designer_test.rs`)

**Running tests:**
```bash
cd rust && cargo test                    # Run all tests
cd rust && cargo test <test_name>        # Run specific test
cd rust && cargo test --test structure_designer  # Run tests in a specific test crate
```

## Debugging

- `println!()` output appears in Flutter console
- `dbg!()` macro for value inspection
