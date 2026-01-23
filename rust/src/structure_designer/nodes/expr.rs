use crate::structure_designer::node_data::NodeData;
use crate::structure_designer::node_network_gadget::NodeNetworkGadget;
use serde::{Serialize, Deserialize};
use crate::structure_designer::structure_designer::StructureDesigner;
use crate::structure_designer::evaluator::network_evaluator::NetworkStackElement;
use crate::structure_designer::node_type_registry::NodeTypeRegistry;
use crate::structure_designer::evaluator::network_result::{NetworkResult, error_in_input, input_missing_error};
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use crate::structure_designer::evaluator::network_evaluator::NetworkEvaluationContext;
use crate::expr::validation::{get_function_signatures, get_function_implementations};
use std::collections::HashMap;
use crate::structure_designer::text_format::TextValue;
use crate::expr::parser::parse;
use crate::structure_designer::node_network::ValidationError;
use crate::expr::expr::Expr;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::node_type::{NodeType, Parameter, generic_node_data_saver};
use crate::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
use serde_json::Value;
use std::io;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExprParameter {
    pub name: String,
    pub data_type: DataType,
    pub data_type_str: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExprData {
  pub parameters: Vec<ExprParameter>,
  pub expression: String,
  #[serde(skip)]
  pub expr: Option<Expr>,
  #[serde(skip)]
  pub error: Option<String>,
  #[serde(skip)]
  pub output_type: Option<DataType>,
}

impl ExprData {
    /// Parses and validates the expression and returns any validation errors
    pub fn parse_and_validate(&mut self, node_id: u64) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        
        // Clear previous state
        self.expr = None;
        self.output_type = None;
        
        // Skip validation if expression is empty
        if self.expression.trim().is_empty() {
            return errors;
        }
        
        // Parse the expression
        let parsed_expr = match parse(&self.expression) {
            Ok(expr) => {
                self.expr = Some(expr.clone());
                expr
            },
            Err(parse_error) => {
                let error_msg = format!("Parse error: {}", parse_error);
                self.error = Some(error_msg.clone());
                errors.push(ValidationError::new(error_msg, Some(node_id)));
                return errors;
            }
        };
        
        // Create variables map for validation
        let mut variables = HashMap::new();
        
        // Add parameters as variables
        for param in &self.parameters {
            variables.insert(param.name.clone(), param.data_type.clone());
        }
        
        // Validate the parsed expression using global function registry
        match parsed_expr.validate(&variables, get_function_signatures()) {
            Ok(output_type) => {
                // Expression is valid - set the output type
                self.output_type = Some(output_type);
            }, 
            Err(validation_error) => {
                let error_msg = format!("Validation error: {}", validation_error);
                self.error = Some(error_msg.clone());
                errors.push(ValidationError::new(error_msg, Some(node_id)));
            }
        }
        
        errors
    }
}

impl NodeData for ExprData {
    fn provide_gadget(&self, _structure_designer: &StructureDesigner) -> Option<Box<dyn NodeNetworkGadget>> {
      None
    }

    fn calculate_custom_node_type(&self, base_node_type: &NodeType) -> Option<NodeType> {
      let mut custom_node_type = base_node_type.clone();
            
      // Update the output type - use DataType::None if self.output_type is None
      custom_node_type.output_type = self.output_type.clone().unwrap_or(DataType::None);
      
      // Convert ExprParameter to Parameter
      custom_node_type.parameters = self.parameters.iter()
        .map(|expr_param| Parameter {
          name: expr_param.name.clone(),
          data_type: expr_param.data_type.clone(),
        })
        .collect();
      
      Some(custom_node_type)
    }

