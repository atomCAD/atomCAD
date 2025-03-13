use glam::i32::IVec3;
use glam::f32::Vec3;
use super::gadgets::gadget::Gadget;
use super::gadgets::half_space_gadget::HalfSpaceGadget;
use super::gadgets::atom_trans_gadget::AtomTransGadget;

#[derive(PartialEq)]
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
}
