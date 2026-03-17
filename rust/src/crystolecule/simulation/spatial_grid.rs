// Spatial grid for fast neighbor queries with distance cutoff.
//
// Partitions 3D space into a uniform grid of cubic cells. Each cell stores
// the indices of atoms whose positions fall within it. Neighbor queries
// iterate the axis-aligned bounding box (AABB) of the query sphere in grid
// coordinates and check actual distances, giving O(N) construction and
// O(k) per-atom neighbor enumeration (where k is the number of neighbors
// within the cutoff radius).

use rustc_hash::FxHashMap;

/// A uniform 3D grid for spatial neighbor queries.
///
/// Built from a flat position array `[x0, y0, z0, x1, y1, z1, ...]`.
/// Each cell is a cube of side `cell_size` and stores a `Vec<usize>` of
/// atom indices whose positions fall within that cell.
pub struct SpatialGrid {
    cell_size: f64,
    inv_cell_size: f64,
    cells: FxHashMap<(i32, i32, i32), Vec<usize>>,
}

impl SpatialGrid {
    /// Builds a spatial grid from a flat position array.
    ///
    /// `cell_size` should typically equal the cutoff radius so that neighbor
    /// queries only need to check at most 3^3 = 27 neighboring cells.
    pub fn from_positions(positions: &[f64], cell_size: f64) -> Self {
        debug_assert!(cell_size > 0.0, "cell_size must be positive");
        let inv_cell_size = 1.0 / cell_size;
        let mut cells: FxHashMap<(i32, i32, i32), Vec<usize>> = FxHashMap::default();

        let num_atoms = positions.len() / 3;
        for i in 0..num_atoms {
            let i3 = i * 3;
            let cx = (positions[i3] * inv_cell_size).floor() as i32;
            let cy = (positions[i3 + 1] * inv_cell_size).floor() as i32;
            let cz = (positions[i3 + 2] * inv_cell_size).floor() as i32;
            cells.entry((cx, cy, cz)).or_default().push(i);
        }

        Self {
            cell_size,
            inv_cell_size,
            cells,
        }
    }

    /// Calls `f(j)` for each atom `j` within `radius` of atom `center_idx`.
    ///
    /// Iterates the AABB of the query sphere in grid coordinates and performs
    /// an exact distance check for each candidate. The callback receives atom
    /// indices `j != center_idx` that satisfy `dist(center, j) < radius`.
    ///
    /// The caller is responsible for deduplication (e.g. only processing pairs
    /// where `j > center_idx`).
    pub fn for_each_neighbor<F: FnMut(usize)>(
        &self,
        positions: &[f64],
        center_idx: usize,
        radius: f64,
        mut f: F,
    ) {
        let i3 = center_idx * 3;
        let cx = positions[i3];
        let cy = positions[i3 + 1];
        let cz = positions[i3 + 2];
        let radius_sq = radius * radius;

        // AABB of sphere in grid coordinates.
        let min_gx = ((cx - radius) * self.inv_cell_size).floor() as i32;
        let max_gx = ((cx + radius) * self.inv_cell_size).floor() as i32;
        let min_gy = ((cy - radius) * self.inv_cell_size).floor() as i32;
        let max_gy = ((cy + radius) * self.inv_cell_size).floor() as i32;
        let min_gz = ((cz - radius) * self.inv_cell_size).floor() as i32;
        let max_gz = ((cz + radius) * self.inv_cell_size).floor() as i32;

        for gx in min_gx..=max_gx {
            for gy in min_gy..=max_gy {
                for gz in min_gz..=max_gz {
                    if let Some(atoms) = self.cells.get(&(gx, gy, gz)) {
                        for &j in atoms {
                            if j == center_idx {
                                continue;
                            }
                            let j3 = j * 3;
                            let dx = cx - positions[j3];
                            let dy = cy - positions[j3 + 1];
                            let dz = cz - positions[j3 + 2];
                            let dist_sq = dx * dx + dy * dy + dz * dz;
                            if dist_sq < radius_sq {
                                f(j);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Returns the cell size used by this grid.
    pub fn cell_size(&self) -> f64 {
        self.cell_size
    }
}
