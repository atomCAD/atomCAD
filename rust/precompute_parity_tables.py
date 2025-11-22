#!/usr/bin/env python3
"""
Precompute lookup tables for is_primary_dimer_atom to optimize runtime performance.
"""

all_fractional = {
    0: (0.0, 0.0, 0.0),       # CORNER
    1: (0.5, 0.5, 0.0),       # FACE_Z
    2: (0.5, 0.0, 0.5),       # FACE_Y
    3: (0.0, 0.5, 0.5),       # FACE_X
    4: (0.25, 0.25, 0.25),    # INTERIOR1
    5: (0.25, 0.75, 0.75),    # INTERIOR2
    6: (0.75, 0.25, 0.75),    # INTERIOR3
    7: (0.75, 0.75, 0.25),    # INTERIOR4
}

site_names = ["CORNER", "FACE_Z", "FACE_Y", "FACE_X", "INTERIOR1", "INTERIOR2", "INTERIOR3", "INTERIOR4"]

# Surface orientations: [+X, -X, +Y, -Y, +Z, -Z]
surface_names = ["+X", "-X", "+Y", "-Y", "+Z", "-Z"]

print("="*70)
print("PRECOMPUTING PARITY LOOKUP TABLES")
print("="*70)

# For each surface orientation, determine perpendicular axis
perp_axes = {
    "+X": 0, "-X": 0,  # X perpendicular
    "+Y": 1, "-Y": 1,  # Y perpendicular
    "+Z": 2, "-Z": 2,  # Z perpendicular
}

# Build a table: [surface_idx][site_idx] -> (use_sum_formula, in_surface_idx, depth_index)
# This gives us everything we need to compute parity quickly
parity_data = []

for surf_idx, surface in enumerate(surface_names):
    perp_idx = perp_axes[surface]
    is_positive = surface[0] == '+'
    
    surface_data = []
    
    for site_idx in range(8):
        frac = all_fractional[site_idx]
        depth = frac[perp_idx]
        depth_index = int(round(depth * 4))
        
        # Find all sites in the same layer
        sites_in_layer = []
        for i in range(8):
            other_frac = all_fractional[i]
            other_depth = other_frac[perp_idx]
            if abs(other_depth - depth) < 0.01:
                sites_in_layer.append(i)
        
        sites_in_layer.sort()
        in_surface_idx = sites_in_layer.index(site_idx)
        
        # Determine formula
        use_sum_formula = (is_positive and depth_index in [0, 3]) or \
                         (not is_positive and depth_index in [1, 2])
        
        surface_data.append((use_sum_formula, in_surface_idx, depth_index))
    
    parity_data.append(surface_data)

# Generate Rust code
print("\n// Precomputed parity data for all (surface_orientation, site_index) combinations")
print("// Each entry: (use_sum_formula: bool, in_surface_idx: u8, depth_index: u8)")
print("// Indexed as: PARITY_DATA[surface_orientation_idx][site_index]")
print("// Surface order: [+X, -X, +Y, -Y, +Z, -Z]")
print("const PARITY_DATA: [[(bool, u8, u8); 8]; 6] = [")

for surf_idx, surface in enumerate(surface_names):
    print(f"  // {surface}")
    print("  [", end="")
    for site_idx in range(8):
        use_sum, in_surf, depth_idx = parity_data[surf_idx][site_idx]
        if site_idx > 0:
            print(", ", end="")
        print(f"({str(use_sum).lower():5s}, {in_surf}, {depth_idx})", end="")
    print("],")

print("];")

# Generate phase flip array (24 booleans, all false for Phase A)
print("\n// Per-layer phase control: 24 booleans (6 surfaces × 4 depths)")
print("// Set to true to flip phase for that layer")
print("// Indexed as: PHASE_FLIP[surface_idx * 4 + depth_idx]")
print("// Order: +X(0,0.25,0.5,0.75), -X(0,0.25,0.5,0.75), +Y(...), -Y(...), +Z(...), -Z(...)")
print("const PHASE_FLIP: [bool; 24] = [")
for surf_idx, surface in enumerate(surface_names):
    print(f"  false, false, false, false, // {surface}: depths 0.00, 0.25, 0.50, 0.75")
print("];")

# Also generate in-plane axis indices table
print("\n// Precomputed in-plane axis indices for each surface orientation")
print("// Each entry: (in_plane_idx_1, in_plane_idx_2)")
print("// Surface order: [+X, -X, +Y, -Y, +Z, -Z]")
print("const IN_PLANE_AXES: [(usize, usize); 6] = [")
in_plane_data = [
    (1, 2),  # +X: YZ plane
    (1, 2),  # -X: YZ plane
    (0, 2),  # +Y: XZ plane
    (0, 2),  # -Y: XZ plane
    (0, 1),  # +Z: XY plane
    (0, 1),  # -Z: XY plane
]
for surf_idx, (idx1, idx2) in enumerate(in_plane_data):
    print(f"  ({idx1}, {idx2}), // {surface_names[surf_idx]}")
print("];")

print("\n" + "="*70)
print("Verification: checking all 48 combinations")
print("="*70)

for surf_idx, surface in enumerate(surface_names):
    for site_idx in range(8):
        use_sum, in_surf, depth_idx = parity_data[surf_idx][site_idx]
        depth = all_fractional[site_idx][perp_axes[surface]]
        layer_idx = surf_idx * 4 + depth_idx
        print(f"{surface} site {site_idx:1d} ({site_names[site_idx]:9s}) depth {depth:4.2f} (layer {layer_idx:2d}): "
              f"{'SUM' if use_sum else 'IDX'} formula, in_surface_idx={in_surf}")

print("\n" + "="*70)
print("HELPER FUNCTION")
print("="*70)
print("""
// Helper function to map SurfaceOrientation to index
fn surface_orientation_to_index(orientation: SurfaceOrientation) -> Option<usize> {
  match orientation {
    SurfaceOrientation::Surface100 => Some(0),    // +X
    SurfaceOrientation::SurfaceNeg100 => Some(1), // -X
    SurfaceOrientation::Surface010 => Some(2),    // +Y
    SurfaceOrientation::SurfaceNeg010 => Some(3), // -Y
    SurfaceOrientation::Surface001 => Some(4),    // +Z
    SurfaceOrientation::SurfaceNeg001 => Some(5), // -Z
    _ => None,
  }
}
""")
