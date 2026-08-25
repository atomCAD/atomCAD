//! The extraction pass: march, dedup, label, paint.

use std::collections::HashMap;

use atomcad_crystolecule::field::{IsosurfaceColoring, IsosurfaceData, ScalarField};
use glam::{DVec3, Vec3};

use super::case_table::{EDGE_CORNERS, case_triangles, corner_offsets, edge_axis};
use super::colormap::sample_colormap;
use super::lattice::Lattice;
use super::{ExtractionError, ExtractionSettings, SurfaceComponent, SurfaceMesh};

/// Extract the surface an [`IsosurfaceData`] specifies.
///
/// Runs the **same** extractor twice with a sign flag, comparing
/// `sign * psi(p)` against `level` and multiplying the gradient by `sign`: `+1`
/// gives the positive lobe, `-1` the negative one. Folding the sign into the
/// comparison is what keeps winding and normals consistent without a second
/// code path.
///
/// The negative pass is skipped when the field reports a non-negative
/// [`ScalarField::value_range`] — a pure optimization, since it would find no
/// crossings.
///
/// Note that a `level` of `0` is **accepted here** even though the *node*
/// rejects `level <= 0`. The node's rule is about the two sign passes being
/// meaningless at zero; this layer sits below it and is tested at zero
/// throughout. Do not push the node's validation down.
pub fn extract_isosurface(
    data: &IsosurfaceData,
    settings: &ExtractionSettings,
) -> Result<SurfaceMesh, ExtractionError> {
    let field = data.field.as_ref();
    let lattice = Lattice::choose(field, settings);

    // Pre-flight, before a single sample is taken or a single vertex allocated.
    let cells = lattice.cell_count();
    if cells > settings.cell_budget as u128 {
        return Err(ExtractionError::CellBudgetExceeded {
            cells,
            budget: settings.cell_budget,
            quality_multiplier: settings.quality_multiplier,
        });
    }

    let mut mesh = SurfaceMesh {
        alpha: data.alpha,
        ..SurfaceMesh::default()
    };
    append_pass(&mut mesh, data, &lattice, 1.0);
    if runs_negative_pass(field) {
        append_pass(&mut mesh, data, &lattice, -1.0);
    }
    Ok(mesh)
}

/// A field whose values are all `>= 0` cannot cross `-level`, so the second
/// pass is skipped. `None` (any analytic field) means "unknown", so both run.
fn runs_negative_pass(field: &dyn ScalarField) -> bool {
    !matches!(field.value_range(), Some((min, _)) if min >= 0.0)
}

