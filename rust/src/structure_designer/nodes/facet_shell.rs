use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::structure_designer::structure_designer::StructureDesigner;
use glam::i32::IVec3;
use serde::{Serialize, Deserialize};
use crate::util::serialization_utils::ivec3_serializer;
use glam::f64::DVec3;
use crate::renderer::mesh::Mesh;
use std::collections::HashSet;
use crate::display::gadget::Gadget;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::evaluator::network_result::GeometrySummary;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::util::transform::Transform;
use crate::display::poly_mesh::PolyMesh;
use crate::structure_designer::utils::half_space_utils;
use crate::geo_tree::GeoNode;
use crate::renderer::tessellator::tessellator::{Tessellatable, TessellationOutput};
use glam::f64::DQuat;
use crate::structure_designer::node_type::NodeType;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::crystolecule::unit_cell_struct::UnitCellStruct;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
  
  // Maps each cached facet to its original facet index
  #[serde(skip)]
  pub cached_facet_to_original_index: Vec<usize>,
}

/// Gets the FacetShellData for the currently active facet_shell node (immutable)
/// 
/// Returns None if:
/// - There is no active node network
/// - No node is selected in the active network
/// - The selected node is not a facet_shell node
/// - The FacetShellData cannot be retrieved or cast
pub fn get_active_facet_shell_data(structure_designer: &StructureDesigner) -> Option<&FacetShellData> {
  let selected_node_id = structure_designer.get_selected_node_id_with_type("facet_shell")?;
    
  // Get the node data and cast it to FacetShellData
  let node_data = structure_designer.get_node_network_data(selected_node_id)?;
    
  // Try to downcast to FacetShellData
  node_data.as_any_ref().downcast_ref::<FacetShellData>()
}

/// Gets the FacetShellData for the currently active facet_shell node (mutable)
/// 
/// Returns None if:
/// - There is no active node network
/// - No node is selected in the active network
/// - The selected node is not a facet_shell node
/// - The FacetShellData cannot be retrieved or cast
pub fn get_active_facet_shell_data_mut(structure_designer: &mut StructureDesigner) -> Option<&mut FacetShellData> {
  let selected_node_id = structure_designer.get_selected_node_id_with_type("facet_shell")?;
    
  // Get the node data and cast it to FacetShellData
  let node_data = structure_designer.get_node_network_data_mut(selected_node_id)?;
    
  // Try to downcast to FacetShellData
  node_data.as_any_mut().downcast_mut::<FacetShellData>()
}

