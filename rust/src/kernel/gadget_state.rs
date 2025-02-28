use glam::i32::IVec3;
use glam::f32::Vec3;

#[derive(Clone)]
pub struct HalfSpaceGadgetState {
    pub int_dir: IVec3,
    pub dir: Vec3,
    pub miller_index: IVec3,
    pub int_shift: i32,
    pub shift: f32,
}

#[derive(Clone)]
pub enum GadgetState {
    Empty,
    HalfSpace(HalfSpaceGadgetState),
}