/// March one sign, then dedup, label and paint it onto `mesh`.
fn append_pass(mesh: &mut SurfaceMesh, data: &IsosurfaceData, lattice: &Lattice, sign: f64) {
    let field = data.field.as_ref();
    let raw = march(field, data.level, sign, lattice);
    let (positions, triangles) = merge_coincident(
        raw.positions,
        raw.triangles,
        lattice.cell_extent() * MERGE_QUANTUM_FRACTION,
    );
    if triangles.is_empty() {
        return;
    }

    let ordered = order_by_component(&positions, &triangles);

    // Compact: walk the component-ordered triangles and mint final vertex
    // indices on first use. A component's vertices therefore occupy a
    // contiguous range too, which is what makes the centroid a slice average.
    let index_base = mesh.indices.len() as u32;
    let vertex_base = mesh.positions.len() as u32;
    let mut remap = vec![u32::MAX; positions.len()];
    let mut pass_positions: Vec<DVec3> = Vec::new();

    for group in &ordered {
        let first_index = mesh.indices.len() as u32;
        let component_vertex_start = pass_positions.len();
        for &triangle in &group.triangles {
            for old in triangle {
                let slot = &mut remap[old as usize];
                if *slot == u32::MAX {
                    *slot = pass_positions.len() as u32;
                    pass_positions.push(positions[old as usize]);
                }
                mesh.indices.push(vertex_base + *slot);
            }
        }
        let centroid = mean(&pass_positions[component_vertex_start..]);
        mesh.components.push(SurfaceComponent {
            first_index,
            index_count: group.triangles.len() as u32 * 3,
            centroid: centroid.as_vec3(),
        });
    }

    // Normals and albedo are computed once per *final* vertex, after the merge,
    // so a corner shared by several cut edges is sampled once.
    let mut normals = vec![Vec3::ZERO; pass_positions.len()];
    let mut needs_fallback = Vec::new();
    for (i, position) in pass_positions.iter().enumerate() {
        let gradient = field.gradient(*position);
        if gradient.length_squared() > 0.0 {
            // For the positive lobe the field decreases outward; for the
            // negative lobe it increases. The sign flag covers both.
            normals[i] = (-sign * gradient.normalize()).as_vec3();
        } else {
            needs_fallback.push(i);
        }
    }
    if !needs_fallback.is_empty() {
        fill_flat_normals(
            &pass_positions,
            &mesh.indices[index_base as usize..],
            vertex_base,
            &mut normals,
            &needs_fallback,
        );
    }

    let albedo: Vec<Vec3> = match &data.coloring {
        IsosurfaceColoring::Phase { positive, negative } => {
            let color = if sign >= 0.0 { *positive } else { *negative };
            vec![color; pass_positions.len()]
        }
        IsosurfaceColoring::Field {
            field: color_field,
            range,
            colormap,
        } => pass_positions
            .iter()
            .map(|p| sample_colormap(*colormap, color_field.sample(*p), *range))
            .collect(),
    };

    mesh.positions
        .extend(pass_positions.iter().map(|p| p.as_vec3()));
    mesh.normals.extend(normals);
    mesh.albedo.extend(albedo);
}

fn mean(positions: &[DVec3]) -> DVec3 {
    if positions.is_empty() {
        return DVec3::ZERO;
    }
    positions.iter().copied().sum::<DVec3>() / positions.len() as f64
}

/// Vertices where the gradient vanished — a flat or quantized region — fall
/// back to area-weighted face normals rather than to `normalize(0)`, which is
/// NaN and would propagate silently into a mesh that renders as nothing.
fn fill_flat_normals(
    positions: &[DVec3],
    indices: &[u32],
    vertex_base: u32,
    normals: &mut [Vec3],
    targets: &[usize],
) {
    let mut accumulated = vec![Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [
            (triangle[0] - vertex_base) as usize,
            (triangle[1] - vertex_base) as usize,
            (triangle[2] - vertex_base) as usize,
        ];
        let face = (positions[b] - positions[a])
            .cross(positions[c] - positions[a])
            .as_vec3();
        accumulated[a] += face;
        accumulated[b] += face;
        accumulated[c] += face;
    }
    for &i in targets {
        normals[i] = if accumulated[i].length_squared() > 0.0 {
            accumulated[i].normalize()
        } else {
            Vec3::Z
        };
    }
}

// ---------------------------------------------------------------------------
// Marching
// ---------------------------------------------------------------------------

struct RawPass {
    positions: Vec<DVec3>,
    triangles: Vec<[u32; 3]>,
}

/// A grid edge, keyed by its lower corner and axis so that the two (or four,
/// or eight) cells that share it agree on one vertex.
///
/// **Edge-keyed dedup is not an optimization**: without it every triangle
/// becomes its own connected component and the transparency sort is labelling
/// noise.
type EdgeKey = (u32, u32, u32, u8);

struct March<'a> {
    lattice: &'a Lattice,
    level: f64,
    flip: bool,
    positions: Vec<DVec3>,
    triangles: Vec<[u32; 3]>,
    edge_vertices: HashMap<EdgeKey, u32>,
}