pub fn select_facet_by_ray(
  structure_designer: &mut StructureDesigner,
  ray_start: &DVec3,
  ray_dir: &DVec3) -> bool {
  
  // Get the unit cell first, before taking mutable borrow
  let unit_cell = {
    let eval_cache = match structure_designer.get_selected_node_eval_cache() {
      Some(cache) => cache,
      None => return false,
    };
    
    let facet_shell_cache = match eval_cache.downcast_ref::<FacetShellEvalCache>() {
      Some(cache) => cache,
      None => return false,
    };

    facet_shell_cache.unit_cell.clone()
  };

  let facet_shell_data = match get_active_facet_shell_data_mut(structure_designer) {
    Some(data) => data,
    None => return false,
  };
  
  let cached_facet_index = match facet_shell_data.hit_facet_by_ray(&unit_cell, ray_start, ray_dir) {
    Some(index) => index,
    None => return false,
  };
  
  // Get the original facet index from the cached facet
  let original_facet_index = facet_shell_data.cached_facet_to_original_index[cached_facet_index];
  
  // Set the selected facet index
  facet_shell_data.selected_facet_index = Some(original_facet_index);
  
  true
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
        self.cached_facet_to_original_index.clear();
        
        // Process each facet - only process visible facets
        for (original_index, facet) in self.facets.iter().enumerate() {
            // Skip facets that are not visible
            if !facet.visible {
                continue;
            }
            if facet.symmetrize {
                let symmetric_variants = self.get_symmetric_variants(facet);
                let num_variants = symmetric_variants.len();
                self.cached_facets.extend(symmetric_variants);
                // Map all symmetric variants to the same original facet index
                for _ in 0..num_variants {
                    self.cached_facet_to_original_index.push(original_index);
                }
            } else {
                // For non-symmetrized facets, create a new instance with the same values
                self.cached_facets.push(Facet {
                    miller_index: facet.miller_index,
                    shift: facet.shift,
                    symmetrize: facet.symmetrize,
                    visible: true, // Always set visible to true for cached facets
                });
                // Map this cached facet to its original index
                self.cached_facet_to_original_index.push(original_index);
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
  
  /// Hit test a ray against the facet shell polyhedron
  /// Returns the index of the cached facet that was hit, or None if no hit
  /// 
  /// The algorithm finds the furthest intersection among facets hit from outside
  /// (which corresponds to the actual surface of the convex polyhedron)
  pub fn hit_facet_by_ray(&self, unit_cell: &UnitCellStruct, ray_start: &DVec3, ray_dir: &DVec3) -> Option<usize> {
      let mut best_hit: Option<(usize, f64)> = None; // (facet_index, distance)
      let center_pos = unit_cell.ivec3_lattice_to_real(&self.center);
      
      for (cached_index, facet) in self.cached_facets.iter().enumerate() {
          // Get crystallographically correct plane properties (normal and d-spacing)
          let plane_props = unit_cell.ivec3_miller_index_to_plane_props(&facet.miller_index)
              .expect("Miller index should be valid for gadget hit testing");
          
          // Calculate shift distance as multiples of d-spacing
          let shift_distance = facet.shift as f64 * plane_props.d_spacing;
          let plane_point = center_pos + plane_props.normal * shift_distance;

          // Ray-plane intersection
          let denom = plane_props.normal.dot(*ray_dir);
          
          // Skip if ray is parallel to plane
          if denom.abs() < 1e-10 {
              continue;
          }
          
          let t = plane_props.normal.dot(plane_point - *ray_start) / denom;
          
          // Skip if intersection is behind ray start
          if t < 0.0 {
              continue;
          }
          
          // Check if ray hits from outside the half-space
          // Ray hits from outside if ray direction is opposite to plane normal (negative dot product)
          if plane_props.normal.dot(*ray_dir) >= 0.0 {
              continue;
          }
          
          // Among valid hits, keep the furthest one (largest t)
          match best_hit {
              None => best_hit = Some((cached_index, t)),
              Some((_, best_t)) => {
                  if t > best_t {
                      best_hit = Some((cached_index, t));
                  }
              }
          }
      }
      
      best_hit.map(|(index, _)| index)
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
            cached_facet_to_original_index: Vec::new(),
        };
        ret.ensure_cached_facets();
        ret
    }
}

impl NodeData for FacetShellData {
    fn provide_gadget(&self, structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      let eval_cache = structure_designer.get_selected_node_eval_cache()?;
      let facet_shell_cache = eval_cache.downcast_ref::<FacetShellEvalCache>()?;
  
      if self.selected_facet_index.is_none() {
        return None;
      }
      let selected_facet = &self.facets[self.selected_facet_index.unwrap()];
      return Some(Box::new(FacetShellGadget {
        max_miller_index: self.max_miller_index,
        center: self.center,
        miller_index: selected_facet.miller_index,
        miller_index_variants: if selected_facet.symmetrize {
          self.get_symmetric_variants(&selected_facet).into_iter().map(|facet| facet.miller_index).collect()
        } else {
          vec![selected_facet.miller_index]
        },
        shift: selected_facet.shift,
        dragged_shift: selected_facet.shift as f64,
        dragged_handle_index: None,
        possible_miller_indices: half_space_utils::generate_possible_miller_indices(self.max_miller_index),
        unit_cell: facet_shell_cache.unit_cell.clone(),
      }));
    }

    fn calculate_custom_node_type(&self, _base_node_type: &NodeType) -> Option<NodeType> {
        None
    }

    fn eval<'a>(
        &self,
        network_evaluator: &NetworkEvaluator,
        network_stack: &Vec<NetworkStackElement<'a>>,
        node_id: u64,
        registry: &NodeTypeRegistry,
        _decorate: bool,
        context: &mut NetworkEvaluationContext
      ) -> NetworkResult {

        let unit_cell = match network_evaluator.evaluate_or_default(
            network_stack, node_id, registry, context, 0, 
            UnitCellStruct::cubic_diamond(), 
            NetworkResult::extract_unit_cell,
            ) {
            Ok(value) => value,
            Err(error) => return error,
        };

        let center = match network_evaluator.evaluate_or_default(
            network_stack, node_id, registry, context, 1, 
            self.center,
            NetworkResult::extract_ivec3
        ) {
            Ok(value) => value,
            Err(error) => return error,
        };

        // Store evaluation cache for root-level evaluations (used for gadget creation when this node is selected)
        // Only store for direct evaluations of visible nodes, not for upstream dependency calculations
        if network_stack.len() == 1 {
          let eval_cache = FacetShellEvalCache {
            unit_cell: unit_cell.clone(),
          };
          context.selected_node_eval_cache = Some(Box::new(eval_cache));
        }

        let center_pos = unit_cell.ivec3_lattice_to_real(&center);

        let mut shapes: Vec<GeoNode> = Vec::new();
        for facet in &self.cached_facets {
          // Get crystallographically correct plane properties (normal and d-spacing)
          let plane_props = match unit_cell.ivec3_miller_index_to_plane_props(&facet.miller_index) {
            Ok(props) => props,
            Err(error_msg) => return NetworkResult::Error(error_msg),
          };

          // Calculate shift distance as multiples of d-spacing
          let shift_distance = facet.shift as f64 * plane_props.d_spacing;
          let shifted_center = center_pos + plane_props.normal * shift_distance;

          shapes.push(GeoNode::half_space(plane_props.normal, shifted_center));
        }

        return NetworkResult::Geometry(GeometrySummary {
          unit_cell: unit_cell.clone(),
          frame_transform: Transform::new(
            center_pos,
            DQuat::IDENTITY, // Use identity quaternion as we don't need rotation
          ),
          geo_tree_root: GeoNode::intersection_3d(shapes)
        });
      }

      fn clone_box(&self) -> Box<dyn NodeData> {
          Box::new(self.clone())
      }

      fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
          let show_center = !connected_input_pins.contains("center");
          let num_facets = self.facets.len();
          
          if show_center {
              Some(format!("c: ({},{},{}) f: {}", 
                  self.center.x, self.center.y, self.center.z, num_facets))
          } else {
              Some(format!("f: {}", num_facets))
          }
      }
}


