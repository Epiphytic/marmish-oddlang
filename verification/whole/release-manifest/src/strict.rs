//! Keep duplicate members visible while decoding consumer-provided JSON.
use serde::{
    Deserialize, Deserializer,
    de::{Error, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Number, Value};
use std::fmt;
pub struct Strict(pub Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(StrictVisitor)
    }
}
struct StrictVisitor;
impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Strict;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("JSON value with unique object members")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Strict, A::Error> {
        let mut obj = serde_json::Map::new();
        while let Some((k, Strict(v))) = map.next_entry::<String, Strict>()? {
            if obj.insert(k.clone(), v).is_some() {
                return Err(A::Error::custom(format!("duplicate JSON member {k:?}")));
            }
        }
        Ok(Strict(Value::Object(obj)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Strict, A::Error> {
        let mut values = Vec::new();
        while let Some(Strict(v)) = seq.next_element()? {
            values.push(v);
        }
        Ok(Strict(Value::Array(values)))
    }
    fn visit_str<E: Error>(self, v: &str) -> Result<Strict, E> {
        Ok(Strict(v.into()))
    }
    fn visit_unit<E: Error>(self) -> Result<Strict, E> {
        Ok(Strict(Value::Null))
    }
    fn visit_bool<E: Error>(self, v: bool) -> Result<Strict, E> {
        Ok(Strict(v.into()))
    }
    fn visit_i64<E: Error>(self, v: i64) -> Result<Strict, E> {
        Ok(Strict(v.into()))
    }
    fn visit_u64<E: Error>(self, v: u64) -> Result<Strict, E> {
        Ok(Strict(v.into()))
    }
    fn visit_f64<E: Error>(self, v: f64) -> Result<Strict, E> {
        Ok(Strict(Value::Number(
            Number::from_f64(v).ok_or_else(|| E::custom("nonfinite number"))?,
        )))
    }
}
