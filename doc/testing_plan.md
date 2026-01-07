# atomCAD Testing Plan

## Current State Analysis

### Rust Tests (rust/tests/)
- **149 tests** across 20 flat test files
- Coverage is uneven—some modules are well-tested (`expr`, `atomic_structure`), others have minimal or no tests (`renderer`, `display`, most nodes)
- Test files do not mirror the `rust/src/` folder structure, making it hard to identify gaps

### Flutter Tests
- **Empty** `test/` folder—no widget/unit tests
- One outdated `integration_test/simple_test.dart` checking basic FFI

---

## Proposed Test Organization

### Rust: Mirror src/ Structure

```
rust/tests/
├── crystolecule/
│   ├── atomic_structure_test.rs
│   ├── drawing_plane_test.rs
│   ├── motif_parser_test.rs
│   ├── unit_cell_test.rs
│   └── io/
│       └── mol_exporter_test.rs
├── expr/
│   ├── lexer_test.rs
│   ├── parser_test.rs
│   ├── evaluation_test.rs
│   └── validation_test.rs
├── geo_tree/
│   ├── batched_implicit_evaluator_test.rs
│   ├── csg_cache_test.rs
│   └── implicit_geometry_test.rs  (NEW)
├── structure_designer/
│   ├── evaluator_test.rs          (NEW)
│   ├── serialization_test.rs      (NEW)
│   ├── node_network_test.rs       (NEW)
│   └── nodes/                      (NEW - snapshot tests)
│       └── node_snapshots_test.rs
├── util/
│   ├── daabox_test.rs
│   └── memory_bounded_lru_cache_test.rs
└── integration/                    (NEW)
    └── cnnd_roundtrip_test.rs
```

This structure makes gaps immediately visible and allows running `cargo test crystolecule::` to test a specific module.

---

## Test Types & Strategy

### 1. Unit Tests (Existing + Expand)
**Low overhead, high value for algorithmic code.**

