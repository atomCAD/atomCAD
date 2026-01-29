use glam::{IVec2, IVec3, DVec2, DVec3};
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use serde::ser::SerializeMap;
use serde::de::{MapAccess, Visitor};
use std::fmt;
use crate::structure_designer::data_type::DataType;
use crate::structure_designer::evaluator::network_result::NetworkResult;

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

impl Serialize for TextValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        match self {
            TextValue::Bool(v) => {
                map.serialize_entry("type", "Bool")?;
                map.serialize_entry("value", v)?;
            }
            TextValue::Int(v) => {
                map.serialize_entry("type", "Int")?;
                map.serialize_entry("value", v)?;
            }
            TextValue::Float(v) => {
                map.serialize_entry("type", "Float")?;
                map.serialize_entry("value", v)?;
            }
            TextValue::String(v) => {
                map.serialize_entry("type", "String")?;
                map.serialize_entry("value", v)?;
            }
            TextValue::IVec2(v) => {
                map.serialize_entry("type", "IVec2")?;
                map.serialize_entry("value", &[v.x, v.y])?;
            }
            TextValue::IVec3(v) => {
                map.serialize_entry("type", "IVec3")?;
                map.serialize_entry("value", &[v.x, v.y, v.z])?;
            }
            TextValue::Vec2(v) => {
                map.serialize_entry("type", "Vec2")?;
                map.serialize_entry("value", &[v.x, v.y])?;
            }
            TextValue::Vec3(v) => {
                map.serialize_entry("type", "Vec3")?;
                map.serialize_entry("value", &[v.x, v.y, v.z])?;
            }
            TextValue::DataType(v) => {
                map.serialize_entry("type", "DataType")?;
                map.serialize_entry("value", v)?;
            }
            TextValue::Array(v) => {
                map.serialize_entry("type", "Array")?;
                map.serialize_entry("value", v)?;
            }
            TextValue::Object(v) => {
                map.serialize_entry("type", "Object")?;
                map.serialize_entry("value", v)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for TextValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TextValueVisitor;

        impl<'de> Visitor<'de> for TextValueVisitor {
            type Value = TextValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a TextValue object with 'type' and 'value' fields")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut type_str: Option<String> = None;
                let mut value: Option<serde_json::Value> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "type" => type_str = Some(map.next_value()?),
                        "value" => value = Some(map.next_value()?),
                        _ => { let _ = map.next_value::<serde_json::Value>()?; }
                    }
                }

                let type_str = type_str.ok_or_else(|| serde::de::Error::missing_field("type"))?;
                let value = value.ok_or_else(|| serde::de::Error::missing_field("value"))?;

                match type_str.as_str() {
                    "Bool" => {
                        let v = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::Bool(v))
                    }
                    "Int" => {
                        let v = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::Int(v))
                    }
                    "Float" => {
                        let v = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::Float(v))
                    }
                    "String" => {
                        let v = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::String(v))
                    }
                    "IVec2" => {
                        let arr: [i32; 2] = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::IVec2(IVec2::new(arr[0], arr[1])))
                    }
                    "IVec3" => {
                        let arr: [i32; 3] = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::IVec3(IVec3::new(arr[0], arr[1], arr[2])))
                    }
                    "Vec2" => {
                        let arr: [f64; 2] = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::Vec2(DVec2::new(arr[0], arr[1])))
                    }
                    "Vec3" => {
                        let arr: [f64; 3] = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::Vec3(DVec3::new(arr[0], arr[1], arr[2])))
                    }
                    "DataType" => {
                        let v = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::DataType(v))
                    }
                    "Array" => {
                        let v = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::Array(v))
                    }
                    "Object" => {
                        let v = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                        Ok(TextValue::Object(v))
                    }
                    _ => Err(serde::de::Error::unknown_variant(&type_str, &["Bool", "Int", "Float", "String", "IVec2", "IVec3", "Vec2", "Vec3", "DataType", "Array", "Object"]))
                }
            }
        }

        deserializer.deserialize_map(TextValueVisitor)
    }
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

    /// Convert this TextValue to a NetworkResult for the expected data type.
    ///
    /// Returns Some(NetworkResult) if the value can be converted to the expected type,
    /// None otherwise. Supports type coercion (e.g., Int to Float).
    pub fn to_network_result(&self, expected_type: &DataType) -> Option<NetworkResult> {
        match (self, expected_type) {
            (TextValue::Int(i), DataType::Int) => Some(NetworkResult::Int(*i)),
            (TextValue::Float(f), DataType::Float) => Some(NetworkResult::Float(*f)),
            (TextValue::Bool(b), DataType::Bool) => Some(NetworkResult::Bool(*b)),
            (TextValue::String(s), DataType::String) => Some(NetworkResult::String(s.clone())),
            (TextValue::Vec2(v), DataType::Vec2) => Some(NetworkResult::Vec2(*v)),
            (TextValue::Vec3(v), DataType::Vec3) => Some(NetworkResult::Vec3(*v)),
            (TextValue::IVec2(v), DataType::IVec2) => Some(NetworkResult::IVec2(*v)),
            (TextValue::IVec3(v), DataType::IVec3) => Some(NetworkResult::IVec3(*v)),
            // Type coercion: int to float
            (TextValue::Int(i), DataType::Float) => Some(NetworkResult::Float(*i as f64)),
            // Type coercion: float to int (truncate)
            (TextValue::Float(f), DataType::Int) => Some(NetworkResult::Int(*f as i32)),
            // Type coercion: IVec to Vec
            (TextValue::IVec2(v), DataType::Vec2) => Some(NetworkResult::Vec2(DVec2::new(v.x as f64, v.y as f64))),
            (TextValue::IVec3(v), DataType::Vec3) => Some(NetworkResult::Vec3(DVec3::new(v.x as f64, v.y as f64, v.z as f64))),
            // Type coercion: Vec to IVec (truncate)
            (TextValue::Vec2(v), DataType::IVec2) => Some(NetworkResult::IVec2(IVec2::new(v.x as i32, v.y as i32))),
            (TextValue::Vec3(v), DataType::IVec3) => Some(NetworkResult::IVec3(IVec3::new(v.x as i32, v.y as i32, v.z as i32))),
            _ => None,
        }
    }
}
