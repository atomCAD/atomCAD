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

## Rust Tests (rust/tests/)

Tests mirror `rust/src/` structure for easy gap identification.

As the backend is split into workspace crates
(`doc/design_rust_crate_split.md`), each extracted module's tests move with it
into `rust/crates/<crate>/tests/`, keeping their original directory name. `cd
rust && cargo test -j 4` still runs everything — the `default-members` entry in
`rust/Cargo.toml` is what makes that true, so do not remove it.

### Unit Tests
| Module | Coverage |
|--------|----------|
| `expr/` | Lexer, parser, evaluation, validation |
| `crystolecule/` | Atomic structure, unit cell, motif parser, drawing plane, lattice fill |
| `geo_tree/` | CSG cache, batched implicit evaluator, SDF evaluation (implicit_eval) |
| `structure_designer/` | Network validator, node network operations, network evaluator |
| `util/` | DAA box, LRU cache — now at `rust/crates/atomcad-util/tests/util/`, run with `cargo test -p atomcad-util` |

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
- `renderer/`, `display/` modules
