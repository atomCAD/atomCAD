# atomCAD Test Coverage

## Overview

~5,000 Rust tests plus Dart unit tests and the Flutter smoke test. Run with:

```bash
cd rust && cargo test          # All Rust tests
flutter test                   # Dart unit tests (test/) — fast, no Rust library
flutter test integration_test/ # Flutter smoke test — HUMAN-ONLY (see note)

# Coverage report (requires cargo-llvm-cov)
.\scripts\coverage.ps1 -Open   # Windows: generate and open HTML report
```

> **Note:** the Flutter smoke test is run **manually by the maintainer only**.
> AI agents must not run it — it is unreliable from an agent shell
> (app-launch/debug-connection failures, window-size-dependent assertions) and
> wastes minutes per attempt. Agents cover their changes with the Rust suite
> and `flutter analyze`, and list the smoke test as a pending manual step.

## Rust Tests

Tests mirror the source structure for easy gap identification.

The backend is a cargo workspace (`doc/design_rust_crate_split.md`), so tests
live in **two places**: each extracted crate's own
`rust/crates/<crate>/tests/`, keeping the module's original directory name, and
`rust/tests/` for the root package. `cd rust && cargo test -j 4` still runs
everything — the `default-members` entry in `rust/Cargo.toml` is what makes that
true, so do not remove it. To run one crate's tests alone:

```bash
cd rust && cargo test -p atomcad-crystolecule -j 4   # one crate, no api, no frb_generated
cd rust && cargo test -p rust_lib_flutter_cad -j 4   # the root package's harnesses only
```

### Where a new test goes

**Decided by what the test imports, not by what it is about.** A member crate
cannot depend on the root crate, so a test that names anything in
`rust_lib_flutter_cad::api` has to sit in a root-package harness even when its
subject lives in an extracted crate. That is why two subjects have two harnesses
each:

| Subject | Domain tests (in the crate) | Tests that reach up into `api` |
|---|---|---|
| structure_designer | `rust/crates/atomcad-structure-designer/tests/structure_designer/` | `rust/tests/structure_designer_api/` |
| renderer | `rust/crates/atomcad-renderer/tests/renderer/` | `rust/tests/renderer_api/` |

The api-side harnesses hold the tests that consume transport types
(`APIValidationError`, `APIAtomEditTool`, `APIDataType`, …) or call the
view-builders in `api/structure_designer/`. Prefer splitting a file at that seam
over dragging its whole subject upward. Everything cross-layer — the `.cnnd`
migration corpus, roundtrips — stays in `rust/tests/integration/`.

Fixtures are shared by three packages and stay at `rust/tests/fixtures/`.
Address them only through `atomcad_test_support::fixture_path` /
`fixture_path_str`; a local `CARGO_MANIFEST_DIR` join or a bare
`"tests/fixtures/…"` string is anchored to the *package* root and means
something different in every crate.

### Unit Tests
| Crate | Location | Coverage |
|--------|----------|----------|
| `atomcad-structure-designer` | `crates/atomcad-structure-designer/tests/structure_designer/` | Network validator, node network operations, network evaluator, nodes, undo, text format, serialization. Also the two scene-tessellation tests (`atomic_impostor_alpha_test.rs`, `atom_label_test.rs`) — `tessellate_scene_content` is a `structure_designer` module, not a `display` one. |
| `atomcad-structure-designer` (`expr`) | `crates/atomcad-structure-designer/tests/expr/` | Lexer, parser, evaluation, validation. `expr` is a submodule of the crate (design doc D8), so its tests ride along with it. |
| `atomcad-crystolecule` | `crates/atomcad-crystolecule/tests/crystolecule/` | Atomic structure, unit cell, motif parser, drawing plane, lattice fill, UFF simulation |
| `atomcad-display` | `crates/atomcad-display/tests/display/` | Poly-mesh tessellation, CSG→poly-mesh, atom color/render style |
| `atomcad-geo-tree` | `crates/atomcad-geo-tree/tests/geo_tree/` | CSG cache, batched implicit evaluator, SDF evaluation (implicit_eval) |
| `atomcad-renderer` | `crates/atomcad-renderer/tests/renderer/` | Camera math, label atlas layout, impostor meshes, transparent sort |
| `atomcad-util` | `crates/atomcad-util/tests/util/` | DAA box, LRU cache |
| `rust_lib_flutter_cad` | `rust/tests/structure_designer_api/`, `rust/tests/renderer_api/` | The api-level tests of those two subjects: error/validation transport types, `APIDataType` conversion, function-pin role views, atom-edit tool adapters, node-type views; the four axis-resolution tests. |

### Snapshot Tests (insta)
Evaluate sample CNND files and compare against golden files:
- Diamond, hexagem, MOF-5, rutile crystals
- Sphere, extrude, half-space, rotation, pattern nodes
- Complex CSG (nut-bolt)

```bash
cargo test node_snapshots    # Run snapshot tests
cargo insta review           # Review changes interactively
```

### Integration Tests
| Test | Description |
|------|-------------|
| CNND roundtrip (12 tests) | Load → modify → save → reload → compare |
| XYZ roundtrip (6 tests) | Import/export atomic structures |
| Lattice fill (2 tests) | Fill geometry with atoms |

## Dart Unit Tests (test/)

Pure-Dart tests for Flutter-side logic that does **not** need the Rust library —
formatters, view-model transforms, anything touching only the generated FRB
*data classes*. They open no DLL, so a whole file runs in well under a second,
and unlike the smoke test below **agents may run them**.

```bash
flutter test                              # every Dart unit test
flutter test test/error_report_test.dart  # one file
```

| Test | Coverage |
|------|----------|
| `error_report_test.dart` | Problem-report formatter (#359): root-cause grouping, per-entry formatting, report headline |

Anything needing a live `StructureDesigner` does not belong here. Prefer the
Rust suite, where the logic can be tested without driving the UI at all; fall
back to `integration_test/` only when the behaviour genuinely is the widget
tree.

## Flutter Tests (integration_test/)

## Test Coverage Reports

Use `cargo-llvm-cov` to generate line-by-line coverage reports:

```powershell
# Install (one-time)
cargo install cargo-llvm-cov

# Generate HTML report and open in browser
.\scripts\coverage.ps1 -Open

# Show summary in terminal only
.\scripts\coverage.ps1 -Summary

# Or run directly from rust/
cd rust
cargo llvm-cov --ignore-filename-regex "(csgrs|frb_generated)" --html
start target/llvm-cov/html/index.html
```

The HTML report shows:
- Per-file coverage percentages
- Line-by-line hit counts (green = covered, red = not covered)
- Function/branch coverage statistics

## Not Tested (Manual Only)

- GPU rendering (wgpu)
- Visual appearance
- the GPU-bound parts of `atomcad-renderer` and `atomcad-display`
