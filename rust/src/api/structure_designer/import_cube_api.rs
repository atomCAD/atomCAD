use crate::api::api_common::{refresh_structure_designer_auto, with_mut_cad_instance};
use crate::api::common_api_types::APIResult;
use atomcad_crystolecule::io::cube_loader::load_cube;
use atomcad_structure_designer::nodes::import_cube::{ImportCubeData, LoadedCube};
use atomcad_util::path_utils::{get_parent_directory, resolve_path, try_make_relative};

/// Load the `.cube` file named by an `import_cube` node's stored property into
/// that node, mirroring `import_xyz`.
///
/// On success the node also keeps the file's units plausibility warning, which
/// surfaces separately as a non-blocking node error — the load itself is a
/// success either way, because the coordinates are always read as Bohr and the
/// warning never re-interprets them.
#[flutter_rust_bridge::frb(sync)]
pub fn import_cube(scope_path: Vec<u64>, node_id: u64) -> APIResult {
    unsafe {
        with_mut_cad_instance(|instance| {
            // Get the design file directory before any mutable borrows
            let design_file_dir = instance
                .structure_designer
                .node_type_registry
                .design_file_name
                .as_ref()
                .and_then(|design_path| get_parent_directory(design_path));

            let node_data = match instance
                .structure_designer
                .get_node_network_data_mut_scoped(&scope_path, node_id)
            {
                Some(data) => data,
                None => {
                    return APIResult {
                        success: false,
                        error_message: "Node not found".to_string(),
                    };
                }
            };
            let import_cube_data = match node_data.as_any_mut().downcast_mut::<ImportCubeData>() {
                Some(data) => data,
                None => {
                    return APIResult {
                        success: false,
                        error_message: "Invalid node type for cube import".to_string(),
                    };
                }
            };

            let stored_file_path = match &import_cube_data.file_name {
                Some(path) => path,
                None => {
                    return APIResult {
                        success: false,
                        error_message: "No file path specified for cube import".to_string(),
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

            // Try to convert absolute path to relative if it's under the design
            // directory. This helps with portability when copying projects.
            if let Some(ref design_dir) = design_file_dir {
                let (potentially_relative_path, should_update) =
                    try_make_relative(&resolved_path, Some(design_dir));
                if should_update && potentially_relative_path != *stored_file_path {
                    import_cube_data.file_name = Some(potentially_relative_path);
                }
            }

            match load_cube(&resolved_path, true) {
                Ok(cube) => match LoadedCube::from_cube_file(cube) {
                    Some(loaded) => {
                        import_cube_data.loaded = Some(loaded);

                        // The units plausibility warning only exists once the
                        // file is parsed, and it reaches the user through
                        // `get_data_error`, which only the validator asks. The
                        // refresh paths do not validate, so validate here or
                        // the warning waits for an unrelated edit to appear
                        // (and a cleared one waits to disappear).
                        instance.structure_designer.validate_active_network();

                        refresh_structure_designer_auto(instance);

                        APIResult {
                            success: true,
                            error_message: String::new(),
                        }
                    }
                    None => {
                        import_cube_data.loaded = None;
                        instance.structure_designer.validate_active_network();

                        APIResult {
                            success: false,
                            error_message: "Cube file contains no field".to_string(),
                        }
                    }
                },
                Err(cube_error) => {
                    // Clear the payload on error
                    import_cube_data.loaded = None;
                    instance.structure_designer.validate_active_network();

                    APIResult {
                        success: false,
                        error_message: format!("Failed to load cube file: {}", cube_error),
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
