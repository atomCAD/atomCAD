use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct FunctionType {
  pub parameter_types: Vec<DataType>,
  pub output_type: Box<DataType>,  
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
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
  UnitCell,
  Geometry2D,
  Geometry,
  Atomic,
  Motif,
  Array(Box<DataType>),
  Function(FunctionType),
}

impl DataType {


  pub fn is_array(&self) -> bool {
    matches!(self, DataType::Array(_))
  }

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
      DataType::UnitCell => "UnitCell".to_string(),
      DataType::Geometry2D => "Geometry2D".to_string(),
      DataType::Geometry => "Geometry".to_string(),
      DataType::Atomic => "Atomic".to_string(),
      DataType::Motif => "Motif".to_string(),
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
          format!("({}) -> {}", params, func_type.output_type.to_string())
        }
      }
    }
  }

  /// Checks if a source data type can be converted to a destination data type
  /// 
  /// # Parameters
  /// * `source_type` - The source data type
  /// * `dest_type` - The destination data type
  /// 
  /// # Returns
  /// True if the source type can be converted to the destination type
  pub fn can_be_converted_to(source_type: &DataType, dest_type: &DataType) -> bool {
    // Same types are always compatible
    if source_type == dest_type {
      return true;
    }
    
    // Check if we can convert T to [T] (single element to array)
    if let DataType::Array(target_element_type) = dest_type {
      if DataType::can_be_converted_to(source_type, target_element_type) {
        return true;
      }
    }
    
    // Check function type conversions for partial evaluation
    // Function F can be converted to function G if:
    // 1. F and G have the same return type
    // 2. F contains all parameters of G as its first parameters
    // 3. F may have additional parameters after G's parameters
    if let (DataType::Function(source_func), DataType::Function(dest_func)) = (source_type, dest_type) {
      // Check if return types are compatible
      if !DataType::can_be_converted_to(&source_func.output_type, &dest_func.output_type) {
        return false;
      }
      
      // Check if source function has at least as many parameters as destination
      if source_func.parameter_types.len() < dest_func.parameter_types.len() {
        return false;
      }
      
      // Check if the first N parameters of source match destination parameters
      // where N is the number of parameters in destination function
      for (i, dest_param) in dest_func.parameter_types.iter().enumerate() {
        if !DataType::can_be_converted_to(&source_func.parameter_types[i], dest_param) {
          return false;
        }
      }
      
      // If we get here, F can be converted to G by partial evaluation
      return true;
    }
    
    // Define conversion rules
    match (source_type, dest_type) {
      // Int <-> Float conversions
      (DataType::Int, DataType::Float) => true,
      (DataType::Float, DataType::Int) => true,
      
      // IVec2 <-> Vec2 conversions
      (DataType::IVec2, DataType::Vec2) => true,
      (DataType::Vec2, DataType::IVec2) => true,
      
      // IVec3 <-> Vec3 conversions
      (DataType::IVec3, DataType::Vec3) => true,
      (DataType::Vec3, DataType::IVec3) => true,
      
      // All other combinations are not compatible
      _ => false,
    }
  }


}

#[derive(Debug, Clone, PartialEq)]
enum DataTypeToken {
  Identifier(String),
  LeftBracket,    // [
  RightBracket,   // ]
  LeftParen,      // (
  RightParen,     // )
  Arrow,          // ->
  FatArrow,       // =>
  Comma,          // ,
  Eof,
}

struct DataTypeLexer {
  input: Vec<char>,
  pos: usize,
}

impl DataType {
  /// Parses a DataType from its textual representation
  pub fn from_string(input: &str) -> Result<DataType, String> {
    let tokens = DataTypeLexer::tokenize(input)?;
    let mut parser = DataTypeParser::new(tokens);
    let data_type = parser.parse_data_type()?;
    parser.expect(DataTypeToken::Eof)?;
    Ok(data_type)
  }
}

struct DataTypeParser {
  tokens: Vec<DataTypeToken>,
  pos: usize,
}

impl DataTypeParser {
  fn new(tokens: Vec<DataTypeToken>) -> Self {
    Self { tokens, pos: 0 }
  }

  fn peek(&self) -> &DataTypeToken {
    self.tokens.get(self.pos).unwrap_or(&DataTypeToken::Eof)
  }

  fn bump(&mut self) {
    if self.pos < self.tokens.len() {
      self.pos += 1;
    }
  }

  fn expect(&mut self, expected: DataTypeToken) -> Result<(), String> {
    if self.peek() == &expected {
      self.bump();
      Ok(())
    } else {
      Err(format!("Expected {:?}, found {:?}", expected, self.peek()))
    }
  }

  fn parse_data_type(&mut self) -> Result<DataType, String> {
    let mut data_type = self.parse_primary_type()?;

    // Handle right-associative '->' for single-parameter functions
    if self.peek() == &DataTypeToken::Arrow {
      self.bump(); // consume '->'
      let return_type = self.parse_data_type()?;
      data_type = DataType::Function(FunctionType {
        parameter_types: vec![data_type],
        output_type: Box::new(return_type),
      });
    }

    Ok(data_type)
  }

  fn parse_primary_type(&mut self) -> Result<DataType, String> {
    match self.peek() {
      DataTypeToken::Identifier(_) => self.parse_builtin_type(),
      DataTypeToken::LeftBracket => self.parse_array_type(),
      DataTypeToken::LeftParen => self.parse_parenthesized_type(),
      other => Err(format!("Unexpected token while parsing primary type: {:?}", other)),
    }
  }

