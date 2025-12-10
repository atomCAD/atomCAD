use glam::i32::IVec3;
use glam::f64::DVec3;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;

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
    /// Maps 2D lattice coordinates to 2D real-space coordinates in the plane's coordinate system.
    /// - `a` basis = real-space vector corresponding to u_axis
    /// - `b` basis = real-space vector corresponding to v_axis  
    /// - `c` basis = perpendicular vector (for potential extrusion)
    pub effective_unit_cell: UnitCellStruct,
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
        // Compute in-plane axes from Miller index
        let (u_axis, mut v_axis) = compute_plane_axes(&miller_index)?;
        
        // Ensure right-handed coordinate system: (u × v) · n > 0
        let normal_dir = unit_cell.ivec3_miller_index_to_plane_props(&miller_index)
            .map_err(|e| format!("Failed to compute plane properties: {}", e))?
            .normal;
        
        let cross = (u_axis.as_dvec3()).cross(v_axis.as_dvec3()).normalize();
        if cross.dot(normal_dir) < 0.0 {
            // Flip v-axis to make right-handed
            v_axis = -v_axis;
        }
        
        // Compute effective unit cell for 2D operations
        // Convert u_axis and v_axis from lattice coordinates to real-space vectors
        let a_real = unit_cell.ivec3_lattice_to_real(&u_axis);
        let b_real = unit_cell.ivec3_lattice_to_real(&v_axis);
        
        // c_real is perpendicular to the plane (for potential extrusion)
        // Use the actual normal vector from plane properties, scaled by d-spacing
        let plane_props = unit_cell.ivec3_miller_index_to_plane_props(&miller_index)
            .map_err(|e| format!("Failed to compute plane properties: {}", e))?;
        let c_real = normal_dir * plane_props.d_spacing;
        
        // Create effective unit cell from these real-space basis vectors
        let effective_unit_cell = UnitCellStruct::new(a_real, b_real, c_real);
        
        Ok(Self {
            unit_cell,
            miller_index,
            center,
            shift,
            subdivision: subdivision.max(1), // Ensure minimum value of 1
            u_axis,
            v_axis,
            effective_unit_cell,
        })
    }
    
    /// Checks if two drawing planes are compatible for boolean operations.
    /// 
    /// Planes are compatible if they have the same unit cell, orientation,
    /// position, and shift parameters.
    pub fn is_compatible(&self, other: &DrawingPlane) -> bool {
        self.unit_cell.is_approximately_equal(&other.unit_cell) &&
        self.miller_index == other.miller_index &&
        self.center == other.center &&
        self.shift == other.shift &&
        self.subdivision == other.subdivision
        // u_axis and v_axis should be deterministically same if above match
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
        if candidates[i] == IVec3::ZERO { continue; }
        for j in (i+1)..3 {
            if candidates[j] == IVec3::ZERO { continue; }
            
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

/// Reduces a lattice vector to primitive form by dividing by GCD of components.
/// 
/// # Arguments
/// * `v` - Input lattice vector
/// 
/// # Returns
/// * Primitive vector with GCD = 1, or zero vector if input is zero
pub fn reduce_to_primitive(v: IVec3) -> IVec3 {
    if v == IVec3::ZERO { return v; }
    
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
