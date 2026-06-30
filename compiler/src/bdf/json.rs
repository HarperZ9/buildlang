// ===============================================================================
// BUILDLANG COMPILER - BDF PLAIN-JSON ADAPTER
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! A faithful, order-preserving bridge between *plain* JSON and [`BdfValue`].
//!
//! The [`BdfValue::to_json`](crate::bdf::BdfValue::to_json) projection is the
//! *tagged* canonical form (`{"t":"int","v":1}`). Host interop formats such as
//! the `project-telos.flagship-action/v1` envelope are *plain* JSON
//! (`{"priority":"normal"}`), so the flagship bridge needs a different adapter:
//! it must read and re-emit ordinary JSON while losing nothing.
//!
//! ## What "lossless" means here
//!
//! - **Object key order** is preserved. `serde_json::Value` would sort keys
//!   (it is `BTreeMap`-backed unless the `preserve_order` feature is on, which
//!   this crate does not enable), so we deserialize straight into a
//!   [`BdfValue::Map`] (an [`IndexMap`](indexmap::IndexMap)) whose visitor sees
//!   keys in document order.
//! - **The integer / float distinction** is preserved. A JSON integer becomes
//!   [`BdfValue::Int`]; a non-integer JSON number becomes [`BdfValue::Float`]
//!   (via the shared bit-exact float discipline). Re-emitting an `Int` writes a
//!   bare integer and a `Float` writes a JSON number, so a value read as an
//!   integer round-trips as an integer.
//!
//! Round-trip equality is judged as *semantic* JSON equality (two
//! `serde_json::Value`s compare equal), which is the standard meaning of
//! "canonical JSON equality" and is insensitive to incidental whitespace.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

use crate::bdf::{BdfError, BdfResult, BdfValue};

/// Parse *plain* JSON text into a [`BdfValue`], preserving object key order and
/// the integer / float distinction. Fails closed on malformed JSON.
pub fn plain_json_to_bdf(json: &str) -> BdfResult<BdfValue> {
    let mut de = serde_json::Deserializer::from_str(json);
    let value = PlainJson::deserialize(&mut de)
        .map(|p| p.0)
        .map_err(|e| BdfError::Json(e.to_string()))?;
    de.end().map_err(|e| BdfError::Json(e.to_string()))?;
    Ok(value)
}

/// Serialize a [`BdfValue`] as *plain* (untagged) JSON text.
///
/// [`BdfValue::Bytes`] has no plain-JSON counterpart; it is emitted as an array
/// of byte integers. Flagship-action payloads never carry `Bytes`, so that arm
/// is a totality fallback, not a round-trip path.
pub fn bdf_to_plain_json(value: &BdfValue) -> BdfResult<String> {
    let json = bdf_to_serde_value(value);
    serde_json::to_string(&json).map_err(|e| BdfError::Json(e.to_string()))
}

/// Serialize a [`BdfValue`] as pretty-printed *plain* JSON text.
pub fn bdf_to_plain_json_pretty(value: &BdfValue) -> BdfResult<String> {
    let json = bdf_to_serde_value(value);
    serde_json::to_string_pretty(&json).map_err(|e| BdfError::Json(e.to_string()))
}

/// Project a [`BdfValue`] into a plain `serde_json::Value` (untagged), so the
/// crate's standard JSON serializer can render it and so callers can compare
/// two payloads by semantic JSON equality.
pub fn bdf_to_serde_value(value: &BdfValue) -> serde_json::Value {
    use serde_json::Value as J;
    match value {
        BdfValue::Null => J::Null,
        BdfValue::Bool(b) => J::Bool(*b),
        BdfValue::Int(i) => J::Number((*i).into()),
        BdfValue::Float(f) => serde_json::Number::from_f64(*f).map_or(J::Null, J::Number),
        BdfValue::Str(s) => J::String(s.clone()),
        BdfValue::Bytes(bytes) => J::Array(bytes.iter().map(|b| J::Number((*b).into())).collect()),
        BdfValue::Array(items) => J::Array(items.iter().map(bdf_to_serde_value).collect()),
        BdfValue::Map(entries) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in entries {
                obj.insert(k.clone(), bdf_to_serde_value(v));
            }
            J::Object(obj)
        }
    }
}

