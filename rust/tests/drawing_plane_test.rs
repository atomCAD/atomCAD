use rust_lib_flutter_cad::crystolecule::drawing_plane::{
    compute_plane_axes, reduce_to_primitive, gcd, gcd3
};
use glam::i32::IVec3;

#[test]
fn test_compute_plane_axes_001() {
    // (001) plane - normal along Z
    let m = IVec3::new(0, 0, 1);
    let (u, v) = compute_plane_axes(&m).unwrap();
    
    // Axes should be in XY plane
    assert_eq!(u.z, 0);
    assert_eq!(v.z, 0);
    
    // Should be perpendicular to normal
    assert_eq!(u.dot(m), 0);
    assert_eq!(v.dot(m), 0);
    
    // Should be non-zero and non-collinear
    assert_ne!(u, IVec3::ZERO);
    assert_ne!(v, IVec3::ZERO);
    let cross = u.as_dvec3().cross(v.as_dvec3());
    assert!(cross.length() > 0.1);
}

#[test]
fn test_compute_plane_axes_100() {
    // (100) plane - normal along X
    let m = IVec3::new(1, 0, 0);
    let (u, v) = compute_plane_axes(&m).unwrap();
    
    // Axes should be in YZ plane
    assert_eq!(u.x, 0);
    assert_eq!(v.x, 0);
    
    // Should be perpendicular to normal
    assert_eq!(u.dot(m), 0);
    assert_eq!(v.dot(m), 0);
}

#[test]
fn test_compute_plane_axes_111() {
    // (111) plane - diagonal
    let m = IVec3::new(1, 1, 1);
    let (u, v) = compute_plane_axes(&m).unwrap();
    
    // Should be perpendicular to normal
    assert_eq!(u.dot(m), 0);
    assert_eq!(v.dot(m), 0);
    
    // Should be primitive (GCD = 1)
    assert_eq!(gcd3(u.x.abs(), u.y.abs(), u.z.abs()), 1);
    assert_eq!(gcd3(v.x.abs(), v.y.abs(), v.z.abs()), 1);
}

#[test]
fn test_reduce_to_primitive() {
    assert_eq!(reduce_to_primitive(IVec3::new(2, 4, 6)), IVec3::new(1, 2, 3));
    assert_eq!(reduce_to_primitive(IVec3::new(0, 3, 6)), IVec3::new(0, 1, 2));
    assert_eq!(reduce_to_primitive(IVec3::new(5, 10, 15)), IVec3::new(1, 2, 3));
    assert_eq!(reduce_to_primitive(IVec3::ZERO), IVec3::ZERO);
}

#[test]
fn test_gcd() {
    assert_eq!(gcd(12, 8), 4);
    assert_eq!(gcd(17, 5), 1);
    assert_eq!(gcd(100, 50), 50);
}
