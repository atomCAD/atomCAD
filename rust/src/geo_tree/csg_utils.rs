use csgrs::float_types::Real;
use glam::DVec3;
use nalgebra::{Point3, Vector3};

/// Scaling factor for CSG operations to handle large geometry.
/// Currently set to 1.0 (no scaling), but can be adjusted as needed.
const CSG_SCALING: f64 = 1.0;

/// Scale coordinate values for CSG operations to handle large geometry.
pub fn scale_to_csg(coord: f64) -> f64 {
    coord * CSG_SCALING
    //coord
}

/// Unscale coordinate values from CSG operations back to original scale.
pub fn unscale_from_csg(coord: f64) -> f64 {
    coord / CSG_SCALING
    //coord
}

pub fn dvec3_to_point3(dvec3: DVec3) -> Point3<Real> {
    Point3::new(
        scale_to_csg(dvec3.x) as Real,
        scale_to_csg(dvec3.y) as Real,
        scale_to_csg(dvec3.z) as Real,
    )
}

pub fn dvec3_to_vector3(dvec3: DVec3) -> Vector3<Real> {
    Vector3::new(
        scale_to_csg(dvec3.x) as Real,
        scale_to_csg(dvec3.y) as Real,
        scale_to_csg(dvec3.z) as Real,
    )
}
