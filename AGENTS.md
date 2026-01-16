# atomCAD - Agent Instructions

## Subdirectory Instructions

**IMPORTANT:** When working on files in these directories (or any of their subdirectories), always read the corresponding AGENTS.md file first:

- Working in `rust/` or any descendant (e.g., `rust/src/`, `rust/src/structure_designer/`, etc.) → Read `rust/AGENTS.md`
- Working in `lib/` or any descendant (e.g., `lib/common/`, `lib/structure_designer/`, etc.) → Read `lib/AGENTS.md` (if it exists)

These files contain directory-specific conventions, testing requirements, and coding standards that must be followed.

## Project Overview

atomCAD is a CAD application for Atomically Precise Manufacturing (APM). It enables designing covalently bonded atomic structures constrained to crystal lattices. The application uses a **Rust backend** for high-performance CAD operations and a **Flutter frontend** for cross-platform UI.

**Repository:** https://github.com/atomCAD/atomCAD

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Flutter Frontend                        │
│  lib/                                                        │
│  ├── main.dart           # Entry point, CLI/GUI modes       │
│  ├── common/             # Shared UI widgets                │
│  ├── structure_designer/ # Main application UI              │
│  └── src/rust/           # Generated FFI bindings           │
├─────────────────────────────────────────────────────────────┤
│              Flutter Rust Bridge (FFI Layer)                 │
├─────────────────────────────────────────────────────────────┤
│                      Rust Backend                            │
│  rust/src/                                                   │
│  ├── api/                # Public API exposed to Flutter    │
│  ├── structure_designer/ # Node network system, evaluator   │
│  ├── crystolecule/       # Atomic structures library        │
│  ├── geo_tree/           # 3D geometry (CSG, SDF)           │
│  ├── renderer/           # GPU rendering (wgpu)             │
│  ├── display/            # Domain→renderer adapter          │
│  └── expr/               # Expression language              │
└─────────────────────────────────────────────────────────────┘
```

## Key Concepts

- **Node Network:** The core editing paradigm. Nodes form a DAG with typed pins (input/output). Wire connections define data flow.
- **Data Types:** `Geometry` (2D/3D shapes), `Atomic` (atoms and bonds), primitives (Float, Int, Vec3, etc.)
- **Non-destructive Editing:** All edits are parametric; the node network can be modified without losing work.
- **Crystolecule:** Atomic structures defined on crystal lattices with unit cells, motifs, and symmetry operations.

## Commands

```powershell
# Run the application (debug)
flutter run

# Run in release mode
flutter run --release

# Rust backend
cd rust && cargo build && cargo test && cargo clippy

# Regenerate FFI bindings after changing rust/src/api/*.rs
flutter_rust_bridge_codegen generate

# Before committing
dart format lib/
cd rust && cargo fmt && cargo clippy && cargo test
flutter analyze

# Run all Rust tests
cd rust && cargo test

# Run specific test categories
cargo test cnnd_roundtrip      # Integration/roundtrip tests
cargo test node_snapshots      # Snapshot tests
cargo test crystolecule        # Crystolecule module

# Update snapshots after intentional changes
cargo insta review

# Flutter smoke test
flutter test integration_test/
```

See `doc/testing.md` for test coverage details.

## Code Conventions

### Dart/Flutter
- State management: `ChangeNotifier` + `Provider`
- Prefix API imports: `import '...' as api_name;`

### Rust
- Edition 2024 (Rust 1.85+), stable toolchain only
- Use `thiserror` for errors, `glam` for math
- Keep modules independent; dependencies form a DAG
- **Tests go in `rust/tests/`, never inline with `#[cfg(test)]`**

### Flutter Rust Bridge
- API types in `rust/src/api/`, config in `flutter_rust_bridge.yaml`
- Generated code in `lib/src/rust/` — **do not edit**

## Adding Features

### New Node Type
1. Create `rust/src/structure_designer/nodes/my_node.rs`
2. Register in `nodes/mod.rs` and `node_type_registry.rs`

### New API Method
1. Add function in `rust/src/api/structure_designer/`
2. Run `flutter_rust_bridge_codegen generate`

## File Formats

- `.cnnd` - atomCAD project files (JSON-based)
- `.mol` - V3000 molecular format (export)
- `.xyz` - XYZ format (import/export)

## Documentation

See `doc/` directory for architecture, tutorials, and platform setup guides.
