use crate::crystolecule::crystolecule_constants::DEFAULT_ZINCBLENDE_MOTIF;
use crate::crystolecule::motif::Motif;
use crate::crystolecule::structure::Structure;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;
use crate::util::transform::Transform;
use glam::DQuat;
use glam::f64::{DVec2, DVec3};
use glam::i32::{IVec2, IVec3};

/// Defines a 2D drawing plane in 3D lattice space.
///
/// The plane is defined by:
/// - A Miller index (defines orientation relative to unit cell)
/// - A center point (origin of the 2D coordinate system)
/// - A shift along the plane normal (with optional subdivision for fractional shifts)
/// - Two in-plane basis vectors (u_axis, v_axis) that form a right-handed coordinate system
///
/// 2D coordinates (u, v) on this plane map to 3D lattice coordinates as:
/// `position_3d = center + shift_offset + u * u_axis + v * v_axis`
#[derive(Clone, Debug)]
pub struct DrawingPlane {
    /// The unit cell that defines the lattice
    pub unit_cell: UnitCellStruct,

    /// Miller index defining the plane orientation (normal direction)
    pub miller_index: IVec3,

    /// Center point in lattice coordinates - serves as the origin of the 2D coordinate system
    pub center: IVec3,

    /// Integer shift along the plane normal (in units of d-spacing/subdivision)
    pub shift: i32,

    /// Subdivision factor for fractional d-spacing shifts
    /// shift_distance = (shift / subdivision) * d_spacing
    pub subdivision: i32,

    /// First in-plane lattice basis vector (u-axis)
    /// Computed from Miller index, guaranteed to be in the plane and primitive
    pub u_axis: IVec3,

    /// Second in-plane lattice basis vector (v-axis)
    /// Computed from Miller index, guaranteed to be in the plane and primitive
    /// Forms right-handed system: (u_axis × v_axis) · normal > 0
    pub v_axis: IVec3,

    /// Effective unit cell for 2D operations within the plane.
    ///
    /// This unit cell lives in a plane-local orthogonal coordinate system:
    /// - the drawing plane is the local XY plane
    /// - `a` and `b` are 2D basis vectors expressed in that local XY
    /// - `c` is local Z (scaled by d-spacing)
    pub effective_unit_cell: UnitCellStruct,

    /// The motif of the crystal structure this plane is embedded in.
    ///
    /// The drawing plane's *geometry* depends only on `unit_cell` + orientation,
    /// but it carries the motif (and `motif_offset`) so the full `Structure` is
    /// the single source of truth for the 2D→3D transition (`extrude`). Defaults
    /// to the zincblende motif (carbon) so pre-existing callers are unaffected.
    /// See `doc/design_drawing_plane_carries_structure.md`.
    pub motif: Motif,

    /// Fractional motif offset carried alongside `motif` (see `motif`).
    pub motif_offset: DVec3,
}

impl DrawingPlane {
    /// Creates a new drawing plane from Miller indices and parameters.
    ///
    /// Automatically computes the in-plane basis vectors (u_axis, v_axis) using the
    /// canonical perpendicular vector construction. The axes form a right-handed
    /// coordinate system with the plane normal.
    ///
    /// # Arguments
    /// * `unit_cell` - The lattice unit cell
    /// * `miller_index` - Miller indices defining plane orientation
    /// * `center` - Origin point in lattice coordinates
    /// * `shift` - Integer offset along normal direction
    /// * `subdivision` - Subdivision factor (default: 1)
    ///
    /// # Returns
    /// * `Ok(DrawingPlane)` - Successfully created plane
    /// * `Err(String)` - If plane axes cannot be computed (e.g., zero miller index)
    pub fn new(
        unit_cell: UnitCellStruct,
        miller_index: IVec3,
        center: IVec3,
        shift: i32,
        subdivision: i32,
    ) -> Result<Self, String> {
        // Thin wrapper around `from_spec` for the classic "Miller index only" path,
        // keeping every existing caller (xy_plane, tests) untouched.
        Self::from_spec(
            unit_cell,
            Some(miller_index),
            None,
            None,
            center,
            shift,
            subdivision,
        )
    }

