//! Table B of `doc/design_isosurface_node.md` P2: **randomized closure fuzz.**
//!
//! The highest-value rows in the plan — they reach combinations tables A and C
//! both miss: asymmetric values, levels away from zero, near-ties, and outright
//! ties on most corners of a cell.
//!
//! Seeds are a fixed range, never `rand::random()`: a fuzz failure that cannot
//! be replayed is a flake, and this suite has to be able to fail loudly in CI.

use std::sync::Arc;

use atomcad_crystolecule::field::ScalarField;
use atomcad_display::isosurface::{ExtractionSettings, extract_isosurface};

use crate::isosurface_common::{
    Rng, assert_closed_manifold, assert_components_partition_indices, assert_finite,
    assert_on_isosurface, euler_characteristic, phase_data, shelled_grid, uniform_grid,
};

/// Enough to reach the awkward corners, still under a second in debug.
const SEEDS: std::ops::Range<u64> = 1..2001;

fn extract_grid(
    field: Arc<dyn ScalarField>,
    level: f64,
) -> atomcad_display::isosurface::SurfaceMesh {
    extract_isosurface(&phase_data(field, level), &ExtractionSettings::default())
        .expect("a 5^3 grid is comfortably inside the cell budget")
}

#[test]
fn random_grids_extract_closed_surfaces() {
    for seed in SEEDS {
        let mut rng = Rng::new(seed);
        let level = rng.range(0.05, 0.95);
        let field = Arc::new(shelled_grid(&mut rng, 5, |rng| rng.range(-1.0, 1.0)));
        let mesh = extract_grid(field.clone(), level);
        let what = format!("seed {seed} at level {level:.4}");

        assert_closed_manifold(&mesh, &what);
        assert_on_isosurface(&mesh, field.as_ref(), level, 1e-5);
        assert_components_partition_indices(&mesh, &what);
        assert_finite(&mesh, &what);
        let chi = euler_characteristic(&mesh);
        assert_eq!(
            chi % 2,
            0,
            "{what}: Euler characteristic {chi} is odd, so the surface is not closed"
        );
    }
}

#[test]
fn tied_grids_do_not_split_into_spurious_components() {
    // The degeneracy generator: values drawn from three levels with the isolevel
    // landing on one of them, so most corners tie. This is what catches the
    // spurious-component split of §Degeneracies — a surface passing exactly
    // through a grid corner gets one vertex per cut edge meeting there, and
    // without the coincident-vertex merge the union-find refuses to join them.
    //
    // The values are shifted up rather than centred on zero (the design writes
    // `{-1, 0, +1}` at level `0`) so that the isolevel can stay strictly
    // positive: at level `0` the two sign passes extract the same zero set
    // twice, which is a degeneracy of the *node's* forbidden `level <= 0`, not
    // of the extractor.
    for seed in SEEDS {
        let mut rng = Rng::new(seed ^ 0xA5A5);
        let level = 0.5;
        let field = Arc::new(shelled_grid(&mut rng, 5, |rng| rng.pick(&[0.0, 0.5, 1.0])));
        let mesh = extract_grid(field.clone(), level);
        let what = format!("tie seed {seed}");

        assert_closed_manifold(&mesh, &what);
        assert_on_isosurface(&mesh, field.as_ref(), level, 1e-5);
        assert_components_partition_indices(&mesh, &what);
        assert_finite(&mesh, &what);

        // No two vertices may be coincident once the merge has run — that is
        // precisely the condition under which one lobe would be labelled as
        // several.
        let mut seen = std::collections::HashSet::new();
        for (i, position) in mesh.positions.iter().enumerate() {
            let key = [
                position.x.to_bits(),
                position.y.to_bits(),
                position.z.to_bits(),
            ];
            assert!(
                seen.insert(key),
                "{what}: vertex {i} at {position:?} is coincident with an earlier one"
            );
        }
    }
}

#[test]
fn a_grid_flat_at_the_level_is_empty_not_broken() {
    let field = Arc::new(uniform_grid(5, 0.25));
    let mesh = extract_grid(field, 0.25);
    // Every corner ties, so every corner is inside under the non-strict rule
    // and no edge is cut.
    assert!(
        mesh.is_empty(),
        "a flat grid at the level produced geometry"
    );
    assert!(mesh.components.is_empty());
    assert_finite(&mesh, "flat grid");
}

#[test]
fn extraction_is_deterministic() {
    for seed in [3u64, 41, 512, 1999] {
        let level = 0.31;
        let build = || {
            let mut rng = Rng::new(seed);
            Arc::new(shelled_grid(&mut rng, 5, |rng| rng.range(-1.0, 1.0)))
        };
        let first = extract_grid(build(), level);
        let second = extract_grid(build(), level);

        assert_eq!(first.positions, second.positions, "seed {seed}: positions");
        assert_eq!(first.normals, second.normals, "seed {seed}: normals");
        assert_eq!(first.indices, second.indices, "seed {seed}: indices");
        assert_eq!(
            first.components, second.components,
            "seed {seed}: component order"
        );
    }
}
