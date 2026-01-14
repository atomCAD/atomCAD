use glam::{IVec2, IVec3, DVec2, DVec3};
use crate::structure_designer::data_type::DataType;

/// Represents a value in the node network text format.
/// Used for both serialization (query) and deserialization (edit).
#[derive(Debug, Clone, PartialEq)]
pub enum TextValue {
    Bool(bool),
    Int(i32),
    Float(f64),
    String(String),
    IVec2(IVec2),
    IVec3(IVec3),
    Vec2(DVec2),
    Vec3(DVec3),
    DataType(DataType),
    Array(Vec<TextValue>),
    /// For complex nested structures like expr parameters
    Object(Vec<(String, TextValue)>),
}

impl TextValue {
    // ========== Helper accessor methods ==========

    /// Try to extract a bool value
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            TextValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to extract an i32 value
    pub fn as_int(&self) -> Option<i32> {
        match self {
            TextValue::Int(i) => Some(*i),
            // Allow float to int conversion (truncate)
            TextValue::Float(f) => Some(*f as i32),
            _ => None,
        }
    }

    /// Try to extract an f64 value
    pub fn as_float(&self) -> Option<f64> {
        match self {
            TextValue::Float(f) => Some(*f),
            // Allow int to float conversion
            TextValue::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Try to extract a String value
    pub fn as_string(&self) -> Option<&str> {
        match self {
            TextValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to extract an IVec2 value
    pub fn as_ivec2(&self) -> Option<IVec2> {
        match self {
            TextValue::IVec2(v) => Some(*v),
            // Allow Vec2 to IVec2 conversion (truncate)
            TextValue::Vec2(v) => Some(IVec2::new(v.x as i32, v.y as i32)),
            _ => None,
        }
    }

    /// Try to extract an IVec3 value
    pub fn as_ivec3(&self) -> Option<IVec3> {
        match self {
            TextValue::IVec3(v) => Some(*v),
            // Allow Vec3 to IVec3 conversion (truncate)
            TextValue::Vec3(v) => Some(IVec3::new(v.x as i32, v.y as i32, v.z as i32)),
            _ => None,
        }
    }

    /// Try to extract a DVec2 value
    pub fn as_vec2(&self) -> Option<DVec2> {
        match self {
            TextValue::Vec2(v) => Some(*v),
            // Allow IVec2 to Vec2 conversion
            TextValue::IVec2(v) => Some(DVec2::new(v.x as f64, v.y as f64)),
            _ => None,
        }
    }

    /// Try to extract a DVec3 value
    pub fn as_vec3(&self) -> Option<DVec3> {
        match self {
            TextValue::Vec3(v) => Some(*v),
            // Allow IVec3 to Vec3 conversion
            TextValue::IVec3(v) => Some(DVec3::new(v.x as f64, v.y as f64, v.z as f64)),
            _ => None,
        }
    }

    /// Try to extract a DataType value
    pub fn as_data_type(&self) -> Option<&DataType> {
        match self {
            TextValue::DataType(dt) => Some(dt),
            _ => None,
        }
    }

    /// Try to extract an array of TextValues
    pub fn as_array(&self) -> Option<&Vec<TextValue>> {
        match self {
            TextValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Try to extract an object (list of key-value pairs)
    pub fn as_object(&self) -> Option<&Vec<(String, TextValue)>> {
        match self {
            TextValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    // ========== Conversion helpers ==========

    /// Create a TextValue from a boolean
    pub fn from_bool(value: bool) -> Self {
        TextValue::Bool(value)
    }

    /// Create a TextValue from an i32
    pub fn from_int(value: i32) -> Self {
        TextValue::Int(value)
    }

    /// Create a TextValue from an f64
    pub fn from_float(value: f64) -> Self {
        TextValue::Float(value)
    }

    /// Create a TextValue from a String
    pub fn from_string(value: String) -> Self {
        TextValue::String(value)
    }

    /// Create a TextValue from an IVec2
    pub fn from_ivec2(value: IVec2) -> Self {
        TextValue::IVec2(value)
    }

    /// Create a TextValue from an IVec3
    pub fn from_ivec3(value: IVec3) -> Self {
        TextValue::IVec3(value)
    }

    /// Create a TextValue from a DVec2
    pub fn from_vec2(value: DVec2) -> Self {
        TextValue::Vec2(value)
    }

    /// Create a TextValue from a DVec3
    pub fn from_vec3(value: DVec3) -> Self {
        TextValue::Vec3(value)
    }

    /// Create a TextValue from a DataType
    pub fn from_data_type(value: DataType) -> Self {
        TextValue::DataType(value)
    }

    // ========== Type checking helpers ==========

    /// Returns true if this is a numeric type (Int or Float)
    pub fn is_numeric(&self) -> bool {
        matches!(self, TextValue::Int(_) | TextValue::Float(_))
    }

    /// Returns true if this is a vector type
    pub fn is_vector(&self) -> bool {
        matches!(self, TextValue::IVec2(_) | TextValue::IVec3(_) | TextValue::Vec2(_) | TextValue::Vec3(_))
    }

    /// Returns the expected DataType for this TextValue
    pub fn inferred_data_type(&self) -> DataType {
        match self {
            TextValue::Bool(_) => DataType::Bool,
            TextValue::Int(_) => DataType::Int,
            TextValue::Float(_) => DataType::Float,
            TextValue::String(_) => DataType::String,
            TextValue::IVec2(_) => DataType::IVec2,
            TextValue::IVec3(_) => DataType::IVec3,
            TextValue::Vec2(_) => DataType::Vec2,
            TextValue::Vec3(_) => DataType::Vec3,
            TextValue::DataType(_) => DataType::None, // DataType itself is meta
            TextValue::Array(arr) => {
                if arr.is_empty() {
                    DataType::Array(Box::new(DataType::None))
                } else {
                    DataType::Array(Box::new(arr[0].inferred_data_type()))
                }
            }
            TextValue::Object(_) => DataType::None, // Objects are structural, no direct DataType
        }
    }
}
