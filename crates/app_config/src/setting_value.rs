use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone)]
pub struct SettingRecord {
    pub group_name: String,
    pub title: String,
    pub name: String,
    pub value: String,
    pub value_type: String,
    pub visible: bool,
    pub description: String,
    pub default_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SettingValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
    // Add more types as needed
}

impl SettingValue {
    pub fn type_as_string(&self) -> &str {
        match self {
            SettingValue::Bool(_) => "bool",
            SettingValue::Int(_) => "int",
            SettingValue::Float(_) => "float",
            SettingValue::String(_) => "string",
        }
    }
}

impl fmt::Display for SettingValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingValue::Bool(value) => write!(f, "{}", value),
            SettingValue::Int(value) => write!(f, "{}", value),
            SettingValue::Float(value) => write!(f, "{}", value),
            SettingValue::String(value) => write!(f, "{}", value),
        }
    }
}
