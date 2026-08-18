//! Tests for `crystolecule::miller` — Miller-index reduction, enumeration and
//! symmetry families.
//!
//! The `symmetry_equivalent_indices` expectations were captured from the
//! pre-extraction implementation in `facet_shell.rs`
//! (`doc/design_push_domain_code_down.md` §3.3), not from the extracted code, so
//! they characterise actual behaviour rather than asserting the code equals
//! itself.

use atomcad_crystolecule::miller::{
    generate_possible_miller_indices, generate_unique_permutations, simplify_miller_index,
    symmetry_equivalent_indices,
};
use glam::i32::IVec3;
use std::collections::HashSet;

fn v(x: i32, y: i32, z: i32) -> IVec3 {
    IVec3::new(x, y, z)
}

/// The family as an order-independent set, with a duplicate check — the
/// enumeration is expected to emit each member exactly once.
fn family_set(miller: IVec3) -> HashSet<(i32, i32, i32)> {
    let family = symmetry_equivalent_indices(miller);
    let set: HashSet<(i32, i32, i32)> = family.iter().map(|i| (i.x, i.y, i.z)).collect();
    assert_eq!(
        set.len(),
        family.len(),
        "symmetry_equivalent_indices({miller:?}) emitted duplicates"
    );
    set
}

fn set_of(members: &[(i32, i32, i32)]) -> HashSet<(i32, i32, i32)> {
    members.iter().copied().collect()
}

/// Every permutation of `(a, b, c)` combined with every sign, as a set. Used to
/// state the two large families independently of the enumeration order.
fn signed_permutations(a: i32, b: i32, c: i32) -> HashSet<(i32, i32, i32)> {
    let mut expected: HashSet<(i32, i32, i32)> = HashSet::new();
    for (x, y, z) in generate_unique_permutations(a, b, c) {
        for sx in [1, -1] {
            for sy in [1, -1] {
                for sz in [1, -1] {
                    expected.insert((x * sx, y * sy, z * sz));
                }
            }
        }
    }
    expected
}

// ==== simplify_miller_index ====

#[test]
fn simplify_reduces_by_the_gcd() {
    assert_eq!(simplify_miller_index(v(2, 4, 6)), v(1, 2, 3));
    assert_eq!(simplify_miller_index(v(3, 3, 3)), v(1, 1, 1));
    assert_eq!(simplify_miller_index(v(0, 2, 4)), v(0, 1, 2));
    assert_eq!(simplify_miller_index(v(0, 0, 5)), v(0, 0, 1));
}

#[test]
fn simplify_preserves_signs_and_already_reduced_indices() {
    assert_eq!(simplify_miller_index(v(-2, 4, -6)), v(-1, 2, -3));
    assert_eq!(simplify_miller_index(v(1, 2, 3)), v(1, 2, 3));
    assert_eq!(simplify_miller_index(v(1, 1, 0)), v(1, 1, 0));
    // Coprime components with no common divisor are left alone.
    assert_eq!(simplify_miller_index(v(2, 3, 4)), v(2, 3, 4));
}

#[test]
fn simplify_leaves_the_origin_alone() {
    // Degenerate: no divisor loop runs, so (0,0,0) passes through unchanged.
    assert_eq!(simplify_miller_index(v(0, 0, 0)), v(0, 0, 0));
}

// ==== generate_possible_miller_indices ====

#[test]
fn possible_indices_are_reduced_and_exclude_the_origin() {
    let indices = generate_possible_miller_indices(2);

    assert!(
        !indices.contains(&v(0, 0, 0)),
        "the origin is not a direction"
    );
    for index in &indices {
        assert_eq!(
            simplify_miller_index(*index),
            *index,
            "{index:?} is not in lowest terms"
        );
        assert!(index.x.abs() <= 2 && index.y.abs() <= 2 && index.z.abs() <= 2);
    }

    // Reduction collapses (2,2,2) onto (1,1,1) and (0,2,2) onto (0,1,1).
    assert!(indices.contains(&v(1, 1, 1)));
    assert!(!indices.contains(&v(2, 2, 2)));
    assert!(indices.contains(&v(0, 1, 1)));
    assert!(!indices.contains(&v(0, 2, 2)));
    // Coprime pairs survive at full magnitude.
    assert!(indices.contains(&v(1, 2, 2)));
    assert!(indices.contains(&v(-2, 1, 0)));
}

#[test]
fn possible_indices_at_bound_one_are_the_26_neighbours() {
    // With max = 1 every non-origin triple is already reduced, so the set is the
    // whole 3x3x3 cube minus its centre.
    let indices = generate_possible_miller_indices(1);
    assert_eq!(indices.len(), 26);
}

