use std::io;
use super::node_type_registry::NodeTypeRegistry;
use super::serialization::node_networks_serialization;

/// Manages importing node networks from external .cnnd library files
/// 
/// This struct provides a clean separation between the main design state
/// and temporary import operations. It maintains the loaded library state
/// across multiple API calls (load -> list -> import -> clear).
pub struct NodeNetworksImportManager {
    /// The loaded library registry containing node networks from external .cnnd file
    pub library_registry: Option<NodeTypeRegistry>,
    /// Path to the currently loaded library file
    pub library_file_path: Option<String>,
}

impl NodeNetworksImportManager {
    
    /// Creates a new empty import manager
    pub fn new() -> Self {
        Self {
            library_registry: None,
            library_file_path: None,
        }
    }
    
    /// Loads node networks from a .cnnd library file
    /// 
    /// This creates a temporary NodeTypeRegistry containing only the networks
    /// from the specified library file. The loaded networks can then be
    /// listed and selectively imported.
    /// 
    /// # Arguments
    /// * `file_path` - Path to the .cnnd library file to load
    /// 
    /// # Returns
    /// * `Ok(())` if the library was loaded successfully
    /// * `Err(io::Error)` if there was an error reading or parsing the file
    pub fn load_library(&mut self, file_path: &str) -> io::Result<()> {
        // Create a new temporary registry for the library
        let mut temp_registry = NodeTypeRegistry::new();
        
        // Load the node networks from the library file into the temporary registry
        node_networks_serialization::load_node_networks_from_file(
            &mut temp_registry, 
            file_path
        )?;
        
        // Store the loaded registry and file path
        self.library_registry = Some(temp_registry);
        self.library_file_path = Some(file_path.to_string());
        
        Ok(())
    }
    
    /// Gets the list of available node network names in the loaded library
    /// 
    /// # Returns
    /// * `Vec<String>` - List of network names available for import
    ///   Returns empty vector if no library is loaded
    pub fn get_available_networks(&self) -> Vec<String> {
        match &self.library_registry {
            Some(registry) => {
                // Get all node network names from the loaded library
                registry.node_networks.keys().cloned().collect()
            }
            None => Vec::new(),
        }
    }
    
    /// Gets the final names that networks would have after import with the given prefix
    /// 
    /// This is useful for UI preview to show users what the imported names will be.
    /// 
    /// # Arguments
    /// * `network_names` - List of network names to preview
    /// * `name_prefix` - Optional prefix to apply
    /// 
    /// # Returns
    /// * `Vec<String>` - List of final names after applying prefix
    pub fn preview_import_names(&self, network_names: &[String], name_prefix: Option<&str>) -> Vec<String> {
        network_names.iter().map(|network_name| {
            match name_prefix {
                Some(prefix) => format!("{}{}", prefix, network_name),
                None => network_name.clone(),
            }
        }).collect()
    }
    
    /// Imports selected node networks from the loaded library into the target registry
    /// and clears the library afterwards.
    /// 
    /// The imported networks are moved (not cloned) from the library registry to the
    /// target registry. This avoids the need to implement Clone for NodeNetwork.
    /// After import, the library is automatically cleared.
    /// 
    /// # Arguments
    /// * `network_names` - List of network names to import
    /// * `target_registry` - The registry to import the networks into
    /// * `name_prefix` - Optional prefix to prepend to imported network names (e.g., "mylib::")
    /// 
    /// # Returns
    /// * `Ok(())` if all networks were imported successfully
    /// * `Err(String)` with error message if:
    ///   - No library is loaded
    ///   - One or more specified networks don't exist in the library
    ///   - A network with the prefixed name already exists in the target registry
    /// 
    /// # Examples
    /// ```
    /// // Import without prefix
    /// manager.import_networks_and_clear(&["network1"], &mut registry, None)?;
    /// // Result: "network1" 
    /// 
    /// // Import with prefix
    /// manager.import_networks_and_clear(&["network1"], &mut registry, Some("physics::"))?;
    /// // Result: "physics::network1"
    /// ```
    pub fn import_networks_and_clear(
        &mut self, 
        network_names: &[String], 
        target_registry: &mut NodeTypeRegistry,
        name_prefix: Option<&str>
    ) -> Result<(), String> {
        // Take ownership of the library registry to enable moving networks
        let mut library_registry = match self.library_registry.take() {
            Some(registry) => registry,
            None => return Err("No library is loaded. Call load_library() first.".to_string()),
        };
        
        // Validate that all requested networks exist in the library
        for network_name in network_names {
            if !library_registry.node_networks.contains_key(network_name) {
                // Restore the library registry since validation failed
                self.library_registry = Some(library_registry);
                return Err(format!(
                    "Network '{}' not found in loaded library", 
                    network_name
                ));
            }
        }
        
        // Check for name conflicts in the target registry (using prefixed names)
        for network_name in network_names {
            let final_name = match name_prefix {
                Some(prefix) => format!("{}{}", prefix, network_name),
                None => network_name.clone(),
            };
            
            if target_registry.node_networks.contains_key(&final_name) {
                // Restore the library registry since validation failed
                self.library_registry = Some(library_registry);
                return Err(format!(
                    "Network '{}' already exists in the target registry. Import would overwrite existing network.", 
                    final_name
                ));
            }
        }
        
        // Import the networks by moving them from library to target
        for network_name in network_names {
            if let Some(network) = library_registry.node_networks.remove(network_name) {
                let final_name = match name_prefix {
                    Some(prefix) => format!("{}{}", prefix, network_name),
                    None => network_name.clone(),
                };
                
                target_registry.node_networks.insert(
                    final_name, 
                    network
                );
            }
        }
        
        // Clear the library (already taken ownership above, so just clear file path)
        self.library_file_path = None;
        
        Ok(())
    }
    
    /// Clears the loaded library and frees associated memory
    /// 
    /// This should be called after import operations are complete to clean up
    /// the temporary library registry.
    pub fn clear_library(&mut self) {
        self.library_registry = None;
        self.library_file_path = None;
    }
    
    /// Returns true if a library is currently loaded
    pub fn is_library_loaded(&self) -> bool {
        self.library_registry.is_some()
    }
    
    /// Gets the path of the currently loaded library file
    /// 
    /// # Returns
    /// * `Some(&str)` - Path to the loaded library file
    /// * `None` - No library is currently loaded
    pub fn get_library_file_path(&self) -> Option<&str> {
        self.library_file_path.as_deref()
    }
}

impl Default for NodeNetworksImportManager {
    fn default() -> Self {
        Self::new()
    }
}
