use super::gadget::Gadget;
use crate::renderer::mesh::Mesh;
use crate::structure_editor::node_data::atom_trans_data::AtomTransData;
use crate::structure_editor::node_data::node_data::NodeData;
use crate::renderer::tessellator::tessellator;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator::Tessellatable;
use glam::f32::Vec3;
use glam::f32::Quat;

pub const GADGET_LENGTH: f32 = 6.0;
pub const AXIS_RADIUS: f32 = 0.1;
pub const AXIS_DIVISIONS: u32 = 16;

#[derive(Clone)]
pub struct AtomTransGadget {
    pub translation: Vec3,
    pub rotation: Vec3, // intrinsic euler angles in radians
    pub rotation_quat: Quat,
}

impl Tessellatable for AtomTransGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {        
        let x_axis_dir = self.rotation_quat.mul_vec3(Vec3::new(1.0, 0.0, 0.0));
        let y_axis_dir = self.rotation_quat.mul_vec3(Vec3::new(0.0, 1.0, 0.0));
        let z_axis_dir = self.rotation_quat.mul_vec3(Vec3::new(0.0, 0.0, 1.0));

        self.tessellate_axis_arrow(output_mesh, &x_axis_dir, &Vec3::new(1.0, 0.0, 0.0));
        self.tessellate_axis_arrow(output_mesh, &y_axis_dir, &Vec3::new(0.0, 1.0, 0.0));
        self.tessellate_axis_arrow(output_mesh, &z_axis_dir, &Vec3::new(0.0, 0.0, 1.0));
    }
}

impl Gadget for AtomTransGadget {
    fn hit_test(&self, ray_origin: Vec3, ray_direction: Vec3) -> Option<i32> {
        None
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: Vec3, ray_direction: Vec3) {

    }

    fn drag(&mut self, handle_index: i32, ray_origin: Vec3, ray_direction: Vec3) {

    }

    fn end_drag(&mut self) {

    }

    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(atom_trans_data) = data.as_any_mut().downcast_mut::<AtomTransData>() {
            atom_trans_data.translation = self.translation;
            atom_trans_data.rotation = self.rotation;
        }
    }

    fn clone_box(&self) -> Box<dyn Gadget> {
        Box::new(self.clone())
    }
    
    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl AtomTransGadget {
    pub fn new(translation: Vec3, rotation: Vec3) -> Self {
        let mut ret = Self {
            translation,
            rotation,
            rotation_quat: Quat::IDENTITY.clone(),
        };
        ret.refresh_rotation_quat();
        return ret;
    }

    fn tessellate_axis_arrow(&self, output_mesh: &mut Mesh, axis_dir: &Vec3, albedo: &Vec3) {
        tessellator::tessellate_cylinder(
            output_mesh,
            &(self.translation + axis_dir * GADGET_LENGTH),
            &self.translation,
            AXIS_RADIUS,
            AXIS_DIVISIONS,
            &Material::new(albedo, 0.4, 0.8), 
            true);        
    }

    fn refresh_rotation_quat(&mut self) {
        self.rotation_quat = Quat::from_euler(
            glam::EulerRot::XYX,
            self.rotation.x, 
            self.rotation.y, 
            self.rotation.z);
    }
}
