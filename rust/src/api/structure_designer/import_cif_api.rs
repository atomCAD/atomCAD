use crate::api::api_common::{refresh_structure_designer_auto, with_mut_cad_instance};
use crate::api::common_api_types::APIResult;
use crate::crystolecule::io::cif::load_cif_extended;
use crate::structure_designer::nodes::import_cif::{ImportCifData, build_cif_import_result};
use crate::util::path_utils::{get_parent_directory, resolve_path, try_make_relative};

#[flutter_rust_bridge::frb(sync)]
pub fn import_cif(node_id: u64) -> APIResult {
    unsafe {
        with_mut_cad_instance(|instance| {
            let design_file_dir = instance
                .structure_designer
                .node_type_registry
                .design_file_name
                .as_ref()
                .and_then(|design_path| get_parent_directory(design_path));

            let node_data = match instance
                .structure_designer
                .get_node_network_data_mut(node_id)
            {
                Some(data) => data,
                None => {
                    return APIResult {
                        success: false,
                        error_message: "Node not found".to_string(),
                    };
                }
            };
            let import_cif_data = match node_data.as_any_mut().downcast_mut::<ImportCifData>() {
                Some(data) => data,
                None => {
                    return APIResult {
                        success: false,
                        error_message: "Invalid node type for CIF import".to_string(),
                    };
                }
            };

            let stored_file_path = match &import_cif_data.file_name {
                Some(path) => path,
                None => {
                    return APIResult {
                        success: false,
                        error_message: "No file path specified for CIF import".to_string(),
                    };
                }
            };

            let resolved_path = match resolve_path(stored_file_path, design_file_dir.as_deref()) {
                Ok((path, _was_relative)) => path,
                Err(error) => {
                    return APIResult {
                        success: false,
                        error_message: format!("Failed to resolve file path: {}", error),
                    };
                }
            };

            // Try to convert absolute path to relative for portability
            if let Some(ref design_dir) = design_file_dir {
                let (potentially_relative_path, should_update) =
                    try_make_relative(&resolved_path, Some(design_dir));
                if should_update && potentially_relative_path != *stored_file_path {
                    import_cif_data.file_name = Some(potentially_relative_path);
                }
            }

            // Load the CIF file
            let block_name = import_cif_data.block_name.clone();
            let use_cif_bonds = import_cif_data.use_cif_bonds;
            let infer_bonds = import_cif_data.infer_bonds;
            let bond_tolerance = import_cif_data.bond_tolerance;

            match load_cif_extended(&resolved_path, block_name.as_deref()) {
                Ok(cif_result) => {
                    match build_cif_import_result(
                        &cif_result,
                        use_cif_bonds,
                        infer_bonds,
                        bond_tolerance,
                    ) {
                        Ok(import_result) => {
                            import_cif_data.cached_result = Some(import_result);
                            refresh_structure_designer_auto(instance);
                            APIResult {
                                success: true,
                                error_message: String::new(),
                            }
                        }
                        Err(e) => {
                            import_cif_data.cached_result = None;
                            APIResult {
                                success: false,
                                error_message: format!("Failed to process CIF data: {}", e),
                            }
                        }
                    }
                }
                Err(cif_error) => {
                    import_cif_data.cached_result = None;
                    APIResult {
                        success: false,
                        error_message: format!("Failed to load CIF file: {}", cif_error),
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
