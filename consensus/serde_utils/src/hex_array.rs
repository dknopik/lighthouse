//! Module that can be directly used with serde's with for all arrays, as an alternative for the
//! macro based [`crate::fixed_bytes_hex`] module.

use serde::{Deserializer, Serializer};
use serde::de::Error;
use crate::hex::PrefixedHexVisitor;

pub fn serialize<S, const LEN: usize>(bytes: &[u8; LEN], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
{
    let mut hex_string: String = "0x".to_string();
    hex_string.push_str(&hex::encode(&bytes));

    serializer.serialize_str(&hex_string)
}

pub fn deserialize<'de, D, const LEN: usize>(deserializer: D) -> Result<[u8; LEN], D::Error>
    where
        D: Deserializer<'de>,
{
    let decoded = deserializer.deserialize_str(PrefixedHexVisitor)?;

    if decoded.len() != LEN {
        return Err(D::Error::custom(format!(
            "expected {} bytes for array, got {}",
            LEN,
            decoded.len()
        )));
    }

    let mut array = [0; LEN];
    // maybe serialize into a array directly instead
    array.copy_from_slice(&decoded);
    Ok(array)
}