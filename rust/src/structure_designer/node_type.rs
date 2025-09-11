use crate::structure_designer::node_data::NodeData;
use crate::api::structure_designer::structure_designer_api_types::APIDataType;
use serde::{Serialize, Deserialize};

pub fn data_type_to_str(data_type: &APIDataType) -> String {
  match data_type {
    APIDataType::None => "None".to_string(),
    APIDataType::Bool => "Bool".to_string(),
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
}
