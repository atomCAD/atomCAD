//! The marching-cubes case table — **generated, not transcribed**.
//!
//! The classic 15-case table (and the 256-entry expansions copied from it)
//! leaves *holes* on ambiguous faces, because complementary cases are resolved
//! with different connectivity. `doc/design_isosurface_node.md` §Ambiguity
//! rules that out: the renderer assumes closed surfaces, an open one destroys
//! the front/back-face pairing the two-pass draw depends on, and a hole
//! fragments the connected components the transparency sort is keyed on.
//!
//! So the table is *derived* from the property that makes closure a theorem
//! rather than a spot-check:
//!
//! > The set of segments a cell's patch leaves on one of its faces is a
//! > function of **that face's four corner signs alone**, and of nothing else
//! > in the cell.
//!
//! Two cells sharing a face see the same four signs, so their patch boundaries
//! on it coincide edge for edge and cancel; closure over the whole grid follows
//! by induction, for *any* field. [`face_segments`] is that function, and
//! [`case_triangles`] is built by running it over the six faces and stitching
//! the resulting directed segments into loops — so face-locality holds by
//! construction rather than by inspection of 256 hand-written rows.
//!
//! # Corner, edge and face numbering
//!
//! Corner `c` sits at `(c & 1, (c >> 1) & 1, (c >> 2) & 1)` of the unit cell —
//! bit `a` of the index is the coordinate along axis `a`. A mask bit is set
//! when that corner is **inside**, i.e. `sign * psi >= level` (non-strict; see
//! the design's §Degeneracies for why the tie goes that way).
//!
//! Edges are grouped by axis, four per axis, each stored as
//! `[lower corner, upper corner]` with the upper corner one step along that
//! axis — so `edge / 4` is the axis and the lower corner's offsets identify the
//! grid edge a cut belongs to.
//!
//! Faces are indexed `axis * 2 + side`.

use std::sync::LazyLock;

/// The two corners of each cube edge, `[lower, upper]` along the edge's axis.
/// Edges 0-3 run along x, 4-7 along y, 8-11 along z.
pub const EDGE_CORNERS: [[u8; 2]; 12] = [
    // x edges: the upper corner is `lower | 1`
    [0, 1],
    [2, 3],
    [4, 5],
    [6, 7],
    // y edges: the upper corner is `lower | 2`
    [0, 2],
    [1, 3],
    [4, 6],
    [5, 7],
    // z edges: the upper corner is `lower | 4`
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];

/// Axis an edge runs along: `0` = x, `1` = y, `2` = z.
pub const fn edge_axis(edge: usize) -> usize {
    edge / 4
}

/// Unit-cell offsets of a corner, one bit per axis.
pub const fn corner_offsets(corner: u8) -> [usize; 3] {
    [
        (corner & 1) as usize,
        ((corner >> 1) & 1) as usize,
        ((corner >> 2) & 1) as usize,
    ]
}

/// In-plane axis pair `(U, V)` of each face, chosen so that `U x V` is the
/// face's **outward** normal.
///
/// That choice is what makes the ambiguity rule below canonical across a shared
/// face. Two cells meeting at a face see it as `(a, 1)` and `(a, 0)`, whose
/// frames here are exactly each other's swap — so their cyclic corner orders
/// are reverses of one another, `q0` and `q2` are the *same two physical
/// corners* for both, and `q1`/`q3` are the other same two. A rule phrased in
/// terms of `q0`/`q2` therefore means the same thing on both sides, which is
/// precisely what face-locality needs.
const FACE_FRAME: [[usize; 2]; 6] = [
    [2, 1], // face 0: x = 0, outward -x;  z X y = -x
    [1, 2], // face 1: x = 1, outward +x;  y X z = +x
    [0, 2], // face 2: y = 0, outward -y;  x X z = -y
    [2, 0], // face 3: y = 1, outward +y;  z X x = +y
    [1, 0], // face 4: z = 0, outward -z;  y X x = -z
    [0, 1], // face 5: z = 1, outward +z;  x X y = +z
];

const fn face_corner(face: usize, uu: usize, vv: usize) -> u8 {
    let axis = face / 2;
    let side = face % 2;
    let u = FACE_FRAME[face][0];
    let v = FACE_FRAME[face][1];
    let mut coords = [0usize; 3];
    coords[axis] = side;
    coords[u] = uu;
    coords[v] = vv;
    (coords[0] | (coords[1] << 1) | (coords[2] << 2)) as u8
}

const fn edge_between(c0: u8, c1: u8) -> u8 {
    let mut e = 0;
    while e < 12 {
        let a = EDGE_CORNERS[e][0];
        let b = EDGE_CORNERS[e][1];
        if (a == c0 && b == c1) || (a == c1 && b == c0) {
            return e as u8;
        }
        e += 1;
    }
    panic!("the two corners are not joined by a cube edge");
}

const fn build_face_corners() -> [[u8; 4]; 6] {
    let mut faces = [[0u8; 4]; 6];
    let mut f = 0;
    while f < 6 {
        faces[f] = [
            face_corner(f, 0, 0),
            face_corner(f, 1, 0),
            face_corner(f, 1, 1),
            face_corner(f, 0, 1),
        ];
        f += 1;
    }
    faces
}

const fn build_face_edges() -> [[u8; 4]; 6] {
    let mut faces = [[0u8; 4]; 6];
    let mut f = 0;
    while f < 6 {
        let c = FACE_CORNERS[f];
        faces[f] = [
            edge_between(c[0], c[1]),
            edge_between(c[1], c[2]),
            edge_between(c[2], c[3]),
            edge_between(c[3], c[0]),
        ];
        f += 1;
    }
    faces
}