#[derive(Clone)]
pub struct FacetShellGadget {
    pub max_miller_index: i32,
    pub center: IVec3,
    pub miller_index: IVec3,
    pub miller_index_variants: Vec<IVec3>,
    pub shift: i32, // symmetric face variants (one element if not symmetrized)
    pub dragged_shift: f64, // this is rounded into 'shift'
    pub dragged_handle_index: Option<i32>, // 0 for the center, from index 1: corresponds to the variant that is dragged
    pub possible_miller_indices: HashSet<IVec3>,
    pub unit_cell: UnitCellStruct,
}

#[derive(Debug, Clone)]
pub struct FacetShellEvalCache {
  pub unit_cell: UnitCellStruct,
}

impl Tessellatable for FacetShellGadget {
  fn tessellate(&self, output: &mut TessellationOutput) {
      let output_mesh: &mut Mesh = &mut output.mesh;
      let center_pos = self.unit_cell.ivec3_lattice_to_real(&self.center);

      // Tessellate center sphere
      half_space_utils::tessellate_center_sphere(output_mesh, &center_pos);

      // Tessellate shift drag handles for all miller index variants
      // Add defensive check to prevent crashes from corrupted Vec
      if self.miller_index_variants.len() <= 1000 { // Reasonable upper bound
          for miller_index in &self.miller_index_variants {
              half_space_utils::tessellate_shift_drag_handle(
                  output_mesh,
                  &self.center,
                  miller_index,
                  self.dragged_shift,
                  &self.unit_cell,
                  1); // subdivision=1 for facet_shell (no subdivision support)
          }
      } else {
          // Log error and skip tessellation if Vec appears corrupted
          eprintln!("Warning: FacetShellGadget miller_index_variants appears corrupted (len={}), skipping tessellation", self.miller_index_variants.len());
      }

      // If we are dragging a handle, show the plane grid for visual reference
      if self.dragged_handle_index.is_some() {
        half_space_utils::tessellate_plane_grid(
            output_mesh,
            &self.center,
            &self.get_dragged_miller_index(),
            self.shift,
            &self.unit_cell,
            1); // subdivision=1 for facet_shell (no subdivision support)
      }

      // Tessellate miller index discs only if we're dragging the central sphere (handle index 0)
      if self.dragged_handle_index == Some(0) {
        half_space_utils::tessellate_miller_indices_discs(
            output_mesh,
            &center_pos,
            &self.miller_index,
            &self.possible_miller_indices,
            self.max_miller_index,
            &self.unit_cell);
      } 
  }

  fn as_tessellatable(&self) -> Box<dyn Tessellatable> {
      Box::new(self.clone())
  }
}