    /// Creates a drawing plane from an explicit orientation spec.
    ///
    /// A plane needs either its normal (`miller`), or two in-plane directions
    /// (`u`, `v`). The four valid cases:
    ///
    /// - **A** (`m` only): auto-generate both in-plane axes from `m` (classic behavior).
    /// - **B** (`m` + `u`): verify `u` lies in the plane (Weiss zone law); basis is
    ///   `u` plus the first auto axis that is not collinear with `u`.
    /// - **C** (`m` + `u` + `v`): verify both `u` and `v` lie in the plane and are
    ///   non-collinear; use them verbatim (no handedness flip).
    /// - **D** (`u` + `v`, no `m`): derive `m = reduce(u × v)`; use `u`, `v` verbatim.
    ///
    /// In-plane directions are **direct-space lattice direction indices** `[u v w]`
    /// (steps along the unit-cell vectors a, b, c), distinct from the Miller plane
    /// index `(h k l)`. A direction lies in a plane iff the Weiss zone law holds:
    /// `h·u + k·v + l·w = 0`.
    ///
    /// Every invalid combination returns a localized `Err(String)` with an explicit
    /// message; nothing is silently reconciled.
    pub fn from_spec(
        unit_cell: UnitCellStruct,
        miller: Option<IVec3>,
        u: Option<IVec3>,
        v: Option<IVec3>,
        center: IVec3,
        shift: i32,
        subdivision: i32,
    ) -> Result<Self, String> {
        match (miller, u, v) {
            // Case A: Miller index only — auto-generate both axes.
            (Some(m), None, None) => {
                let (ua, va) = compute_auto_axes(&unit_cell, &m)?;
                Self::build_from_axes(unit_cell, m, ua, va, center, shift, subdivision, true)
            }

            // Case B: Miller index + first in-plane axis.
            (Some(m), Some(u), None) => {
                if !in_plane(&m, &u) {
                    return Err(format!(
                        "u direction [{},{},{}] does not lie in the ({},{},{}) plane \
                         (Weiss zone law violated)",
                        u.x, u.y, u.z, m.x, m.y, m.z
                    ));
                }
                let (ua, va) = compute_auto_axes(&unit_cell, &m)?;
                // Reuse the first already-valid auto axis that is not collinear with `u`.
                // At least one of the two auto axes is non-collinear with any single
                // in-plane direction, so this always resolves.
                let second = if !collinear(&u, &ua) {
                    ua
                } else if !collinear(&u, &va) {
                    va
                } else {
                    return Err(format!(
                        "Could not find an auto axis non-collinear with u [{},{},{}]",
                        u.x, u.y, u.z
                    ));
                };
                Self::build_from_axes(unit_cell, m, u, second, center, shift, subdivision, true)
            }

            // Case C: Miller index + both in-plane axes, used verbatim.
            (Some(m), Some(u), Some(v)) => {
                if !in_plane(&m, &u) {
                    return Err(format!(
                        "u direction [{},{},{}] does not lie in the ({},{},{}) plane \
                         (Weiss zone law violated)",
                        u.x, u.y, u.z, m.x, m.y, m.z
                    ));
                }
                if !in_plane(&m, &v) {
                    return Err(format!(
                        "v direction [{},{},{}] does not lie in the ({},{},{}) plane \
                         (Weiss zone law violated)",
                        v.x, v.y, v.z, m.x, m.y, m.z
                    ));
                }
                if collinear(&u, &v) {
                    return Err(format!(
                        "u [{},{},{}] and v [{},{},{}] are collinear; \
                         in-plane axes must be non-collinear",
                        u.x, u.y, u.z, v.x, v.y, v.z
                    ));
                }
                // Honor the user's axes verbatim — no handedness flip (decision 6).
                Self::build_from_axes(unit_cell, m, u, v, center, shift, subdivision, false)
            }

            // Case D: two in-plane axes, derive the Miller index.
            (None, Some(u), Some(v)) => {
                let m = derive_miller(&u, &v)?;
                Self::build_from_axes(unit_cell, m, u, v, center, shift, subdivision, false)
            }

            // Error: only `v` provided (with a Miller index).
            (Some(_), None, Some(_)) => Err("specify `u`, not only `v`".to_string()),

            // Errors: under-specified plane.
            (None, Some(_), None) | (None, None, Some(_)) => {
                Err("under-specified plane: give a Miller index or both `u` and `v`".to_string())
            }

            // Error: nothing provided.
            (None, None, None) => Err("plane orientation unspecified".to_string()),
        }
    }