/// The four corners of each face in cyclic order, counter-clockwise **as seen
/// from outside the cell**.
pub const FACE_CORNERS: [[u8; 4]; 6] = build_face_corners();

/// The four cube edges of each face, `FACE_EDGES[f][i]` joining
/// `FACE_CORNERS[f][i]` to `FACE_CORNERS[f][(i + 1) % 4]`.
pub const FACE_EDGES: [[u8; 4]; 6] = build_face_edges();

/// The directed contour segments a cell's patch leaves on one face: at most
/// two, each `(from edge, to edge)` as cube edge indices.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FaceSegments {
    segments: [(u8, u8); 2],
    len: usize,
}

impl FaceSegments {
    pub fn as_slice(&self) -> &[(u8, u8)] {
        &self.segments[..self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Marching **squares** on one face of the cell — the whole of the ambiguity
/// policy, and a pure function of `mask`'s four bits on that face.
///
/// Segments are directed so that the inside region lies to the *right* when the
/// face is viewed from outside the cell. Stitched into a loop and fanned, that
/// yields triangles wound counter-clockwise seen from outside — consistent with
/// the outward normal `-sign * grad(psi)`, which is what the two-pass
/// transparency draw depends on.
///
/// **The ambiguous case (alternating signs) always cuts off `q1` and `q3`**,
/// leaving `q0` and `q2` connected. The rule names *positions*, never signs, so
/// a mask and its complement produce the identical segment set with reversed
/// directions — the "resolved consistently" requirement — and, because
/// [`FACE_FRAME`] makes `q0`/`q2` the same physical pair for both cells sharing
/// the face, the two neighbours agree. It buys closure at the price of
/// occasionally producing a tunnel where two sheets were intended; per the
/// design, guaranteed-closed beats topologically-optimal.
pub fn face_segments(mask: u8, face: usize) -> FaceSegments {
    let corners = FACE_CORNERS[face];
    let edges = FACE_EDGES[face];
    let inside = [
        mask & (1 << corners[0]) != 0,
        mask & (1 << corners[1]) != 0,
        mask & (1 << corners[2]) != 0,
        mask & (1 << corners[3]) != 0,
    ];

    let cut_count = (0..4).filter(|&i| inside[i] != inside[(i + 1) % 4]).count();

    let mut out = FaceSegments::default();
    match cut_count {
        // Uniform face: the patch does not reach it.
        0 => {}
        // One segment, and its direction is forced: it runs from the edge the
        // inside region is entered on (walking the cyclic order) to the edge it
        // is left on.
        2 => {
            let from = (0..4).find(|&i| !inside[i] && inside[(i + 1) % 4]).unwrap();
            let to = (0..4).find(|&i| inside[i] && !inside[(i + 1) % 4]).unwrap();
            out.segments[0] = (edges[from], edges[to]);
            out.len = 1;
        }
        // Alternating signs — the ambiguous case. Cut off `q1` and `q3`.
        4 => {
            if inside[0] {
                // `q1` and `q3` are the *outside* corners being cut off here,
                // so each segment runs against the cyclic order.
                out.segments = [(edges[1], edges[0]), (edges[3], edges[2])];
            } else {
                out.segments = [(edges[0], edges[1]), (edges[2], edges[3])];
            }
            out.len = 2;
        }
        _ => unreachable!("a quad face has an even number of sign changes, at most 4"),
    }
    out
}

/// Triangles of the patch inside one cell, as triples of **cube edge indices**
/// wound counter-clockwise seen from outside — in a right-handed index space.
/// A mirroring lattice flips them at the call site; see
/// [`Lattice::flips_winding`](super::lattice::Lattice::flips_winding).
pub fn case_triangles(mask: u8) -> &'static [[u8; 3]] {
    &CASE_TABLE[mask as usize]
}

static CASE_TABLE: LazyLock<Vec<Vec<[u8; 3]>>> =
    LazyLock::new(|| (0..256).map(|mask| build_case(mask as u8)).collect());

/// Stitch the six faces' directed segments into closed loops and fan them.
///
/// Every cut edge borders exactly two faces and carries exactly one segment
/// endpoint on each, one incoming and one outgoing — so the segments decompose
/// into disjoint directed cycles with no bookkeeping beyond a successor array.
fn build_case(mask: u8) -> Vec<[u8; 3]> {
    const NONE: u8 = u8::MAX;
    let mut next = [NONE; 12];
    let mut prev = [NONE; 12];
    for face in 0..6 {
        for &(from, to) in face_segments(mask, face).as_slice() {
            debug_assert_eq!(next[from as usize], NONE, "edge {from} is left twice");
            debug_assert_eq!(prev[to as usize], NONE, "edge {to} is entered twice");
            next[from as usize] = to;
            prev[to as usize] = from;
        }
    }

    let mut triangles = Vec::new();
    let mut visited = [false; 12];
    // Ascending scan, so the first unvisited edge of a cycle is that cycle's
    // smallest index and the cycles come out ordered by it. Both halves matter:
    // the design requires deterministic emission order, and starting each fan
    // at the cycle's minimum is what makes a mask and its complement — whose
    // cycles are exact reverses of one another — fan into the same triangles
    // with reversed winding.
    for start in 0..12u8 {
        if next[start as usize] == NONE || visited[start as usize] {
            continue;
        }
        let mut cycle = Vec::new();
        let mut edge = start;
        loop {
            visited[edge as usize] = true;
            cycle.push(edge);
            edge = next[edge as usize];
            if edge == start {
                break;
            }
            debug_assert!(
                !visited[edge as usize],
                "segments do not decompose into simple cycles"
            );
        }
        for i in 1..cycle.len().saturating_sub(1) {
            triangles.push([cycle[0], cycle[i], cycle[i + 1]]);
        }
    }
    triangles
}