    fn eval<'a>(
      &self,
      network_evaluator: &NetworkEvaluator,
      network_stack: &Vec<NetworkStackElement<'a>>,
      node_id: u64,
      registry: &NodeTypeRegistry,
      _decorate: bool,
      context: &mut NetworkEvaluationContext,
    ) -> NetworkResult {
      // Collect variable values for evaluation
      let mut variables: HashMap<String, NetworkResult> = HashMap::new();
      
      // Go through all parameter indices and evaluate them
      for (param_index, param) in self.parameters.iter().enumerate() {
        let result = network_evaluator.evaluate_arg(
          network_stack,
          node_id,
          registry,
          context,
          param_index,
        );
        
        // Check if the result is None (input not connected)
        if let NetworkResult::None = result {
          return input_missing_error(&param.name);
        }
        
        // Check if the result is an error
        if let NetworkResult::Error(_) = result {
          return error_in_input(&param.name);
        }
        
        // Add the variable to our collection
        variables.insert(param.name.clone(), result);
      }
      
      // If we have a parsed expression, evaluate it
      if let Some(ref expr) = self.expr {
        let function_implementations = get_function_implementations();
        expr.evaluate(&variables, function_implementations)
      } else {
        NetworkResult::Error("Expression not parsed".to_string())
      }
    }

    fn clone_box(&self) -> Box<dyn NodeData> {
        Box::new(self.clone())
    }

    fn get_subtitle(&self, _connected_input_pins: &std::collections::HashSet<String>) -> Option<String> {
        if self.expression.is_empty() {
            None
        } else {
            Some(self.expression.clone())
        }
    }

    fn get_text_properties(&self) -> Vec<(String, TextValue)> {
        // Serialize parameters as an array of objects
        let params: Vec<TextValue> = self.parameters.iter().map(|p| {
            let mut obj = vec![
                ("name".to_string(), TextValue::String(p.name.clone())),
                ("data_type".to_string(), TextValue::DataType(p.data_type.clone())),
            ];
            if let Some(ref dt_str) = p.data_type_str {
                obj.push(("data_type_str".to_string(), TextValue::String(dt_str.clone())));
            }
            TextValue::Object(obj)
        }).collect();

        vec![
            ("expression".to_string(), TextValue::String(self.expression.clone())),
            ("parameters".to_string(), TextValue::Array(params)),
        ]
    }

    fn set_text_properties(&mut self, props: &HashMap<String, TextValue>) -> Result<(), String> {
        if let Some(v) = props.get("expression") {
            self.expression = v.as_string().ok_or_else(|| "expression must be a string".to_string())?.to_string();
        }
        if let Some(TextValue::Array(params_arr)) = props.get("parameters") {
            let mut new_params = Vec::new();
            for param_val in params_arr {
                if let TextValue::Object(obj) = param_val {
                    let name = obj.iter().find(|(k, _)| k == "name")
                        .and_then(|(_, v)| v.as_string())
                        .ok_or_else(|| "parameter name must be a string".to_string())?
                        .to_string();
                    let data_type = obj.iter().find(|(k, _)| k == "data_type")
                        .and_then(|(_, v)| v.as_data_type())
                        .ok_or_else(|| "parameter data_type must be a DataType".to_string())?
                        .clone();
                    let data_type_str = obj.iter().find(|(k, _)| k == "data_type_str")
                        .and_then(|(_, v)| v.as_string())
                        .map(|s| s.to_string());
                    new_params.push(ExprParameter { name, data_type, data_type_str });
                }
            }
            self.parameters = new_params;
        }
        // Parse and validate expression after properties are set
        // (matches what expr_data_loader does after deserializing)
        let _validation_errors = self.parse_and_validate(0);
        Ok(())
    }
}

