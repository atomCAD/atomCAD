use glam::i32::IVec3;
use glam::f32::Vec3;

#[derive(Debug)]
pub struct HalfSpaceGadgetState {
    pub int_normal: IVec3,
    pub normal: Vec3,
    pub int_shift: i32,
    pub shift: f32,
}
