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
        
        // Vector constructor functions
        context.functions.insert("vec2".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float, APIDataType::Float], APIDataType::Vec2));
        context.functions.insert("vec3".to_string(), 
            FunctionSignature::new(vec![APIDataType::Float, APIDataType::Float, APIDataType::Float], APIDataType::Vec3));
        context.functions.insert("ivec2".to_string(), 
            FunctionSignature::new(vec![APIDataType::Int, APIDataType::Int], APIDataType::IVec2));
        context.functions.insert("ivec3".to_string(), 
            FunctionSignature::new(vec![APIDataType::Int, APIDataType::Int, APIDataType::Int], APIDataType::IVec3));
        
        // Vector math functions - using specific names for now to avoid overloading issues
        context.functions.insert("length2".to_string(), 
            FunctionSignature::new(vec![APIDataType::Vec2], APIDataType::Float));
        context.functions.insert("length3".to_string(), 
            FunctionSignature::new(vec![APIDataType::Vec3], APIDataType::Float));
        context.functions.insert("normalize2".to_string(), 
            FunctionSignature::new(vec![APIDataType::Vec2], APIDataType::Vec2));
        context.functions.insert("normalize3".to_string(), 
            FunctionSignature::new(vec![APIDataType::Vec3], APIDataType::Vec3));
        context.functions.insert("dot2".to_string(), 
            FunctionSignature::new(vec![APIDataType::Vec2, APIDataType::Vec2], APIDataType::Float));
        context.functions.insert("dot3".to_string(), 
            FunctionSignature::new(vec![APIDataType::Vec3, APIDataType::Vec3], APIDataType::Float));
        context.functions.insert("cross".to_string(), 
            FunctionSignature::new(vec![APIDataType::Vec3, APIDataType::Vec3], APIDataType::Vec3));
        context.functions.insert("distance2".to_string(), 
            FunctionSignature::new(vec![APIDataType::Vec2, APIDataType::Vec2], APIDataType::Float));
        context.functions.insert("distance3".to_string(), 
            FunctionSignature::new(vec![APIDataType::Vec3, APIDataType::Vec3], APIDataType::Float));
        
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
        
        // Vector constructor functions
        context.functions.insert("vec2".to_string(), Box::new(|args| {
            if args.len() != 2 {
                return NetworkResult::Error("vec2() requires exactly 2 arguments".to_string());
            }
            let x = match args[0].clone() {
                NetworkResult::Float(val) => val,
                NetworkResult::Int(val) => val as f64,
                _ => return NetworkResult::Error("vec2() requires numeric arguments".to_string()),
            };
            let y = match args[1].clone() {
                NetworkResult::Float(val) => val,
                NetworkResult::Int(val) => val as f64,
                _ => return NetworkResult::Error("vec2() requires numeric arguments".to_string()),
            };
            NetworkResult::Vec2(glam::f64::DVec2::new(x, y))
        }));
        
        context.functions.insert("vec3".to_string(), Box::new(|args| {
            if args.len() != 3 {
                return NetworkResult::Error("vec3() requires exactly 3 arguments".to_string());
            }
            let x = match args[0].clone() {
                NetworkResult::Float(val) => val,
                NetworkResult::Int(val) => val as f64,
                _ => return NetworkResult::Error("vec3() requires numeric arguments".to_string()),
            };
            let y = match args[1].clone() {
                NetworkResult::Float(val) => val,
                NetworkResult::Int(val) => val as f64,
                _ => return NetworkResult::Error("vec3() requires numeric arguments".to_string()),
            };
            let z = match args[2].clone() {
                NetworkResult::Float(val) => val,
                NetworkResult::Int(val) => val as f64,
                _ => return NetworkResult::Error("vec3() requires numeric arguments".to_string()),
            };
            NetworkResult::Vec3(glam::f64::DVec3::new(x, y, z))
        }));
        
        context.functions.insert("ivec2".to_string(), Box::new(|args| {
            if args.len() != 2 {
                return NetworkResult::Error("ivec2() requires exactly 2 arguments".to_string());
            }
            let x = match args[0].clone() {
                NetworkResult::Int(val) => val,
                NetworkResult::Float(val) => val.round() as i32,
                _ => return NetworkResult::Error("ivec2() requires numeric arguments".to_string()),
            };
            let y = match args[1].clone() {
                NetworkResult::Int(val) => val,
                NetworkResult::Float(val) => val.round() as i32,
                _ => return NetworkResult::Error("ivec2() requires numeric arguments".to_string()),
            };
            NetworkResult::IVec2(glam::i32::IVec2::new(x, y))
        }));
        
        context.functions.insert("ivec3".to_string(), Box::new(|args| {
            if args.len() != 3 {
                return NetworkResult::Error("ivec3() requires exactly 3 arguments".to_string());
            }
            let x = match args[0].clone() {
                NetworkResult::Int(val) => val,
                NetworkResult::Float(val) => val.round() as i32,
                _ => return NetworkResult::Error("ivec3() requires numeric arguments".to_string()),
            };
            let y = match args[1].clone() {
                NetworkResult::Int(val) => val,
                NetworkResult::Float(val) => val.round() as i32,
                _ => return NetworkResult::Error("ivec3() requires numeric arguments".to_string()),
            };
            let z = match args[2].clone() {
                NetworkResult::Int(val) => val,
                NetworkResult::Float(val) => val.round() as i32,
                _ => return NetworkResult::Error("ivec3() requires numeric arguments".to_string()),
            };
            NetworkResult::IVec3(glam::i32::IVec3::new(x, y, z))
        }));
        
        // Vector math functions
        context.functions.insert("length2".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("length2() requires exactly 1 argument".to_string());
            }
            match &args[0] {
                NetworkResult::Vec2(vec) => NetworkResult::Float(vec.length()),
                _ => NetworkResult::Error("length2() requires a Vec2 argument".to_string()),
            }
        }));
        
        context.functions.insert("length3".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("length3() requires exactly 1 argument".to_string());
            }
            match &args[0] {
                NetworkResult::Vec3(vec) => NetworkResult::Float(vec.length()),
                _ => NetworkResult::Error("length3() requires a Vec3 argument".to_string()),
            }
        }));
        
        context.functions.insert("normalize2".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("normalize2() requires exactly 1 argument".to_string());
            }
            match &args[0] {
                NetworkResult::Vec2(vec) => {
                    let length = vec.length();
                    if length == 0.0 {
                        NetworkResult::Error("Cannot normalize zero-length vector".to_string())
                    } else {
                        NetworkResult::Vec2(*vec / length)
                    }
                },
                _ => NetworkResult::Error("normalize2() requires a Vec2 argument".to_string()),
            }
        }));
        
        context.functions.insert("normalize3".to_string(), Box::new(|args| {
            if args.len() != 1 {
                return NetworkResult::Error("normalize3() requires exactly 1 argument".to_string());
            }
            match &args[0] {
                NetworkResult::Vec3(vec) => {
                    let length = vec.length();
                    if length == 0.0 {
                        NetworkResult::Error("Cannot normalize zero-length vector".to_string())
                    } else {
                        NetworkResult::Vec3(*vec / length)
                    }
                },
                _ => NetworkResult::Error("normalize3() requires a Vec3 argument".to_string()),
            }
        }));
        
        context.functions.insert("dot2".to_string(), Box::new(|args| {
            if args.len() != 2 {
                return NetworkResult::Error("dot2() requires exactly 2 arguments".to_string());
            }
            match (&args[0], &args[1]) {
                (NetworkResult::Vec2(a), NetworkResult::Vec2(b)) => NetworkResult::Float(a.dot(*b)),
                _ => NetworkResult::Error("dot2() requires two Vec2 arguments".to_string()),
            }
        }));
        
        context.functions.insert("dot3".to_string(), Box::new(|args| {
            if args.len() != 2 {
                return NetworkResult::Error("dot3() requires exactly 2 arguments".to_string());
            }
            match (&args[0], &args[1]) {
                (NetworkResult::Vec3(a), NetworkResult::Vec3(b)) => NetworkResult::Float(a.dot(*b)),
                _ => NetworkResult::Error("dot3() requires two Vec3 arguments".to_string()),
            }
        }));
        
        context.functions.insert("cross".to_string(), Box::new(|args| {
            if args.len() != 2 {
                return NetworkResult::Error("cross() requires exactly 2 arguments".to_string());
            }
            match (&args[0], &args[1]) {
                (NetworkResult::Vec3(a), NetworkResult::Vec3(b)) => NetworkResult::Vec3(a.cross(*b)),
                _ => NetworkResult::Error("cross() requires two Vec3 arguments".to_string()),
            }
        }));
        
        context.functions.insert("distance2".to_string(), Box::new(|args| {
            if args.len() != 2 {
                return NetworkResult::Error("distance2() requires exactly 2 arguments".to_string());
            }
            match (&args[0], &args[1]) {
                (NetworkResult::Vec2(a), NetworkResult::Vec2(b)) => NetworkResult::Float((*a - *b).length()),
                _ => NetworkResult::Error("distance2() requires two Vec2 arguments".to_string()),
            }
        }));
        
        context.functions.insert("distance3".to_string(), Box::new(|args| {
            if args.len() != 2 {
                return NetworkResult::Error("distance3() requires exactly 2 arguments".to_string());
            }
            match (&args[0], &args[1]) {
                (NetworkResult::Vec3(a), NetworkResult::Vec3(b)) => NetworkResult::Float((*a - *b).length()),
                _ => NetworkResult::Error("distance3() requires two Vec3 arguments".to_string()),
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
