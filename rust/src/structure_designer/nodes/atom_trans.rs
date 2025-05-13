use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::dvec3_serializer;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::gadget::Gadget;
use glam::f64::DQuat;
use glam::f32::Vec3;

pub const GADGET_LENGTH: f64 = 6.0;
pub const AXIS_RADIUS: f64 = 0.1;
pub const AXIS_DIVISIONS: u32 = 16;

#[derive(Debug, Serialize, Deserialize)]
pub struct AtomTransData {
  #[serde(with = "dvec3_serializer")]
  pub translation: DVec3,
  #[serde(with = "dvec3_serializer")]
  pub rotation: DVec3, // intrinsic euler angles in radians
}

impl NodeData for AtomTransData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      return Some(Box::new(AtomTransGadget::new(self.translation, self.rotation)));
    }
}

#[derive(Clone)]
pub struct AtomTransGadget {
    pub translation: DVec3,
    pub rotation: DVec3, // intrinsic euler angles in radians
    pub rotation_quat: DQuat,
}

impl Tessellatable for AtomTransGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {        
        let x_axis_dir = self.rotation_quat.mul_vec3(DVec3::new(1.0, 0.0, 0.0));
        let y_axis_dir = self.rotation_quat.mul_vec3(DVec3::new(0.0, 1.0, 0.0));
        let z_axis_dir = self.rotation_quat.mul_vec3(DVec3::new(0.0, 0.0, 1.0));

        self.tessellate_axis_arrow(output_mesh, &x_axis_dir, &Vec3::new(1.0, 0.0, 0.0));
        self.tessellate_axis_arrow(output_mesh, &y_axis_dir, &Vec3::new(0.0, 1.0, 0.0));
        self.tessellate_axis_arrow(output_mesh, &z_axis_dir, &Vec3::new(0.0, 0.0, 1.0));
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for AtomTransGadget {
    fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
        None
    }

    fn start_drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {

    }

    fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {

    }

    fn end_drag(&mut self) {

    }
}

impl NodeNetworkGadget for AtomTransGadget {
    fn sync_data(&self, data: &mut dyn NodeData) {
        if let Some(atom_trans_data) = data.as_any_mut().downcast_mut::<AtomTransData>() {
            atom_trans_data.translation = self.translation;
            atom_trans_data.rotation = self.rotation;
        }
    }

    fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
        Box::new(self.clone())
    }
}

impl AtomTransGadget {
    pub fn new(translation: DVec3, rotation: DVec3) -> Self {
        let mut ret = Self {
            translation,
            rotation,
            rotation_quat: DQuat::IDENTITY.clone(),
        };
        ret.refresh_rotation_quat();
        return ret;
    }

    fn tessellate_axis_arrow(&self, output_mesh: &mut Mesh, axis_dir: &DVec3, albedo: &Vec3) {
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
        self.rotation_quat = DQuat::from_euler(
            glam::EulerRot::XYX,
            self.rotation.x, 
            self.rotation.y, 
            self.rotation.z);
    }
}

