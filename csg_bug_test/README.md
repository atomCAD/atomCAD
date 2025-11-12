# CSG Intersection Bug Test Case

This is a minimal test case demonstrating a bug in the `csgrs` library where intersecting 6 half-spaces (representing a cube) results in only 4 faces instead of the expected 6 faces.

## Problem Description

When creating a cube by intersecting 6 half-spaces (one for each face), the CSG intersection operation produces an incorrect result:
- **Expected**: 6 polygons (one for each cube face)
- **Actual**: 4 polygons (missing 2 faces)

This bug occurs in real-world usage within atomCAD, a molecular design application, when creating cuboid geometries in diamond crystal lattices.

## Test Case Details

### Geometry Parameters
- **Diamond unit cell size**: 3.567 Ångströms
- **Problematic cuboid extents**: (72, 67, 72) unit cells
- **Real-world dimensions**: ~256.8 × ~238.0 × ~256.8 Ångströms
- **Half-space representation**: 800×800 squares (as used in atomCAD)

### Half-Space Configuration
Each half-space is represented as an 800×800 square positioned at the appropriate face center with inward-pointing normals:

1. **Min X face**: Normal (+1, 0, 0) at (0, center_y, center_z)
2. **Max X face**: Normal (-1, 0, 0) at (max_x, center_y, center_z)
3. **Min Y face**: Normal (0, +1, 0) at (center_x, 0, center_z)
4. **Max Y face**: Normal (0, -1, 0) at (center_x, max_y, center_z)
5. **Min Z face**: Normal (0, 0, +1) at (center_x, center_y, 0)
6. **Max Z face**: Normal (0, 0, -1) at (center_x, center_y, max_z)

## Running the Test

### Prerequisites
- Rust (latest stable)
- `csgrs` version 0.20.1
- `nalgebra` version 0.33

### Commands
```bash
# Run the main test
cargo run

# Run unit tests
cargo test

# Run with verbose output
cargo run -- --verbose
```

### Expected Output
```
CSG Intersection Bug Test
========================

Testing cube intersection with extents: (72, 67, 72)
Real-world dimensions: 256.824 x 238.989 x 256.824 Ångströms
Created 6 half-spaces
Intersecting with half-space 2
Result has X polygons after intersection 2
...
Final result:
  Polygons: 4
  Expected: 6
❌ BUG DETECTED: Expected 6 faces, got 4
```

## Test Variations

The test includes multiple cube sizes to help isolate the issue:
- Original case: (72, 67, 72) unit cells
- Small cube: (1, 1, 1) unit cells  
- Medium cube: (10, 10, 10) unit cells
- Large cube: (100, 100, 100) unit cells

## Modifying the Test

To test different cube sizes, modify the `CubeTestConfig::default()` method or add new test cases:

```rust
impl Default for CubeTestConfig {
    fn default() -> Self {
        Self {
            extent_units: (YOUR_X, YOUR_Y, YOUR_Z), // Change these values
        }
    }
}
```

## Files Structure
```
csg_bug_test/
├── Cargo.toml          # Project dependencies
├── README.md           # This file
└── src/
    └── main.rs         # Complete test case implementation
```

## Context

This bug was discovered in atomCAD when creating cuboid geometries for molecular mechanical systems. The missing faces cause incorrect geometry that affects downstream atom filling and visualization operations.

The test case preserves the exact geometric parameters and CSG operations used in the production application to ensure the bug reproduction is accurate.

## Dependencies Used

- `csgrs = "0.20.1"` - The CSG library being tested
- `nalgebra = "0.33"` - For 3D math operations (Point3, Vector3, Rotation3)

## Contact

This test case was created for debugging purposes. Please let us know if you need any additional information or modifications to help diagnose the issue.
