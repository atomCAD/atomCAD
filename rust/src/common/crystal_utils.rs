use glam::IVec3;
use lazy_static::lazy_static;

use crate::util::imat3::IMat3;

#[derive(Debug, PartialEq, Clone)]
pub enum ZincBlendeAtomType {
    Primary,
    Secondary,
}

// Define the 6 possible basis vectors outside the function
lazy_static! {
    static ref BASIS_VECTORS: [IVec3; 6] = [
        IVec3::new(1, 0, 0),  // +x
        IVec3::new(-1, 0, 0), // -x
        IVec3::new(0, 1, 0),  // +y
        IVec3::new(0, -1, 0), // -y
        IVec3::new(0, 0, 1),  // +z
        IVec3::new(0, 0, -1), // -z
    ];

    pub static ref CRYSTAL_ROTATION_MATRICES: [IMat3; 12] = [
        // 1. Identity
        IMat3::new(&IVec3::new(1, 0, 0), &IVec3::new(0, 1, 0), &IVec3::new(0, 0, 1)),
        // 2. 180° around [1 0 0]
        IMat3::new(&IVec3::new(1, 0, 0), &IVec3::new(0, -1, 0), &IVec3::new(0, 0, -1)),
        // 3. 180° around [0 1 0]
        IMat3::new(&IVec3::new(-1, 0, 0), &IVec3::new(0, 1, 0), &IVec3::new(0, 0, -1)),
        // 4. 180° around [0 0 1]
        IMat3::new(&IVec3::new(-1, 0, 0), &IVec3::new(0, -1, 0), &IVec3::new(0, 0, 1)),
        // 5. +120° around [1 1 1]
        IMat3::new(&IVec3::new(0, 1, 0), &IVec3::new(0, 0, 1), &IVec3::new(1, 0, 0)),
        // 6. –120° around [1 1 1]
        IMat3::new(&IVec3::new(0, 0, 1), &IVec3::new(1, 0, 0), &IVec3::new(0, 1, 0)),
        // 7. +120° around [-1 1 1]
        IMat3::new(&IVec3::new(0, 0, -1), &IVec3::new(-1, 0, 0), &IVec3::new(0, 1, 0)),
        // 8. –120° around [-1 1 1]
        IMat3::new(&IVec3::new(0, -1, 0), &IVec3::new(0, 0, 1), &IVec3::new(-1, 0, 0)),
        // 9. +120° around [1 -1 1]
        IMat3::new(&IVec3::new(0, 0, 1), &IVec3::new(-1, 0, 0), &IVec3::new(0, -1, 0)),
        // 10. –120° around [1 -1 1]
        IMat3::new(&IVec3::new(0, -1, 0), &IVec3::new(0, 0, -1), &IVec3::new(1, 0, 0)),
        // 11. +120° around [1 1 -1]
        IMat3::new(&IVec3::new(0, 0, -1), &IVec3::new(1, 0, 0), &IVec3::new(0, -1, 0)),
        // 12. –120° around [1 1 -1]
        IMat3::new(&IVec3::new(0, 1, 0), &IVec3::new(0, 0, -1), &IVec3::new(-1, 0, 0)),
    ];
}

/// Converts a crystal lattice position to a unique 64-bit ID.
/// Format:
/// - lower 16 bits: signed X position
/// - next 16 bits: signed Y position
/// - next 16 bits: signed Z position
/// - most significant bit (bit 63) set to 1, signifying that this is special id with this format
pub fn in_crystal_pos_to_id(pos: &IVec3) -> u64 {
    // Extract the components
    let x = pos.x as i16;
    let y = pos.y as i16;
    let z = pos.z as i16;
    
    // Convert to unsigned for bit manipulation
    let x_bits = (x as u16) as u64;
    let y_bits = (y as u16) as u64;
    let z_bits = (z as u16) as u64;
    
    // Construct the ID
    let mut id: u64 = 0;
    id |= x_bits;                  // Bits 0-15: X position
    id |= y_bits << 16;          // Bits 16-31: Y position
    id |= z_bits << 32;          // Bits 32-47: Z position
    id |= 1u64 << 63;            // Bit 63: Set to 1 to indicate crystal atom ID
    
    id
}

