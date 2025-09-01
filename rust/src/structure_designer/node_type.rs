use crate::structure_designer::node_data::NodeData;
use serde::{Serialize, Deserialize};

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum DataType {
  None,
  Int,
  Float,
  Vec2,
  Vec3,
  IVec2,
  IVec3,
  Geometry2D,
  Geometry,
  Atomic
}

pub fn data_type_to_str(data_type: &DataType) -> String {
  match data_type {
    DataType::None => "None".to_string(),
    DataType::Int => "Int".to_string(),
    DataType::Float => "Float".to_string(),
    DataType::Vec2 => "Vec2".to_string(),
    DataType::Vec3 => "Vec3".to_string(),
    DataType::IVec2 => "IVec2".to_string(),
    DataType::IVec3 => "IVec3".to_string(),
    DataType::Geometry2D => "Geometry2D".to_string(),
    DataType::Geometry => "Geometry".to_string(),
    DataType::Atomic => "Atomic".to_string(),
  }
}

pub fn str_to_data_type(s: &str) -> Option<DataType> {
  match s {
    "None" => Some(DataType::None),
    "Int" => Some(DataType::Int),
    "Float" => Some(DataType::Float),
    "Vec2" => Some(DataType::Vec2),
    "Vec3" => Some(DataType::Vec3),
    "IVec2" => Some(DataType::IVec2),
    "IVec3" => Some(DataType::IVec3),
    "Geometry2D" => Some(DataType::Geometry2D),
    "Geometry" => Some(DataType::Geometry),
    "Atomic" => Some(DataType::Atomic),
    _ => None
  }
}

pub struct Parameter {
  pub name: String,
  pub data_type: DataType,
  pub multi: bool, // whether this parameter accepts multiple inputs. If yes, they are treated as a set of values (with no order).
}

// A built-in or user defined node type.
pub struct NodeType {
  pub name: String, // name of the node type
  pub parameters: Vec<Parameter>,
  pub output_type: DataType,
  pub node_data_creator: fn() -> Box<dyn NodeData>,
}
