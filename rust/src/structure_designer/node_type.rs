use crate::structure_designer::node_data::node_data::NodeData;

#[derive(PartialEq, Clone, Copy)]
pub enum DataType {
  Geometry,
  Atomic
}

pub fn data_type_to_str(data_type: &DataType) -> String {
  match data_type {
    DataType::Geometry => "Geometry".to_string(),
    DataType::Atomic => "Atomic".to_string(),
  }
}

pub fn str_to_data_type(s: &str) -> Option<DataType> {
  match s {
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
