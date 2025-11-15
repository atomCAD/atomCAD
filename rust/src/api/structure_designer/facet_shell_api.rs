use crate::structure_designer::nodes::facet_shell::FacetShellData;
use crate::structure_designer::nodes::facet_shell::Facet;
use crate::structure_designer::nodes::facet_shell;
use crate::api::api_common::refresh_structure_designer;
use crate::api::api_common::from_api_ivec3;
use crate::api::api_common::to_api_ivec3;
use crate::api::api_common::from_api_vec3;
use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_cad_instance_or;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::structure_designer::structure_designer_api_types::APIFacetShellData;
use crate::api::structure_designer::structure_designer_api_types::APIFacet;
use crate::api::common_api_types::APIIVec3;
use crate::api::common_api_types::APIVec3;

/// Gets the facet shell data for a node
#[flutter_rust_bridge::frb(sync)]
pub fn get_facet_shell_data(node_id: u64) -> Option<APIFacetShellData> {
  unsafe {
    with_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data(node_id) {
          Some(data) => data,
          None => return None,
        };
        let facet_shell_data = match node_data.as_any_ref().downcast_ref::<FacetShellData>() {
          Some(data) => data,
          None => return None,
        };
        
        let api_facets = facet_shell_data.facets.iter().map(|facet| {
          APIFacet {
            miller_index: to_api_ivec3(&facet.miller_index),
            shift: facet.shift,
            symmetrize: facet.symmetrize,
            visible: facet.visible,
          }
        }).collect();
        
        Some(APIFacetShellData {
          max_miller_index: facet_shell_data.max_miller_index,
          center: to_api_ivec3(&facet_shell_data.center),
          facets: api_facets,
          selected_facet_index: facet_shell_data.selected_facet_index,
        })
      },
      None
    )
  }
}

/// Sets the center and max miller index for a facet shell node
#[flutter_rust_bridge::frb(sync)]
pub fn set_facet_shell_center(node_id: u64, center: APIIVec3, max_miller_index: i32) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data_mut(node_id) {
          Some(data) => data,
          None => return false,
        };
        
        let facet_shell_data = match node_data.as_any_mut().downcast_mut::<FacetShellData>() {
          Some(data) => data,
          None => return false,
        };
        
        // Update the facet shell data in-place
        facet_shell_data.center = from_api_ivec3(&center);
        facet_shell_data.max_miller_index = max_miller_index;
        facet_shell_data.ensure_cached_facets();
        
        refresh_structure_designer(cad_instance, false);
        true
      },
      false
    )
  }
}

/// Adds a new facet to the facet shell node
#[flutter_rust_bridge::frb(sync)]
pub fn add_facet(node_id: u64, facet: APIFacet) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data_mut(node_id) {
          Some(data) => data,
          None => return false,
        };
        
        let facet_shell_data = match node_data.as_any_mut().downcast_mut::<FacetShellData>() {
          Some(data) => data,
          None => return false,
        };
        
        // Create a new facet and add it
        facet_shell_data.facets.push(Facet {
          miller_index: from_api_ivec3(&facet.miller_index),
          shift: facet.shift,
          symmetrize: facet.symmetrize,
          visible: facet.visible,
        });
        facet_shell_data.ensure_cached_facets();
        
        refresh_structure_designer(cad_instance, false);
        true
      },
      false
    )
  }
}

/// Updates a facet at the specified index
#[flutter_rust_bridge::frb(sync)]
pub fn update_facet(node_id: u64, index: usize, facet: APIFacet) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data_mut(node_id) {
          Some(data) => data,
          None => return false,
        };
        
        let facet_shell_data = match node_data.as_any_mut().downcast_mut::<FacetShellData>() {
          Some(data) => data,
          None => return false,
        };
        
        if index >= facet_shell_data.facets.len() {
          return false;
        }
        
        // Update the facet at the specified index
        facet_shell_data.facets[index] = Facet {
          miller_index: from_api_ivec3(&facet.miller_index),
          shift: facet.shift,
          symmetrize: facet.symmetrize,
          visible: facet.visible,
        };
        facet_shell_data.ensure_cached_facets();

        refresh_structure_designer(cad_instance, false);
        true
      },
      false
    )
  }
}

/// Removes a facet at the specified index
#[flutter_rust_bridge::frb(sync)]
pub fn remove_facet(node_id: u64, index: usize) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data_mut(node_id) {
          Some(data) => data,
          None => return false,
        };
        
        let facet_shell_data = match node_data.as_any_mut().downcast_mut::<FacetShellData>() {
          Some(data) => data,
          None => return false,
        };
        
        if index >= facet_shell_data.facets.len() {
          return false;
        }
        
        // Remove the facet at the specified index
        facet_shell_data.facets.remove(index);
        facet_shell_data.ensure_cached_facets();

        refresh_structure_designer(cad_instance, false);
        true
      },
      false
    )
  }
}

/// Removes all facets from the facet shell
#[flutter_rust_bridge::frb(sync)]
pub fn clear_facets(node_id: u64) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data_mut(node_id) {
          Some(data) => data,
          None => return false,
        };
        
        let facet_shell_data = match node_data.as_any_mut().downcast_mut::<FacetShellData>() {
          Some(data) => data,
          None => return false,
        };
        
        // Clear all facets
        facet_shell_data.facets.clear();
        facet_shell_data.ensure_cached_facets();

        refresh_structure_designer(cad_instance, false);
        true
      },
      false
    )
  }
}

/// Selects a facet at the specified index
#[flutter_rust_bridge::frb(sync)]
pub fn select_facet(node_id: u64, index: Option<usize>) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data_mut(node_id) {
          Some(data) => data,
          None => return false,
        };
        
        let facet_shell_data = match node_data.as_any_mut().downcast_mut::<FacetShellData>() {
          Some(data) => data,
          None => return false,
        };
        
        // If index is Some, validate it's in bounds
        if let Some(idx) = index {
          if idx >= facet_shell_data.facets.len() {
            return false;
          }
        }
        
        // Set the selected facet index
        facet_shell_data.selected_facet_index = index;
        // No need to regenerate cached_facets since only selection changed

        refresh_structure_designer(cad_instance, false);
        true
      },
      false
    )
  }
}

#[flutter_rust_bridge::frb(sync)]
pub fn select_facet_by_ray(ray_start: APIVec3, ray_dir: APIVec3) {
  unsafe {
    with_mut_cad_instance(|instance| {
      let ray_start_dvec3 = from_api_vec3(&ray_start);
      let ray_dir_dvec3 = from_api_vec3(&ray_dir);
      if facet_shell::select_facet_by_ray(&mut instance.structure_designer, &ray_start_dvec3, &ray_dir_dvec3) {
        refresh_structure_designer(instance, false);
      }
    });
  }
}

/// Splits a symmetrized facet into its individual symmetric variants
#[flutter_rust_bridge::frb(sync)]
pub fn split_symmetry_members(node_id: u64, facet_index: usize) -> bool {
  unsafe {
    with_mut_cad_instance_or(
      |cad_instance| {
        let node_data = match cad_instance.structure_designer.get_node_network_data_mut(node_id) {
          Some(data) => data,
          None => return false,
        };
        let facet_shell_data = match node_data.as_any_mut().downcast_mut::<FacetShellData>() {
          Some(data) => data,
          None => return false,
        };
        
        // Split the facet into its symmetric variants
        let result = facet_shell_data.split_symmetry_members(facet_index);
        
        // Refresh renderer as the facets have changed
        if result {
          refresh_structure_designer(cad_instance, false);
        }
        
        result
      },
      false
    )
  }
}