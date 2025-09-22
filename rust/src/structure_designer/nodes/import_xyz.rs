use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::common::atomic_structure::AtomicStructure;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::common::xyz_loader::load_xyz;
use crate::util::path_utils::{resolve_path, get_parent_directory, try_make_relative};
use serde_json::Value;
use std::io;


#[derive(Serialize, Deserialize)]
pub struct ImportXYZData {
  pub file_name: Option<String>, // If none, nothing has been imported yet.

  #[serde(skip)]
  pub atomic_structure: Option<AtomicStructure>,
}

impl NodeData for ImportXYZData {
  fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
    None
  }
}

impl ImportXYZData {
  pub fn new() -> Self {
      Self {
          file_name: None,
          atomic_structure: None,
      }
  }
}

pub fn eval_import_xyz<'a>(
  network_evaluator: &NetworkEvaluator,
  network_stack: &Vec<NetworkStackElement<'a>>,
  node_id: u64,
  registry: &NodeTypeRegistry,
  context: &mut crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext) -> NetworkResult {  
  let node = NetworkStackElement::get_top_node(network_stack, node_id);
  let node_data = &node.data.as_any_ref().downcast_ref::<ImportXYZData>().unwrap();

  let atomic_structure = if let Some(result) = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0) {
    // Check for error first
    if result.is_error() {
      return result;
    }
    
    // Extract the file name from the string result
    if let NetworkResult::String(file_name) = result {
      // Load the XYZ file using the file name parameter
      let design_dir = registry.design_file_name
        .as_ref()
        .and_then(|design_path| get_parent_directory(design_path));
      
      match resolve_path(&file_name, design_dir.as_deref()) {
        Ok((resolved_path, _was_relative)) => {
          match load_xyz(&resolved_path, true) {
            Ok(atomic_structure) => Some(atomic_structure),
            Err(_) => return NetworkResult::Error(format!("Failed to load XYZ file: {}", file_name)),
          }
        }
        Err(_) => return NetworkResult::Error(format!("Failed to resolve path: {}", file_name)),
      }
    } else {
      return NetworkResult::Error("Expected string parameter for file name".to_string());
    }
  } else {
    // No parameter provided, use the preloaded atomic structure
    node_data.atomic_structure.clone()
  };

  return match atomic_structure {
      Some(atomic_structure) => NetworkResult::Atomic(atomic_structure.clone()),
      None => NetworkResult::Error("No atomic structure imported".to_string()),
  };
}

/// Special loader for ImportXYZData that loads the atomic structure after deserializing
pub fn import_xyz_data_loader(value: &Value, design_dir: Option<&str>) -> io::Result<Box<dyn NodeData>> {
    // First deserialize the basic data
    let mut data: ImportXYZData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    
    // If there's a file name, try to load the atomic structure
    if let Some(ref file_name) = data.file_name {
        // Resolve the path (convert relative to absolute if needed)
        match resolve_path(file_name, design_dir) {
            Ok((resolved_path, _was_relative)) => {
                // Load the XYZ file using the resolved absolute path
                match load_xyz(&resolved_path, true) {
                    Ok(mut atomic_structure) => {
                        data.atomic_structure = Some(atomic_structure);
                    }
                    Err(_xyz_error) => {
                        // If loading fails, leave atomic_structure as None
                        // This allows the node to exist but show an error when evaluated
                        data.atomic_structure = None;
                    }
                }
            }
            Err(_path_error) => {
                // If path resolution fails, leave atomic_structure as None
                data.atomic_structure = None;
            }
        }
    }
    
    Ok(Box::new(data))
}

/// Special saver for ImportXYZData that converts file path to relative before saving
pub fn import_xyz_data_saver(node_data: &mut dyn NodeData, design_dir: Option<&str>) -> io::Result<Value> {
    if let Some(data) = node_data.as_any_mut().downcast_mut::<ImportXYZData>() {
        // If there's a file name and design directory, try to convert to relative path
        if let (Some(file_name), Some(design_dir)) = (&data.file_name, design_dir) {
            let (potentially_relative_path, should_update) = try_make_relative(file_name, Some(design_dir));
            if should_update {
                // Update the stored path to use relative path for better portability
                data.file_name = Some(potentially_relative_path);
            }
        }
        
        // Now serialize the (potentially modified) data
        serde_json::to_value(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch for import_xyz"))
    }
}
