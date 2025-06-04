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