# atomCAD - Agent Instructions

## Subdirectory Instructions

**IMPORTANT:** When working on files in these directories (or any of their subdirectories), always read the corresponding AGENTS.md file first:

- Working in `rust/` or any descendant (e.g., `rust/src/`, `rust/src/structure_designer/`, etc.) → Read `rust/AGENTS.md`
- Working in `rust/crates/atomcad-crystolecule/` or any descendant → Also read `rust/crates/atomcad-crystolecule/src/AGENTS.md`
- Working in `rust/crates/atomcad-crystolecule/src/simulation/` or any descendant → Also read `rust/crates/atomcad-crystolecule/src/simulation/AGENTS.md`
- Working in `rust/crates/atomcad-crystolecule/src/simulation/uff/` → Also read `rust/crates/atomcad-crystolecule/src/simulation/uff/AGENTS.md`
- Working in `rust/crates/atomcad-geo-tree/` or any descendant → Also read `rust/crates/atomcad-geo-tree/src/AGENTS.md`
- Working in `rust/src/structure_designer/` or any descendant → Also read `rust/src/structure_designer/AGENTS.md`
- Working in `rust/src/structure_designer/undo/` or any descendant → Also read `rust/src/structure_designer/undo/AGENTS.md`
- Working in `lib/` or any descendant (e.g., `lib/common/`, `lib/structure_designer/`, etc.) → Read `lib/AGENTS.md`
- Working in `lib/structure_designer/` or any descendant → Also read `lib/structure_designer/AGENTS.md`

These files contain directory-specific conventions, testing requirements, and coding standards that must be followed.

## Project Overview

atomCAD is a CAD application for Atomically Precise Manufacturing (APM). It enables designing covalently bonded atomic structures constrained to crystal lattices. The application uses a **Rust backend** for high-performance CAD operations and a **Flutter frontend** for cross-platform UI.

**Repository:** https://github.com/atomCAD/atomCAD

**Reading GitHub issues/PRs:** Use the `gh` CLI, e.g. `gh issue view <number> --repo atomCAD/atomCAD` (add `--comments` for discussion). `gh` requires authentication even for this public repo, so if it errors with "not logged in," ask the user to run `gh auth login` rather than falling back to scraping the web UI.

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
│  ├── (crystolecule → rust/crates/atomcad-crystolecule)      │
│  ├── (geo_tree → rust/crates/atomcad-geo-tree: CSG, SDF)    │
│  ├── (renderer → rust/crates/atomcad-renderer: wgpu)        │
│  ├── display/            # Domain→renderer adapter          │
│  └── expr/               # Expression language              │
└─────────────────────────────────────────────────────────────┘
```

## Key Concepts

- **Node Network:** The core editing paradigm. Nodes form a DAG with typed pins (input/output). Wire connections define data flow.
- **Data Types:** `Geometry` (2D/3D shapes), `Atomic` (atoms and bonds), primitives (Float, Int, Vec3, etc.), and structurally-typed `Record`s (named or anonymous; see `doc/design_record_types.md`)
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

# Flutter smoke test — HUMAN-ONLY, do not run as an AI agent (see below)
flutter test integration_test/
```

**AI agents must NOT run the Flutter smoke test (`flutter test
integration_test/`).** It is unreliable when driven from an agent shell on this
machine (app-launch/debug-connection failures, window-size-dependent layout
assertions) and burns minutes per attempt. The maintainer runs it manually.
When a task's checklist calls for the smoke test, treat it as part of the
manual walkthrough handed to the human — run the Rust suite and
`flutter analyze` yourself, then list the smoke test as a pending manual step.

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
- `.cif` - Crystallographic Information File (import)

## Documentation

See `doc/` directory for architecture, tutorials, and platform setup guides.

### User-facing reference guide — keep it in sync

`doc/atomCAD_reference_guide.md` is the **user-facing reference manual** (a hub
that links out to the pages under `doc/reference_guide/` — `ui.md`,
`node_networks.md`, `direct_editing.md`, the per-node pages under
`reference_guide/nodes/`, etc.). It documents the application from a user's
point of view: what each UI panel/control does, how navigation works, what every
node type does. It is distinct from the `doc/design_*.md` design docs (which
capture engineering rationale) and from the `AGENTS.md` files (which capture
conventions for contributors).

**When you add or change user-visible behavior — a new node type, a new UI
control, a changed interaction, a new menu item, a renamed field — update the
relevant reference-guide page in the same change.** Treat it like a test that
must be kept green: a feature is not "done" until the guide reflects it. If you
are unsure which page a change belongs on, grep `doc/reference_guide/` for the
nearest existing topic. Backend-only or internal-only changes (evaluator
internals, refactors, new private helpers) do **not** need a guide update — only
things a user can see or do.

### `AGENTS.md` files — keep them in sync too

The `AGENTS.md` files are the contributor-facing counterpart of the reference
guide: they describe the conventions, invariants, and gotchas of the code in
their own directory. They go stale the same way documentation does.

**When a change invalidates, outgrows, or adds to something a directory's
`AGENTS.md` describes, update that file in the same change.** Concretely, update
it when you:

- break or replace a documented convention or invariant (e.g. a new required
  parameter on a family of API functions, a changed refresh/validation order)
- add a new subsystem, module, or node category that belongs in that directory's
  overview or file map
- discover a non-obvious pitfall that cost you time and would cost the next
  contributor the same
- add a new subdirectory with its own `AGENTS.md` — also add it to the
  "Subdirectory Instructions" list at the top of this file and to the parent
  directory's list

Update the **most specific** file that covers the change (e.g. a nodes-only
convention belongs in `rust/src/structure_designer/nodes/AGENTS.md`, not here).
Routine changes that merely follow the documented conventions — a new node that
looks like its siblings, a bug fix, a refactor that preserves the invariants —
need **no** `AGENTS.md` update. These files are guidance, not a changelog: keep
them short and don't record individual fixes or feature history in them.