Priorities:
- **expr/** — Already well-covered, maintain
- **crystolecule/atomic_structure** — Good coverage, maintain
- **util/** — Small but critical utilities, add tests for `transform.rs`, `hit_test_utils.rs`

### 2. Snapshot Tests (NEW)
**Ideal for node evaluation—low LOC/coverage ratio.**

Each node type outputs deterministic results. We can:
1. Evaluate a node network with known inputs
2. Serialize the output (JSON or custom format)
3. Compare against a golden file

```rust
// rust/tests/structure_designer/nodes/node_snapshots_test.rs
#[test]
fn test_sphere_node_output() {
    let result = evaluate_node_network("sphere_basic.cnnd");
    insta::assert_json_snapshot!(result);
}
```

Use [`insta`](https://crates.io/crates/insta) crate for snapshot management. One test can cover an entire node type's correctness.

#### Snapshot Testing Workflow

**Running snapshot tests:**
```bash
cd rust
cargo test node_snapshots_test
```

**When snapshots change (intentionally):**
```bash
# Review all pending snapshots interactively
cargo insta review

# Or accept all pending snapshots
cargo insta accept
```

**When tests fail unexpectedly:**
1. Run `cargo insta review` to see the diff
2. If the change is correct, accept it
3. If the change is a bug, fix the code and re-run tests

**Adding new snapshot tests:**
1. Add a test function in `rust/tests/structure_designer/nodes/node_snapshots_test.rs`
2. Use `evaluate_cnnd_file("../samples/your_file.cnnd")` to evaluate a CNND file
3. Use `insta::assert_json_snapshot!(snapshot)` to create the snapshot
4. Run tests—new snapshots will be created as `.snap.new` files
5. Run `cargo insta accept` to accept the new snapshots

**Current snapshot tests (10 total):**
- `test_diamond_cnnd_evaluation` - Diamond crystal (Atomic output)
- `test_sphere_node_basic` - Basic sphere node (Geometry output)
- `test_hexagem_cnnd_evaluation` - Hexagonal gem pattern
- `test_extrude_demo_evaluation` - 2D→3D extrusion
- `test_mof5_motif_evaluation` - MOF-5 metal-organic framework
- `test_rutile_motif_evaluation` - Rutile crystal motif
- `test_halfspace_demo_evaluation` - CSG half-space operations
- `test_rotation_demo_evaluation` - Polar pattern transforms
- `test_pattern_evaluation` - Pattern node with parameters
- `test_nut_bolt_evaluation` - Complex CSG (sphere cut)

### 3. Integration Tests (NEW)
**Test multi-component workflows.**

| Test | Description |
|------|-------------|
| CNND Roundtrip | Load → Modify → Save → Reload → Compare |
| Evaluation Pipeline | Node network → Evaluator → Geometry output |
| Export Formats | XYZ/MOL file generation from structures |

Sample tests from `samples/*.cnnd` provide real-world test data.

### 4. Property-Based Tests (NEW - Optional)
For mathematical code (transforms, SDF evaluation), consider [`proptest`](https://crates.io/crates/proptest):

```rust
proptest! {
    #[test]
    fn sphere_sdf_is_correct(x in -10.0..10.0, y in -10.0..10.0, z in -10.0..10.0) {
        let sphere = Sphere::new(1.0);
        let dist = sphere.evaluate(DVec3::new(x, y, z));
        let expected = (x*x + y*y + z*z).sqrt() - 1.0;
        prop_assert!((dist - expected).abs() < 1e-10);
    }
}
```

---

## Flutter Testing Strategy

### Widget Tests (test/)
**Test UI logic without full rendering.**

Focus on:
- `StructureDesignerModel` state transitions
- Property panel data binding
- Node network widget interactions (selection, connection)

```dart
// test/structure_designer/structure_designer_model_test.dart
void main() {
  test('addNode updates network', () {
    final model = StructureDesignerModel();
    model.addNode('Sphere', 100, 200);
    expect(model.nodeCount, 1);
  });
}
```

### Integration Tests (integration_test/)
**Test Flutter ↔ Rust interaction.**

Update existing `simple_test.dart` to:
- Open a sample CNND file
- Verify node network loads correctly
- Add a node, verify it appears
- Save and verify file written

```dart
testWidgets('Can load and modify project', (tester) async {
  await tester.pumpWidget(const MyApp());
  await tester.tap(find.text('File'));
  await tester.tap(find.text('Open'));
  // ... verify project loaded
});
```

### What NOT to Test in Flutter
- Rendering correctness (wgpu/GPU code)—trust Rust side
- Complex business logic—test in Rust where it lives
- Visual appearance—snapshot testing possible but brittle

---

## Coverage Priorities (High → Low)

| Priority | Module | Why |
|----------|--------|-----|
| HIGH | `structure_designer/serialization/` | Data loss bugs are catastrophic |
| HIGH | `crystolecule/atomic_structure` | Core data model |
| HIGH | `expr/` | User-facing, easy to break |
| MEDIUM | `structure_designer/evaluator/` | Complex logic, snapshot tests |
| MEDIUM | `geo_tree/` | SDF correctness matters |
| LOW | `renderer/` | GPU code, hard to test |
| LOW | `display/` | Visual output, hard to validate |

---

## Implementation Phases

### Phase 1: Reorganize (1-2 hours)
1. Create directory structure in `rust/tests/`
2. Move existing tests to matching folders
3. Add `mod.rs` files as needed
4. Verify all tests still pass

### Phase 2: Add Snapshot Testing (4-6 hours)
1. Add `insta` dependency
2. Create golden files from sample CNND projects
3. Write snapshot tests for each node type category
4. Document snapshot update workflow

### Phase 3: Integration Tests (2-3 hours)
1. CNND roundtrip test (load → modify → save → reload → verify)
2. Export format tests (XYZ/MOL file generation)

*Note: Evaluation pipeline is already covered by snapshot tests. Flutter integration tests belong in Phase 4.*

### Phase 4: Flutter Tests (2-4 hours)
1. `StructureDesignerModel` unit tests
2. Basic widget tests for critical UI

---

## Tooling Recommendations

| Tool | Purpose |
|------|---------|
| `cargo-tarpaulin` | Coverage reports for Rust |
| `insta` | Snapshot testing |
| `proptest` | Property-based testing (optional) |
| `flutter test --coverage` | Flutter coverage |

---

## Success Metrics

- Test reorganization mirrors `src/` structure
- Snapshot tests cover all node types
- CNND roundtrip test ensures no data corruption
- Coverage increases without excessive test LOC