    /// Finalizes a drawing plane from chosen integer in-plane axes.
    ///
    /// This is the portion of construction *after* the axes are selected:
    /// optional right-handed flip, Gram-Schmidt orthonormalization, and
    /// `effective_unit_cell` construction in plane-local XY.
    ///
    /// When `enforce_right_handed` is true, `v_axis` is flipped if needed so that
    /// `(u × v) · n > 0`. Case C/D pass `false` to honor the user's orientation.
    fn build_from_axes(
        unit_cell: UnitCellStruct,
        miller_index: IVec3,
        u_axis: IVec3,
        mut v_axis: IVec3,
        center: IVec3,
        shift: i32,
        subdivision: i32,
        enforce_right_handed: bool,
    ) -> Result<Self, String> {
        // Plane normal (real-space direction) for handedness checks.
        let normal_dir = unit_cell
            .ivec3_miller_index_to_plane_props(&miller_index)
            .map_err(|e| format!("Failed to compute plane properties: {}", e))?
            .normal;

        if enforce_right_handed {
            // Ensure right-handed coordinate system: (u × v) · n > 0
            let cross = (u_axis.as_dvec3()).cross(v_axis.as_dvec3()).normalize();
            if cross.dot(normal_dir) < 0.0 {
                // Flip v-axis to make right-handed
                v_axis = -v_axis;
            }
        }

        // 2D geometry nodes operate in plane-local coordinates. We therefore store
        // the effective unit cell in a local orthogonal XY system, not in world XYZ.
        let u_real = unit_cell.ivec3_lattice_to_real(&u_axis);
        let v_real = unit_cell.ivec3_lattice_to_real(&v_axis);

        let u_dir = u_real.normalize();
        let v_ortho = v_real - u_dir * v_real.dot(u_dir);
        if v_ortho.length_squared() < 1e-12 {
            return Err(
                "Failed to construct drawing plane basis: in-plane axes are nearly collinear"
                    .to_string(),
            );
        }
        let v_dir = v_ortho.normalize();

        let plane_props = unit_cell
            .ivec3_miller_index_to_plane_props(&miller_index)
            .map_err(|e| format!("Failed to compute plane properties: {}", e))?;

        // Express the lattice basis vectors (u_axis, v_axis) in plane-local XY.
        let a_local = DVec3::new(u_real.length(), 0.0, 0.0);
        let b_local = DVec3::new(v_real.dot(u_dir), v_real.dot(v_dir), 0.0);
        let c_local = DVec3::new(0.0, 0.0, plane_props.d_spacing);

        let effective_unit_cell = UnitCellStruct::new(a_local, b_local, c_local);

        Ok(Self {
            unit_cell,
            miller_index,
            center,
            shift,
            subdivision: subdivision.max(1), // Ensure minimum value of 1
            u_axis,
            v_axis,
            effective_unit_cell,
            // Default to the standard zincblende (carbon) motif + zero offset so
            // existing callers get the historical behavior; callers that have a
            // full `Structure` override these via `with_structure`.
            motif: DEFAULT_ZINCBLENDE_MOTIF.clone(),
            motif_offset: DVec3::ZERO,
        })
    }

    /// Attaches the motif and motif offset of the crystal structure this plane
    /// is embedded in, returning the enriched plane. The plane's `unit_cell`
    /// (lattice vectors) is unchanged — it stays the single source of the
    /// in-plane geometry — while `motif`/`motif_offset` ride along so `extrude`
    /// can reconstitute the full `Structure`.
    pub fn with_structure(mut self, motif: Motif, motif_offset: DVec3) -> Self {
        self.motif = motif;
        self.motif_offset = motif_offset;
        self
    }

    /// The full crystal `Structure` this plane is embedded in: the plane's
    /// `unit_cell` as lattice vectors, plus the carried `motif`/`motif_offset`.
    pub fn structure(&self) -> Structure {
        Structure {
            lattice_vecs: self.unit_cell.clone(),
            motif: self.motif.clone(),
            motif_offset: self.motif_offset,
        }
    }

    /// Creates a drawing plane with default XY plane orientation (001 Miller index) at origin.
    ///
    /// This is a convenience function for creating a standard horizontal plane with the given unit cell.
    ///
    /// # Arguments
    /// * `unit_cell` - The lattice unit cell
    ///
    /// # Returns
    /// * `Result<DrawingPlane, String>` - Drawing plane or error if construction fails
    pub fn xy_plane(unit_cell: UnitCellStruct) -> Result<Self, String> {
        Self::new(
            unit_cell,
            IVec3::new(0, 0, 1), // XY plane (001 Miller index)
            IVec3::ZERO,         // Center at origin
            0,                   // No shift
            1,                   // Default subdivision
        )
    }

