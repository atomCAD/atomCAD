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
#[serde(default)]
pub struct FacetShellData {
  pub max_miller_index: i32,
  #[serde(with = "ivec3_serializer")]
  pub center: IVec3,
  pub facets: Vec<Facet>,
  pub selected_facet_index: Option<usize>,
  
  // This field won't be serialized/deserialized
  #[serde(skip)]
  pub cached_facets: Vec<Facet>,
}

impl FacetShellData {
    /// Regenerates the cached facets based on the current facets
    pub fn ensure_cached_facets(&mut self) {
        // Clear and regenerate the cached facets
        self.cached_facets.clear();
        
        // Process each facet
        for facet in &self.facets {
            if facet.symmetrize {
                // Generate all symmetric variants for this facet
                let miller = facet.miller_index;
                let shift = facet.shift;
                
                // Generate all permutations with sign changes
                let h = miller.x;
                let k = miller.y;
                let l = miller.z;
                
                // Store absolute values to identify the family type
                let abs_h = h.abs();
                let abs_k = k.abs();
                let abs_l = l.abs();
                
                // Helper closure to add a symmetrized facet with given miller indices
                let mut add_symmetric_facet = |x: i32, y: i32, z: i32| {
                    self.cached_facets.push(Facet {
                        miller_index: IVec3::new(x, y, z),
                        shift,
                        symmetrize: false, // Set to false in the cached copy
                    });
                };
                
                // Generate all permutations with sign combinations
                // This covers all cases: {100}, {110}, {111}, {hhl}, and general {hkl}
                
                // Generate permutations of the absolute values
                let abs_permutations = Self::generate_unique_permutations(abs_h, abs_k, abs_l);
                
                // For each base permutation, generate all sign combinations
                for (x, y, z) in abs_permutations {
                    // Add all sign combinations
                    add_symmetric_facet(x, y, z);
                    
                    if x != 0 {
                        add_symmetric_facet(-x, y, z);
                    }
                    
                    if y != 0 {
                        add_symmetric_facet(x, -y, z);
                        
                        if x != 0 {
                            add_symmetric_facet(-x, -y, z);
                        }
                    }
                    
                    if z != 0 {
                        add_symmetric_facet(x, y, -z);
                        
                        if x != 0 {
                            add_symmetric_facet(-x, y, -z);
                        }
                        
                        if y != 0 {
                            add_symmetric_facet(x, -y, -z);
                            
                            if x != 0 {
                                add_symmetric_facet(-x, -y, -z);
                            }
                        }
                    }
                }
            } else {
                // For non-symmetrized facets, create a new instance with the same values
                self.cached_facets.push(Facet {
                    miller_index: facet.miller_index,
                    shift: facet.shift,
                    symmetrize: facet.symmetrize,
                });
            }
            //println!("Cached facets: {:?}", self.cached_facets);
        }
        
        // Cached facets are now up-to-date
    }

    pub fn generate_unique_permutations(a: i32, b: i32, c: i32) -> Vec<(i32, i32, i32)> {
      // Use a HashSet to automatically handle uniqueness of permutations.
      let mut unique_perms: HashSet<(i32, i32, i32)> = HashSet::new();
  
      // Manually list all 3! = 6 possible permutations for three elements.
      // The HashSet will ensure that only unique combinations are stored,
      // which is crucial if the input numbers themselves contain duplicates.
      unique_perms.insert((a, b, c));
      unique_perms.insert((a, c, b));
      unique_perms.insert((b, a, c));
      unique_perms.insert((b, c, a));
      unique_perms.insert((c, a, b));
      unique_perms.insert((c, b, a));
  
      // Convert the HashSet into a Vec and return it.
      // The order of elements in the resulting Vec is not guaranteed.
      unique_perms.into_iter().collect()
  }

}

impl Default for FacetShellData {
    fn default() -> Self {
        let mut ret = Self {
            max_miller_index: 2,
            center: IVec3::new(0, 0, 0),
            facets: vec![
                Facet {
                    miller_index: IVec3::new(0, 1, 0),
                    shift: 1,
                    symmetrize: true,
                }
            ],
            selected_facet_index: None,
            cached_facets: Vec::new(),
        };
        ret.ensure_cached_facets();
        ret
    }
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
  let facet_shell_data = node.data.as_any_ref().downcast_ref::<FacetShellData>().unwrap();
  
  // Use the cached facets for evaluation
  let cached_facets = &facet_shell_data.cached_facets;

  // Only create geometry if explicit geometry evaluation is needed
  // Otherwise, return an empty CSG object
  let mut geometry = if context.explicit_geo_eval_needed {
    // If we have no facets, return an empty CSG object
    if cached_facets.is_empty() {
      CSG::new()
    } else {
      // Initialize with the first facet's half-space
      let first_facet = &cached_facets[0];
      create_plane(&first_facet.miller_index, &facet_shell_data.center, first_facet.shift)
    }
  } else {
    CSG::new()
  };

  // If we have facets and need explicit geometry evaluation,
  // intersect with the remaining facets' half-spaces
  if context.explicit_geo_eval_needed && cached_facets.len() > 1 {
    for facet in &cached_facets[1..] {
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
  let facet_shell_data = node.data.as_any_ref().downcast_ref::<FacetShellData>().unwrap();
  
  // Use the cached facets for evaluation
  let cached_facets = &facet_shell_data.cached_facets;
  
  // If there are no facets, return MAX value (infinitely far outside)
  if cached_facets.is_empty() {
    return f64::MAX;
  }
  
  // Calculate the signed distance for each facet and return the maximum
  // Using max for intersection in implicit geometry
  cached_facets.iter()
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
