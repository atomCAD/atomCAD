use crate::api::api_common::with_mut_cad_instance;
use crate::api::api_common::with_cad_instance_or;
use crate::api::api_common::with_mut_cad_instance_or;
use crate::api::common_api_types::APIResult;

/// Loads node networks from a .cnnd library file for import
/// 
/// This creates a temporary registry containing the networks from the specified
/// library file. The loaded networks can then be listed and selectively imported.
#[flutter_rust_bridge::frb(sync)]
pub fn load_import_library(file_path: &str) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                match cad_instance.structure_designer.import_manager.load_library(file_path) {
                    Ok(()) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: format!("Failed to load library: {}", e),
                    }
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            }
        )
    }
}

/// Gets the list of available node network names in the loaded library
/// 
/// Returns empty vector if no library is loaded.
#[flutter_rust_bridge::frb(sync)]
pub fn get_importable_network_names() -> Vec<String> {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance.structure_designer.import_manager.get_available_networks()
            },
            Vec::new()
        )
    }
}

/// Imports selected node networks from the loaded library into the current design
/// 
/// The imported networks are moved from the library to the current design and
/// the library is automatically cleared afterwards. An optional name prefix can
/// be specified to avoid name collisions.
/// 
/// # Arguments
/// * `network_names` - List of network names to import
/// * `name_prefix` - Optional prefix to prepend to imported network names (e.g., "physics::")
#[flutter_rust_bridge::frb(sync)]
pub fn import_networks_and_clear(network_names: Vec<String>, name_prefix: Option<String>) -> APIResult {
    unsafe {
        with_mut_cad_instance_or(
            |cad_instance| {
                let prefix_ref = name_prefix.as_deref();
                match cad_instance.structure_designer.import_networks(&network_names, prefix_ref) {
                    Ok(()) => APIResult {
                        success: true,
                        error_message: String::new(),
                    },
                    Err(e) => APIResult {
                        success: false,
                        error_message: e,
                    }
                }
            },
            APIResult {
                success: false,
                error_message: "CAD instance not available".to_string(),
            }
        )
    }
}

/// Clears the loaded library and frees associated memory
/// 
/// This should be called if you want to cancel an import operation or
/// clean up after import is complete.
#[flutter_rust_bridge::frb(sync)]
pub fn clear_import_library() {
    unsafe {
        with_mut_cad_instance(|cad_instance| {
            cad_instance.structure_designer.import_manager.clear_library();
        });
    }
}

/// Returns true if a library is currently loaded for import
#[flutter_rust_bridge::frb(sync)]
pub fn is_import_library_loaded() -> bool {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance.structure_designer.import_manager.is_library_loaded()
            },
            false
        )
    }
}

/// Gets the path of the currently loaded library file
/// 
/// Returns empty string if no library is loaded.
#[flutter_rust_bridge::frb(sync)]
pub fn get_import_library_file_path() -> String {
    unsafe {
        with_cad_instance_or(
            |cad_instance| {
                cad_instance.structure_designer.import_manager.get_library_file_path()
                    .unwrap_or("")
                    .to_string()
            },
            String::new()
        )
    }
}
