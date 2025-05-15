use serde::{Serialize, Serializer, Deserialize, Deserializer};
use glam::i32::IVec3;
use glam::f64::{DVec3, DVec2, DQuat};

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

/// Module to handle serialization of DQuat type
pub mod dquat_serializer {
    use super::*;

    pub fn serialize<S>(quat: &DQuat, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize DQuat as an array of 4 f64 values (x, y, z, w)
        (quat.x, quat.y, quat.z, quat.w).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DQuat, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize from an array of 4 f64 values
        let (x, y, z, w) = <(f64, f64, f64, f64)>::deserialize(deserializer)?;
        Ok(DQuat::from_xyzw(x, y, z, w))
    }
}

/// Module to handle serialization of Option<IVec3> type
pub mod option_ivec3_serializer {
    use super::*;
    
    pub fn serialize<S>(option_vec: &Option<IVec3>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match option_vec {
            Some(vec) => ivec3_serializer::serialize(vec, serializer),
            None => serializer.serialize_none(),
        }
    }
    
    // Helper enum to handle multiple deserialization cases
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IVec3OrNull {
        Vec(#[serde(with = "ivec3_serializer")] IVec3),
        Null(Option<()>),
    }
    
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<IVec3>, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Use serde's untagged enum to handle both cases without moving the deserializer
        match IVec3OrNull::deserialize(deserializer)? {
            IVec3OrNull::Vec(vec) => Ok(Some(vec)),
            IVec3OrNull::Null(None) => Ok(None),
            _ => Err(serde::de::Error::custom("Expected IVec3 or null")),
        }
    }
}