fn march(field: &dyn ScalarField, level: f64, sign: f64, lattice: &Lattice) -> RawPass {
    let cells = lattice.cell_dims();
    let mut state = March {
        lattice,
        level,
        flip: lattice.flips_winding(),
        positions: Vec::new(),
        triangles: Vec::new(),
        edge_vertices: HashMap::new(),
    };
    if cells.contains(&0) {
        return RawPass {
            positions: state.positions,
            triangles: state.triangles,
        };
    }

    let plane_len = (cells[0] + 1) * (cells[1] + 1);
    let mut scratch = vec![DVec3::ZERO; plane_len];
    let mut lower = vec![0.0f64; plane_len];
    let mut upper = vec![0.0f64; plane_len];
    sample_plane(field, lattice, sign, cells, 0, &mut scratch, &mut lower);

    let stride = cells[1] + 1;
    for k in 0..cells[2] {
        sample_plane(field, lattice, sign, cells, k + 1, &mut scratch, &mut upper);
        for i in 0..cells[0] {
            for j in 0..cells[1] {
                let mut values = [0.0f64; 8];
                let mut mask = 0u8;
                for corner in 0..8u8 {
                    let offsets = corner_offsets(corner);
                    let plane = if offsets[2] == 0 { &lower } else { &upper };
                    let value = plane[(i + offsets[0]) * stride + (j + offsets[1])];
                    values[corner as usize] = value;
                    // Non-strict, and not a taste call: it makes the
                    // edge-interpolation denominator provably non-zero, because
                    // an edge is cut only when one endpoint is strictly below
                    // `level` and the other is at or above it.
                    if value >= level {
                        mask |= 1 << corner;
                    }
                }
                if mask == 0 || mask == u8::MAX {
                    continue;
                }
                for triangle in case_triangles(mask) {
                    let a = state.vertex([i, j, k], &values, triangle[0] as usize);
                    let b = state.vertex([i, j, k], &values, triangle[1] as usize);
                    let c = state.vertex([i, j, k], &values, triangle[2] as usize);
                    state
                        .triangles
                        .push(if state.flip { [a, c, b] } else { [a, b, c] });
                }
            }
        }
        std::mem::swap(&mut lower, &mut upper);
    }

    RawPass {
        positions: state.positions,
        triangles: state.triangles,
    }
}

/// One plane of lattice corner values, sign-folded.
///
/// Batched through [`ScalarField::sample_batch`], the contract's own path for
/// bulk evaluation — the trait is `Send + Sync` so this could be parallel, but
/// nothing here is until timings ask for it.
fn sample_plane(
    field: &dyn ScalarField,
    lattice: &Lattice,
    sign: f64,
    cells: [usize; 3],
    k: usize,
    points: &mut [DVec3],
    out: &mut [f64],
) {
    let stride = cells[1] + 1;
    for i in 0..=cells[0] {
        for j in 0..=cells[1] {
            points[i * stride + j] = lattice.corner_position(i, j, k);
        }
    }
    field.sample_batch(points, out);
    if sign < 0.0 {
        for value in out.iter_mut() {
            *value = -*value;
        }
    }
}