    /// Checks if two drawing planes are compatible for boolean operations.
    ///
    /// Planes are compatible if they have the same unit cell, orientation,
    /// in-plane axes, position, and shift parameters.
    ///
    /// The in-plane axes (`u_axis`, `v_axis`) must match too: with user-pinned
    /// axes two planes can share the same `miller_index` yet have entirely
    /// different in-plane frames (e.g. case A auto axes vs. case C explicit,
    /// rotated axes). Combining those as if identical would silently produce
    /// wrong geometry — precisely the reconciliation this design forbids. The
    /// compared axes are the *finalized* ones (post right-handed flip /
    /// Gram-Schmidt), so two case-A planes with the same `miller_index` still
    /// match.
    pub fn is_compatible(&self, other: &DrawingPlane) -> bool {
        self.unit_cell.is_approximately_equal(&other.unit_cell)
            && self.miller_index == other.miller_index
            && self.center == other.center
            && self.shift == other.shift
            && self.subdivision == other.subdivision
            && self.u_axis == other.u_axis
            && self.v_axis == other.v_axis
    }

    /// Maps a 2D real coordinate (in plane space) to 3D world position.
    ///
    /// This places a point on the actual drawing plane in 3D space by:
    /// 1. Using the plane's u_axis and v_axis to position in 3D world space
    /// 2. Starting from the plane's center point
    /// 3. Applying the shift offset along the plane normal
    ///
    /// # Arguments
    /// * `real_2d` - 2D real coordinate in plane space (in length units)
    ///
    /// # Returns
    /// * 3D position in world space on this plane
    pub fn real_2d_to_world_3d(&self, real_2d: &DVec2) -> DVec3 {
        // 1. Get plane basis vectors in 3D world space
        let u_real = self.unit_cell.ivec3_lattice_to_real(&self.u_axis);
        let v_real = self.unit_cell.ivec3_lattice_to_real(&self.v_axis);

        // Use an orthonormal in-plane basis so plane-local XY distances map correctly
        // for arbitrary Miller indices.
        let u_dir = u_real.normalize();
        let v_ortho = v_real - u_dir * v_real.dot(u_dir);
        let v_dir = if v_ortho.length_squared() < 1e-12 {
            v_real.normalize()
        } else {
            v_ortho.normalize()
        };

        // 2. Get plane origin (center point in 3D)
        let plane_origin = self.unit_cell.ivec3_lattice_to_real(&self.center);

        // 3. Calculate shift offset along plane normal
        // Get plane properties to obtain d_spacing
        let plane_props = self
            .unit_cell
            .ivec3_miller_index_to_plane_props(&self.miller_index)
            .expect("Miller index should be valid for DrawingPlane");
        let shift_distance = (self.shift as f64 / self.subdivision as f64) * plane_props.d_spacing;
        let shifted_origin = plane_origin + plane_props.normal * shift_distance;

        // 4. Construct 3D position: shifted_origin + x*u_dir + y*v_dir
        shifted_origin + u_dir * real_2d.x + v_dir * real_2d.y
    }

    /// Maps a 2D lattice coordinate (in plane space) to 3D world position.
    ///
    /// This places vertices on the actual drawing plane in 3D space by:
    /// 1. Converting lattice coordinates to 2D real space in the plane
    /// 2. Using the plane's u_axis and v_axis to position in 3D world space
    ///
    /// # Arguments
    /// * `lattice_2d` - 2D lattice coordinate in plane space
    ///
    /// # Returns
    /// * 3D position in world space on this plane
    pub fn lattice_2d_to_world_3d(&self, lattice_2d: &IVec2) -> DVec3 {
        // Convert lattice → 2D real coordinates, then map to 3D
        let real_2d = self.effective_unit_cell.ivec2_lattice_to_real(lattice_2d);
        self.real_2d_to_world_3d(&real_2d)
    }

