use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use glam::f64::DVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::dvec3_serializer;
use crate::renderer::mesh::Mesh;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::common::gadget::Gadget;
use glam::f64::DQuat;
use glam::f32::Vec3;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::common::atomic_structure::AtomicStructure;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::utils::xyz_gadget_utils;

#[derive(Debug, Serialize, Deserialize)]
pub struct AtomTransData {
  #[serde(with = "dvec3_serializer")]
  pub translation: DVec3,
  #[serde(with = "dvec3_serializer")]
  pub rotation: DVec3, // intrinsic euler angles in radians
}

impl NodeData for AtomTransData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      return Some(Box::new(AtomTransGadget::new(self.translation, self.rotation)));
    }
}

pub fn eval_atom_trans<'a>(network_evaluator: &NetworkEvaluator, network_stack: &Vec<NetworkStackElement<'a>>, node_id: u64, registry: &NodeTypeRegistry, context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {  
  let node = NetworkStackElement::get_top_node(network_stack, node_id);

  if node.arguments[0].is_empty() {
    return NetworkResult::Atomic(AtomicStructure::new());
  }
  let input_molecule_node_id = node.arguments[0].get_node_id().unwrap();

  let result = &network_evaluator.evaluate(network_stack, input_molecule_node_id, registry, false, context)[0];
  if let NetworkResult::Atomic(atomic_structure) = result {
    let atom_trans_data = &node.data.as_any_ref().downcast_ref::<AtomTransData>().unwrap();

    let rotation_quat = DQuat::from_euler(
      glam::EulerRot::XYZ,
      atom_trans_data.rotation.x, 
      atom_trans_data.rotation.y, 
      atom_trans_data.rotation.z);

    let mut result_atomic_structure = atomic_structure.clone();
    result_atomic_structure.transform(&rotation_quat, &atom_trans_data.translation);

    return NetworkResult::Atomic(result_atomic_structure);
  }
  return NetworkResult::None;
}

#[derive(Clone)]
pub struct AtomTransGadget {
    pub translation: DVec3,
    pub rotation: DVec3, // intrinsic euler angles in radians
    pub rotation_quat: DQuat,
}

impl Tessellatable for AtomTransGadget {
    fn tessellate(&self, output_mesh: &mut Mesh) {
        xyz_gadget_utils::tessellate_xyz_gadget(output_mesh, self.rotation_quat, &self.translation);
    }

    fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
        Box::new(self.clone())
    }
}

impl Gadget for AtomTransGadget {
    fn hit_test(&self, _ray_origin: DVec3, _ray_direction: DVec3) -> Option<i32> {
        None
    }

    fn start_drag(&mut self, _handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {

    }

    fn drag(&mut self, _handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {

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

    fn refresh_rotation_quat(&mut self) {
        self.rotation_quat = DQuat::from_euler(
            glam::EulerRot::XYZ,
            self.rotation.x, 
            self.rotation.y, 
            self.rotation.z);
    }
}