/// Special loader for ExprData that parses and validates the expression after deserializing
pub fn expr_data_loader(value: &Value, _design_dir: Option<&str>) -> io::Result<Box<dyn NodeData>> {
    // First deserialize the basic data
    let mut data: ExprData = serde_json::from_value(value.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    
    // Use the existing parse_and_validate method to handle expression parsing and validation
    // We pass a dummy node_id (0) since validation errors aren't used in the loader context
    let _validation_errors = data.parse_and_validate(0);
    
    Ok(Box::new(data))
}

pub fn get_node_type() -> NodeType {
  NodeType {
      name: "expr".to_string(),
      description: r#"You can type in a mathematical expression and it will be evaluated on its output pin.
The input pins can be dynamically added on the node editor panel, you can select the name and data type of the input parameters.

The expr node supports scalar arithmetic, vector operations, conditional expressions, and a comprehensive set of built-in mathematical functions.

## Expression Language Features

### Literals
- Integer literals (e.g., `42`, `-10`)
- Floating point literals (e.g., `3.14`, `1.5e-3`, `.5`)
- Boolean values (`true`, `false`)

### Arithmetic Operators
- `+` - Addition
- `-` - Subtraction
- `*` - Multiplication
- `/` - Division
- `%` - Modulo (integer remainder, only works on integers)
- `^` - Exponentiation
- `+x`, `-x` - Unary plus/minus

### Comparison Operators
- `==` - Equality
- `!=` - Inequality
- `<` - Less than
- `<=` - Less than or equal
- `>` - Greater than
- `>=` - Greater than or equal

### Logical Operators
- `&&` - Logical AND
- `||` - Logical OR
- `!` - Logical NOT

### Conditional Expressions
```
if condition then value1 else value2
```
Example: `if x > 0 then 1 else -1`

### Vector Operations

**Vector Constructors:**
- `vec2(x, y)` - Create 2D float vector
- `vec3(x, y, z)` - Create 3D float vector
- `ivec2(x, y)` - Create 2D integer vector
- `ivec3(x, y, z)` - Create 3D integer vector

**Member Access:**
- `vector.x`, `vector.y`, `vector.z` - Access vector components

**Vector Arithmetic:**
- Vector + Vector (component-wise)
- Vector - Vector (component-wise)
- Vector * Vector (component-wise)
- Vector * Scalar (scaling)
- Scalar * Vector (scaling)
- Vector / Scalar (scaling)

**Type Promotion:**
Integers and integer vectors automatically promote to floats and float vectors when mixed with floats.

### Vector Math Functions
- `length2(vec2)` - Calculate 2D vector magnitude
- `length3(vec3)` - Calculate 3D vector magnitude
- `normalize2(vec2)` - Normalize 2D vector to unit length
- `normalize3(vec3)` - Normalize 3D vector to unit length
- `dot2(vec2, vec2)` - 2D dot product
- `dot3(vec3, vec3)` - 3D dot product
- `cross(vec3, vec3)` - 3D cross product
- `distance2(vec2, vec2)` - Distance between 2D points
- `distance3(vec3, vec3)` - Distance between 3D points

### Integer Vector Math Functions
- `idot2(ivec2, ivec2)` - 2D integer dot product (returns int)
- `idot3(ivec3, ivec3)` - 3D integer dot product (returns int)
- `icross(ivec3, ivec3)` - 3D integer cross product (returns ivec3)

### Mathematical Functions
- `sin(x)`, `cos(x)`, `tan(x)` - Trigonometric functions
- `sqrt(x)` - Square root
- `abs(x)` - Absolute value (float)
- `abs_int(x)` - Absolute value (integer)
- `floor(x)`, `ceil(x)`, `round(x)` - Rounding functions

### Operator Precedence (highest to lowest)
1. Function calls, member access, parentheses
2. Unary operators (`+`, `-`, `!`)
3. Exponentiation (`^`) - right associative
4. Multiplication, division, modulo (`*`, `/`, `%`)
5. Addition, subtraction (`+`, `-`)
6. Comparison operators (`<`, `<=`, `>`, `>=`)
7. Equality operators (`==`, `!=`)
8. Logical AND (`&&`)
9. Logical OR (`||`)
10. Conditional expressions (`if-then-else`)

### Example Expressions
```
2 * x + 1                           // Simple arithmetic
x % 2 == 0                          // Check if x is even (modulo)
if x % 2 > 0 then -1 else 1         // Conditional with modulo
vec3(1, 2, 3) * 2.0                 // Vector scaling
length3(vec3(3, 4, 0))              // Vector length (returns 5.0)
if x > 0 then sqrt(x) else 0        // Conditional with function
dot3(normalize3(a), normalize3(b))  // Normalized dot product
sin(3.14159 / 4) * 2                // Trigonometry
vec2(x, y).x + vec2(z, w).y         // Member access
distance3(vec3(0,0,0), vec3(1,1,1)) // 3D distance
```"#.to_string(),
      category: NodeTypeCategory::MathAndProgramming,
      parameters: vec![],
      output_type: DataType::None, // will change based on the expression
      public: true,
      node_data_creator: || Box::new(ExprData {
        parameters: vec![
          ExprParameter {
            name: "x".to_string(),
            data_type: DataType::Int,
            data_type_str: None,
          },
        ],
        expression: "x".to_string(),
        expr: None,
        error: None,
        output_type: Some(DataType::Int),
      }),
      node_data_saver: generic_node_data_saver::<ExprData>,
      node_data_loader: expr_data_loader,
    }
}













