use crate::api::api_common::refresh_renderer;
use crate::api::api_common::with_mut_cad_instance;
use crate::common::atomic_structure_utils::auto_create_bonds;
use crate::structure_designer::nodes::import_xyz::ImportXYZData;
use crate::api::common_api_types::APIResult;
use crate::common::xyz_loader::load_xyz;

#[flutter_rust_bridge::frb(sync)]
pub fn import_xyz(node_id: u64) -> APIResult {
  unsafe {
    with_mut_cad_instance(|instance| {
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
      let file_path = match &import_xyz_data.file_name {
        Some(path) => path,
        None => {
          return APIResult {
            success: false,
            error_message: "No file path specified for XYZ import".to_string(),
          };
        }
      };

      // Load the XYZ file using the load_xyz function
      match load_xyz(file_path) {
        Ok(mut atomic_structure) => {
          auto_create_bonds(&mut atomic_structure);
          // Set the atomic structure in the import_xyz_data
          import_xyz_data.atomic_structure = Some(atomic_structure);
          
          refresh_renderer(instance, false);
          
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