use super::types::{MAX_MANIFEST_BYTES, fail};
use crate::error::Result;
use serde::de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

/// Parse once with duplicate/null/depth checks, then use strict typed serde
/// fields. Only Manifest.files has a default, for inventory-less build specs.
pub fn decode_strict_json<T: DeserializeOwned>(data: &[u8]) -> Result<T> {
    if data.is_empty()
        || data.len() as u64 > MAX_MANIFEST_BYTES
        || std::str::from_utf8(data).is_err()
    {
        return Err(fail(
            "JSON_INVALID",
            "JSON size or encoding outside allowed limits",
        ));
    }
    let mut decoder = serde_json::Deserializer::from_slice(data);
    let value = Seed { depth: 0 }.deserialize(&mut decoder).map_err(|_| {
        fail(
            "JSON_INVALID",
            "JSON must have unique fields, nonnull values and bounded nesting",
        )
    })?;
    decoder
        .end()
        .map_err(|_| fail("JSON_INVALID", "additional JSON content"))?;
    serde_json::from_value(value).map_err(|_| {
        fail(
            "JSON_INVALID",
            "JSON required fields or types do not match schema",
        )
    })
}
struct Seed {
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Seed {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(
        self,
        decoder: D,
    ) -> std::result::Result<Value, D::Error> {
        if self.depth > 32 {
            return Err(de::Error::custom("depth"));
        }
        decoder.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded nonnull JSON")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| de::Error::custom("number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E: de::Error>(self) -> std::result::Result<Value, E> {
        Err(de::Error::custom("null"))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = seq.next_element_seed(Seed {
            depth: self.depth + 1,
        })? {
            out.push(v)
        }
        Ok(Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if out.contains_key(&key) {
                return Err(de::Error::custom("duplicate"));
            }
            out.insert(
                key,
                map.next_value_seed(Seed {
                    depth: self.depth + 1,
                })?,
            );
        }
        Ok(Value::Object(out))
    }
}
