//! Characterisation tests for `FacetShellData`'s symmetry-family handling.
//!
//! These pin the *node-state* half of the facet symmetry code — the part that
//! `atomcad-crystolecule`'s `miller_test.rs` cannot see, because it decides what
//! `symmetrize` / `visible` a split facet ends up with rather than which Miller
//! indices the family contains.
//!
//! Written against the pre-`crystolecule::miller` implementation and left
//! unchanged across the extraction (`doc/design_push_domain_code_down.md` §3.3),
//! so a red result here means the rewrite changed behaviour.

use atomcad_structure_designer::nodes::facet_shell::{Facet, FacetShellData};
use glam::i32::IVec3;

/// Builds a `FacetShellData` holding exactly the given facets.
fn data_with(facets: Vec<Facet>) -> FacetShellData {
    let mut data = FacetShellData {
        facets,
        ..Default::default()
    };
    data.ensure_cached_facets();
    data
}

#[test]
fn split_symmetry_members_yields_family_and_preserves_visibility() {
    // The load-bearing case: an invisible, symmetrized {111} facet. Splitting it
    // must produce the 8 members of the family, each inheriting the original's
    // `visible == false` and each no longer symmetrized.
    let mut data = data_with(vec![Facet {
        miller_index: IVec3::new(1, 1, 1),
        shift: 3,
        symmetrize: true,
        visible: false,
    }]);

    assert!(data.split_symmetry_members(0));
    assert_eq!(data.facets.len(), 8);

    for facet in &data.facets {
        assert!(
            !facet.visible,
            "split facet {:?} must inherit the original's visible=false",
            facet.miller_index
        );
        assert!(
            !facet.symmetrize,
            "split facet {:?} must not stay symmetrized",
            facet.miller_index
        );
        assert_eq!(facet.shift, 3, "split facet must keep the original shift");
    }

    // The 8 members are exactly (±1, ±1, ±1).
    let mut got: Vec<(i32, i32, i32)> = data
        .facets
        .iter()
        .map(|f| (f.miller_index.x, f.miller_index.y, f.miller_index.z))
        .collect();
    got.sort();
    let mut want: Vec<(i32, i32, i32)> = Vec::new();
    for x in [-1, 1] {
        for y in [-1, 1] {
            for z in [-1, 1] {
                want.push((x, y, z));
            }
        }
    }
    want.sort();
    assert_eq!(got, want);

    // The selection is dropped and the cache is rebuilt. All 8 are invisible, so
    // `ensure_cached_facets` skips every one of them.
    assert_eq!(data.selected_facet_index, None);
    assert!(data.cached_facets.is_empty());
}

#[test]
fn split_symmetry_members_of_visible_facet_keeps_them_visible() {
    let mut data = data_with(vec![Facet {
        miller_index: IVec3::new(1, 1, 0),
        shift: 2,
        symmetrize: true,
        visible: true,
    }]);

    assert!(data.split_symmetry_members(0));
    assert_eq!(data.facets.len(), 12);
    assert!(data.facets.iter().all(|f| f.visible && !f.symmetrize));
    // Every split facet is visible and no longer symmetrized, so the cache holds
    // one entry per facet, each mapped back to its own index.
    assert_eq!(data.cached_facets.len(), 12);
    assert_eq!(
        data.cached_facet_to_original_index,
        (0..12).collect::<Vec<usize>>()
    );
}

#[test]
fn split_symmetry_members_ignores_non_symmetrized_facet() {
    let mut data = data_with(vec![Facet {
        miller_index: IVec3::new(1, 0, 0),
        shift: 1,
        symmetrize: false,
        visible: true,
    }]);

    assert!(!data.split_symmetry_members(0));
    assert_eq!(data.facets.len(), 1);
    assert_eq!(data.facets[0].miller_index, IVec3::new(1, 0, 0));
    assert!(!data.facets[0].symmetrize);
}

#[test]
fn split_symmetry_members_ignores_out_of_range_index() {
    let mut data = data_with(vec![Facet {
        miller_index: IVec3::new(1, 1, 1),
        shift: 1,
        symmetrize: true,
        visible: true,
    }]);

    assert!(!data.split_symmetry_members(7));
    assert_eq!(data.facets.len(), 1);
    assert!(data.facets[0].symmetrize);
}
