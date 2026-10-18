//! JSON transports integers as JavaScript numbers. Reject lossy values at this
//! adapter boundary; domain times, IDs and counters remain ordinary Rust types.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod finite;

pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const ERROR: &str = "Integer exceeds JavaScript's exact nonnegative number range";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SafeU64(u64);

/// Signed offsets and mapped subtitle times must remain exact in JavaScript too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SafeI64(i64);
impl SafeI64 {
    pub fn get(self) -> i64 {
        self.0
    }
}
impl TryFrom<i64> for SafeI64 {
    type Error = &'static str;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value.unsigned_abs() > MAX_SAFE_INTEGER {
            Err("Integer exceeds JavaScript's exact signed number range")
        } else {
            Ok(Self(value))
        }
    }
}
impl Serialize for SafeI64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for SafeI64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(i64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
impl specta::Type for SafeI64 {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <specta_typescript::Number as specta::Type>::definition(types)
    }
}

impl SafeU64 {
    pub fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for SafeU64 {
    type Error = &'static str;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value > MAX_SAFE_INTEGER {
            Err(ERROR)
        } else {
            Ok(Self(value))
        }
    }
}

impl Serialize for SafeU64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SafeU64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(u64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl specta::Type for SafeU64 {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        // This is safe only because both Serde directions enforce the range.
        // There is deliberately no global wide-integer-to-number override.
        <specta_typescript::Number as specta::Type>::definition(types)
    }
}

// Field adapters let existing Rust DTO consumers retain u64 while enforcing the
// same policy as command arguments. Specta field overrides reference SafeU64.
pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    SafeU64::try_from(*value)
        .map_err(serde::ser::Error::custom)?
        .serialize(serializer)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    SafeU64::deserialize(deserializer).map(SafeU64::get)
}

pub fn serialize_usize<S: Serializer>(value: &usize, serializer: S) -> Result<S::Ok, S::Error> {
    let value = u64::try_from(*value).map_err(serde::ser::Error::custom)?;
    serialize(&value, serializer)
}

pub mod optional {
    use super::*;

    pub fn serialize<S: Serializer>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        value
            .map(SafeU64::try_from)
            .transpose()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<u64>, D::Error> {
        Option::<SafeU64>::deserialize(deserializer).map(|value| value.map(SafeU64::get))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_numbers_reject_lossy_extremes_and_fractional_offsets() {
        for value in [
            -(MAX_SAFE_INTEGER as i64),
            -1,
            0,
            1,
            MAX_SAFE_INTEGER as i64,
        ] {
            let encoded = serde_json::to_string(&SafeI64::try_from(value).unwrap()).unwrap();
            assert_eq!(
                serde_json::from_str::<SafeI64>(&encoded).unwrap().get(),
                value
            );
        }
        for json in [
            "-9007199254740992",
            "9007199254740992",
            "-9223372036854775808",
            "1.5",
            "null",
            "\"1\"",
        ] {
            assert!(serde_json::from_str::<SafeI64>(json).is_err());
        }
    }

    #[test]
    fn json_numbers_are_exact_and_lossy_or_noninteger_inputs_are_rejected() {
        for value in [0, 1, MAX_SAFE_INTEGER] {
            let json = value.to_string();
            let decoded: SafeU64 = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.get(), value);
            assert_eq!(serde_json::to_string(&decoded).unwrap(), json);
        }
        for json in [
            "9007199254740992",
            "18446744073709551615",
            "-1",
            "1.5",
            "null",
            "\"1\"",
        ] {
            assert!(serde_json::from_str::<SafeU64>(json).is_err(), "{json}");
        }
        assert!(SafeU64::try_from(MAX_SAFE_INTEGER + 1).is_err());
    }
}