    /// Finds the nearest lattice point by intersecting a ray with this drawing plane.
    ///
    /// This method:
    /// 1. Computes the ray-plane intersection point in 3D
    /// 2. Projects the intersection onto the plane's 2D coordinate system
    /// 3. Converts to lattice coordinates
    ///
    /// # Arguments
    /// * `ray_origin` - Origin of the ray in 3D world space
    /// * `ray_direction` - Direction of the ray (need not be normalized)
    ///
    /// # Returns
    /// * `Some(IVec2)` - Lattice coordinates where ray intersects plane
    /// * `None` - If ray doesn't intersect plane in forward direction or is parallel
    pub fn find_lattice_point_by_ray(
        &self,
        ray_origin: &DVec3,
        ray_direction: &DVec3,
    ) -> Option<IVec2> {
        // Get plane basis vectors in 3D world space
        let u_real = self.unit_cell.ivec3_lattice_to_real(&self.u_axis);
        let v_real = self.unit_cell.ivec3_lattice_to_real(&self.v_axis);
        let plane_origin = self.unit_cell.ivec3_lattice_to_real(&self.center);

        let plane_props = self
            .unit_cell
            .ivec3_miller_index_to_plane_props(&self.miller_index)
            .expect("Miller index should be valid for DrawingPlane");
        let shift_distance = (self.shift as f64 / self.subdivision as f64) * plane_props.d_spacing;
        let shifted_origin = plane_origin + plane_props.normal * shift_distance;

        // Compute plane normal: u × v (cross product)
        let plane_normal = u_real.cross(v_real).normalize();

        // Ray-plane intersection: t = (plane_point - ray_origin) · n / (ray_direction · n)
        let denominator = ray_direction.dot(plane_normal);

        // Avoid division by zero (ray parallel to plane)
        if denominator.abs() < 1e-6 {
            return None;
        }

        let t = (shifted_origin - ray_origin).dot(plane_normal) / denominator;

        if t <= 0.0 {
            // Ray doesn't hit the plane in the forward direction
            return None;
        }

        let intersection_3d = *ray_origin + *ray_direction * t;

        // Map intersection into plane-local XY by projecting onto the orthonormal basis.
        let relative_pos = intersection_3d - shifted_origin;
        let u_dir = u_real.normalize();
        let v_ortho = v_real - u_dir * v_real.dot(u_dir);
        if v_ortho.length_squared() < 1e-12 {
            return None;
        }
        let v_dir = v_ortho.normalize();

        let x = relative_pos.dot(u_dir);
        let y = relative_pos.dot(v_dir);

        let local_real_3d = DVec3::new(x, y, 0.0);
        let lattice_3d = self
            .effective_unit_cell
            .real_to_ivec3_lattice(&local_real_3d);

        Some(IVec2::new(lattice_3d.x, lattice_3d.y))
    }

    /// Validates an extrusion direction for this drawing plane.
    ///
    /// An extrusion direction is valid if it points away from the plane (has positive
    /// projection onto the plane normal). This ensures extrusion creates geometry
    /// extending outward from the plane rather than into it.
    ///
    /// # Arguments
    /// * `extrude_direction` - Miller index direction in the unit cell (lattice coordinates)
    ///
    /// # Returns
    /// * `Ok((normalized_direction, d_spacing))` - Normalized direction vector and d-spacing in real space
    /// * `Err(String)` - Error message if direction is invalid
    ///
    /// # Errors
    /// * If direction is zero vector
    /// * If direction is parallel or nearly parallel to plane (zero projection)
    /// * If direction points into the plane (negative projection)
    pub fn validate_extrude_direction(
        &self,
        extrude_direction: &IVec3,
    ) -> Result<(DVec3, f64), String> {
        // Check for zero vector
        if *extrude_direction == IVec3::ZERO {
            return Err("Extrusion direction cannot be zero vector [0,0,0]".to_string());
        }

        let extrude_plane_props = self
            .unit_cell
            .ivec3_miller_index_to_plane_props(extrude_direction)
            .map_err(|e| format!("Failed to compute extrusion direction properties: {}", e))?;

        let drawing_plane_props = self
            .unit_cell
            .ivec3_miller_index_to_plane_props(&self.miller_index)
            .map_err(|e| format!("Failed to compute drawing plane properties: {}", e))?;

        let projection = extrude_plane_props.normal.dot(drawing_plane_props.normal);

        if projection.abs() < 1e-10 {
            return Err(format!(
                "Invalid extrusion direction [{},{},{}]: parallel or nearly parallel to plane (no outward component)",
                extrude_direction.x, extrude_direction.y, extrude_direction.z
            ));
        }

        if projection < 0.0 {
            return Err(format!(
                "Invalid extrusion direction [{},{},{}]: points into plane (negative projection). Try negating the direction.",
                extrude_direction.x, extrude_direction.y, extrude_direction.z
            ));
        }

        Ok((extrude_plane_props.normal, extrude_plane_props.d_spacing))
    }