impl March<'_> {
    /// Index of the vertex on one cut edge of a cell, minting it on first use.
    fn vertex(&mut self, cell: [usize; 3], values: &[f64; 8], edge: usize) -> u32 {
        let lower_corner = EDGE_CORNERS[edge][0];
        let upper_corner = EDGE_CORNERS[edge][1];
        let lower_offsets = corner_offsets(lower_corner);
        let upper_offsets = corner_offsets(upper_corner);
        let lower_index = [
            cell[0] + lower_offsets[0],
            cell[1] + lower_offsets[1],
            cell[2] + lower_offsets[2],
        ];
        let key: EdgeKey = (
            lower_index[0] as u32,
            lower_index[1] as u32,
            lower_index[2] as u32,
            edge_axis(edge) as u8,
        );
        if let Some(&index) = self.edge_vertices.get(&key) {
            return index;
        }

        let lower_position =
            self.lattice
                .corner_position(lower_index[0], lower_index[1], lower_index[2]);
        let upper_position = self.lattice.corner_position(
            cell[0] + upper_offsets[0],
            cell[1] + upper_offsets[1],
            cell[2] + upper_offsets[2],
        );
        let lower_value = values[lower_corner as usize];
        let upper_value = values[upper_corner as usize];

        // Exactly one endpoint is inside — the case table only asks for cut
        // edges. Naming them "outside" and "inside" rather than "a" and "b"
        // makes the denominator's sign obvious: `inside_value >= level` and
        // `outside_value < level`, strictly, so it is positive and `t` lands in
        // `(0, 1]`. A strict inside test would leave `0 / 0` here on a tie, and
        // the resulting NaN would travel through normals and centroids into a
        // mesh that renders as nothing with no error anywhere.
        let (outside_position, outside_value, inside_position, inside_value) =
            if lower_value >= self.level {
                (upper_position, upper_value, lower_position, lower_value)
            } else {
                (lower_position, lower_value, upper_position, upper_value)
            };
        let t = (self.level - outside_value) / (inside_value - outside_value);
        // `t == 1` is the tie, and it must land *exactly* on the corner: the
        // coincident-vertex merge keys on bit-identical positions, and every
        // cell reaching this corner computes it through the same
        // `corner_position` call.
        let position = if t >= 1.0 {
            inside_position
        } else {
            outside_position + (inside_position - outside_position) * t
        };

        let index = self.positions.len() as u32;
        self.positions.push(position);
        self.edge_vertices.insert(key, index);
        index
    }
}

// ---------------------------------------------------------------------------
// Coincident-vertex merge
// ---------------------------------------------------------------------------

/// How finely the coincident-vertex merge quantizes, as a fraction of the
/// shortest lattice cell edge.
///
/// Coarse enough to swallow the sampling noise described below, and finer than
/// `f32` resolves at these magnitudes — so two vertices that survive this merge
/// cannot collapse into one when the positions narrow to `f32`.
const MERGE_QUANTUM_FRACTION: f64 = 1e-6;

/// Second dedup pass, and skipping it corrupts the component labelling in a way
/// that only shows up as a transparency bug.
///
/// With the non-strict tie rule a tied corner puts the vertex at a grid corner.
/// Up to three cut edges of that cell meet there, and edge-keyed dedup gives
/// each its own index — coincident in space, distinct in the index buffer — so
/// union-find refuses to join them and the surface splits into spurious
/// components. Ties are routine, not exotic: an integer-valued grid against a
/// level it can hit exactly produces them everywhere.
///
/// **Quantized, not exact.** An exact-bit key looks sufficient — a tie gives
/// `t == 1` and the corner position comes from one shared
/// `Lattice::corner_position` call — but it is not, because the *values* are
/// only approximately tied. `SampledField::sample` at a lattice corner
/// round-trips the point through `inv_basis`, so a stored `0.5` comes back as
/// `0.49999999…`; `t` then lands a hair under `1` and each edge places its own
/// vertex a hair off the corner, in a different direction. Those are coincident
/// for every purpose that matters and differ in every bit that does not.
///
/// The 27-cell probe is what keeps quantization honest: without it two vertices
/// a nanometre apart could still straddle a bucket boundary and refuse to
/// merge, which is the failure mode the quantum was introduced to remove.
fn merge_coincident(
    positions: Vec<DVec3>,
    triangles: Vec<[u32; 3]>,
    quantum: f64,
) -> (Vec<DVec3>, Vec<[u32; 3]>) {
    let mut merged: HashMap<[i64; 3], u32> = HashMap::with_capacity(positions.len());
    let mut remap = vec![0u32; positions.len()];
    let mut kept: Vec<DVec3> = Vec::with_capacity(positions.len());
    for (old, position) in positions.iter().enumerate() {
        let key = position_key(*position, quantum);
        let existing = neighbor_keys(key).find_map(|probe| merged.get(&probe).copied());
        let index = match existing {
            Some(index) => index,
            None => {
                kept.push(*position);
                let index = (kept.len() - 1) as u32;
                merged.insert(key, index);
                index
            }
        };
        remap[old] = index;
    }

    let triangles = triangles
        .into_iter()
        .map(|t| {
            [
                remap[t[0] as usize],
                remap[t[1] as usize],
                remap[t[2] as usize],
            ]
        })
        // A triangle two of whose vertices merged is zero-area. Dropping it is
        // safe for closure: its two surviving edges become the same undirected
        // edge with opposite directions and cancel.
        .filter(|t| t[0] != t[1] && t[1] != t[2] && t[2] != t[0])
        .collect();

    (kept, triangles)
}

