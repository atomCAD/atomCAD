use csgrs::float_types::Real;
use glam::{DMat2, DMat3, DVec2, DVec3};
use nalgebra::{Matrix4, Point3, Vector3};

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

/// Build the 4×4 affine matrix for the point inversion `x ↦ 2·center − x`.
///
/// The linear part (−I) is dimensionless and must NOT be scaled; the
/// translation `2·center` carries length units and gets `scale_to_csg` once —
/// unlike [`dmat3_affine_to_csg_matrix4`], whose basis columns also carry
/// units. The transformed mesh is already in CSG-scaled coordinates.
pub fn point_inversion_csg_matrix4(center: DVec3) -> Matrix4<Real> {
    Matrix4::new(
        -1.0,
        0.0,
        0.0,
        scale_to_csg(2.0 * center.x) as Real,
        0.0,
        -1.0,
        0.0,
        scale_to_csg(2.0 * center.y) as Real,
        0.0,
        0.0,
        -1.0,
        scale_to_csg(2.0 * center.z) as Real,
        0.0,
        0.0,
        0.0,
        1.0,
    )
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

/// Build the 4×4 affine matrix `x ↦ basis·x + translation` for csgrs.
///
/// `basis` columns and `translation` both carry length units, so `scale_to_csg`
/// is applied to each exactly once here (the source primitive must be built at
/// plain unit size, e.g. `CSGMesh::sphere(1.0, ..)`). Matrix layout is
/// column-major in both glam and nalgebra; `Matrix4::new` takes row-major args.
pub fn dmat3_affine_to_csg_matrix4(basis: DMat3, translation: DVec3) -> Matrix4<Real> {
    let c0 = basis.x_axis; // maps unit-ball +X
    let c1 = basis.y_axis; // maps unit-ball +Y
    let c2 = basis.z_axis; // maps unit-ball +Z
    Matrix4::new(
        scale_to_csg(c0.x) as Real,
        scale_to_csg(c1.x) as Real,
        scale_to_csg(c2.x) as Real,
        scale_to_csg(translation.x) as Real,
        scale_to_csg(c0.y) as Real,
        scale_to_csg(c1.y) as Real,
        scale_to_csg(c2.y) as Real,
        scale_to_csg(translation.y) as Real,
        scale_to_csg(c0.z) as Real,
        scale_to_csg(c1.z) as Real,
        scale_to_csg(c2.z) as Real,
        scale_to_csg(translation.z) as Real,
        0.0,
        0.0,
        0.0,
        1.0,
    )
}

/// Build the 4×4 affine matrix for the 2D map `y ↦ basis·y + translation`,
/// embedded in the XY plane with a z-passthrough. `Sketch::transform` reads only
/// the top-left 2×2 sub-block plus the translation column (m14, m24), so the z
/// row/column here are the identity and never affect the result.
///
/// As in [`dmat3_affine_to_csg_matrix4`], `basis` columns and `translation` carry
/// length units, so `scale_to_csg` is applied to each exactly once (the source
/// sketch must be built at plain unit size, e.g. `CSGSketch::circle(1.0, ..)`).
pub fn dmat2_affine_to_csg_matrix4(basis: DMat2, translation: DVec2) -> Matrix4<Real> {
    let c0 = basis.x_axis; // maps unit-disk +X
    let c1 = basis.y_axis; // maps unit-disk +Y
    Matrix4::new(
        scale_to_csg(c0.x) as Real,
        scale_to_csg(c1.x) as Real,
        0.0,
        scale_to_csg(translation.x) as Real,
        scale_to_csg(c0.y) as Real,
        scale_to_csg(c1.y) as Real,
        0.0,
        scale_to_csg(translation.y) as Real,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    )
}
