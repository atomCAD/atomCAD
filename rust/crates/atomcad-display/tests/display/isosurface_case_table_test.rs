//! Table A of `doc/design_isosurface_node.md` P2: **the case table, directly
//! and exhaustively.**
//!
//! No field, no grid, no tolerance-fiddling — one unit cell, corner values
//! `+1`/`-1` from a bitmask, level `0`. All 256 run in microseconds.
//!
//! These rows carry more of the plan's weight than the end-to-end checks on
//! smooth analytic fields do: a sphere exercises perhaps a third of the cases
//! and systematically the easy third, and every configuration that produces a
//! hole is one a smooth blob rarely generates.

use std::collections::HashMap;

use atomcad_display::isosurface::case_table::{
    EDGE_CORNERS, FACE_CORNERS, case_triangles, corner_offsets, face_segments,
};
use glam::DVec3;

/// Midpoint of a cube edge, which is where every vertex lands when the corner
/// values are `+1`/`-1` and the level is `0`.
fn edge_midpoint(edge: u8) -> DVec3 {
    let a = corner_position(EDGE_CORNERS[edge as usize][0]);
    let b = corner_position(EDGE_CORNERS[edge as usize][1]);
    (a + b) * 0.5
}

fn corner_position(corner: u8) -> DVec3 {
    let o = corner_offsets(corner);
    DVec3::new(o[0] as f64, o[1] as f64, o[2] as f64)
}

fn inside(mask: u8, corner: u8) -> bool {
    mask & (1 << corner) != 0
}

#[test]
fn every_vertex_sits_on_a_cut_edge() {
    for mask in 0..=255u8 {
        for triangle in case_triangles(mask) {
            for &edge in triangle {
                let [a, b] = EDGE_CORNERS[edge as usize];
                assert_ne!(
                    inside(mask, a),
                    inside(mask, b),
                    "mask {mask:#010b} emits a vertex on edge {edge}, whose corners \
                     {a} and {b} classify the same way"
                );
            }
        }
    }
    // The converse: an edge that *is* cut must be used, or the patch has a hole
    // where it should have crossed.
    for mask in 1..255u8 {
        let used: Vec<u8> = case_triangles(mask)
            .iter()
            .flat_map(|t| t.iter().copied())
            .collect();
        for edge in 0..12u8 {
            let [a, b] = EDGE_CORNERS[edge as usize];
            if inside(mask, a) != inside(mask, b) {
                assert!(
                    used.contains(&edge),
                    "mask {mask:#010b} cuts edge {edge} but no triangle uses it"
                );
            }
        }
    }
}

#[test]
fn patches_are_valid_and_bounded_by_their_faces() {
    for mask in 0..=255u8 {
        let triangles = case_triangles(mask);

        for triangle in triangles {
            let [a, b, c] = triangle.map(edge_midpoint);
            let area = (b - a).cross(c - a).length() * 0.5;
            assert!(
                area > 1e-12,
                "mask {mask:#010b} emits the degenerate triangle {triangle:?}"
            );
        }

        // A patch edge lying on a cell face is a boundary edge, used once, and
        // cancels against the neighbouring cell. Every other edge runs through
        // the cell's interior — a fan diagonal — and must be shared by exactly
        // two triangles.
        let mut on_a_face: std::collections::HashSet<(u8, u8)> = std::collections::HashSet::new();
        for face in 0..6 {
            for &(from, to) in face_segments(mask, face).as_slice() {
                on_a_face.insert((from.min(to), from.max(to)));
            }
        }

        let mut used: HashMap<(u8, u8), usize> = HashMap::new();
        for triangle in triangles {
            for (x, y) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                *used.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        for (&(x, y), &count) in &used {
            let expected = if on_a_face.contains(&(x, y)) { 1 } else { 2 };
            assert_eq!(
                count, expected,
                "mask {mask:#010b}: patch edge {x}-{y} is used {count} times, expected {expected}"
            );
        }
    }
}

#[test]
fn face_segments_depend_only_on_that_faces_corner_signs() {
    // The closure property of the design's §Ambiguity, and the only test that
    // proves closure for fields nobody thought to try: a table can be closed on
    // every sphere anyone tries and still violate it on a configuration a
    // sphere never produces.
    for (face, corners) in FACE_CORNERS.iter().enumerate() {
        let mut seen: HashMap<u8, Vec<(u8, u8)>> = HashMap::new();
        for mask in 0..=255u8 {
            let signature = (0..4).fold(0u8, |acc, i| {
                acc | (u8::from(inside(mask, corners[i])) << i)
            });
            let segments = face_segments(mask, face).as_slice().to_vec();
            match seen.get(&signature) {
                Some(previous) => assert_eq!(
                    previous, &segments,
                    "face {face}: corner signature {signature:#06b} gives {segments:?} for \
                     mask {mask:#010b} but {previous:?} for an earlier mask — the segments \
                     depend on corners outside the face"
                ),
                None => {
                    seen.insert(signature, segments);
                }
            }
        }
        assert_eq!(seen.len(), 16, "face {face} did not see all 16 signatures");
    }
}

#[test]
fn patch_boundary_is_exactly_the_face_segments() {
    // Ties the previous test to the emitted geometry: the once-used edges of a
    // patch are precisely the segments the six faces asked for, directions
    // included. Without this, face-locality could hold while the triangulation
    // stitched the segments up wrongly.
    for mask in 0..=255u8 {
        let mut boundary: HashMap<(u8, u8), i32> = HashMap::new();
        for triangle in case_triangles(mask) {
            for edge in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                *boundary.entry(edge).or_insert(0) += 1;
                *boundary.entry((edge.1, edge.0)).or_insert(0) -= 1;
            }
        }
        // What survives the cancellation is the directed boundary.
        let mut expected: HashMap<(u8, u8), i32> = HashMap::new();
        for face in 0..6 {
            for &segment in face_segments(mask, face).as_slice() {
                *expected.entry(segment).or_insert(0) += 1;
                *expected.entry((segment.1, segment.0)).or_insert(0) -= 1;
            }
        }
        boundary.retain(|_, v| *v != 0);
        expected.retain(|_, v| *v != 0);
        assert_eq!(
            boundary, expected,
            "mask {mask:#010b}: the patch boundary is not the union of its face segments"
        );
    }
}

