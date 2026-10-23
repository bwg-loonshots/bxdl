//! Executor JSON allows explicitly nullable response fields, unlike manifests.
//! Typed response DTOs decide where null is legal. Duplicates remain forbidden.
use serde::de::{self, DeserializeOwned, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

// Never propagate serde diagnostics: a malformed key may contain a secret.
pub(super) fn decode<T: DeserializeOwned>(raw: &[u8]) -> Result<T, ()> {
    if raw.is_empty() || raw.len() > 2 * 1024 * 1024 {
        return Err(());
    }
    let mut reader = serde_json::Deserializer::from_slice(raw);
    let value = Seed(0).deserialize(&mut reader).map_err(|_| ())?;
    reader.end().map_err(|_| ())?;
    serde_json::from_value(value).map_err(|_| ())
}
struct Seed(usize);
impl<'de> DeserializeSeed<'de> for Seed {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, reader: D) -> Result<Value, D::Error> {
        if self.0 > 16 {
            return Err(de::Error::custom("depth"));
        }
        reader.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("strict JSON")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| de::Error::custom("number"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.into()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(v) = seq.next_element_seed(Seed(self.0 + 1))? {
            if values.len() == 512 {
                return Err(de::Error::custom("array limit"));
            }
            values.push(v);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.len() == 128 {
                return Err(de::Error::custom("object limit"));
            }
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate"));
            }
            values.insert(key, map.next_value_seed(Seed(self.0 + 1))?);
        }
        Ok(Value::Object(values))
    }
}
