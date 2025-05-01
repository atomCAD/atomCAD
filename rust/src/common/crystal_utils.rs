use glam::IVec3;

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