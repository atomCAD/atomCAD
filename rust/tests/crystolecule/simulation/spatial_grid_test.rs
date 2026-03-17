// Tests for the SpatialGrid data structure.
//
// Validates construction, neighbor queries, edge cases, and correctness
// against brute-force enumeration.

use rust_lib_flutter_cad::crystolecule::simulation::spatial_grid::SpatialGrid;

// ============================================================================
// Construction tests
// ============================================================================

#[test]
fn empty_grid() {
    let positions: Vec<f64> = vec![];
    let grid = SpatialGrid::from_positions(&positions, 5.0);
    // No atoms, no neighbors.
    assert_eq!(grid.cell_size(), 5.0);
}

#[test]
fn single_atom() {
    let positions = vec![1.0, 2.0, 3.0];
    let grid = SpatialGrid::from_positions(&positions, 5.0);
    // Single atom has no neighbors.
    let mut neighbors = Vec::new();
    grid.for_each_neighbor(&positions, 0, 10.0, |j| neighbors.push(j));
    assert!(neighbors.is_empty());
}

#[test]
fn two_atoms_within_radius() {
    // Two atoms 2.0 apart.
    let positions = vec![0.0, 0.0, 0.0, 2.0, 0.0, 0.0];
    let grid = SpatialGrid::from_positions(&positions, 5.0);

    // Query from atom 0 with radius 3.0 — should find atom 1.
    let mut neighbors = Vec::new();
    grid.for_each_neighbor(&positions, 0, 3.0, |j| neighbors.push(j));
    assert_eq!(neighbors, vec![1]);

    // Query from atom 1 with radius 3.0 — should find atom 0.
    let mut neighbors = Vec::new();
    grid.for_each_neighbor(&positions, 1, 3.0, |j| neighbors.push(j));
    assert_eq!(neighbors, vec![0]);
}

#[test]
fn two_atoms_outside_radius() {
    // Two atoms 10.0 apart.
    let positions = vec![0.0, 0.0, 0.0, 10.0, 0.0, 0.0];
    let grid = SpatialGrid::from_positions(&positions, 5.0);

    // Query with radius 5.0 — should not find each other (dist = 10.0 >= 5.0).
    let mut neighbors = Vec::new();
    grid.for_each_neighbor(&positions, 0, 5.0, |j| neighbors.push(j));
    assert!(neighbors.is_empty());
}

#[test]
fn atom_exactly_at_radius_boundary() {
    // Two atoms exactly 5.0 apart. The grid uses strict < comparison.
    let positions = vec![0.0, 0.0, 0.0, 5.0, 0.0, 0.0];
    let grid = SpatialGrid::from_positions(&positions, 5.0);

    // dist = 5.0, radius = 5.0 → not found (strict <)
    let mut neighbors = Vec::new();
    grid.for_each_neighbor(&positions, 0, 5.0, |j| neighbors.push(j));
    assert!(neighbors.is_empty());

    // radius = 5.01 → found
    let mut neighbors = Vec::new();
    grid.for_each_neighbor(&positions, 0, 5.01, |j| neighbors.push(j));
    assert_eq!(neighbors, vec![1]);
}

// ============================================================================
// Correctness against brute-force
// ============================================================================

/// Brute-force: find all atoms j != i within radius of atom i.
fn brute_force_neighbors(positions: &[f64], center_idx: usize, radius: f64) -> Vec<usize> {
    let num_atoms = positions.len() / 3;
    let i3 = center_idx * 3;
    let cx = positions[i3];
    let cy = positions[i3 + 1];
    let cz = positions[i3 + 2];
    let radius_sq = radius * radius;

    let mut result = Vec::new();
    for j in 0..num_atoms {
        if j == center_idx {
            continue;
        }
        let j3 = j * 3;
        let dx = cx - positions[j3];
        let dy = cy - positions[j3 + 1];
        let dz = cz - positions[j3 + 2];
        let dist_sq = dx * dx + dy * dy + dz * dz;
        if dist_sq < radius_sq {
            result.push(j);
        }
    }
    result
}

