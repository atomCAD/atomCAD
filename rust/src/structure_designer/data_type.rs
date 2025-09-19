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

impl DataType {
  /// Converts the DataType to its textual representation
  pub fn to_string(&self) -> String {
    match self {
      DataType::None => "None".to_string(),
      DataType::Bool => "Bool".to_string(),
      DataType::String => "String".to_string(),
      DataType::Int => "Int".to_string(),
      DataType::Float => "Float".to_string(),
      DataType::Vec2 => "Vec2".to_string(),
      DataType::Vec3 => "Vec3".to_string(),
      DataType::IVec2 => "IVec2".to_string(),
      DataType::IVec3 => "IVec3".to_string(),
      DataType::Geometry2D => "Geometry2D".to_string(),
      DataType::Geometry => "Geometry".to_string(),
      DataType::Atomic => "Atomic".to_string(),
      DataType::Array(element_type) => {
        format!("[{}]", element_type.to_string())
      },
      DataType::Function(func_type) => {
        if func_type.parameter_types.is_empty() {
          format!("() -> {}", func_type.output_type.to_string())
        } else if func_type.parameter_types.len() == 1 {
          format!("{} -> {}", 
            func_type.parameter_types[0].to_string(),
            func_type.output_type.to_string())
        } else {
          let params = func_type.parameter_types
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(",");
          format!("({}) => {}", params, func_type.output_type.to_string())
        }
      }
    }
  }
}