    /// Computes the transformation from plane-local coordinates to world coordinates.
    ///
    /// This creates a Transform that maps:
    /// - Plane-local X axis → world space u_axis direction
    /// - Plane-local Y axis → world space v_axis direction
    /// - Plane-local Z axis → world space plane normal
    /// - Origin → plane center with shift applied
    ///
    /// This is used by extrusion to place plane-local geometry in world space.
    ///
    /// # Returns
    /// * Transform that maps plane-local coordinates to world coordinates
    pub fn to_world_transform(&self) -> Transform {
        // 1. Get plane basis vectors in world space
        let u_real = self.unit_cell.ivec3_lattice_to_real(&self.u_axis);
        let v_real = self.unit_cell.ivec3_lattice_to_real(&self.v_axis);

        let u_unit = u_real.normalize();
        let v_ortho = v_real - u_unit * v_real.dot(u_unit);
        let v_unit = v_ortho.normalize();
        let normal = u_unit.cross(v_unit).normalize();

        // 3. Create rotation matrix from basis vectors
        // Columns: [u_unit, v_unit, normal] maps local (x,y,z) → world
        let rotation = DQuat::from_mat3(&glam::f64::DMat3::from_cols(u_unit, v_unit, normal));

        // 4. Get translation (plane origin with shift applied)
        let plane_origin = self.unit_cell.ivec3_lattice_to_real(&self.center);
        let plane_props = self
            .unit_cell
            .ivec3_miller_index_to_plane_props(&self.miller_index)
            .expect("Miller index should be valid for DrawingPlane");
        let shift_distance = (self.shift as f64 / self.subdivision as f64) * plane_props.d_spacing;
        let translation = plane_origin + plane_props.normal * shift_distance;

        Transform::new(translation, rotation)
    }
}

impl Default for DrawingPlane {
    /// Creates a default drawing plane with cubic diamond unit cell and XY orientation.
    ///
    /// This is the most common default for 2D geometry nodes.
    /// Equivalent to `DrawingPlane::xy_plane(UnitCellStruct::cubic_diamond())`.
    fn default() -> Self {
        Self::xy_plane(UnitCellStruct::cubic_diamond())
            .expect("Default drawing plane construction should never fail")
    }
}

/// Computes two primitive in-plane lattice basis vectors from a Miller index.
///
/// Uses the canonical perpendicular vector construction:
/// For Miller index m = [h, k, l], the three canonical solutions to m · t = 0 are:
/// - t1 = [0, l, -k]
/// - t2 = [-l, 0, h]
/// - t3 = [k, -h, 0]
///
/// Each is reduced to primitive form by dividing by GCD of components.
/// Returns the first two non-collinear non-zero vectors.
///
/// # Arguments
/// * `m` - Miller index vector
///
/// # Returns
/// * `Ok((u, v))` - Two non-collinear primitive in-plane vectors
/// * `Err(String)` - If no suitable vectors found (shouldn't happen for valid Miller indices)
pub fn compute_plane_axes(m: &IVec3) -> Result<(IVec3, IVec3), String> {
    if *m == IVec3::ZERO {
        return Err("Miller index cannot be zero vector".to_string());
    }

    // Three canonical solutions to m · t = 0
    let t1 = IVec3::new(0, m.z, -m.y);
    let t2 = IVec3::new(-m.z, 0, m.x);
    let t3 = IVec3::new(m.y, -m.x, 0);

    // Reduce to primitive vectors
    let v1 = reduce_to_primitive(t1);
    let v2 = reduce_to_primitive(t2);
    let v3 = reduce_to_primitive(t3);

    // Select first two non-collinear non-zero vectors
    let candidates = [v1, v2, v3];

    for i in 0..3 {
        if candidates[i] == IVec3::ZERO {
            continue;
        }
        for j in (i + 1)..3 {
            if candidates[j] == IVec3::ZERO {
                continue;
            }

            // Check non-collinear: |u × v| > 0
            let cross = candidates[i].as_dvec3().cross(candidates[j].as_dvec3());
            if cross.length() > 1e-10 {
                return Ok((candidates[i], candidates[j]));
            }
        }
    }

    Err(format!(
        "Could not find two non-collinear in-plane vectors for Miller index ({}, {}, {})",
        m.x, m.y, m.z
    ))
}