// ==== generate_unique_permutations ====

#[test]
fn permutations_are_sorted_and_deduplicated() {
    // The documented sort: downstream intersection geometry relies on it, so
    // this is the one place order is asserted rather than set membership.
    assert_eq!(
        generate_unique_permutations(1, 2, 3),
        vec![
            (1, 2, 3),
            (1, 3, 2),
            (2, 1, 3),
            (2, 3, 1),
            (3, 1, 2),
            (3, 2, 1)
        ]
    );
    // Repeated components collapse: 3 permutations, not 6.
    assert_eq!(
        generate_unique_permutations(1, 1, 2),
        vec![(1, 1, 2), (1, 2, 1), (2, 1, 1)]
    );
    // All three equal: a single permutation.
    assert_eq!(generate_unique_permutations(0, 0, 0), vec![(0, 0, 0)]);
    assert_eq!(generate_unique_permutations(4, 4, 4), vec![(4, 4, 4)]);
}

// ==== symmetry_equivalent_indices ====

#[test]
fn family_100_has_six_members() {
    assert_eq!(
        family_set(v(1, 0, 0)),
        set_of(&[
            (0, 0, 1),
            (0, 0, -1),
            (0, 1, 0),
            (0, -1, 0),
            (1, 0, 0),
            (-1, 0, 0),
        ])
    );
}

#[test]
fn family_110_has_twelve_members() {
    assert_eq!(
        family_set(v(1, 1, 0)),
        set_of(&[
            (0, 1, 1),
            (0, -1, 1),
            (0, 1, -1),
            (0, -1, -1),
            (1, 0, 1),
            (-1, 0, 1),
            (1, 0, -1),
            (-1, 0, -1),
            (1, 1, 0),
            (-1, 1, 0),
            (1, -1, 0),
            (-1, -1, 0),
        ])
    );
}

#[test]
fn family_111_has_eight_members() {
    assert_eq!(
        family_set(v(1, 1, 1)),
        set_of(&[
            (1, 1, 1),
            (-1, 1, 1),
            (1, -1, 1),
            (-1, -1, 1),
            (1, 1, -1),
            (-1, 1, -1),
            (1, -1, -1),
            (-1, -1, -1),
        ])
    );
}

#[test]
fn family_112_has_twentyfour_members() {
    // {hhl}: three permutations of the absolute values, eight signs each.
    let family = family_set(v(1, 1, 2));
    assert_eq!(family.len(), 24);
    assert_eq!(family, signed_permutations(1, 1, 2));
}

#[test]
fn general_family_123_is_the_full_48_member_orbit() {
    // The full point-group orbit: six permutations, eight signs each.
    let family = family_set(v(1, 2, 3));
    assert_eq!(family.len(), 48);
    assert_eq!(family, signed_permutations(1, 2, 3));
}

#[test]
fn input_signs_do_not_change_the_family() {
    // The function takes absolute values first, so (-1,2,-3) names {123}.
    assert_eq!(family_set(v(-1, 2, -3)), family_set(v(1, 2, 3)));
    assert_eq!(family_set(v(-1, 2, -3)).len(), 48);
    assert_eq!(family_set(v(-1, -1, -1)), family_set(v(1, 1, 1)));
    assert_eq!(family_set(v(0, 0, -1)), family_set(v(1, 0, 0)));
}

#[test]
fn origin_family_is_the_degenerate_single_member() {
    // Pins today's behaviour rather than asserting it is right: (0,0,0) is not a
    // real direction, but the enumeration returns it as a one-member family.
    assert_eq!(symmetry_equivalent_indices(v(0, 0, 0)), vec![v(0, 0, 0)]);
}

#[test]
fn family_order_is_deterministic() {
    // Not just the set: the emission order follows the permutation sort, and the
    // facet-shell cache indexes into the result.
    assert_eq!(
        symmetry_equivalent_indices(v(1, 0, 0)),
        vec![
            v(0, 0, 1),
            v(0, 0, -1),
            v(0, 1, 0),
            v(0, -1, 0),
            v(1, 0, 0),
            v(-1, 0, 0),
        ]
    );
    assert_eq!(
        symmetry_equivalent_indices(v(1, 1, 1)),
        vec![
            v(1, 1, 1),
            v(-1, 1, 1),
            v(1, -1, 1),
            v(-1, -1, 1),
            v(1, 1, -1),
            v(-1, 1, -1),
            v(1, -1, -1),
            v(-1, -1, -1),
        ]
    );
}