/// A newtype carrying a [`BdfValue`] decoded directly from plain JSON so object
/// key order survives. The custom [`Visitor`] is what preserves order: it
/// collects map entries into the `BdfValue::Map`'s [`IndexMap`](indexmap::IndexMap)
/// in the order the deserializer yields them (document order).
struct PlainJson(BdfValue);

impl<'de> Deserialize<'de> for PlainJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer
            .deserialize_any(PlainJsonVisitor)
            .map(PlainJson)
    }
}

struct PlainJsonVisitor;

impl<'de> Visitor<'de> for PlainJsonVisitor {
    type Value = BdfValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(BdfValue::Null)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(BdfValue::Null)
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
        Ok(BdfValue::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
        Ok(BdfValue::Int(v))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        // JSON integers larger than i64::MAX cannot be carried as Int without
        // loss; fall back to the float representation, which is what the value
        // model offers. (Flagship-action envelopes do not use such integers.)
        i64::try_from(v).map_or_else(|_| Ok(BdfValue::Float(v as f64)), |i| Ok(BdfValue::Int(i)))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E> {
        Ok(BdfValue::Float(v))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
        Ok(BdfValue::Str(v.to_string()))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
        Ok(BdfValue::Str(v))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::new();
        while let Some(PlainJson(v)) = seq.next_element()? {
            items.push(v);
        }
        Ok(BdfValue::Array(items))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut entries = indexmap::IndexMap::new();
        while let Some((k, PlainJson(v))) = map.next_entry::<String, PlainJson>()? {
            entries.insert(k, v);
        }
        Ok(BdfValue::Map(entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip plain JSON -> BdfValue -> plain JSON is semantically identical.
    fn round_trips(json: &str) {
        let value = plain_json_to_bdf(json).expect("parse");
        let back = bdf_to_plain_json(&value).expect("serialize");
        let original: serde_json::Value = serde_json::from_str(json).expect("orig");
        let returned: serde_json::Value = serde_json::from_str(&back).expect("back");
        assert_eq!(original, returned, "plain-json round-trip mismatch: {json}");
    }

    #[test]
    fn scalars_round_trip() {
        round_trips("null");
        round_trips("true");
        round_trips("false");
        round_trips("0");
        round_trips("-42");
        round_trips("1.5");
        round_trips("\"hello\"");
        round_trips("\"café ☃ 日本語\"");
    }

    #[test]
    fn containers_round_trip() {
        round_trips("[]");
        round_trips("[1,2,3]");
        round_trips("{}");
        round_trips(r#"{"a":1,"b":[true,null,"x"],"c":{"d":2.5}}"#);
    }

    #[test]
    fn integer_and_float_distinction_is_preserved() {
        let v = plain_json_to_bdf("7").expect("int");
        assert_eq!(v, BdfValue::Int(7));
        let f = plain_json_to_bdf("7.5").expect("float");
        assert!(matches!(f, BdfValue::Float(x) if (x - 7.5).abs() < f64::EPSILON));
    }

    #[test]
    fn object_key_order_is_preserved() {
        // Keys that are NOT in sorted order; a BTreeMap-backed Value would sort
        // them, but the IndexMap-backed BdfValue must keep document order.
        let v = plain_json_to_bdf(r#"{"zeta":1,"alpha":2,"mu":3}"#).expect("parse");
        if let BdfValue::Map(m) = v {
            let keys: Vec<&str> = m.keys().map(String::as_str).collect();
            assert_eq!(keys, vec!["zeta", "alpha", "mu"]);
        } else {
            panic!("expected map");
        }
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(plain_json_to_bdf("{not json").is_err());
        assert!(plain_json_to_bdf("").is_err());
        assert!(plain_json_to_bdf("{} trailing").is_err());
    }
}
