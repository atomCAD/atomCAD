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
use crate::structure_designer::utils::half_space_utils::{create_half_space_geo, HalfSpaceVisualization};
use crate::structure_designer::utils::half_space_utils::implicit_eval_half_space_calc;
use crate::common::poly_mesh::PolyMesh;

#[derive(Debug, Serialize, Deserialize)]
pub struct Facet {
  #[serde(with = "ivec3_serializer")]
  pub miller_index: IVec3,
  pub shift: i32,
  pub symmetrize: bool,
  #[serde(default = "default_visible")]
  pub visible: bool,
}

fn default_visible() -> bool {
  true
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
    /// Highlights the faces in the poly_mesh that correspond to the selected facet
    pub fn highlight_selected_facets(&self, poly_mesh: &mut PolyMesh) {
        // Early return if no facet is selected
        let selected_idx = match self.selected_facet_index {
            Some(idx) => idx,
            None => return,
        };
        
        // Early return if selected index is invalid
        if selected_idx >= self.facets.len() {
            return;
        }
        
        // Get the selected facet
        let selected_facet = &self.facets[selected_idx];
        
        // Create a collection of facet variants to process
        let facet_variants = if selected_facet.symmetrize {
            // If symmetrized, get all symmetric variants
            self.get_symmetric_variants(selected_facet)
        } else {
            // Otherwise, just use the selected facet
            vec![Facet {
                miller_index: selected_facet.miller_index,
                shift: selected_facet.shift,
                symmetrize: false, // Not relevant for highlighting
                visible: true,
            }]
        };
            
        // For each facet variant, find and highlight matching faces
        for facet in facet_variants {
            // Get the normal vector from miller index
            let float_miller = facet.miller_index.as_dvec3();
            let miller_magnitude = float_miller.length();
            
            // Skip invalid miller indices
            if miller_magnitude <= 1e-6 {
                continue;
            }
            
            // Normalize the miller index to get the normal vector
            let normal = float_miller / miller_magnitude;
            
            // Compare with each face normal in the PolyMesh
            for face in &mut poly_mesh.faces {
                // Compare normals with epsilon tolerance (0.05 radians ≈ 2.9 degrees)
                // Dot product close to 1 means vectors are parallel                
                // If the normals are aligned (within tolerance)
                if face.normal.dot(normal) > 0.998 { // cos(0.05) ≈ 0.998
                    face.highlighted = true;
                }
            }
        }
    }

    /// Regenerates the cached facets based on the current facets
    pub fn ensure_cached_facets(&mut self) {
        // Clear and regenerate the cached facets
        self.cached_facets.clear();
        
        // Process each facet - only process visible facets
        for facet in &self.facets {
            // Skip facets that are not visible
            if !facet.visible {
                continue;
            }
            if facet.symmetrize {
              self.cached_facets.extend(self.get_symmetric_variants(facet));
            } else {
                // For non-symmetrized facets, create a new instance with the same values
                self.cached_facets.push(Facet {
                    miller_index: facet.miller_index,
                    shift: facet.shift,
                    symmetrize: facet.symmetrize,
                    visible: true, // Always set visible to true for cached facets
                });
            }
            //println!("Cached facets: {:?}", self.cached_facets);
        }
        
        // Cached facets are now up-to-date
    }

    /// Splits a symmetrized facet into its individual symmetric variants
    /// Returns true if the facet was split, false otherwise
    pub fn split_symmetry_members(&mut self, facet_index: usize) -> bool {
        // Check if the index is valid and the facet is symmetrized
        if facet_index >= self.facets.len() {
            return false;
        }
        
        // First check if the facet has symmetrize=true
        if !self.facets[facet_index].symmetrize {
            return false;
        }
        
        // Clone the necessary data before borrowing mutably
        let miller_index = self.facets[facet_index].miller_index;
        let shift = self.facets[facet_index].shift;
        let visible = self.facets[facet_index].visible;
        
        // Create a temporary facet to generate variants
        let temp_facet = Facet {
            miller_index,
            shift,
            symmetrize: true,
            visible,
        };
        
        // Generate all symmetric variants
        let variants = self.get_symmetric_variants(&temp_facet);
        
        // Remove the original facet
        self.facets.remove(facet_index);
        
        // Add all variants (with visible set to the same as the original)
        for mut variant in variants {
            variant.visible = visible;
            self.facets.push(variant);
        }
        
        self.selected_facet_index = None;

        // Update cached facets
        self.ensure_cached_facets();
        
        true
    }

    // Generate all symmetric variants for the given facet
    fn get_symmetric_variants(&self, facet: &Facet) -> Vec<Facet> {
        let mut ret: Vec<Facet> = Vec::new();
                
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
            ret.push(Facet {
                miller_index: IVec3::new(x, y, z),
                shift,
                symmetrize: false, // Set to false in the cached copy
                visible: true,     // Set visible to true for all cached facets
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
        ret
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
                    shift: 2,
                    symmetrize: true,
                    visible: true,
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
      create_half_space_geo(&first_facet.miller_index, &facet_shell_data.center, first_facet.shift, HalfSpaceVisualization::Cuboid)
    }
  } else {
    CSG::new()
  };

  // If we have facets and need explicit geometry evaluation,
  // intersect with the remaining facets' half-spaces
  if context.explicit_geo_eval_needed && cached_facets.len() > 1 {
    for facet in &cached_facets[1..] {
      let half_space = create_half_space_geo(&facet.miller_index, &facet_shell_data.center, facet.shift, HalfSpaceVisualization::Cuboid);
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