/// Checks if the given ID represents a crystal atom (has bit 63 set).
pub fn is_crystal_atom_id(id: u64) -> bool {
    (id & (1u64 << 63)) != 0
}

/// Extracts the crystal lattice position from a crystal atom ID.
/// WARNING: This function assumes the ID is a valid crystal atom ID.
/// Use is_crystal_atom_id() to check before calling this function.
pub fn id_to_in_crystal_pos(id: u64) -> IVec3 {
    // Extract each 16-bit component and convert to signed integers
    let x = ((id & 0xFFFF) as u16) as i16;
    let y = (((id >> 16) & 0xFFFF) as u16) as i16;
    let z = (((id >> 32) & 0xFFFF) as u16) as i16;
    
    IVec3::new(x as i32, y as i32, z as i32)
}

/// Returns an integer rotation matrix for cubic diamond crystal symmetry operations.
/// 
/// # Arguments
/// 
/// * `x_dir` - Direction of the new x-axis (0-5)
///   * 0: +x (1, 0, 0)
///   * 1: -x (-1, 0, 0)
///   * 2: +y (0, 1, 0)
///   * 3: -y (0, -1, 0)
///   * 4: +z (0, 0, 1)
///   * 5: -z (0, 0, -1)
/// * `y_dir` - Direction of the new y-axis (0-3), interpreted based on x_dir
/// 
/// # Returns
/// 
/// An IMat3 representing the rotation matrix
pub fn crystal_rot_to_mat(x_dir: i32, y_dir: i32) -> IMat3 {
    // Get the first basis vector (new x-axis)
    let x_basis = BASIS_VECTORS[(x_dir as usize) % 6];
    
    // Calculate the possible y-axis directions
    // First, find the 4 directions that are perpendicular to x_basis
    let mut perpendicular_dirs = Vec::with_capacity(4);
    for i in 0..6 {
        let dir = BASIS_VECTORS[i];
        // Check if perpendicular (dot product is 0)
        if x_basis.dot(dir) == 0 {
            perpendicular_dirs.push(i);
        }
    }
    
    // Get the selected perpendicular direction (new y-axis)
    let y_dir_idx = (y_dir as usize) % 4;
    let y_basis_idx = perpendicular_dirs[y_dir_idx];
    let y_basis = BASIS_VECTORS[y_basis_idx];
    
    // Calculate the third basis vector (new z-axis) using cross product
    // For integer vectors, we can compute the cross product directly
    let z_basis = IVec3::new(
        x_basis.y * y_basis.z - x_basis.z * y_basis.y,
        x_basis.z * y_basis.x - x_basis.x * y_basis.z,
        x_basis.x * y_basis.y - x_basis.y * y_basis.x
    );
    
    // Create and return the rotation matrix
    println!("DEBUG - Crystal Rotation Matrix: x_dir={}, y_dir={}, x_basis={:?}, y_basis={:?}, z_basis={:?}", x_dir, y_dir, x_basis, y_basis, z_basis);
    IMat3::new(&x_basis, &y_basis, &z_basis)
}

pub fn get_zinc_blende_atom_type_for_pos(pos: &IVec3) -> ZincBlendeAtomType {
    if pos.x % 2 == 0 {
        ZincBlendeAtomType::Primary
    } else {
        ZincBlendeAtomType::Secondary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_crystal_id_roundtrip() {
        let test_positions = [
            IVec3::new(0, 0, 0),
            IVec3::new(1, 2, 3),
            IVec3::new(-1, -2, -3),
            IVec3::new(32767, 32767, 32767),      // Max positive values for i16
            IVec3::new(-32768, -32768, -32768),   // Min negative values for i16
        ];
        
        for pos in test_positions.iter() {
            let id = in_crystal_pos_to_id(pos);
            assert!(is_crystal_atom_id(id), "ID should be marked as crystal atom ID");
            
            let decoded_pos = id_to_in_crystal_pos(id);
            assert_eq!(*pos, decoded_pos, "Position should round-trip correctly");
        }
    }
}