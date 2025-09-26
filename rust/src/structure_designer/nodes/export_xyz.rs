use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::common::xyz_saver::save_xyz;
use crate::util::path_utils::{resolve_path, get_parent_directory, try_make_relative};
use serde_json::Value;
use std::io;
use crate::structure_designer::node_type::NodeType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportXYZData {
  pub file_name: String, // If empty, the file name is not given yet.
}

impl NodeData for ExportXYZData {
  fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
    None
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
    context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {  

    let atomic_structure = match network_evaluator.evaluate_required(
      network_stack, node_id, registry, context, 0, 
      NetworkResult::extract_atomic
    ) {
      Ok(value) => value,
      Err(error) => return error,
    };
  
    let file_name = match network_evaluator.evaluate_or_default(
      network_stack, node_id, registry, context, 1, 
      self.file_name.clone(), 
      NetworkResult::extract_string
    ) {
      Ok(value) => value,
      Err(error) => return error,
    };
  
    // Check if file name is empty
    if file_name.is_empty() {
      return NetworkResult::Error("Missing export XYZ file name".to_string());
    }
  
    // Get design directory from registry
    let design_dir = registry.design_file_name
      .as_ref()
      .and_then(|design_path| get_parent_directory(design_path));
    
    // Resolve the file path (handle relative paths)
    let resolved_path = match resolve_path(&file_name, design_dir.as_deref()) {
      Ok((path, _was_relative)) => path,
      Err(_) => return NetworkResult::Error(format!("Failed to resolve export path: {}", file_name)),
    };
  
    // Save the atomic structure to XYZ file
    match save_xyz(&atomic_structure, &resolved_path) {
      Ok(()) => {
        // Return the atomic structure (pass-through)
        NetworkResult::Atomic(atomic_structure)
      }
      Err(err) => {
        NetworkResult::Error(format!("Failed to save XYZ file '{}': {}", file_name, err))
      }
    }
  }

  fn clone_box(&self) -> Box<dyn NodeData> {
      Box::new(self.clone())
  }
}

impl ExportXYZData {
  pub fn new() -> Self {
      Self {
          file_name: String::new(),
      }
  }
}

/// Special saver for ExportXYZData that converts file path to relative before saving
pub fn export_xyz_data_saver(node_data: &mut dyn NodeData, design_dir: Option<&str>) -> io::Result<Value> {
    if let Some(data) = node_data.as_any_mut().downcast_mut::<ExportXYZData>() {
        // If there's a file name and design directory, try to convert to relative path
        if let (Some(design_dir), file_name) = (design_dir, &data.file_name) {
            if !file_name.is_empty() {
                let (potentially_relative_path, should_update) = try_make_relative(file_name, Some(design_dir));
                if should_update {
                    // Update the stored path to use relative path for better portability
                    data.file_name = potentially_relative_path;
                }
            }
        }
        
        // Now serialize the (potentially modified) data
        serde_json::to_value(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch for export_xyz"))
    }
}

/// Special loader for ExportXYZData that loads the data after deserializing
pub fn export_xyz_data_loader(value: &Value, _design_dir: Option<&str>) -> io::Result<Box<dyn NodeData>> {
    // Simply deserialize the data - no special loading needed for export
    let data: ExportXYZData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    
    Ok(Box::new(data))
}
