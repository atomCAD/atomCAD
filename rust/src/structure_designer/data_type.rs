pub struct FunctionType {
  parameter_types: Vec<DataType>,
  output_type: Box<DataType>,  
}

pub enum DataType {
  None,
  Bool,
  String,
  Int,
  Float,
  Vec2,
  Vec3,
  IVec2,
  IVec3,
  Geometry2D,
  Geometry,
  Atomic,
  Array(Box<DataType>),
  Function(FunctionType),
}