#[test]
fn complementary_masks_give_the_same_geometry_reversed() {
    // The "resolved consistently" rule of §Ambiguity, until now stated and
    // unverified. Triangles are compared as oriented cycles, since a fan can
    // legitimately emit the same triangle rotated.
    fn canonical(triangle: [u8; 3]) -> [u8; 3] {
        let smallest = (0..3).min_by_key(|&i| triangle[i]).unwrap();
        [
            triangle[smallest],
            triangle[(smallest + 1) % 3],
            triangle[(smallest + 2) % 3],
        ]
    }

    for mask in 0..=255u8 {
        let mut forward: Vec<[u8; 3]> = case_triangles(mask)
            .iter()
            .copied()
            .map(canonical)
            .collect();
        let mut reversed: Vec<[u8; 3]> = case_triangles(!mask)
            .iter()
            .map(|t| canonical([t[0], t[2], t[1]]))
            .collect();
        forward.sort_unstable();
        reversed.sort_unstable();
        assert_eq!(
            forward, reversed,
            "mask {mask:#010b} and its complement do not mirror each other"
        );
    }
}

#[test]
fn face_sharing_cells_leave_no_boundary_on_the_shared_face() {
    // Implied by face-locality; kept as the concrete corollary, because it
    // fails legibly. All 2^12 sign combinations of two cells stacked along z:
    // the four shared corners plus four private ones on each side.
    let shared_face_of_lower = 5; // z = 1
    let shared_face_of_upper = 4; // z = 0

    for combination in 0..(1u32 << 12) {
        // Lower cell corners 4..8 are the shared plane; upper cell corners 0..4
        // are the same four points.
        let lower_private = (combination & 0xF) as u8;
        let shared = ((combination >> 4) & 0xF) as u8;
        let upper_private = ((combination >> 8) & 0xF) as u8;

        let lower_mask = lower_private | (shared << 4);
        let upper_mask = shared | (upper_private << 4);

        let lower = face_segments(lower_mask, shared_face_of_lower);
        let upper = face_segments(upper_mask, shared_face_of_upper);

        // Same physical edges in each cell's own numbering: the upper cell's
        // z = 0 face uses edges 0, 1, 4, 5, and each is the lower cell's z = 1
        // edge two slots along (0 -> 2, 1 -> 3, 4 -> 6, 5 -> 7).
        let lift = |edge: u8| edge + 2;
        let mut net: HashMap<(u8, u8), i32> = HashMap::new();
        for &(from, to) in lower.as_slice() {
            *net.entry((from, to)).or_insert(0) += 1;
        }
        // Reversed, because the two cells see the face from opposite sides —
        // which is exactly what makes the boundaries cancel.
        for &(from, to) in upper.as_slice() {
            *net.entry((lift(to), lift(from))).or_insert(0) -= 1;
        }
        net.retain(|_, count| *count != 0);
        assert!(
            net.is_empty(),
            "combination {combination:#014b}: cells sharing a face leave {net:?} \
             uncancelled on it (lower mask {lower_mask:#010b}, upper {upper_mask:#010b})"
        );
    }
}
