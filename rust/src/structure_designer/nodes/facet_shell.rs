use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::util::timer::Timer;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::common::serialization_utils::ivec3_serializer;
use glam::f32::Vec3;
use glam::f64::DQuat;
use glam::f64::DVec3;
use crate::renderer::mesh::Mesh;
use crate::renderer::mesh::Material;
use crate::renderer::tessellator::tessellator;
use crate::renderer::tessellator::tessellator::Tessellatable;
use crate::util::hit_test_utils::sphere_hit_test;
use crate::util::hit_test_utils::cylinder_hit_test;
use crate::structure_designer::common_constants;
use std::collections::HashSet;
use crate::common::gadget::Gadget;
use crate::structure_designer::evaluator::network_evaluator::NetworkResult;
use crate::structure_designer::evaluator::network_evaluator::GeometrySummary;
use crate::structure_designer::evaluator::implicit_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::util::transform::Transform;
use crate::structure_designer::evaluator::implicit_evaluator::ImplicitEvaluator;
use crate::structure_designer::node_network::Node;
use crate::common::csg_types::CSG;
use csgrs::polygon::Polygon;
use csgrs::vertex::Vertex;
use crate::common::csg_utils::dvec3_to_point3;
use crate::common::csg_utils::dvec3_to_vector3;
use crate::structure_designer::utils::half_space_utils::create_plane;
use crate::structure_designer::utils::half_space_utils::implicit_eval_half_space_calc;

#[derive(Debug, Serialize, Deserialize)]
pub struct Facet {
  #[serde(with = "ivec3_serializer")]
  pub miller_index: IVec3,
  pub shift: i32,
  pub symmetrize: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FacetShellData {
  pub max_miller_index: i32,
  #[serde(with = "ivec3_serializer")]
  pub center: IVec3,
  pub facets: Vec<Facet>,
  pub selected_facet_index: Option<usize>,
}

impl NodeData for FacetShellData {
    fn provide_gadget(&self) -> Option<Box<dyn NodeNetworkGadget>> {
      return None;
    }
}

pub fn eval_facet_shell<'a>(
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  _registry: &NodeTypeRegistry,
  context: &mut NetworkEvaluationContext
) -> NetworkResult {
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let facet_shell_data = &node.data.as_any_ref().downcast_ref::<FacetShellData>().unwrap();

  // Only create geometry if explicit geometry evaluation is needed
  // Otherwise, return an empty CSG object
  let mut geometry = if context.explicit_geo_eval_needed {
    // If we have no facets, return an empty CSG object
    if facet_shell_data.facets.is_empty() {
      CSG::new()
    } else {
      // Initialize with the first facet's half-space
      let first_facet = &facet_shell_data.facets[0];
      create_plane(&first_facet.miller_index, &facet_shell_data.center, first_facet.shift)
    }
  } else {
    CSG::new()
  };

  // If we have facets and need explicit geometry evaluation,
  // intersect with the remaining facets' half-spaces
  if context.explicit_geo_eval_needed && facet_shell_data.facets.len() > 1 {
    for facet in &facet_shell_data.facets[1..] {
      let half_space = create_plane(&facet.miller_index, &facet_shell_data.center, facet.shift);
      geometry = geometry.intersection(&half_space);
    }
  }
  
  // Calculate transform for the result
  // Use center position for translation
  let center_pos = facet_shell_data.center.as_dvec3();
  
  return NetworkResult::Geometry(GeometrySummary {
    frame_transform: Transform::new(
      center_pos,
      DQuat::IDENTITY, // Use identity quaternion as we don't need rotation
    ),
    csg: geometry
  });
}

pub fn implicit_eval_facet_shell<'a>(
  _evaluator: &ImplicitEvaluator,
  _registry: &NodeTypeRegistry,
  _network_stack: &Vec<NetworkStackElement<'a>>,
  node: &Node,
  sample_point: &DVec3) -> f64 {
  let facet_shell_data = &node.data.as_any_ref().downcast_ref::<FacetShellData>().unwrap();
  
  // If there are no facets, return MAX value (infinitely far outside)
  if facet_shell_data.facets.is_empty() {
    return f64::MAX;
  }
  
  // Calculate the signed distance for each facet and return the maximum
  // Using max for intersection in implicit geometry
  facet_shell_data.facets.iter()
    .map(|facet| {
      implicit_eval_half_space_calc(
        &facet.miller_index,
        &facet_shell_data.center,
        facet.shift,
        sample_point
      )
    })
    .fold(f64::MIN, f64::max)
}
