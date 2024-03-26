pub fn serialize_string(value: &str) -> String {
    value.to_owned()
}

pub fn deserialize_string(value: &str) -> String {
    value.to_owned()
}

pub fn serialize_bool(value: bool) -> String {
    value.to_string()
}

pub fn deserialize_bool(value: &str) -> Result<bool, std::str::ParseBoolError> {
    value.parse::<bool>()
}

pub fn serialize_i32(value: i32) -> String {
    value.to_string()
}

pub fn deserialize_i32(value: &str) -> Result<i32, std::num::ParseIntError> {
    value.parse::<i32>()
}

pub fn serialize_u32(value: u32) -> String {
    value.to_string()
}

pub fn deserialize_u32(value: &str) -> Result<u32, std::num::ParseIntError> {
    value.parse::<u32>()
}

pub fn serialize_f32(value: f32) -> String {
    value.to_string()
}

pub fn deserialize_f32(value: &str) -> Result<f32, std::num::ParseFloatError> {
    value.parse::<f32>()
}