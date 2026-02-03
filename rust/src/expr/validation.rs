use std::collections::HashMap;
use std::sync::OnceLock;
use crate::structure_designer::evaluator::network_result::NetworkResult;
use crate::structure_designer::data_type::DataType;

/// Function signature for validation and type checking
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub parameter_types: Vec<DataType>,
    pub return_type: DataType,
}

impl FunctionSignature {
    pub fn new(parameter_types: Vec<DataType>, return_type: DataType) -> Self {
        Self {
            parameter_types,
            return_type,
        }
    }
}


/// Type for evaluation functions - takes parameters and returns a result
pub type EvaluationFunction = Box<dyn Fn(&[NetworkResult]) -> NetworkResult + Send + Sync>;

/// Global registry for function signatures used in validation
static FUNCTION_SIGNATURES: OnceLock<HashMap<String, FunctionSignature>> = OnceLock::new();

/// Global registry for function implementations used in evaluation
static FUNCTION_IMPLEMENTATIONS: OnceLock<HashMap<String, EvaluationFunction>> = OnceLock::new();

/// Initialize the global function registries
pub fn init_function_registries() {
    FUNCTION_SIGNATURES.get_or_init(create_standard_function_signatures);
    FUNCTION_IMPLEMENTATIONS.get_or_init(|| create_standard_function_implementations());
}

/// Get reference to the global function signatures registry
pub fn get_function_signatures() -> &'static HashMap<String, FunctionSignature> {
    FUNCTION_SIGNATURES.get_or_init(create_standard_function_signatures)
}

/// Get reference to the global function implementations registry
pub fn get_function_implementations() -> &'static HashMap<String, EvaluationFunction> {
    FUNCTION_IMPLEMENTATIONS.get_or_init(|| create_standard_function_implementations())
}


/// Create the standard function signatures for validation
fn create_standard_function_signatures() -> HashMap<String, FunctionSignature> {
    let mut functions = HashMap::new();
    
    // Add standard math functions
    functions.insert("sin".to_string(), 
        FunctionSignature::new(vec![DataType::Float], DataType::Float));
    functions.insert("cos".to_string(), 
        FunctionSignature::new(vec![DataType::Float], DataType::Float));
    functions.insert("tan".to_string(), 
        FunctionSignature::new(vec![DataType::Float], DataType::Float));
    functions.insert("sqrt".to_string(), 
        FunctionSignature::new(vec![DataType::Float], DataType::Float));
    functions.insert("abs".to_string(), 
        FunctionSignature::new(vec![DataType::Float], DataType::Float));
    functions.insert("floor".to_string(), 
        FunctionSignature::new(vec![DataType::Float], DataType::Float));
    functions.insert("ceil".to_string(), 
        FunctionSignature::new(vec![DataType::Float], DataType::Float));
    functions.insert("round".to_string(), 
        FunctionSignature::new(vec![DataType::Float], DataType::Float));
    
    // Integer versions
    functions.insert("abs_int".to_string(), 
        FunctionSignature::new(vec![DataType::Int], DataType::Int));
    
    // Vector constructor functions
    functions.insert("vec2".to_string(), 
        FunctionSignature::new(vec![DataType::Float, DataType::Float], DataType::Vec2));
    functions.insert("vec3".to_string(), 
        FunctionSignature::new(vec![DataType::Float, DataType::Float, DataType::Float], DataType::Vec3));
    functions.insert("ivec2".to_string(), 
        FunctionSignature::new(vec![DataType::Int, DataType::Int], DataType::IVec2));
    functions.insert("ivec3".to_string(), 
        FunctionSignature::new(vec![DataType::Int, DataType::Int, DataType::Int], DataType::IVec3));
    
    // Vector math functions - using specific names for now to avoid overloading issues
    functions.insert("length2".to_string(), 
        FunctionSignature::new(vec![DataType::Vec2], DataType::Float));
    functions.insert("length3".to_string(), 
        FunctionSignature::new(vec![DataType::Vec3], DataType::Float));
    functions.insert("normalize2".to_string(), 
        FunctionSignature::new(vec![DataType::Vec2], DataType::Vec2));
    functions.insert("normalize3".to_string(), 
        FunctionSignature::new(vec![DataType::Vec3], DataType::Vec3));
    functions.insert("dot2".to_string(), 
        FunctionSignature::new(vec![DataType::Vec2, DataType::Vec2], DataType::Float));
    functions.insert("dot3".to_string(), 
        FunctionSignature::new(vec![DataType::Vec3, DataType::Vec3], DataType::Float));
    functions.insert("cross".to_string(), 
        FunctionSignature::new(vec![DataType::Vec3, DataType::Vec3], DataType::Vec3));
    functions.insert("distance2".to_string(), 
        FunctionSignature::new(vec![DataType::Vec2, DataType::Vec2], DataType::Float));
    functions.insert("distance3".to_string(), 
        FunctionSignature::new(vec![DataType::Vec3, DataType::Vec3], DataType::Float));
    
    // Integer vector math functions
    functions.insert("idot2".to_string(), 
        FunctionSignature::new(vec![DataType::IVec2, DataType::IVec2], DataType::Int));
    functions.insert("idot3".to_string(), 
        FunctionSignature::new(vec![DataType::IVec3, DataType::IVec3], DataType::Int));
    functions.insert("icross".to_string(), 
        FunctionSignature::new(vec![DataType::IVec3, DataType::IVec3], DataType::IVec3));
    
    functions
}

