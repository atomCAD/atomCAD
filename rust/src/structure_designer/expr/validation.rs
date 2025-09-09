use std::collections::HashMap;
use crate::api::structure_designer::structure_designer_api_types::APIDataType;
use crate::structure_designer::evaluator::network_result::NetworkResult;

/// Function signature for validation and type checking
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub parameter_types: Vec<APIDataType>,
    pub return_type: APIDataType,
}

impl FunctionSignature {
    pub fn new(parameter_types: Vec<APIDataType>, return_type: APIDataType) -> Self {
        Self {
            parameter_types,
            return_type,
        }
    }
}

/// Context for expression validation containing variable and function type information
pub struct ValidationContext {
    pub variables: HashMap<String, APIDataType>,
    pub functions: HashMap<String, FunctionSignature>,
}

impl ValidationContext {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    /// Creates a validation context with standard mathematical functions
    pub fn with_standard_functions() -> Self {
        let mut context = Self::new();
        
        // Add standard math functions
        context.functions.insert("sin".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float], APIDataType::Float));
        context.functions.insert("cos".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float], APIDataType::Float));
        context.functions.insert("tan".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float], APIDataType::Float));
        context.functions.insert("sqrt".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float], APIDataType::Float));
        context.functions.insert("abs".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float], APIDataType::Float));
        context.functions.insert("floor".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float], APIDataType::Float));
        context.functions.insert("ceil".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float], APIDataType::Float));
        context.functions.insert("round".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float], APIDataType::Float));
        
        // Integer versions
        context.functions.insert("abs_int".to_string(), 
            FunctionSignature::new(vec![APIDataType::Int], APIDataType::Int));
        
        context
    }

    pub fn add_variable(&mut self, name: String, data_type: APIDataType) {
        self.variables.insert(name, data_type);
    }

    pub fn add_function(&mut self, name: String, signature: FunctionSignature) {
        self.functions.insert(name, signature);
    }
}

/// Type for evaluation functions - takes parameters and returns a result
pub type EvaluationFunction = Box<dyn Fn(&[NetworkResult]) -> NetworkResult + Send + Sync>;

/// Context for expression evaluation containing variable values and function implementations
pub struct EvaluationContext {
    pub variables: HashMap<String, NetworkResult>,
    pub functions: HashMap<String, EvaluationFunction>,
}

impl EvaluationContext {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    /// Creates an evaluation context with standard mathematical functions
    pub fn with_standard_functions() -> Self {
        let mut context = Self::new();
        
        // Add standard math functions
        context.functions.insert("sin".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("sin() requires exactly 1 argument".to_string());
            }
            match args[0].clone().extract_float() {
                Some(val) => NetworkResult::Float(val.sin()),
                None => NetworkResult::Error("sin() requires a float argument".to_string()),
            }
        }));
        
        context.functions.insert("cos".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("cos() requires exactly 1 argument".to_string());
            }
            match args[0].clone().extract_float() {
                Some(val) => NetworkResult::Float(val.cos()),
                None => NetworkResult::Error("cos() requires a float argument".to_string()),
            }
        }));
        
        context.functions.insert("tan".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("tan() requires exactly 1 argument".to_string());
            }
            match args[0].clone().extract_float() {
                Some(val) => NetworkResult::Float(val.tan()),
                None => NetworkResult::Error("tan() requires a float argument".to_string()),
            }
        }));
        
        context.functions.insert("sqrt".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("sqrt() requires exactly 1 argument".to_string());
            }
            match args[0].clone().extract_float() {
                Some(val) => {
                    if val < 0.0 {
                        NetworkResult::Error("sqrt() of negative number".to_string())
                    } else {
                        NetworkResult::Float(val.sqrt())
                    }
                },
                None => NetworkResult::Error("sqrt() requires a float argument".to_string()),
            }
        }));
        
        context.functions.insert("abs".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("abs() requires exactly 1 argument".to_string());
            }
            match args[0].clone().extract_float() {
                Some(val) => NetworkResult::Float(val.abs()),
                None => NetworkResult::Error("abs() requires a float argument".to_string()),
            }
        }));
        
        context.functions.insert("floor".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("floor() requires exactly 1 argument".to_string());
            }
            match args[0].clone().extract_float() {
                Some(val) => NetworkResult::Float(val.floor()),
                None => NetworkResult::Error("floor() requires a float argument".to_string()),
            }
        }));
        
        context.functions.insert("ceil".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("ceil() requires exactly 1 argument".to_string());
            }
            match args[0].clone().extract_float() {
                Some(val) => NetworkResult::Float(val.ceil()),
                None => NetworkResult::Error("ceil() requires a float argument".to_string()),
            }
        }));
        
        context.functions.insert("round".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("round() requires exactly 1 argument".to_string());
            }
            match args[0].clone().extract_float() {
                Some(val) => NetworkResult::Float(val.round()),
                None => NetworkResult::Error("round() requires a float argument".to_string()),
            }
        }));
        
        context.functions.insert("abs_int".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("abs_int() requires exactly 1 argument".to_string());
            }
            match args[0].clone().extract_int() {
                Some(val) => NetworkResult::Int(val.abs()),
                None => NetworkResult::Error("abs_int() requires an int argument".to_string()),
            }
        }));
        
        context
    }

    pub fn add_variable(&mut self, name: String, value: NetworkResult) {
        self.variables.insert(name, value);
    }

    pub fn add_function(&mut self, name: String, func: EvaluationFunction) {
        self.functions.insert(name, func);
    }
}
