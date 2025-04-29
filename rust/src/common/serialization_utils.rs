use serde::{Serialize, Serializer, Deserialize, Deserializer};
use glam::i32::IVec3;
use glam::f64::{DVec3, DVec2};

/// Module to handle serialization of IVec3 type
pub mod ivec3_serializer {
    use super::*;

    pub fn serialize<S>(vec: &IVec3, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize IVec3 as an array of 3 i32 values
        (vec.x, vec.y, vec.z).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<IVec3, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize from an array of 3 i32 values
        let (x, y, z) = <(i32, i32, i32)>::deserialize(deserializer)?;
        Ok(IVec3::new(x, y, z))
    }
}

/// Module to handle serialization of DVec3 type
pub mod dvec3_serializer {
    use super::*;

    pub fn serialize<S>(vec: &DVec3, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize DVec3 as an array of 3 f64 values
        (vec.x, vec.y, vec.z).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DVec3, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize from an array of 3 f64 values
        let (x, y, z) = <(f64, f64, f64)>::deserialize(deserializer)?;
        Ok(DVec3::new(x, y, z))
    }
}

/// Module to handle serialization of DVec2 type
pub mod dvec2_serializer {
    use super::*;

    pub fn serialize<S>(vec: &DVec2, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize DVec2 as an array of 2 f64 values
        (vec.x, vec.y).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DVec2, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize from an array of 2 f64 values
        let (x, y) = <(f64, f64)>::deserialize(deserializer)?;
        Ok(DVec2::new(x, y))
    }
}
