use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use crate::crystolecule::atomic_structure::AtomicStructure;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::text_format::TextValue;
use crate::crystolecule::io::xyz_loader::load_xyz;
use crate::util::path_utils::{resolve_path, get_parent_directory, try_make_relative};
use serde_json::Value;
use std::io;
use crate::structure_designer::node_type::{NodeType, Parameter};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use crate::structure_designer::data_type::DataType;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportXYZData {
  pub file_name: Option<String>, // If none, nothing has been imported yet.

  #[serde(skip)]
  pub atomic_structure: Option<AtomicStructure>,
}

impl NodeData for ImportXYZData {
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
  
    let result = network_evaluator.evaluate_arg(network_stack, node_id, registry, context, 0);
    
    let atomic_structure = if let NetworkResult::None = result {
      // No parameter provided, use the preloaded atomic structure
      self.atomic_structure.clone()
    } else {
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
    };
  
    return match atomic_structure {
        Some(atomic_structure) => NetworkResult::Atomic(atomic_structure.clone()),
        None => NetworkResult::Error("No atomic structure imported".to_string()),
    };
  }

  fn clone_box(&self) -> Box<dyn NodeData> {
      Box::new(self.clone())
  }

  fn get_subtitle(&self, connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
      if connected_input_pins.contains("file_name") {
          None
      } else {
          self.file_name.clone()
      }
  }

  fn get_text_properties(&self) -> Vec<(String, TextValue)> {
      let mut props = Vec::new();
      if let Some(ref file_name) = self.file_name {
          props.push(("file_name".to_string(), TextValue::String(file_name.clone())));
      }
      props
  }

  fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
      if let Some(v) = props.get("file_name") {
          self.file_name = Some(v.as_string().ok_or_else(|| "file_name must be a string".to_string())?.to_string());
      }
      Ok(())
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
                    Ok(atomic_structure) => {
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

pub fn get_node_type() -> NodeType {
    NodeType {
      name: "import_xyz".to_string(),
      description: "Imports an atomic structure from an xyz file.
It converts file paths to relative paths whenever possible (if the file is in the same directory as the node or in a subdirectory) so that when you copy your whole project to another location or machine the XYZ file references will remain valid.".to_string(),
      summary: None,
      category: NodeTypeCategory::AtomicStructure,
      parameters: vec![
        Parameter {
          id: None,
          name: "file_name".to_string(),
          data_type: DataType::String,
        },
      ],
      output_type: DataType::Atomic,
      public: true,
      node_data_creator: || Box::new(ImportXYZData::new()),
      node_data_saver: import_xyz_data_saver,
      node_data_loader: import_xyz_data_loader,
    }
}