  fn parse_builtin_type(&mut self) -> Result<DataType, String> {
    match self.peek().clone() {
      DataTypeToken::Identifier(name) => {
        self.bump();
        match name.as_str() {
          "None" => Ok(DataType::None),
          "Bool" => Ok(DataType::Bool),
          "String" => Ok(DataType::String),
          "Int" => Ok(DataType::Int),
          "Float" => Ok(DataType::Float),
          "Vec2" => Ok(DataType::Vec2),
          "Vec3" => Ok(DataType::Vec3),
          "IVec2" => Ok(DataType::IVec2),
          "IVec3" => Ok(DataType::IVec3),
          "UnitCell" => Ok(DataType::UnitCell),
          "Geometry2D" => Ok(DataType::Geometry2D),
          "Geometry" => Ok(DataType::Geometry),
          "Atomic" => Ok(DataType::Atomic),
          "Motif" => Ok(DataType::Motif),
          _ => Err(format!("Unknown data type: {}", name)),
        }
      },
      other => Err(format!("Expected identifier, found {:?}", other)),
    }
  }

  fn parse_array_type(&mut self) -> Result<DataType, String> {
    self.expect(DataTypeToken::LeftBracket)?;
    let element_type = self.parse_data_type()?;
    self.expect(DataTypeToken::RightBracket)?;
    Ok(DataType::Array(Box::new(element_type)))
  }

  fn parse_parenthesized_type(&mut self) -> Result<DataType, String> {
    self.expect(DataTypeToken::LeftParen)?;

    // Case 1: Empty parameter list for a function, e.g., '() -> Int'
    if self.peek() == &DataTypeToken::RightParen {
      self.bump(); // consume ')'
      self.expect(DataTypeToken::Arrow)?;
      let output_type = self.parse_data_type()?;
      return Ok(DataType::Function(FunctionType {
        parameter_types: vec![],
        output_type: Box::new(output_type),
      }));
    }

    // It's not an empty list, so parse the first type.
    let first_type = self.parse_data_type()?;

    // After the first type, we can have a comma (multi-param func) or a right paren (grouped type).
    if self.peek() == &DataTypeToken::Comma {
      // Case 2: Multi-parameter function, e.g., '(Int, Float) => Bool'
      let mut params = vec![first_type];
      while self.peek() == &DataTypeToken::Comma {
        self.bump(); // consume ','
        params.push(self.parse_data_type()?);
      }
      self.expect(DataTypeToken::RightParen)?;
      self.expect(DataTypeToken::FatArrow)?;
      let output_type = self.parse_data_type()?;
      Ok(DataType::Function(FunctionType {
        parameter_types: params,
        output_type: Box::new(output_type),
      }))
    } else {
      // Case 3: A single, grouped type, e.g., '(Int)' or '(Int -> Bool)'
      self.expect(DataTypeToken::RightParen)?;
      Ok(first_type)
    }
  }

}

impl DataTypeLexer {
  fn new(input: &str) -> Self {
    Self {
      input: input.chars().collect(),
      pos: 0,
    }
  }

  pub fn tokenize(input: &str) -> Result<Vec<DataTypeToken>, String> {
    let mut lexer = Self::new(input);
    let mut tokens = Vec::new();
    
    loop {
      let token = lexer.next_token()?;
      if token == DataTypeToken::Eof {
        tokens.push(token);
        break;
      }
      tokens.push(token);
    }
    
    Ok(tokens)
  }

  fn peek(&self) -> Option<char> {
    self.input.get(self.pos).copied()
  }

  fn advance(&mut self) -> Option<char> {
    let ch = self.peek();
    if ch.is_some() {
      self.pos += 1;
    }
    ch
  }

  fn skip_whitespace(&mut self) {
    while let Some(ch) = self.peek() {
      if ch.is_whitespace() {
        self.advance();
      } else {
        break;
      }
    }
  }

  fn read_identifier(&mut self) -> String {
    let mut result = String::new();
    while let Some(ch) = self.peek() {
      if ch.is_alphanumeric() || ch == '_' {
        result.push(ch);
        self.advance();
      } else {
        break;
      }
    }
    result
  }

  fn next_token(&mut self) -> Result<DataTypeToken, String> {
    self.skip_whitespace();
    
    match self.peek() {
      None => Ok(DataTypeToken::Eof),
      Some('[') => {
        self.advance();
        Ok(DataTypeToken::LeftBracket)
      },
      Some(']') => {
        self.advance();
        Ok(DataTypeToken::RightBracket)
      },
      Some('(') => {
        self.advance();
        Ok(DataTypeToken::LeftParen)
      },
      Some(')') => {
        self.advance();
        Ok(DataTypeToken::RightParen)
      },
      Some(',') => {
        self.advance();
        Ok(DataTypeToken::Comma)
      },
      Some('-') => {
        self.advance();
        if self.peek() == Some('>') {
          self.advance();
          Ok(DataTypeToken::Arrow)
        } else {
          Err("Expected '>' after '-'".to_string())
        }
      },
      Some('=') => {
        self.advance();
        if self.peek() == Some('>') {
          self.advance();
          Ok(DataTypeToken::FatArrow)
        } else {
          Err("Expected '>' after '='".to_string())
        }
      },
      Some(ch) if ch.is_alphabetic() || ch == '_' => {
        let identifier = self.read_identifier();
        Ok(DataTypeToken::Identifier(identifier))
      },
      Some(other) => Err(format!("Unexpected character: {}", other)),
    }
  }
}
