/// Integer cross product (exact, no float round-off).
///
/// Used for in-plane integer-lattice tests (collinearity, derived Miller index).
fn ivec3_cross(a: &IVec3, b: &IVec3) -> IVec3 {
    IVec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

/// Weiss zone law: direction `d` `[u v w]` lies in the plane with Miller index
/// `m` `(h k l)` iff `h·u + k·v + l·w == 0`.
pub fn in_plane(m: &IVec3, d: &IVec3) -> bool {
    m.x * d.x + m.y * d.y + m.z * d.z == 0
}

/// Two integer lattice directions are collinear iff their integer cross product
/// is the zero vector (e.g. `[2,0,0]` is collinear with `[1,0,0]`).
pub fn collinear(a: &IVec3, b: &IVec3) -> bool {
    ivec3_cross(a, b) == IVec3::ZERO
}

/// Derives a Miller plane index from two in-plane directions via the Weiss zone
/// law run backwards: `m = reduce(u × v)`.
///
/// The integer cross product preserves the sign of `u × v`, and reduction to
/// primitive form divides by a positive GCD, so the resulting normal satisfies
/// `(u × v) · n > 0` — the pair is already right-handed by construction.
///
/// # Errors
/// * If `u` and `v` are parallel (`u × v == 0`), the plane is degenerate.
pub fn derive_miller(u: &IVec3, v: &IVec3) -> Result<IVec3, String> {
    let cross = ivec3_cross(u, v);
    if cross == IVec3::ZERO {
        return Err(format!(
            "Degenerate plane: u [{},{},{}] and v [{},{},{}] are parallel \
             (zero cross product); cannot derive a Miller index",
            u.x, u.y, u.z, v.x, v.y, v.z
        ));
    }
    Ok(reduce_to_primitive(cross))
}

/// Picks a deterministic pair of in-plane lattice axes for a Miller plane.
///
/// The chosen axes are scored to best match global X/Y projected onto the plane,
/// producing a stable, predictable in-plane orientation. This is the case-A
/// auto-basis (formerly `compute_preferred_plane_axes`).
pub fn compute_auto_axes(unit_cell: &UnitCellStruct, m: &IVec3) -> Result<(IVec3, IVec3), String> {
    if *m == IVec3::ZERO {
        return Err("Miller index cannot be zero vector".to_string());
    }

    let abs_m = m.abs();
    let prefer_111_obtuse = abs_m.x != 0 && abs_m.x == abs_m.y && abs_m.y == abs_m.z;
    let tie_eps = 1e-6;

    let plane_props = unit_cell
        .ivec3_miller_index_to_plane_props(m)
        .map_err(|e| format!("Failed to compute plane properties: {}", e))?;
    let n = plane_props.normal;

    let x_world = DVec3::new(1.0, 0.0, 0.0);
    let y_world = DVec3::new(0.0, 1.0, 0.0);

    let x_proj = x_world - n * x_world.dot(n);
    let y_proj = y_world - n * y_world.dot(n);

    // Pick a stable preferred in-plane frame derived from projecting global X/Y onto the plane.
    // If one axis is parallel to the plane normal (projection becomes ~0), we still want a
    // well-defined in-plane perpendicular to avoid discrete basis flips.
    let ref_u = if x_proj.length_squared() > 1e-12 {
        x_proj.normalize()
    } else if y_proj.length_squared() > 1e-12 {
        y_proj.normalize()
    } else {
        x_world
    };

    // Preferred second direction: use projected Y if available; otherwise use a stable
    // in-plane perpendicular derived from the normal.
    let mut ref_v = if y_proj.length_squared() > 1e-12 {
        y_proj.normalize()
    } else {
        n.cross(ref_u)
    };
    if ref_v.length_squared() > 1e-12 {
        ref_v = ref_v.normalize();
    } else {
        ref_v = y_world;
    }

    // Canonical in-plane integer solutions to m·t=0 (same as compute_plane_axes)
    let t1 = reduce_to_primitive(IVec3::new(0, m.z, -m.y));
    let t2 = reduce_to_primitive(IVec3::new(-m.z, 0, m.x));
    let t3 = reduce_to_primitive(IVec3::new(m.y, -m.x, 0));

    let mut candidates = Vec::new();
    for v in [t1, t2, t3] {
        if v == IVec3::ZERO {
            continue;
        }
        candidates.push(v);
        candidates.push(-v);
    }

    let mut best_score = f64::NEG_INFINITY;
    let mut best_angle_score = f64::NEG_INFINITY;
    let mut best_pair: Option<(IVec3, IVec3)> = None;

    for &u in &candidates {
        let u_real = unit_cell.ivec3_lattice_to_real(&u);
        if u_real.length_squared() < 1e-12 {
            continue;
        }
        let u_dir = u_real.normalize();
        let u_score = u_dir.dot(ref_u);

        for &v in &candidates {
            if v == u {
                continue;
            }

            let cross = u.as_dvec3().cross(v.as_dvec3());
            if cross.length_squared() < 1e-12 {
                continue;
            }

            let v_real = unit_cell.ivec3_lattice_to_real(&v);
            if v_real.length_squared() < 1e-12 {
                continue;
            }

            // Match DrawingPlane::new convention: keep (u×v)·n > 0 by possibly flipping v.
            // We must apply this *before* scoring so that the score corresponds to the final
            // basis actually used by the drawing plane.
            let mut v_corrected = v;
            let mut v_real_corrected = v_real;
            if u_real.cross(v_real).dot(n) < 0.0 {
                v_corrected = -v_corrected;
                v_real_corrected = -v_real_corrected;
            }

            // Prefer v aligned with projected global Y after removing the u component.
            let v_ref_ortho = ref_v - u_dir * ref_v.dot(u_dir);
            if v_ref_ortho.length_squared() < 1e-12 {
                continue;
            }
            let v_ref_dir = v_ref_ortho.normalize();
            let v_dir = v_real_corrected.normalize();
            let v_score = v_dir.dot(v_ref_dir);

            let score = u_score + v_score;

            let angle_score = if prefer_111_obtuse {
                let uv_cos = u_dir.dot(v_dir);
                -(uv_cos + 0.5).abs()
            } else {
                0.0
            };

            if score > best_score + tie_eps
                || (prefer_111_obtuse
                    && (score - best_score).abs() <= tie_eps
                    && angle_score > best_angle_score + tie_eps)
            {
                best_score = score;
                best_angle_score = angle_score;
                best_pair = Some((u, v_corrected));
            }
        }
    }

    best_pair.ok_or_else(|| {
        format!(
            "Could not find preferred in-plane vectors for Miller index ({}, {}, {})",
            m.x, m.y, m.z
        )
    })
}

/// Reduces a lattice vector to primitive form by dividing by GCD of components.
///
/// # Arguments
/// * `v` - Input lattice vector
///
/// # Returns
/// * Primitive vector with GCD = 1, or zero vector if input is zero
pub fn reduce_to_primitive(v: IVec3) -> IVec3 {
    if v == IVec3::ZERO {
        return v;
    }

    let g = gcd3(v.x.abs(), v.y.abs(), v.z.abs());
    IVec3::new(v.x / g, v.y / g, v.z / g)
}

/// Computes GCD of three integers
pub fn gcd3(a: i32, b: i32, c: i32) -> i32 {
    gcd(gcd(a, b), c)
}

/// Computes GCD of two integers using Euclidean algorithm
pub fn gcd(mut a: i32, mut b: i32) -> i32 {
    while b != 0 {
        let temp = b;
        b = a % b;
        a = temp;
    }
    a
}

/// Solves for scalars (u, v) such that p = u*a + v*b.
///
/// This function uses the Gram matrix formula to handle non-orthogonal basis vectors correctly.
/// It solves the 2x2 linear system:
/// ```text
/// [aa  ab] [u] = [a·p]
/// [ab  bb] [v]   [b·p]
/// ```
/// where aa = a·a, ab = a·b, bb = b·b.
///
/// The solution is: u = (bb*ap - ab*bp)/det, v = (aa*bp - ab*ap)/det
///
/// # Arguments
/// * `a` - First basis vector
/// * `b` - Second basis vector  
/// * `p` - Point to express in the basis {a, b}
///
/// # Returns
/// * `Some((u, v))` - Coefficients such that p = u*a + v*b
/// * `None` - If a and b are nearly linearly dependent (det ≈ 0)
pub fn coords_in_plane(a: DVec3, b: DVec3, p: DVec3) -> Option<(f64, f64)> {
    let aa = a.dot(a);
    let bb = b.dot(b);
    let ab = a.dot(b);

    let ap = a.dot(p);
    let bp = b.dot(p);

    let det = aa * bb - ab * ab;
    if det.abs() <= 1e-12 {
        return None; // Basis vectors are linearly dependent
    }

    let u = (bb * ap - ab * bp) / det;
    let v = (aa * bp - ab * ap) / det;

    Some((u, v))
}
