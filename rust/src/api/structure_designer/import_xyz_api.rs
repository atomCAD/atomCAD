use crate::api::api_common::{refresh_structure_designer_auto, with_mut_cad_instance};
use crate::structure_designer::nodes::import_xyz::ImportXYZData;
use crate::api::common_api_types::APIResult;
use crate::crystolecule::io::xyz_loader::load_xyz;
use crate::util::path_utils::{resolve_path, try_make_relative, get_parent_directory};

#[flutter_rust_bridge::frb(sync)]
pub fn import_xyz(node_id: u64) -> APIResult {
  unsafe {
    with_mut_cad_instance(|instance| {
      // Get the design file directory before any mutable borrows
      let design_file_dir = instance.structure_designer.node_type_registry.design_file_name
        .as_ref()
        .and_then(|design_path| get_parent_directory(design_path));

      let node_data = match instance.structure_designer.get_node_network_data_mut(node_id) {
        Some(data) => data,
        None => {
          return APIResult {
            success: false,
            error_message: "Node not found".to_string(),
          };
        }
      };
      let import_xyz_data = match node_data.as_any_mut().downcast_mut::<ImportXYZData>() {
        Some(data) => data,
        None => {
          return APIResult {
            success: false,
            error_message: "Invalid node type for XYZ import".to_string(),
          };
        }
      };

      // Get the file path from the import_xyz_data
      let stored_file_path = match &import_xyz_data.file_name {
        Some(path) => path,
        None => {
          return APIResult {
            success: false,
            error_message: "No file path specified for XYZ import".to_string(),
          };
        }
      };

      // Resolve the path (convert relative to absolute if needed)
      let resolved_path = match resolve_path(stored_file_path, design_file_dir.as_deref()) {
        Ok((path, _was_relative)) => path,
        Err(error) => {
          return APIResult {
            success: false,
            error_message: format!("Failed to resolve file path: {}", error),
          };
        }
      };

      // Try to convert absolute path to relative if it's under the design directory
      // This helps with portability when copying projects
      if let Some(ref design_dir) = design_file_dir {
        let (potentially_relative_path, should_update) = try_make_relative(&resolved_path, Some(design_dir));
        if should_update && potentially_relative_path != *stored_file_path {
          // Update the stored path to use relative path for better portability
          import_xyz_data.file_name = Some(potentially_relative_path);
        }
      }

      // Load the XYZ file using the resolved absolute path
      match load_xyz(&resolved_path, true) {
        Ok(atomic_structure) => {
          // Set the atomic structure in the import_xyz_data
          import_xyz_data.atomic_structure = Some(atomic_structure);
          
          refresh_structure_designer_auto(instance);
          
          APIResult {
            success: true,
            error_message: String::new(),
          }
        }
        Err(xyz_error) => {
          // Clear the atomic structure on error
          import_xyz_data.atomic_structure = None;
          
          APIResult {
            success: false,
            error_message: format!("Failed to load XYZ file: {}", xyz_error),
          }
        }
      }
    })
    .unwrap_or_else(|| APIResult {
      success: false,
      error_message: "Failed to access CAD instance".to_string(),
    })
  }
}