impl Gadget for FacetShellGadget {
  // Returns the index of the handle that was hit, or None if no handle was hit
  // handle 0: miller index handle (central red sphere)
  // handle from index 1: corresponds to the variant that is dragged
  fn hit_test(&self, ray_origin: DVec3, ray_direction: DVec3) -> Option<i32> {
      // Test central sphere
      if let Some(_t) = half_space_utils::hit_test_center_sphere(
          &self.unit_cell,
          &self.center,
          &ray_origin,
          &ray_direction
      ) {
          return Some(0); // Central sphere hit
      }
      
      // Test shift handle cylinders for all miller index variants
      // Add defensive check to prevent crashes from corrupted Vec
      if self.miller_index_variants.len() <= 1000 { // Reasonable upper bound
          for (variant_index, miller_index_variant) in self.miller_index_variants.iter().enumerate() {
              if let Some(_t) = half_space_utils::hit_test_shift_handle(
                  &self.unit_cell,
                  &self.center,
                  miller_index_variant,
                  self.shift as f64,
                  &ray_origin,
                  &ray_direction,
                  1, // subdivision=1 for facet_shell (no subdivision support)
              ) {
                  return Some(1 + variant_index as i32); // Shift handle hit for this variant
              }
          }
      } else {
          eprintln!("Warning: FacetShellGadget miller_index_variants appears corrupted in hit test (len={}), skipping hit testing", self.miller_index_variants.len());
      }

      None // No handle was hit
  }

  fn start_drag(&mut self, handle_index: i32, _ray_origin: DVec3, _ray_direction: DVec3) {
      self.dragged_handle_index = Some(handle_index);
  }

  fn drag(&mut self, handle_index: i32, ray_origin: DVec3, ray_direction: DVec3) {
      // Calculate center position in world space
      let center_pos = self.unit_cell.ivec3_lattice_to_real(&self.center);

      if handle_index == 0 {
          // Handle index already stored in dragged_handle_index during start_drag
          
          // Check if any miller index disc is hit
          if let Some(new_miller_index) = half_space_utils::hit_test_miller_indices_discs(
              &self.unit_cell,
              &center_pos,
              &self.possible_miller_indices,
              self.max_miller_index,
              ray_origin,
              ray_direction) {
              // Set the miller index to the hit disc's miller index
              self.miller_index = new_miller_index;
          }
      } else {
          // Handle dragging the shift handle
          // We need to determine the new shift value based on where the mouse ray is closest to the normal ray
          self.dragged_shift = half_space_utils::get_dragged_shift(
              &self.unit_cell,
              &self.get_dragged_miller_index(),
              &self.center,
              &ray_origin,
              &ray_direction, 
              half_space_utils::SHIFT_HANDLE_ACCESSIBILITY_OFFSET,
              1, // subdivision=1 for facet_shell (no subdivision support)
          );
          self.shift = self.dragged_shift.round() as i32;
      }
  }

  fn end_drag(&mut self) {
      // Clear the dragged handle index to stop displaying the grid and conditional miller index discs
      self.dragged_handle_index = None;
  }
}

impl NodeNetworkGadget for FacetShellGadget {
  fn clone_box(&self) -> Box<dyn NodeNetworkGadget> {
      Box::new(self.clone())
  }

  fn sync_data(&self, data: &mut dyn NodeData) {
      if let Some(facet_shell_data) = data.as_any_mut().downcast_mut::<FacetShellData>() {
        facet_shell_data.facets[facet_shell_data.selected_facet_index.unwrap()].miller_index = self.miller_index;
        facet_shell_data.center = self.center;
        facet_shell_data.facets[facet_shell_data.selected_facet_index.unwrap()].shift = self.shift;
        facet_shell_data.ensure_cached_facets();
      }
  }
}

impl FacetShellGadget {
    pub fn get_dragged_miller_index(&self) -> IVec3 {
        if self.dragged_handle_index.is_some() && self.dragged_handle_index.unwrap() > 0 && !self.miller_index_variants.is_empty() {
          let dragged_variant_index = (self.dragged_handle_index.unwrap() - 1) as usize;
          // Add bounds checking to prevent crashes from corrupted Vec
          if dragged_variant_index < self.miller_index_variants.len() && self.miller_index_variants.len() <= 1000 {
              return self.miller_index_variants[dragged_variant_index];
          } else {
              eprintln!("Warning: FacetShellGadget miller_index_variants bounds check failed (index={}, len={})", dragged_variant_index, self.miller_index_variants.len());
              return self.miller_index;
          }
        } else {
          return self.miller_index;
        }
    }
}
