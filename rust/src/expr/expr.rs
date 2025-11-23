use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::expr::validation::{FunctionSignature, EvaluationFunction};
use std::collections::HashMap;
use crate::structure_designer::data_type::DataType;

#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    Neg,
    Pos,
    Not,
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add, Sub, Mul, Div, Pow, Mod,
    // Comparison operators
    Eq, Ne, Lt, Le, Gt, Ge,
    // Logical operators
    And, Or,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i32),
    Float(f64),
    Bool(bool),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(Box<Expr>, BinOp, Box<Expr>),
    Call(String, Vec<Expr>),
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>), // if condition then expr1 else expr2
    MemberAccess(Box<Expr>, String), // expr.member (e.g., vec.x, vec.y, vec.z)
}

impl Expr {
    /// Validates the expression and returns its inferred type
    pub fn validate(&self, variables: &HashMap<String, DataType>, functions: &HashMap<String, FunctionSignature>) -> Result<DataType, String> {
        
        match self {
            Expr::Int(_) => Ok(DataType::Int),
            Expr::Float(_) => Ok(DataType::Float),
            Expr::Bool(_) => Ok(DataType::Bool),
            Expr::Var(name) => {
                variables.get(name)
                    .cloned()
                    .ok_or_else(|| format!("Unknown variable: {}", name))
            }
            Expr::Unary(op, expr) => {
                let expr_type = expr.validate(variables, functions)?;
                match op {
                    UnOp::Neg | UnOp::Pos => {
                        match expr_type {
                            DataType::Int | DataType::Float => Ok(expr_type),
                            _ => Err(format!("Unary {:?} operator requires numeric type, got {:?}", op, expr_type))
                        }
                    }
                    UnOp::Not => {
                        match expr_type {
                            DataType::Bool => Ok(DataType::Bool),
                            DataType::Int => Ok(DataType::Bool), // Allow int as bool
                            _ => Err(format!("Logical NOT requires boolean or int type, got {:?}", expr_type))
                        }
                    }
                }
            }
            Expr::Binary(left, op, right) => {
                let left_type = left.validate(variables, functions)?.clone();
                let right_type = right.validate(variables, functions)?.clone();
                
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
                        // Arithmetic operations
                        match (&left_type, &right_type) {
                            // Scalar arithmetic
                            (DataType::Int, DataType::Int) => Ok(DataType::Int),
                            (DataType::Float, DataType::Float) => Ok(DataType::Float),
                            (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int) => Ok(DataType::Float),
                            
                            // Vector-vector arithmetic (component-wise)
                            (DataType::Vec2, DataType::Vec2) => Ok(DataType::Vec2),
                            (DataType::Vec3, DataType::Vec3) => Ok(DataType::Vec3),
                            (DataType::IVec2, DataType::IVec2) => Ok(DataType::IVec2),
                            (DataType::IVec3, DataType::IVec3) => Ok(DataType::IVec3),
                            
                            // Vector type promotion (ivec + vec → vec)
                            (DataType::IVec2, DataType::Vec2) | (DataType::Vec2, DataType::IVec2) => Ok(DataType::Vec2),
                            (DataType::IVec3, DataType::Vec3) | (DataType::Vec3, DataType::IVec3) => Ok(DataType::Vec3),
                            
                            // Vector-scalar operations (only for Mul and Div)
                            (DataType::Vec2, DataType::Float) | (DataType::Float, DataType::Vec2) if matches!(op, BinOp::Mul | BinOp::Div) => Ok(DataType::Vec2),
                            (DataType::Vec3, DataType::Float) | (DataType::Float, DataType::Vec3) if matches!(op, BinOp::Mul | BinOp::Div) => Ok(DataType::Vec3),
                            (DataType::IVec2, DataType::Int) | (DataType::Int, DataType::IVec2) if matches!(op, BinOp::Mul | BinOp::Div) => Ok(DataType::IVec2),
                            (DataType::IVec3, DataType::Int) | (DataType::Int, DataType::IVec3) if matches!(op, BinOp::Mul | BinOp::Div) => Ok(DataType::IVec3),
                            
                            // Mixed vector-scalar with promotion
                            (DataType::Vec2, DataType::Int) | (DataType::Int, DataType::Vec2) if matches!(op, BinOp::Mul | BinOp::Div) => Ok(DataType::Vec2),
                            (DataType::Vec3, DataType::Int) | (DataType::Int, DataType::Vec3) if matches!(op, BinOp::Mul | BinOp::Div) => Ok(DataType::Vec3),
                            (DataType::IVec2, DataType::Float) | (DataType::Float, DataType::IVec2) if matches!(op, BinOp::Mul | BinOp::Div) => Ok(DataType::Vec2),
                            (DataType::IVec3, DataType::Float) | (DataType::Float, DataType::IVec3) if matches!(op, BinOp::Mul | BinOp::Div) => Ok(DataType::Vec3),
                            
                            _ => Err(format!("Arithmetic operation {:?} not supported for types {:?} and {:?}", op, left_type, right_type))
                        }
                    }
                    BinOp::Mod => {
                        // Modulo operation - only works with integers
                        match (&left_type, &right_type) {
                            (DataType::Int, DataType::Int) => Ok(DataType::Int),
                            _ => Err(format!("Modulo operation not supported for types {:?} and {:?}", left_type, right_type))
                        }
                    }
                    BinOp::Eq | BinOp::Ne => {
                        // Equality comparison - can compare any compatible types
                        if Self::types_compatible(&left_type, &right_type) {
                            Ok(DataType::Bool)
                        } else {
                            Err(format!("Cannot compare incompatible types {:?} and {:?}", left_type, right_type))
                        }
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        // Ordering comparison - requires numeric types
                        match (&left_type, &right_type) {
                            (DataType::Int, DataType::Int) | 
                            (DataType::Float, DataType::Float) |
                            (DataType::Int, DataType::Float) | 
                            (DataType::Float, DataType::Int) => Ok(DataType::Bool),
                            _ => Err(format!("Comparison operation {:?} requires numeric types, got {:?} and {:?}", op, left_type, right_type))
                        }
                    }
                    BinOp::And | BinOp::Or => {
                        // Logical operations
                        match (&left_type, &right_type) {
                            (DataType::Bool, DataType::Bool) => Ok(DataType::Bool),
                            (DataType::Int, DataType::Int) => Ok(DataType::Bool), // Allow int as bool
                            (DataType::Bool, DataType::Int) | (DataType::Int, DataType::Bool) => Ok(DataType::Bool),
                            _ => Err(format!("Logical operation {:?} requires boolean or int types, got {:?} and {:?}", op, left_type, right_type))
                        }
                    }
                }
            }
            Expr::Call(name, args) => {
                // Validate function exists
                let signature = functions.get(name)
                    .ok_or_else(|| format!("Unknown function: {}", name))?;
                
                // Check argument count
                if args.len() != signature.parameter_types.len() {
                    return Err(format!("Function {} expects {} arguments, got {}", 
                        name, signature.parameter_types.len(), args.len()));
                }
                
                // Validate each argument type
                for (i, (arg, expected_type)) in args.iter().zip(&signature.parameter_types).enumerate() {
                    let arg_type = arg.validate(variables, functions)?;
                    if !Self::types_compatible(&arg_type, expected_type) {
                        return Err(format!("Function {} argument {} expects type {}, got {}", 
                            name, i + 1, expected_type.to_string(), arg_type.to_string()));
                    }
                }
                
                Ok(signature.return_type.clone())
            }
            Expr::Conditional(condition, then_expr, else_expr) => {
                let condition_type = condition.validate(variables, functions)?;
                let then_type = then_expr.validate(variables, functions)?;
                let else_type = else_expr.validate(variables, functions)?;
                
                // Condition must be boolean or int
                match condition_type {
                    DataType::Bool | DataType::Int => {},
                    _ => return Err(format!("Conditional condition must be boolean or int, got {}", condition_type.to_string()))
                }
                
                // Then and else branches must have compatible types
                if Self::types_compatible(&then_type, &else_type) {
                    // Return the more general type
                    match (&then_type, &else_type) {
                        (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int) => Ok(DataType::Float),
                        _ => Ok(then_type.clone()) // Same types or other compatible combinations
                    }
                } else {
                    Err(format!("Conditional branches have incompatible types: {} and {}", then_type.to_string(), else_type.to_string()))
                }
            }
            Expr::MemberAccess(expr, member) => {
                let expr_type = expr.validate(variables, functions)?.clone();
                match (expr_type.clone(), member.as_str()) {
                    // Vec2 components
                    (DataType::Vec2, "x" | "y") => Ok(DataType::Float),
                    // Vec3 components
                    (DataType::Vec3, "x" | "y" | "z") => Ok(DataType::Float),
                    // IVec2 components
                    (DataType::IVec2, "x" | "y") => Ok(DataType::Int),
                    // IVec3 components
                    (DataType::IVec3, "x" | "y" | "z") => Ok(DataType::Int),
                    _ => Err(format!("Type {} does not have member '{}'", expr_type.to_string(), member))
                }
            }
        }
    }
    
    /// Evaluates the expression and returns the result
    pub fn evaluate(&self, variables: &HashMap<String, NetworkResult>, functions: &HashMap<String, EvaluationFunction>) -> NetworkResult {
        
        match self {
            Expr::Int(n) => NetworkResult::Int(*n),
            Expr::Float(n) => NetworkResult::Float(*n),
            Expr::Bool(b) => NetworkResult::Bool(*b),
            Expr::Var(name) => {
                variables.get(name)
                    .cloned()
                    .unwrap_or_else(|| NetworkResult::Error(format!("Unknown variable: {}", name)))
            }
            Expr::Unary(op, expr) => {
                let value = expr.evaluate(variables, functions);
                if let NetworkResult::Error(_) = value {
                    return value;
                }
                
                match op {
                    UnOp::Neg => {
                        match value {
                            NetworkResult::Int(n) => NetworkResult::Int(-n),
                            NetworkResult::Float(n) => NetworkResult::Float(-n),
                            _ => NetworkResult::Error("Negation requires numeric type".to_string())
                        }
                    }
                    UnOp::Pos => {
                        match value {
                            NetworkResult::Int(_) | NetworkResult::Float(_) => value,
                            _ => NetworkResult::Error("Positive operator requires numeric type".to_string())
                        }
                    }
                    UnOp::Not => {
                        match value {
                            NetworkResult::Bool(b) => NetworkResult::Bool(!b),
                            NetworkResult::Int(n) => NetworkResult::Bool(n == 0),
                            _ => NetworkResult::Error("Logical NOT requires boolean or int type".to_string())
                        }
                    }
                }
            }
            Expr::Binary(left, op, right) => {
                let left_val = left.evaluate(variables, functions);
                if let NetworkResult::Error(_) = left_val {
                    return left_val;
                }
                
                let right_val = right.evaluate(variables, functions);
                if let NetworkResult::Error(_) = right_val {
                    return right_val;
                }
                
                Self::evaluate_binary_op(left_val, *op, right_val)
            }
            Expr::Call(name, args) => {
                // Evaluate all arguments first
                let mut arg_values = Vec::new();
                for arg in args {
                    let val = arg.evaluate(variables, functions);
                    if let NetworkResult::Error(_) = val {
                        return val;
                    }
                    arg_values.push(val);
                }
                
                // Call the function
                if let Some(func) = functions.get(name) {
                    func(&arg_values)
                } else {
                    NetworkResult::Error(format!("Unknown function: {}", name))
                }
            }
            Expr::Conditional(condition, then_expr, else_expr) => {
                let condition_val = condition.evaluate(variables, functions);
                if let NetworkResult::Error(_) = condition_val {
                    return condition_val;
                }
                
                let is_true = match condition_val {
                    NetworkResult::Bool(b) => b,
                    NetworkResult::Int(n) => n != 0,
                    _ => return NetworkResult::Error("Conditional condition must be boolean or int".to_string())
                };
                
                if is_true {
                    then_expr.evaluate(variables, functions)
                } else {
                    else_expr.evaluate(variables, functions)
                }
            }
            Expr::MemberAccess(expr, member) => {
                let value = expr.evaluate(variables, functions);
                if let NetworkResult::Error(_) = value {
                    return value;
                }
                
                match (value, member.as_str()) {
                    // Vec2 components
                    (NetworkResult::Vec2(vec), "x") => NetworkResult::Float(vec.x),
                    (NetworkResult::Vec2(vec), "y") => NetworkResult::Float(vec.y),
                    // Vec3 components
                    (NetworkResult::Vec3(vec), "x") => NetworkResult::Float(vec.x),
                    (NetworkResult::Vec3(vec), "y") => NetworkResult::Float(vec.y),
                    (NetworkResult::Vec3(vec), "z") => NetworkResult::Float(vec.z),
                    // IVec2 components
                    (NetworkResult::IVec2(vec), "x") => NetworkResult::Int(vec.x),
                    (NetworkResult::IVec2(vec), "y") => NetworkResult::Int(vec.y),
                    // IVec3 components
                    (NetworkResult::IVec3(vec), "x") => NetworkResult::Int(vec.x),
                    (NetworkResult::IVec3(vec), "y") => NetworkResult::Int(vec.y),
                    (NetworkResult::IVec3(vec), "z") => NetworkResult::Int(vec.z),
                    _ => NetworkResult::Error(format!("Cannot access member '{}' on value", member))
                }
            }
        }
    }

    /// Helper function to check if two types are compatible for operations
    fn types_compatible(type1: &DataType, type2: &DataType) -> bool {
        match (type1, type2) {
            // Same types are always compatible
            (a, b) if a == b => true,
            // Numeric types are compatible with each other
            (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int) => true,
            // Bool and Int are compatible for logical operations
            (DataType::Bool, DataType::Int) | (DataType::Int, DataType::Bool) => true,
            // Vector type compatibility (for comparisons)
            (DataType::IVec2, DataType::Vec2) | (DataType::Vec2, DataType::IVec2) => true,
            (DataType::IVec3, DataType::Vec3) | (DataType::Vec3, DataType::IVec3) => true,
            _ => false
        }
    }
    
    /// Helper function to evaluate binary operations
    fn evaluate_binary_op(left: NetworkResult, op: BinOp, right: NetworkResult) -> NetworkResult {
        
        match op {
            BinOp::Add => Self::arithmetic_op(left, right, |a, b| a + b, |a, b| a + b),
            BinOp::Sub => Self::arithmetic_op(left, right, |a, b| a - b, |a, b| a - b),
            BinOp::Mul => Self::arithmetic_op(left, right, |a, b| a * b, |a, b| a * b),
            BinOp::Div => {
                // Check for division by zero first
                match &right {
                    NetworkResult::Int(0) => {
                        return NetworkResult::Error("Division by zero".to_string());
                    }
                    NetworkResult::Float(f) if *f == 0.0 => {
                        return NetworkResult::Error("Division by zero".to_string());
                    }
                    _ => {}
                }
                Self::arithmetic_op(left, right, |a, b| a / b, |a, b| a / b)
            }
            BinOp::Mod => {
                // Check for modulo by zero first
                match &right {
                    NetworkResult::Int(0) => {
                        return NetworkResult::Error("Modulo by zero".to_string());
                    }
                    _ => {}
                }
                // Modulo only works with integers
                match (left, right) {
                    (NetworkResult::Int(a), NetworkResult::Int(b)) => NetworkResult::Int(a % b),
                    _ => NetworkResult::Error("Modulo operation requires integer operands".to_string()),
                }
            }
            BinOp::Pow => Self::arithmetic_op(left, right, |a, b| a.pow(b as u32), |a, b| a.powf(b)),
            BinOp::Eq => Self::comparison_op(left, right, |a, b| a == b, |a, b| (a - b).abs() < f64::EPSILON),
            BinOp::Ne => Self::comparison_op(left, right, |a, b| a != b, |a, b| (a - b).abs() >= f64::EPSILON),
            BinOp::Lt => Self::comparison_op(left, right, |a, b| a < b, |a, b| a < b),
            BinOp::Le => Self::comparison_op(left, right, |a, b| a <= b, |a, b| a <= b),
            BinOp::Gt => Self::comparison_op(left, right, |a, b| a > b, |a, b| a > b),
            BinOp::Ge => Self::comparison_op(left, right, |a, b| a >= b, |a, b| a >= b),
            BinOp::And => Self::logical_op(left, right, |a, b| a && b),
            BinOp::Or => Self::logical_op(left, right, |a, b| a || b),
        }
    }
    
    /// Helper for arithmetic operations
    fn arithmetic_op<F1, F2>(left: NetworkResult, right: NetworkResult, int_op: F1, float_op: F2) -> NetworkResult
    where
        F1: Fn(i32, i32) -> i32,
        F2: Fn(f64, f64) -> f64,
    {
        use glam::f64::{DVec2, DVec3};
        use glam::i32::{IVec2, IVec3};
        
        match (left, right) {
            // Scalar operations
            (NetworkResult::Int(a), NetworkResult::Int(b)) => NetworkResult::Int(int_op(a, b)),
            (NetworkResult::Float(a), NetworkResult::Float(b)) => NetworkResult::Float(float_op(a, b)),
            (NetworkResult::Int(a), NetworkResult::Float(b)) => NetworkResult::Float(float_op(a as f64, b)),
            (NetworkResult::Float(a), NetworkResult::Int(b)) => NetworkResult::Float(float_op(a, b as f64)),
            
            // Vector-vector operations (component-wise)
            (NetworkResult::Vec2(a), NetworkResult::Vec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x, b.x), float_op(a.y, b.y)))
            }
            (NetworkResult::Vec3(a), NetworkResult::Vec3(b)) => {
                NetworkResult::Vec3(DVec3::new(float_op(a.x, b.x), float_op(a.y, b.y), float_op(a.z, b.z)))
            }
            (NetworkResult::IVec2(a), NetworkResult::IVec2(b)) => {
                NetworkResult::IVec2(IVec2::new(int_op(a.x, b.x), int_op(a.y, b.y)))
            }
            (NetworkResult::IVec3(a), NetworkResult::IVec3(b)) => {
                NetworkResult::IVec3(IVec3::new(int_op(a.x, b.x), int_op(a.y, b.y), int_op(a.z, b.z)))
            }
            
            // Vector type promotion (ivec + vec → vec)
            (NetworkResult::IVec2(a), NetworkResult::Vec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x as f64, b.x), float_op(a.y as f64, b.y)))
            }
            (NetworkResult::Vec2(a), NetworkResult::IVec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x, b.x as f64), float_op(a.y, b.y as f64)))
            }
            (NetworkResult::IVec3(a), NetworkResult::Vec3(b)) => {
                NetworkResult::Vec3(DVec3::new(float_op(a.x as f64, b.x), float_op(a.y as f64, b.y), float_op(a.z as f64, b.z)))
            }
            (NetworkResult::Vec3(a), NetworkResult::IVec3(b)) => {
                NetworkResult::Vec3(DVec3::new(float_op(a.x, b.x as f64), float_op(a.y, b.y as f64), float_op(a.z, b.z as f64)))
            }
            
            // Vector-scalar operations (only for multiplication and division)
            (NetworkResult::Vec2(a), NetworkResult::Float(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x, b), float_op(a.y, b)))
            }
            (NetworkResult::Float(a), NetworkResult::Vec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a, b.x), float_op(a, b.y)))
            }
            (NetworkResult::Vec3(a), NetworkResult::Float(b)) => {
                NetworkResult::Vec3(DVec3::new(float_op(a.x, b), float_op(a.y, b), float_op(a.z, b)))
            }
            (NetworkResult::Float(a), NetworkResult::Vec3(b)) => {
                NetworkResult::Vec3(DVec3::new(float_op(a, b.x), float_op(a, b.y), float_op(a, b.z)))
            }
            (NetworkResult::IVec2(a), NetworkResult::Int(b)) => {
                NetworkResult::IVec2(IVec2::new(int_op(a.x, b), int_op(a.y, b)))
            }
            (NetworkResult::Int(a), NetworkResult::IVec2(b)) => {
                NetworkResult::IVec2(IVec2::new(int_op(a, b.x), int_op(a, b.y)))
            }
            (NetworkResult::IVec3(a), NetworkResult::Int(b)) => {
                NetworkResult::IVec3(IVec3::new(int_op(a.x, b), int_op(a.y, b), int_op(a.z, b)))
            }
            (NetworkResult::Int(a), NetworkResult::IVec3(b)) => {
                NetworkResult::IVec3(IVec3::new(int_op(a, b.x), int_op(a, b.y), int_op(a, b.z)))
            }
            
            // Mixed vector-scalar with promotion
            (NetworkResult::Vec2(a), NetworkResult::Int(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x, b as f64), float_op(a.y, b as f64)))
            }
            (NetworkResult::Int(a), NetworkResult::Vec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a as f64, b.x), float_op(a as f64, b.y)))
            }
            (NetworkResult::Vec3(a), NetworkResult::Int(b)) => {
                NetworkResult::Vec3(DVec3::new(float_op(a.x, b as f64), float_op(a.y, b as f64), float_op(a.z, b as f64)))
            }
            (NetworkResult::Int(a), NetworkResult::Vec3(b)) => {
                NetworkResult::Vec3(DVec3::new(float_op(a as f64, b.x), float_op(a as f64, b.y), float_op(a as f64, b.z)))
            }
            (NetworkResult::IVec2(a), NetworkResult::Float(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a.x as f64, b), float_op(a.y as f64, b)))
            }
            (NetworkResult::Float(a), NetworkResult::IVec2(b)) => {
                NetworkResult::Vec2(DVec2::new(float_op(a, b.x as f64), float_op(a, b.y as f64)))
            }
            (NetworkResult::IVec3(a), NetworkResult::Float(b)) => {
                NetworkResult::Vec3(DVec3::new(float_op(a.x as f64, b), float_op(a.y as f64, b), float_op(a.z as f64, b)))
            }
            (NetworkResult::Float(a), NetworkResult::IVec3(b)) => {
                NetworkResult::Vec3(DVec3::new(float_op(a, b.x as f64), float_op(a, b.y as f64), float_op(a, b.z as f64)))
            }
            
            _ => NetworkResult::Error("Arithmetic operation not supported for these types".to_string())
        }
    }
    
    /// Helper for comparison operations
    fn comparison_op<F1, F2>(left: NetworkResult, right: NetworkResult, int_op: F1, float_op: F2) -> NetworkResult
    where
        F1: FnOnce(i32, i32) -> bool,
        F2: FnOnce(f64, f64) -> bool,
    {
        
        match (left, right) {
            (NetworkResult::Int(a), NetworkResult::Int(b)) => NetworkResult::Bool(int_op(a, b)),
            (NetworkResult::Float(a), NetworkResult::Float(b)) => NetworkResult::Bool(float_op(a, b)),
            (NetworkResult::Int(a), NetworkResult::Float(b)) => NetworkResult::Bool(float_op(a as f64, b)),
            (NetworkResult::Float(a), NetworkResult::Int(b)) => NetworkResult::Bool(float_op(a, b as f64)),
            (NetworkResult::Bool(a), NetworkResult::Bool(b)) => NetworkResult::Bool(int_op(a as i32, b as i32)),
            _ => NetworkResult::Error("Comparison operation requires compatible types".to_string())
        }
    }
    
    /// Helper for logical operations
    fn logical_op<F>(left: NetworkResult, right: NetworkResult, op: F) -> NetworkResult
    where
        F: FnOnce(bool, bool) -> bool,
    {
        
        let left_bool = match left {
            NetworkResult::Bool(b) => b,
            NetworkResult::Int(n) => n != 0,
            _ => return NetworkResult::Error("Logical operation requires boolean or int types".to_string())
        };
        
        let right_bool = match right {
            NetworkResult::Bool(b) => b,
            NetworkResult::Int(n) => n != 0,
            _ => return NetworkResult::Error("Logical operation requires boolean or int types".to_string())
        };
        
        NetworkResult::Bool(op(left_bool, right_bool))
    }

    /// Convert the expression to prefix notation string representation
    pub fn to_prefix_string(&self) -> String {
        match self {
            Expr::Int(n) => n.to_string(),
            Expr::Float(n) => n.to_string(),
            Expr::Bool(b) => b.to_string(),
            Expr::Var(name) => name.clone(),
            Expr::Unary(op, expr) => {
                let op_str = match op {
                    UnOp::Neg => "neg",
                    UnOp::Pos => "pos",
                    UnOp::Not => "not",
                };
                format!("({} {})", op_str, expr.to_prefix_string())
            }
            Expr::Binary(left, op, right) => {
                let op_str = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::Pow => "^",
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                format!("({} {} {})", op_str, left.to_prefix_string(), right.to_prefix_string())
            }
            Expr::Call(name, args) => {
                let args_str = args.iter()
                    .map(|arg| arg.to_prefix_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                if args.is_empty() {
                    format!("(call {})", name)
                } else {
                    format!("(call {} {})", name, args_str)
                }
            }
            Expr::Conditional(condition, then_expr, else_expr) => {
                format!("(if {} then {} else {})", 
                    condition.to_prefix_string(), 
                    then_expr.to_prefix_string(), 
                    else_expr.to_prefix_string())
            }
            Expr::MemberAccess(expr, member) => {
                format!("(. {} {})", expr.to_prefix_string(), member)
            }
        }
    }
}





