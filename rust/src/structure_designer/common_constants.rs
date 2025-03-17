// TODO: these will not be constant, will be set by the user
use glam::i32::IVec3;

pub const IMPLICIT_VOLUME_MIN: IVec3 = IVec3::new(-10, -10, -10);
pub const IMPLICIT_VOLUME_MAX: IVec3 = IVec3::new(10, 10, 10);

pub const DIAMOND_UNIT_CELL_SIZE_ANGSTROM: f32 = 3.567;  // Size of one complete unit cell in Ångströms