#[test]
fn matches_brute_force_small_cluster() {
    // 5 atoms in a small cluster.
    #[rustfmt::skip]
    let positions = vec![
        0.0, 0.0, 0.0,
        1.5, 0.0, 0.0,
        0.0, 1.5, 0.0,
        3.0, 3.0, 3.0,
        0.5, 0.5, 0.5,
    ];
    let radius = 2.5;
    let grid = SpatialGrid::from_positions(&positions, radius);

    for i in 0..5 {
        let mut grid_result = Vec::new();
        grid.for_each_neighbor(&positions, i, radius, |j| grid_result.push(j));
        grid_result.sort();

        let mut bf_result = brute_force_neighbors(&positions, i, radius);
        bf_result.sort();

        assert_eq!(
            grid_result, bf_result,
            "atom {i}: grid={grid_result:?} != brute_force={bf_result:?}"
        );
    }
}

#[test]
fn matches_brute_force_many_atoms() {
    // 20 atoms spread across a 10x10x10 box.
    let mut positions = Vec::new();
    // Deterministic positions using a simple formula.
    for i in 0..20 {
        let f = i as f64;
        positions.push((f * 1.7) % 10.0);
        positions.push((f * 2.3) % 10.0);
        positions.push((f * 3.1) % 10.0);
    }

    let radius = 4.0;
    let grid = SpatialGrid::from_positions(&positions, radius);

    for i in 0..20 {
        let mut grid_result = Vec::new();
        grid.for_each_neighbor(&positions, i, radius, |j| grid_result.push(j));
        grid_result.sort();

        let mut bf_result = brute_force_neighbors(&positions, i, radius);
        bf_result.sort();

        assert_eq!(
            grid_result, bf_result,
            "atom {i}: grid result != brute force"
        );
    }
}

#[test]
fn negative_coordinates() {
    // Atoms at negative positions.
    #[rustfmt::skip]
    let positions = vec![
        -5.0, -3.0, -1.0,
        -4.0, -3.0, -1.0,
        10.0, 10.0, 10.0,
    ];
    let grid = SpatialGrid::from_positions(&positions, 3.0);

    // Atom 0 and 1 are 1.0 apart — within radius.
    let mut neighbors = Vec::new();
    grid.for_each_neighbor(&positions, 0, 3.0, |j| neighbors.push(j));
    assert_eq!(neighbors, vec![1]);

    // Atom 2 is far from both.
    let mut neighbors = Vec::new();
    grid.for_each_neighbor(&positions, 2, 3.0, |j| neighbors.push(j));
    assert!(neighbors.is_empty());
}

#[test]
fn atoms_across_cell_boundaries() {
    // Two atoms in adjacent cells, close enough to be neighbors.
    // Cell size = 5.0. Atom 0 at (4.9, 0, 0), atom 1 at (5.1, 0, 0) — dist = 0.2.
    let positions = vec![4.9, 0.0, 0.0, 5.1, 0.0, 0.0];
    let grid = SpatialGrid::from_positions(&positions, 5.0);

    let mut neighbors = Vec::new();
    grid.for_each_neighbor(&positions, 0, 1.0, |j| neighbors.push(j));
    assert_eq!(neighbors, vec![1]);
}

// ============================================================================
// Pair enumeration test (deduplication pattern used by vdW cutoff)
// ============================================================================

#[test]
fn pair_enumeration_no_duplicates() {
    // Verify that iterating i=0..N, j>i gives each pair exactly once.
    #[rustfmt::skip]
    let positions = vec![
        0.0, 0.0, 0.0,
        1.0, 0.0, 0.0,
        0.0, 1.0, 0.0,
        1.0, 1.0, 0.0,
    ];
    let radius = 2.0;
    let grid = SpatialGrid::from_positions(&positions, radius);

    let mut pairs = Vec::new();
    for i in 0..4 {
        grid.for_each_neighbor(&positions, i, radius, |j| {
            if j > i {
                pairs.push((i, j));
            }
        });
    }
    pairs.sort();

    // Check no duplicates.
    let mut deduped = pairs.clone();
    deduped.dedup();
    assert_eq!(pairs, deduped, "duplicate pairs found");

    // Brute-force expected pairs within radius 2.0.
    let mut expected = Vec::new();
    for i in 0..4 {
        for j in (i + 1)..4 {
            let bf = brute_force_neighbors(&positions, i, radius);
            if bf.contains(&j) {
                expected.push((i, j));
            }
        }
    }
    expected.sort();
    assert_eq!(pairs, expected);
}
