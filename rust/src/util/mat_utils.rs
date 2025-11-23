use glam::i32::IVec3;

/// A consistent rounding function that adds a small bias to ensure symmetrical rounding.
/// This helps avoid asymmetry in geometric calculations due to floating-point precision issues.
/// 
/// The function adds a small positive bias (0.001) to positive numbers and subtracts the same bias
/// from negative numbers before rounding. This ensures that values very close to x.5 (like 0.499...)
/// are consistently rounded in the same direction regardless of tiny floating-point errors.
///
/// # Arguments
/// * `value` - The floating-point value to round
///
/// # Returns
/// The rounded value as an integer
pub fn consistent_round(value: f64) -> i32 {
    const BIAS: f64 = 0.001;
    
    if value >= 0.0 {
        (value + BIAS).round() as i32
    } else {
        (value - BIAS).round() as i32
    }
}

/// Version of consistent_round for DVec2
/// 
/// # Arguments
/// * `vec` - The 2D vector to round consistently
///
/// # Returns
/// An integer vector with consistently rounded components
pub fn consistent_round_dvec2(vec: &glam::f64::DVec2) -> glam::i32::IVec2 {
    glam::i32::IVec2::new(
        consistent_round(vec.x),
        consistent_round(vec.y)
    )
}

/// Returns the n-th standard unit vector as an IVec3.
/// 
/// This function returns the standard basis vectors:
/// - n = 0: (1, 0, 0) - X-axis unit vector
/// - n = 1: (0, 1, 0) - Y-axis unit vector  
/// - n = 2: (0, 0, 1) - Z-axis unit vector
///
/// # Arguments
/// * `n` - The index of the unit vector (0, 1, or 2)
///
/// # Returns
/// The n-th unit vector as an IVec3
///
/// # Panics
/// Panics if n is not 0, 1, or 2
pub fn unit_ivec3(n: i32) -> IVec3 {
    match n {
        0 => glam::i32::IVec3::new(1, 0, 0), // X-axis
        1 => glam::i32::IVec3::new(0, 1, 0), // Y-axis
        2 => glam::i32::IVec3::new(0, 0, 1), // Z-axis
        _ => panic!("Invalid unit vector index: {}. Must be 0, 1, or 2", n),
    }
}















