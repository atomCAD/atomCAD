# atomCAD Test Coverage

## Overview

~384 Rust tests + 1 Flutter smoke test. Run with:

```bash
cd rust && cargo test          # All Rust tests
flutter test integration_test/ # Flutter smoke test
```

## Rust Tests (rust/tests/)

Tests mirror `rust/src/` structure for easy gap identification.

### Unit Tests
| Module | Coverage |
|--------|----------|
| `expr/` | Lexer, parser, evaluation, validation |
| `crystolecule/` | Atomic structure, unit cell, motif parser, drawing plane |
| `geo_tree/` | CSG cache, batched implicit evaluator |
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

## Flutter Tests (integration_test/)

Single FFI smoke test only. Full workflow tests run in Rust.

**Why minimal Flutter tests:**
- `StructureDesignerModel` is a thin wrapper around Rust APIs
- wgpu/GPU initialization fails in test environments
- Rust tests already cover CNND roundtrip, node evaluation, exports

## Not Tested (Manual Only)

- GPU rendering (wgpu)
- Visual appearance
- `renderer/`, `display/` modules
