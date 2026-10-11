//! Required floating-point fields must serialize as numbers, never JSON null.
//! Optional native properties retain their existing number-or-null contract.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug)]
pub struct FiniteF64(f64);

impl FiniteF64 {
    pub fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for FiniteF64 {
    type Error = &'static str;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err("Expected a finite JSON number")
        }
    }
}

impl<'de> Deserialize<'de> for FiniteF64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(f64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl specta::Type for FiniteF64 {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <specta_typescript::Number as specta::Type>::definition(types)
    }
}

pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    FiniteF64::try_from(*value).map_err(serde::ser::Error::custom)?;
    value.serialize(serializer)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    FiniteF64::deserialize(deserializer).map(FiniteF64::get)
}

pub fn serialize_cuts<S: Serializer>(values: &[f64], serializer: S) -> Result<S::Ok, S::Error> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(serde::ser::Error::custom("Non-finite scene timestamp"));
    }
    values.serialize(serializer)
}

pub fn serialize_peaks<S: Serializer>(
    values: &[[f32; 2]],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if values.iter().flatten().any(|value| !value.is_finite()) {
        return Err(serde::ser::Error::custom("Non-finite waveform peak"));
    }
    values.serialize(serializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_numbers_reject_nonfinite_values_instead_of_serializing_null() {
        #[derive(Serialize, Deserialize)]
        struct Sample {
            #[serde(with = "super")]
            value: f64,
        }
        for value in [0., -0.125, f64::MAX] {
            let json = serde_json::to_string(&Sample { value }).unwrap();
            assert_eq!(serde_json::from_str::<Sample>(&json).unwrap().value, value);
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(serde_json::to_string(&Sample { value }).is_err());
        }
        for value in ["null", "1e999", "\"1\""] {
            assert!(serde_json::from_str::<FiniteF64>(value).is_err());
        }
    }
}