/// Create the standard function implementations for evaluation
fn create_standard_function_implementations() -> HashMap<String, EvaluationFunction> {
    let mut functions: HashMap<String, EvaluationFunction> = HashMap::new();
    
    // Add standard math functions
    functions.insert("sin".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("sin() requires exactly 1 argument".to_string());
        }
        match args[0].clone().extract_float() {
            Some(val) => NetworkResult::Float(val.sin()),
            None => NetworkResult::Error("sin() requires a float argument".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("cos".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("cos() requires exactly 1 argument".to_string());
        }
        match args[0].clone().extract_float() {
            Some(val) => NetworkResult::Float(val.cos()),
            None => NetworkResult::Error("cos() requires a float argument".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("tan".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("tan() requires exactly 1 argument".to_string());
        }
        match args[0].clone().extract_float() {
            Some(val) => NetworkResult::Float(val.tan()),
            None => NetworkResult::Error("tan() requires a float argument".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("sqrt".to_string(), Box::new(|args: &[NetworkResult]| {
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
    }) as EvaluationFunction);
    
    functions.insert("abs".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("abs() requires exactly 1 argument".to_string());
        }
        match args[0].clone().extract_float() {
            Some(val) => NetworkResult::Float(val.abs()),
            None => NetworkResult::Error("abs() requires a float argument".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("floor".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("floor() requires exactly 1 argument".to_string());
        }
        match args[0].clone().extract_float() {
            Some(val) => NetworkResult::Float(val.floor()),
            None => NetworkResult::Error("floor() requires a float argument".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("ceil".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("ceil() requires exactly 1 argument".to_string());
        }
        match args[0].clone().extract_float() {
            Some(val) => NetworkResult::Float(val.ceil()),
            None => NetworkResult::Error("ceil() requires a float argument".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("round".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("round() requires exactly 1 argument".to_string());
        }
        match args[0].clone().extract_float() {
            Some(val) => NetworkResult::Float(val.round()),
            None => NetworkResult::Error("round() requires a float argument".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("abs_int".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("abs_int() requires exactly 1 argument".to_string());
        }
        match args[0].clone().extract_int() {
            Some(val) => NetworkResult::Int(val.abs()),
            None => NetworkResult::Error("abs_int() requires an int argument".to_string()),
        }
    }) as EvaluationFunction);
    
    // Vector constructor functions
    functions.insert("vec2".to_string(), Box::new(|args: &[NetworkResult]| {
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
    }) as EvaluationFunction);
    
    functions.insert("vec3".to_string(), Box::new(|args: &[NetworkResult]| {
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
    }) as EvaluationFunction);
    
    functions.insert("ivec2".to_string(), Box::new(|args: &[NetworkResult]| {
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
    }) as EvaluationFunction);
    
    functions.insert("ivec3".to_string(), Box::new(|args: &[NetworkResult]| {
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
    }) as EvaluationFunction);
    
    // Vector math functions
    functions.insert("length2".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("length2() requires exactly 1 argument".to_string());
        }
        match &args[0] {
            NetworkResult::Vec2(vec) => NetworkResult::Float(vec.length()),
            _ => NetworkResult::Error("length2() requires a Vec2 argument".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("length3".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 1 {
            return NetworkResult::Error("length3() requires exactly 1 argument".to_string());
        }
        match &args[0] {
            NetworkResult::Vec3(vec) => NetworkResult::Float(vec.length()),
            _ => NetworkResult::Error("length3() requires a Vec3 argument".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("normalize2".to_string(), Box::new(|args: &[NetworkResult]| {
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
    }) as EvaluationFunction);
    
    functions.insert("normalize3".to_string(), Box::new(|args: &[NetworkResult]| {
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
    }) as EvaluationFunction);
    
    functions.insert("dot2".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 2 {
            return NetworkResult::Error("dot2() requires exactly 2 arguments".to_string());
        }
        match (&args[0], &args[1]) {
            (NetworkResult::Vec2(a), NetworkResult::Vec2(b)) => NetworkResult::Float(a.dot(*b)),
            _ => NetworkResult::Error("dot2() requires two Vec2 arguments".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("dot3".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 2 {
            return NetworkResult::Error("dot3() requires exactly 2 arguments".to_string());
        }
        match (&args[0], &args[1]) {
            (NetworkResult::Vec3(a), NetworkResult::Vec3(b)) => NetworkResult::Float(a.dot(*b)),
            _ => NetworkResult::Error("dot3() requires two Vec3 arguments".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("cross".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 2 {
            return NetworkResult::Error("cross() requires exactly 2 arguments".to_string());
        }
        match (&args[0], &args[1]) {
            (NetworkResult::Vec3(a), NetworkResult::Vec3(b)) => NetworkResult::Vec3(a.cross(*b)),
            _ => NetworkResult::Error("cross() requires two Vec3 arguments".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("distance2".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 2 {
            return NetworkResult::Error("distance2() requires exactly 2 arguments".to_string());
        }
        match (&args[0], &args[1]) {
            (NetworkResult::Vec2(a), NetworkResult::Vec2(b)) => NetworkResult::Float((*a - *b).length()),
            _ => NetworkResult::Error("distance2() requires two Vec2 arguments".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("distance3".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 2 {
            return NetworkResult::Error("distance3() requires exactly 2 arguments".to_string());
        }
        match (&args[0], &args[1]) {
            (NetworkResult::Vec3(a), NetworkResult::Vec3(b)) => NetworkResult::Float((*a - *b).length()),
            _ => NetworkResult::Error("distance3() requires two Vec3 arguments".to_string()),
        }
    }) as EvaluationFunction);
    
    // Integer vector math functions
    functions.insert("idot2".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 2 {
            return NetworkResult::Error("idot2() requires exactly 2 arguments".to_string());
        }
        match (&args[0], &args[1]) {
            (NetworkResult::IVec2(a), NetworkResult::IVec2(b)) => {
                // Dot product: a.x * b.x + a.y * b.y
                let result = a.x * b.x + a.y * b.y;
                NetworkResult::Int(result)
            },
            _ => NetworkResult::Error("idot2() requires two IVec2 arguments".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("idot3".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 2 {
            return NetworkResult::Error("idot3() requires exactly 2 arguments".to_string());
        }
        match (&args[0], &args[1]) {
            (NetworkResult::IVec3(a), NetworkResult::IVec3(b)) => {
                // Dot product: a.x * b.x + a.y * b.y + a.z * b.z
                let result = a.x * b.x + a.y * b.y + a.z * b.z;
                NetworkResult::Int(result)
            },
            _ => NetworkResult::Error("idot3() requires two IVec3 arguments".to_string()),
        }
    }) as EvaluationFunction);
    
    functions.insert("icross".to_string(), Box::new(|args: &[NetworkResult]| {
        if args.len() != 2 {
            return NetworkResult::Error("icross() requires exactly 2 arguments".to_string());
        }
        match (&args[0], &args[1]) {
            (NetworkResult::IVec3(a), NetworkResult::IVec3(b)) => {
                // Cross product: (a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)
                use glam::i32::IVec3;
                let result = IVec3::new(
                    a.y * b.z - a.z * b.y,
                    a.z * b.x - a.x * b.z,
                    a.x * b.y - a.y * b.x
                );
                NetworkResult::IVec3(result)
            },
            _ => NetworkResult::Error("icross() requires two IVec3 arguments".to_string()),
        }
    }) as EvaluationFunction);
    
    functions
}
















