use crate::structure_designer::node_data::NodeData;
use crate::api::structure_designer::structure_designer_api_types::APIDataType;
use serde_json::Value;
use std::io;
use serde::{Serialize, Deserialize};
use crate::util::as_any::AsAny;

pub fn data_type_to_str(data_type: &APIDataType) -> String {
  match data_type {
    APIDataType::None => "None".to_string(),
    APIDataType::Bool => "Bool".to_string(),
    APIDataType::String => "String".to_string(),
    APIDataType::Int => "Int".to_string(),
    APIDataType::Float => "Float".to_string(),
    APIDataType::Vec2 => "Vec2".to_string(),
    APIDataType::Vec3 => "Vec3".to_string(),
    APIDataType::IVec2 => "IVec2".to_string(),
    APIDataType::IVec3 => "IVec3".to_string(),
    APIDataType::Geometry2D => "Geometry2D".to_string(),
    APIDataType::Geometry => "Geometry".to_string(),
    APIDataType::Atomic => "Atomic".to_string(),
  }
}

pub fn str_to_data_type(s: &str) -> Option<APIDataType> {
  match s {
    "None" => Some(APIDataType::None),
    "Bool" => Some(APIDataType::Bool),
    "String" => Some(APIDataType::String),
    "Int" => Some(APIDataType::Int),
    "Float" => Some(APIDataType::Float),
    "Vec2" => Some(APIDataType::Vec2),
    "Vec3" => Some(APIDataType::Vec3),
    "IVec2" => Some(APIDataType::IVec2),
    "IVec3" => Some(APIDataType::IVec3),
    "Geometry2D" => Some(APIDataType::Geometry2D),
    "Geometry" => Some(APIDataType::Geometry),
    "Atomic" => Some(APIDataType::Atomic),
    _ => None
  }
}

#[derive(Clone)]
pub struct Parameter {
  pub name: String,
  pub data_type: APIDataType,
  pub multi: bool, // whether this parameter accepts multiple inputs. If yes, they are treated as a set of values (with no order).
}

// A built-in or user defined node type.
#[derive(Clone)]
pub struct NodeType {
  pub name: String, // name of the node type
  pub parameters: Vec<Parameter>,
  pub output_type: APIDataType,
  pub node_data_creator: fn() -> Box<dyn NodeData>,
  pub node_data_saver: fn(&mut dyn NodeData, Option<&str>) -> io::Result<Value>,
  pub node_data_loader: fn(&Value, Option<&str>) -> io::Result<Box<dyn NodeData>>,
}

/// Generic saver function for node data types that implement Serialize
pub fn generic_node_data_saver<T: NodeData + Serialize + 'static>(node_data: &mut dyn NodeData, _design_dir: Option<&str>) -> io::Result<Value> {
    if let Some(typed_data) = node_data.as_any_mut().downcast_ref::<T>() {
        serde_json::to_value(typed_data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "Data type mismatch"))
    }
}

/// Generic loader function for node data types that implement Deserialize
pub fn generic_node_data_loader<T: NodeData + for<'de> Deserialize<'de> + 'static>(value: &Value, _design_dir: Option<&str>) -> io::Result<Box<dyn NodeData>> {
    let data: T = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Box::new(data))
}

/// Saver function for NoData types (returns empty JSON object)
pub fn no_data_saver(_node_data: &mut dyn NodeData, _design_dir: Option<&str>) -> io::Result<Value> {
    Ok(serde_json::json!({}))
}

/// Loader function for NoData types (returns NoData instance)
pub fn no_data_loader(_value: &Value, _design_dir: Option<&str>) -> io::Result<Box<dyn NodeData>> {
    Ok(Box::new(crate::structure_designer::node_data::NoData {}))
}
