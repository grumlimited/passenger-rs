pub mod completion;
pub mod responses;

/// This module is helpful in cases where raw json objects are serialized and deserialized as
///  strings such as `"{\"key\": \"value\"}"`. This might seem odd but it's actually how some
///  some providers such as OpenAI return function arguments (for some reason).
pub mod stringified_json {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &serde_json::Value, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = value.to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        serde_json::from_str(&s).map_err(serde::de::Error::custom)
    }
}