fn position_key(position: DVec3, quantum: f64) -> [i64; 3] {
    [position.x, position.y, position.z].map(|c| (c / quantum).round() as i64)
}

/// The key itself, then its 26 neighbours — the key first so an exact hit,
/// which is the overwhelmingly common case, costs one lookup.
fn neighbor_keys(key: [i64; 3]) -> impl Iterator<Item = [i64; 3]> {
    std::iter::once(key).chain((0..27).map(move |i| {
        [
            key[0] + (i % 3) - 1,
            key[1] + (i / 3 % 3) - 1,
            key[2] + (i / 9) - 1,
        ]
    }))
}

// ---------------------------------------------------------------------------
// Connected components
// ---------------------------------------------------------------------------

struct ComponentGroup {
    triangles: Vec<[u32; 3]>,
}

/// Union-find over triangle vertex indices, re-emitted so each component holds
/// a contiguous run of triangles.
///
/// Components are ordered by their smallest vertex index. Component *order*
/// feeds nothing but the draw sort at runtime, so a nondeterministic one is
/// invisible until a snapshot test flakes — and the seeded fuzz is only
/// reproducible if identical input gives byte-identical output.
fn order_by_component(positions: &[DVec3], triangles: &[[u32; 3]]) -> Vec<ComponentGroup> {
    let mut parent: Vec<u32> = (0..positions.len() as u32).collect();
    for triangle in triangles {
        union(&mut parent, triangle[0], triangle[1]);
        union(&mut parent, triangle[1], triangle[2]);
    }

    let mut order: HashMap<u32, usize> = HashMap::new();
    let mut groups: Vec<(u32, ComponentGroup)> = Vec::new();
    for triangle in triangles {
        let root = find(&mut parent, triangle[0]);
        let slot = match order.get(&root) {
            Some(&slot) => slot,
            None => {
                order.insert(root, groups.len());
                groups.push((
                    root,
                    ComponentGroup {
                        triangles: Vec::new(),
                    },
                ));
                groups.len() - 1
            }
        };
        groups[slot].1.triangles.push(*triangle);
    }

    // The smallest vertex index in a component is the smallest of its
    // triangles' — cheaper to fold than to scan the vertex array.
    let mut keyed: Vec<(u32, ComponentGroup)> = groups
        .into_iter()
        .map(|(_, group)| {
            let smallest = group
                .triangles
                .iter()
                .flat_map(|t| t.iter())
                .copied()
                .min()
                .unwrap_or(u32::MAX);
            (smallest, group)
        })
        .collect();
    keyed.sort_by_key(|(smallest, _)| *smallest);
    keyed.into_iter().map(|(_, group)| group).collect()
}

fn find(parent: &mut [u32], mut node: u32) -> u32 {
    while parent[node as usize] != node {
        let grandparent = parent[parent[node as usize] as usize];
        parent[node as usize] = grandparent;
        node = grandparent;
    }
    node
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        // Always point the larger root at the smaller, so the representative of
        // a component is its smallest reachable index and the labelling does
        // not depend on visit order.
        let (small, large) = if ra < rb { (ra, rb) } else { (rb, ra) };
        parent[large as usize] = small;
    }
}
