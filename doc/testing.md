# atomCAD Test Coverage

## Overview

~450+ Rust tests + Flutter tests. Run with:

```bash
cd rust && cargo test          # All Rust tests
flutter test integration_test/ # Flutter test

# Coverage report (requires cargo-llvm-cov)
.\scripts\coverage.ps1 -Open   # Windows: generate and open HTML report
```

## Rust Tests (rust/tests/)

Tests mirror `rust/src/` structure for easy gap identification.

### Unit Tests
| Module | Coverage |
|--------|----------|
| `expr/` | Lexer, parser, evaluation, validation |
| `crystolecule/` | Atomic structure, unit cell, motif parser, drawing plane, lattice fill |
| `geo_tree/` | CSG cache, batched implicit evaluator, SDF evaluation (implicit_eval) |
| `structure_designer/` | Network validator, node network operations, network evaluator |
| `util/` | DAA box, LRU cache |

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
