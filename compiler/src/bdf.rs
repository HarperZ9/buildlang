// ===============================================================================
// BUILDLANG COMPILER - BUILD DATA FORMAT (BDF) v0
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. BuildLang Fair-Source License v1.0 (see LICENSE).
// ===============================================================================

//! Build Data Format (BDF) v0 - the native, effect-typed interchange format.
//!
//! BDF is the data and information interface through which buildlang tools
//! function together. A value carries a typed shape, the canonical binary
//! encoding is deterministic and content-hashable, and the
//! [`BdfMessage`] envelope records the capability effects an action exercised
//! plus a re-checkable receipt.
//!
//! See `docs/superpowers/specs/2026-06-29-build-data-format-v0-design.md`.
//!
//! ## Two reconcilable forms
//!
//! - **Canonical binary**: [`to_bytes`] / [`from_bytes`]. Tagged fields with
//!   length prefixes, fixed-width little-endian scalars, and deterministic
//!   ordering, so the byte stream is stable and a content hash is meaningful.
//! - **Canonical JSON projection**: [`BdfValue::to_json`] / [`BdfValue::from_json`]
//!   for debugging and host interop during the JSON-to-BDF transition.
//!
//! Both forms round-trip losslessly, including `NaN`, the infinities, negative
//! zero, and subnormal floats (the IEEE-754 bit-pattern discipline shared with
//! the MIR interlingua via [`crate::serde_float`]).
//!
//! ## Fail-closed discipline
//!
//! Decoding rejects unknown tags, truncated input, trailing garbage, and
//! non-UTF-8 strings with a typed [`BdfError`]. The envelope rejects unknown
//! schema strings. Admission is never silently granted.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub mod flagship;
pub mod json;

pub use flagship::{
    bdf_to_flagship_action, bdf_to_flagship_action_pretty, flagship_action_to_bdf,
    FLAGSHIP_ACTION_SCHEMA,
};

/// Schema id for a standalone BDF value stream's binary header.
pub const BDF_VALUE_SCHEMA: &str = "buildlang.bdf/v0";

/// Schema id for the effect-typed [`BdfMessage`] envelope.
pub const BDF_MESSAGE_SCHEMA: &str = "buildlang.bdf-message/v0";

/// 4-byte magic header at the head of a canonical binary value stream.
///
/// `b"BDF0"`. A reader checks this before interpreting any tags so that a file
/// of an unrelated format fails closed instead of being misread.
pub const BDF_MAGIC: [u8; 4] = *b"BDF0";

// ============================================================================
// BINARY TAGS
// ============================================================================

/// Tag byte for [`BdfValue::Null`].
const TAG_NULL: u8 = 0x00;
/// Tag byte for [`BdfValue::Bool`].
const TAG_BOOL: u8 = 0x01;
/// Tag byte for [`BdfValue::Int`].
const TAG_INT: u8 = 0x02;
/// Tag byte for [`BdfValue::Float`].
const TAG_FLOAT: u8 = 0x03;
/// Tag byte for [`BdfValue::Str`].
const TAG_STR: u8 = 0x04;
/// Tag byte for [`BdfValue::Bytes`].
const TAG_BYTES: u8 = 0x05;
/// Tag byte for [`BdfValue::Array`].
const TAG_ARRAY: u8 = 0x06;
/// Tag byte for [`BdfValue::Map`].
const TAG_MAP: u8 = 0x07;

// ============================================================================
// ERRORS
// ============================================================================

/// A typed BDF decode/validate error. Decoding fails closed: any malformed,
/// truncated, or unknown input yields one of these rather than a partial or
/// silently-coerced value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BdfError {
    /// The binary stream did not begin with [`BDF_MAGIC`].
    #[error("bad BDF magic: expected {expected:02x?}, found {found:02x?}")]
    BadMagic {
        /// The expected magic bytes.
        expected: [u8; 4],
        /// The bytes actually found (may be fewer than 4 if truncated).
        found: Vec<u8>,
    },

    /// The input ended before a complete value could be read.
    #[error("truncated BDF input: needed {needed} more byte(s) at offset {offset}")]
    Truncated {
        /// Offset into the stream where the read ran out of bytes.
        offset: usize,
        /// How many more bytes were required.
        needed: usize,
    },

    /// A value tag byte was not one of the known tags.
    #[error("unknown BDF tag 0x{tag:02x} at offset {offset}")]
    UnknownTag {
        /// The unknown tag byte.
        tag: u8,
        /// Offset into the stream where the tag was read.
        offset: usize,
    },

    /// A boolean payload byte was neither 0 nor 1.
    #[error("invalid BDF bool byte 0x{byte:02x} at offset {offset}")]
    InvalidBool {
        /// The invalid byte.
        byte: u8,
        /// Offset into the stream where the byte was read.
        offset: usize,
    },

    /// A string field did not hold valid UTF-8.
    #[error("invalid UTF-8 in BDF string at offset {offset}")]
    InvalidUtf8 {
        /// Offset into the stream where the string started.
        offset: usize,
    },

    /// A length prefix exceeded the remaining input (defends against a hostile
    /// length that would otherwise demand a huge allocation).
    #[error("BDF length prefix {len} exceeds {remaining} remaining byte(s) at offset {offset}")]
    LengthOverflow {
        /// The declared length.
        len: u64,
        /// Bytes actually remaining in the stream.
        remaining: usize,
        /// Offset into the stream where the length prefix was read.
        offset: usize,
    },

    /// A complete value was decoded but bytes remained after it.
    #[error("trailing bytes after BDF value: {count} byte(s) left at offset {offset}")]
    TrailingBytes {
        /// Offset of the first trailing byte.
        offset: usize,
        /// Number of trailing bytes.
        count: usize,
    },

    /// The JSON projection could not be parsed or serialized.
    #[error("BDF JSON error: {0}")]
    Json(String),

    /// An envelope carried a schema string this codec does not understand.
    #[error("unsupported BDF schema '{found}', expected '{expected}'")]
    UnsupportedSchema {
        /// The schema string that was found.
        found: String,
        /// The schema string that was expected.
        expected: &'static str,
    },
}

/// Result type for BDF codec operations.
pub type BdfResult<T> = Result<T, BdfError>;

// ============================================================================
// VALUE MODEL
// ============================================================================

/// A BDF value: the self-describing data model BDF encodes.
///
/// `Map` preserves key insertion order (an [`IndexMap`]); that order is the
/// canonical order for v0, so encoding the same value always yields the same
/// bytes and the same content hash.
///
/// `Float` uses the IEEE-754 bit-pattern discipline (see [`crate::serde_float`])
/// in the JSON projection so non-finite and signed-zero floats round-trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v", rename_all = "snake_case")]
pub enum BdfValue {
    /// The null / absent value.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed 64-bit integer.
    Int(i64),
    /// A 64-bit float, serialized bit-exact via [`crate::serde_float`].
    Float(#[serde(with = "crate::serde_float")] f64),
    /// A UTF-8 string.
    Str(String),
    /// An opaque byte string.
    Bytes(Vec<u8>),
    /// An ordered sequence of values.
    Array(Vec<BdfValue>),
    /// An ordered string-keyed map of values.
    Map(IndexMap<String, BdfValue>),
}

impl BdfValue {
    /// Build an ordered map from `(key, value)` pairs, preserving order.
    pub fn map_from<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = (String, BdfValue)>,
    {
        BdfValue::Map(entries.into_iter().collect())
    }

    /// Encode to the canonical binary form (with the [`BDF_MAGIC`] header).
    pub fn to_bytes(&self) -> Vec<u8> {
        to_bytes(self)
    }

    /// Decode from the canonical binary form.
    pub fn from_bytes(bytes: &[u8]) -> BdfResult<Self> {
        from_bytes(bytes)
    }

    /// Serialize to compact canonical JSON.
    pub fn to_json(&self) -> BdfResult<String> {
        serde_json::to_string(self).map_err(|e| BdfError::Json(e.to_string()))
    }

    /// Serialize to pretty-printed canonical JSON.
    pub fn to_json_pretty(&self) -> BdfResult<String> {
        serde_json::to_string_pretty(self).map_err(|e| BdfError::Json(e.to_string()))
    }

    /// Parse from the canonical JSON projection.
    pub fn from_json(json: &str) -> BdfResult<Self> {
        serde_json::from_str(json).map_err(|e| BdfError::Json(e.to_string()))
    }
}

// ============================================================================
// CANONICAL BINARY ENCODER
// ============================================================================

/// Encode a [`BdfValue`] to the canonical binary form.
///
/// Layout: [`BDF_MAGIC`] (4 bytes) followed by one encoded value. Each value is
/// `tag (1 byte)` then a tag-specific payload:
///
/// - `Null`   -> tag only.
/// - `Bool`   -> tag, then `0x00` or `0x01`.
/// - `Int`    -> tag, then 8-byte little-endian two's-complement `i64`.
/// - `Float`  -> tag, then 8-byte little-endian IEEE-754 bit pattern.
/// - `Str`    -> tag, then `u64` LE byte length, then UTF-8 bytes.
/// - `Bytes`  -> tag, then `u64` LE byte length, then raw bytes.
/// - `Array`  -> tag, then `u64` LE element count, then each element encoded.
/// - `Map`    -> tag, then `u64` LE entry count, then for each entry: `u64` LE
///   key length, key UTF-8 bytes, then the value encoded. Entries are emitted
///   in the map's iteration (insertion) order, which is the canonical order.
pub fn to_bytes(value: &BdfValue) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&BDF_MAGIC);
    encode_value(value, &mut out);
    out
}

fn encode_value(value: &BdfValue, out: &mut Vec<u8>) {
    match value {
        BdfValue::Null => out.push(TAG_NULL),
        BdfValue::Bool(b) => {
            out.push(TAG_BOOL);
            out.push(u8::from(*b));
        }
        BdfValue::Int(i) => {
            out.push(TAG_INT);
            out.extend_from_slice(&i.to_le_bytes());
        }
        BdfValue::Float(f) => {
            out.push(TAG_FLOAT);
            out.extend_from_slice(&f.to_bits().to_le_bytes());
        }
        BdfValue::Str(s) => {
            out.push(TAG_STR);
            encode_bytes_payload(s.as_bytes(), out);
        }
        BdfValue::Bytes(b) => {
            out.push(TAG_BYTES);
            encode_bytes_payload(b, out);
        }
        BdfValue::Array(items) => {
            out.push(TAG_ARRAY);
            out.extend_from_slice(&(items.len() as u64).to_le_bytes());
            for item in items {
                encode_value(item, out);
            }
        }
        BdfValue::Map(entries) => {
            out.push(TAG_MAP);
            out.extend_from_slice(&(entries.len() as u64).to_le_bytes());
            for (key, val) in entries {
                encode_bytes_payload(key.as_bytes(), out);
                encode_value(val, out);
            }
        }
    }
}

fn encode_bytes_payload(bytes: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}

// ============================================================================
// CANONICAL BINARY DECODER
// ============================================================================

/// Decode a [`BdfValue`] from the canonical binary form.
///
/// Fails closed on a bad magic header, an unknown tag, truncated input, a
/// length prefix that overruns the buffer, non-UTF-8 string contents, or any
/// trailing bytes after the single top-level value.
pub fn from_bytes(bytes: &[u8]) -> BdfResult<BdfValue> {
    let mut cursor = Cursor::new(bytes);
    // A stream too short to even hold the magic is a bad-magic failure, not a
    // mid-value truncation: read whatever prefix exists and report it.
    if cursor.remaining() < BDF_MAGIC.len() {
        return Err(BdfError::BadMagic {
            expected: BDF_MAGIC,
            found: bytes.to_vec(),
        });
    }
    let magic = cursor.take(BDF_MAGIC.len())?;
    if magic != BDF_MAGIC {
        return Err(BdfError::BadMagic {
            expected: BDF_MAGIC,
            found: magic.to_vec(),
        });
    }
    let value = decode_value(&mut cursor)?;
    if cursor.remaining() != 0 {
        return Err(BdfError::TrailingBytes {
            offset: cursor.pos,
            count: cursor.remaining(),
        });
    }
    Ok(value)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    /// Take exactly `n` bytes, advancing the cursor, or fail closed.
    fn take(&mut self, n: usize) -> BdfResult<&'a [u8]> {
        if self.remaining() < n {
            return Err(BdfError::Truncated {
                offset: self.pos,
                needed: n - self.remaining(),
            });
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn take_u8(&mut self) -> BdfResult<u8> {
        Ok(self.take(1)?[0])
    }

    fn take_u64(&mut self) -> BdfResult<u64> {
        let raw = self.take(8)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(raw);
        Ok(u64::from_le_bytes(buf))
    }

    fn take_i64(&mut self) -> BdfResult<i64> {
        let raw = self.take(8)?;
        let mut buf = [0u8; 8];
        buf.copy_from_slice(raw);
        Ok(i64::from_le_bytes(buf))
    }

    /// Read a `u64` length prefix and then that many bytes, checking the length
    /// against the remaining buffer before allocating.
    fn take_len_prefixed(&mut self) -> BdfResult<&'a [u8]> {
        let offset = self.pos;
        let len = self.take_u64()?;
        let remaining = self.remaining();
        if len > remaining as u64 {
            return Err(BdfError::LengthOverflow {
                len,
                remaining,
                offset,
            });
        }
        self.take(len as usize)
    }
}

fn decode_value(cursor: &mut Cursor<'_>) -> BdfResult<BdfValue> {
    let tag_offset = cursor.pos;
    let tag = cursor.take_u8()?;
    match tag {
        TAG_NULL => Ok(BdfValue::Null),
        TAG_BOOL => {
            let offset = cursor.pos;
            match cursor.take_u8()? {
                0 => Ok(BdfValue::Bool(false)),
                1 => Ok(BdfValue::Bool(true)),
                byte => Err(BdfError::InvalidBool { byte, offset }),
            }
        }
        TAG_INT => Ok(BdfValue::Int(cursor.take_i64()?)),
        TAG_FLOAT => Ok(BdfValue::Float(f64::from_bits(cursor.take_u64()?))),
        TAG_STR => {
            let offset = cursor.pos;
            let raw = cursor.take_len_prefixed()?;
            let s = std::str::from_utf8(raw).map_err(|_| BdfError::InvalidUtf8 { offset })?;
            Ok(BdfValue::Str(s.to_string()))
        }
        TAG_BYTES => Ok(BdfValue::Bytes(cursor.take_len_prefixed()?.to_vec())),
        TAG_ARRAY => {
            let count = cursor.take_u64()?;
            let mut items = Vec::with_capacity(count.min(1024) as usize);
            for _ in 0..count {
                items.push(decode_value(cursor)?);
            }
            Ok(BdfValue::Array(items))
        }
        TAG_MAP => {
            let count = cursor.take_u64()?;
            let mut entries = IndexMap::with_capacity(count.min(1024) as usize);
            for _ in 0..count {
                let key_offset = cursor.pos;
                let raw_key = cursor.take_len_prefixed()?;
                let key = std::str::from_utf8(raw_key)
                    .map_err(|_| BdfError::InvalidUtf8 { offset: key_offset })?
                    .to_string();
                let val = decode_value(cursor)?;
                entries.insert(key, val);
            }
            Ok(BdfValue::Map(entries))
        }
        tag => Err(BdfError::UnknownTag {
            tag,
            offset: tag_offset,
        }),
    }
}

// ============================================================================
// EFFECT-TYPED ENVELOPE
// ============================================================================

/// A capability effect a producing action exercised, per the buildlang effects
/// system. The variants mirror `compiler/src/types/capabilities.rs`.
///
/// The serde names are the stable wire strings (`"FileSystem"`, `"Gpu"`, ...),
/// matching the check-receipt and policy vocabulary so a BDF envelope's effect
/// row is reconcilable with a `buildc check` receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Standard input / output streams.
    Console,
    /// File-system reads and writes.
    FileSystem,
    /// Network sockets.
    Network,
    /// Process control (spawn, exit).
    Process,
    /// Environment variables and process arguments.
    Environment,
    /// Wall-clock and monotonic time.
    Clock,
    /// Foreign function interface (FFI) calls.
    Foreign,
    /// GPU / accelerator compute.
    Gpu,
}

impl Capability {
    /// The stable wire string for this capability.
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Console => "Console",
            Capability::FileSystem => "FileSystem",
            Capability::Network => "Network",
            Capability::Process => "Process",
            Capability::Environment => "Environment",
            Capability::Clock => "Clock",
            Capability::Foreign => "Foreign",
            Capability::Gpu => "Gpu",
        }
    }

    /// Parse a capability from its wire string, failing closed on anything else.
    pub fn from_str_strict(name: &str) -> BdfResult<Self> {
        match name {
            "Console" => Ok(Capability::Console),
            "FileSystem" => Ok(Capability::FileSystem),
            "Network" => Ok(Capability::Network),
            "Process" => Ok(Capability::Process),
            "Environment" => Ok(Capability::Environment),
            "Clock" => Ok(Capability::Clock),
            "Foreign" => Ok(Capability::Foreign),
            "Gpu" => Ok(Capability::Gpu),
            other => Err(BdfError::UnsupportedSchema {
                found: other.to_string(),
                expected: "a buildlang capability effect",
            }),
        }
    }
}

/// Identifies the tool (and its version) that produced a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducedBy {
    /// Tool name, e.g. `"buildc"`.
    pub tool: String,
    /// Tool version, e.g. `"1.0.0"`.
    pub tool_version: String,
}

/// A re-checkable receipt anchoring a message to its content and derivation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// SHA-256 hex digest of the canonical-binary payload encoding.
    pub sha256: String,
    /// SHA-256 hex digests this payload was derived from.
    #[serde(default)]
    pub derived_from: Vec<String>,
    /// How the digest was produced, e.g. `"bdf-canonical-sha256"`.
    pub method: String,
}

/// A continuation: a suggested next action, like flagship-action `next_actions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextAction {
    /// Tool that should act next.
    pub tool: String,
    /// Action that tool should take.
    pub action: String,
    /// Why this continuation is suggested.
    pub reason: String,
}

/// The effect-typed BDF envelope (`buildlang.bdf-message/v0`).
///
/// Carries the typed payload, the capability effects the producing action
/// claimed, and a re-checkable receipt. Admission is separate from
/// verification: this records what was produced and the effects claimed; a
/// separate gate decides allow / block / escalate / review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BdfMessage {
    /// Format tag; always [`BDF_MESSAGE_SCHEMA`] when produced by this codec.
    pub schema: String,
    /// Which tool produced this message.
    pub produced_by: ProducedBy,
    /// Capability effects the producing action exercised.
    pub effects: Vec<Capability>,
    /// Schema id describing the shape of `payload`.
    pub payload_schema: String,
    /// The typed payload value.
    pub payload: BdfValue,
    /// Re-checkable content/derivation receipt.
    pub receipt: Receipt,
    /// Suggested continuations.
    #[serde(default)]
    pub next: Vec<NextAction>,
}

impl BdfMessage {
    /// Construct a message, stamping the schema and computing the payload
    /// receipt digest from the canonical-binary encoding of `payload`.
    pub fn new(
        produced_by: ProducedBy,
        effects: Vec<Capability>,
        payload_schema: impl Into<String>,
        payload: BdfValue,
        derived_from: Vec<String>,
        next: Vec<NextAction>,
    ) -> Self {
        let sha256 = payload_digest_hex(&payload);
        Self {
            schema: BDF_MESSAGE_SCHEMA.to_string(),
            produced_by,
            effects,
            payload_schema: payload_schema.into(),
            payload,
            receipt: Receipt {
                sha256,
                derived_from,
                method: "bdf-canonical-sha256".to_string(),
            },
            next,
        }
    }

    /// Encode the whole message to canonical JSON (compact).
    pub fn to_json(&self) -> BdfResult<String> {
        serde_json::to_string(self).map_err(|e| BdfError::Json(e.to_string()))
    }

    /// Encode the whole message to canonical JSON (pretty).
    pub fn to_json_pretty(&self) -> BdfResult<String> {
        serde_json::to_string_pretty(self).map_err(|e| BdfError::Json(e.to_string()))
    }

    /// Parse a message from JSON, rejecting unknown schema strings (fail closed).
    pub fn from_json(json: &str) -> BdfResult<Self> {
        let message: Self =
            serde_json::from_str(json).map_err(|e| BdfError::Json(e.to_string()))?;
        message.validate_schema()?;
        Ok(message)
    }

    /// Encode the whole message to the canonical binary form.
    ///
    /// The message is projected through its [`BdfValue`] representation
    /// ([`Self::to_bdf_value`]) so the message and a bare value share one
    /// binary codec and one content-hash discipline.
    pub fn to_bytes(&self) -> Vec<u8> {
        to_bytes(&self.to_bdf_value())
    }

    /// Decode a message from the canonical binary form, rejecting unknown
    /// schema strings (fail closed).
    pub fn from_bytes(bytes: &[u8]) -> BdfResult<Self> {
        let value = from_bytes(bytes)?;
        let message = Self::from_bdf_value(&value)?;
        message.validate_schema()?;
        Ok(message)
    }

    /// Reject the message if its schema string is not [`BDF_MESSAGE_SCHEMA`].
    pub fn validate_schema(&self) -> BdfResult<()> {
        if self.schema != BDF_MESSAGE_SCHEMA {
            return Err(BdfError::UnsupportedSchema {
                found: self.schema.clone(),
                expected: BDF_MESSAGE_SCHEMA,
            });
        }
        Ok(())
    }

    /// Project the message into a [`BdfValue`] map (the binary representation).
    ///
    /// This is the bridge that lets one value codec serve both forms: the map's
    /// key order is fixed here, so the binary encoding is deterministic.
    pub fn to_bdf_value(&self) -> BdfValue {
        let effects = BdfValue::Array(
            self.effects
                .iter()
                .map(|c| BdfValue::Str(c.as_str().to_string()))
                .collect(),
        );
        let derived_from = BdfValue::Array(
            self.receipt
                .derived_from
                .iter()
                .map(|d| BdfValue::Str(d.clone()))
                .collect(),
        );
        let receipt = BdfValue::map_from([
            (
                "sha256".to_string(),
                BdfValue::Str(self.receipt.sha256.clone()),
            ),
            ("derived_from".to_string(), derived_from),
            (
                "method".to_string(),
                BdfValue::Str(self.receipt.method.clone()),
            ),
        ]);
        let next = BdfValue::Array(
            self.next
                .iter()
                .map(|n| {
                    BdfValue::map_from([
                        ("tool".to_string(), BdfValue::Str(n.tool.clone())),
                        ("action".to_string(), BdfValue::Str(n.action.clone())),
                        ("reason".to_string(), BdfValue::Str(n.reason.clone())),
                    ])
                })
                .collect(),
        );
        let produced_by = BdfValue::map_from([
            (
                "tool".to_string(),
                BdfValue::Str(self.produced_by.tool.clone()),
            ),
            (
                "tool_version".to_string(),
                BdfValue::Str(self.produced_by.tool_version.clone()),
            ),
        ]);
        BdfValue::map_from([
            ("schema".to_string(), BdfValue::Str(self.schema.clone())),
            ("produced_by".to_string(), produced_by),
            ("effects".to_string(), effects),
            (
                "payload_schema".to_string(),
                BdfValue::Str(self.payload_schema.clone()),
            ),
            ("payload".to_string(), self.payload.clone()),
            ("receipt".to_string(), receipt),
            ("next".to_string(), next),
        ])
    }

    /// Reconstruct a message from its [`BdfValue`] map representation.
    pub fn from_bdf_value(value: &BdfValue) -> BdfResult<Self> {
        let map = as_map(value, "message")?;
        let schema = field_str(map, "schema")?;
        let produced_by_map = as_map(field(map, "produced_by")?, "produced_by")?;
        let produced_by = ProducedBy {
            tool: field_str(produced_by_map, "tool")?,
            tool_version: field_str(produced_by_map, "tool_version")?,
        };
        let effects = as_array(field(map, "effects")?, "effects")?
            .iter()
            .map(|c| Capability::from_str_strict(&value_str(c, "effects entry")?))
            .collect::<BdfResult<Vec<_>>>()?;
        let payload_schema = field_str(map, "payload_schema")?;
        let payload = field(map, "payload")?.clone();
        let receipt_map = as_map(field(map, "receipt")?, "receipt")?;
        let derived_from = as_array(field(receipt_map, "derived_from")?, "derived_from")?
            .iter()
            .map(|d| value_str(d, "derived_from entry"))
            .collect::<BdfResult<Vec<_>>>()?;
        let receipt = Receipt {
            sha256: field_str(receipt_map, "sha256")?,
            derived_from,
            method: field_str(receipt_map, "method")?,
        };
        let next = as_array(field(map, "next")?, "next")?
            .iter()
            .map(|n| {
                let n_map = as_map(n, "next entry")?;
                Ok(NextAction {
                    tool: field_str(n_map, "tool")?,
                    action: field_str(n_map, "action")?,
                    reason: field_str(n_map, "reason")?,
                })
            })
            .collect::<BdfResult<Vec<_>>>()?;
        Ok(Self {
            schema,
            produced_by,
            effects,
            payload_schema,
            payload,
            receipt,
            next,
        })
    }
}

/// SHA-256 hex digest of a payload value's canonical-binary encoding.
///
/// This is the receipt anchor: a stable, content-addressable digest of the
/// deterministic byte stream, so two parties hashing the same value agree.
pub fn payload_digest_hex(payload: &BdfValue) -> String {
    use sha2::{Digest, Sha256};
    let bytes = to_bytes(payload);
    let digest = Sha256::digest(&bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

// --- small helpers for the message <-> BdfValue bridge ---

fn as_map<'a>(
    value: &'a BdfValue,
    what: &'static str,
) -> BdfResult<&'a IndexMap<String, BdfValue>> {
    match value {
        BdfValue::Map(m) => Ok(m),
        _ => Err(BdfError::Json(format!("expected {what} to be a map"))),
    }
}

fn as_array<'a>(value: &'a BdfValue, what: &'static str) -> BdfResult<&'a [BdfValue]> {
    match value {
        BdfValue::Array(a) => Ok(a),
        _ => Err(BdfError::Json(format!("expected {what} to be an array"))),
    }
}

fn value_str(value: &BdfValue, what: &'static str) -> BdfResult<String> {
    match value {
        BdfValue::Str(s) => Ok(s.clone()),
        _ => Err(BdfError::Json(format!("expected {what} to be a string"))),
    }
}

fn field<'a>(map: &'a IndexMap<String, BdfValue>, key: &'static str) -> BdfResult<&'a BdfValue> {
    map.get(key)
        .ok_or_else(|| BdfError::Json(format!("missing field '{key}'")))
}

fn field_str(map: &IndexMap<String, BdfValue>, key: &'static str) -> BdfResult<String> {
    value_str(field(map, key)?, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A representative spread of values: every variant, the float edge cases,
    /// nested containers, bytes, unicode, and empty containers.
    fn representative_values() -> Vec<BdfValue> {
        vec![
            BdfValue::Null,
            BdfValue::Bool(true),
            BdfValue::Bool(false),
            BdfValue::Int(0),
            BdfValue::Int(1),
            BdfValue::Int(-1),
            BdfValue::Int(i64::MAX),
            BdfValue::Int(i64::MIN),
            BdfValue::Float(0.0),
            BdfValue::Float(-0.0),
            BdfValue::Float(1.5),
            BdfValue::Float(-2.25),
            BdfValue::Float(f64::INFINITY),
            BdfValue::Float(f64::NEG_INFINITY),
            BdfValue::Float(f64::NAN),
            BdfValue::Float(f64::MIN_POSITIVE),
            BdfValue::Float(f64::from_bits(1)), // smallest subnormal
            BdfValue::Float(f64::MAX),
            BdfValue::Str(String::new()),
            BdfValue::Str("hello".to_string()),
            BdfValue::Str("unicode: café ☃ 𝄞 日本語".to_string()),
            BdfValue::Bytes(Vec::new()),
            BdfValue::Bytes(vec![0x00, 0x01, 0xff, 0x80, 0x7f]),
            BdfValue::Array(Vec::new()),
            BdfValue::Array(vec![
                BdfValue::Int(1),
                BdfValue::Str("two".to_string()),
                BdfValue::Bool(false),
                BdfValue::Null,
            ]),
            BdfValue::Map(IndexMap::new()),
            nested_value(),
        ]
    }

    /// A deeply nested value mixing arrays and maps.
    fn nested_value() -> BdfValue {
        BdfValue::map_from([
            ("schema".to_string(), BdfValue::Str("demo".to_string())),
            ("count".to_string(), BdfValue::Int(3)),
            ("ratio".to_string(), BdfValue::Float(-0.0)),
            (
                "items".to_string(),
                BdfValue::Array(vec![
                    BdfValue::map_from([
                        ("id".to_string(), BdfValue::Int(1)),
                        ("name".to_string(), BdfValue::Str("α".to_string())),
                        ("blob".to_string(), BdfValue::Bytes(vec![1, 2, 3])),
                    ]),
                    BdfValue::map_from([
                        ("id".to_string(), BdfValue::Int(2)),
                        (
                            "tags".to_string(),
                            BdfValue::Array(vec![
                                BdfValue::Str("a".to_string()),
                                BdfValue::Str("b".to_string()),
                            ]),
                        ),
                        ("nested".to_string(), BdfValue::Array(vec![BdfValue::Null])),
                    ]),
                ]),
            ),
        ])
    }

    /// Structural equality that treats `NaN` as equal to `NaN` by bit pattern,
    /// since `f64::NAN != f64::NAN` would break a naive `==`.
    fn structurally_eq(a: &BdfValue, b: &BdfValue) -> bool {
        match (a, b) {
            (BdfValue::Float(x), BdfValue::Float(y)) => x.to_bits() == y.to_bits(),
            (BdfValue::Array(xs), BdfValue::Array(ys)) => {
                xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| structurally_eq(x, y))
            }
            (BdfValue::Map(xs), BdfValue::Map(ys)) => {
                xs.len() == ys.len()
                    && xs
                        .iter()
                        .zip(ys.iter())
                        .all(|((kx, vx), (ky, vy))| kx == ky && structurally_eq(vx, vy))
            }
            _ => a == b,
        }
    }

    // ---- ROUND-TRIP GOLDEN TESTS ----

    #[test]
    fn binary_round_trip_is_structurally_identical() {
        for value in representative_values() {
            let bytes = to_bytes(&value);
            let decoded = from_bytes(&bytes).expect("decode");
            assert!(
                structurally_eq(&value, &decoded),
                "binary round-trip mismatch for {value:?} -> {decoded:?}"
            );
        }
    }

    #[test]
    fn json_round_trip_is_structurally_identical() {
        for value in representative_values() {
            let json = value.to_json().expect("to_json");
            let decoded = BdfValue::from_json(&json).expect("from_json");
            assert!(
                structurally_eq(&value, &decoded),
                "json round-trip mismatch for {value:?} via {json}"
            );
        }
    }

    #[test]
    fn json_pretty_round_trip_is_structurally_identical() {
        for value in representative_values() {
            let json = value.to_json_pretty().expect("to_json_pretty");
            let decoded = BdfValue::from_json(&json).expect("from_json");
            assert!(structurally_eq(&value, &decoded));
        }
    }

    #[test]
    fn cross_form_stability_bytes_identical_through_json() {
        // to_bytes(from_json(to_json(v))) is byte-identical to to_bytes(v).
        for value in representative_values() {
            let direct = to_bytes(&value);
            let json = value.to_json().expect("to_json");
            let via_json = BdfValue::from_json(&json).expect("from_json");
            let through_json = to_bytes(&via_json);
            assert_eq!(
                direct, through_json,
                "cross-form byte stability failed for {value:?}"
            );
        }
    }

    #[test]
    fn binary_encoding_is_deterministic() {
        // Encoding the same value twice yields the same bytes (content-hashable).
        for value in representative_values() {
            assert_eq!(to_bytes(&value), to_bytes(&value));
        }
    }

    #[test]
    fn map_key_order_is_preserved_and_load_bearing() {
        let a = BdfValue::map_from([
            ("x".to_string(), BdfValue::Int(1)),
            ("y".to_string(), BdfValue::Int(2)),
        ]);
        let b = BdfValue::map_from([
            ("y".to_string(), BdfValue::Int(2)),
            ("x".to_string(), BdfValue::Int(1)),
        ]);
        // Same entries, different insertion order -> different canonical bytes.
        assert_ne!(to_bytes(&a), to_bytes(&b));
        // ...and order survives a round-trip.
        let decoded = from_bytes(&to_bytes(&a)).expect("decode");
        if let BdfValue::Map(m) = decoded {
            let keys: Vec<&str> = m.keys().map(String::as_str).collect();
            assert_eq!(keys, vec!["x", "y"]);
        } else {
            panic!("expected map");
        }
    }

    #[test]
    fn negative_zero_survives_both_forms() {
        let v = BdfValue::Float(-0.0);
        let from_bin = from_bytes(&to_bytes(&v)).expect("bin");
        let from_js = BdfValue::from_json(&v.to_json().unwrap()).expect("json");
        assert!(matches!(from_bin, BdfValue::Float(f) if f.is_sign_negative() && f == 0.0));
        assert!(matches!(from_js, BdfValue::Float(f) if f.is_sign_negative() && f == 0.0));
    }

    // ---- NEGATIVE TESTS (fail closed) ----

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = to_bytes(&BdfValue::Int(7));
        bytes[0] = b'X';
        assert!(matches!(from_bytes(&bytes), Err(BdfError::BadMagic { .. })));
    }

    #[test]
    fn rejects_empty_input() {
        assert!(matches!(from_bytes(&[]), Err(BdfError::BadMagic { .. })));
    }

    #[test]
    fn rejects_truncated_after_magic() {
        // Magic + INT tag but no 8-byte payload.
        let mut bytes = BDF_MAGIC.to_vec();
        bytes.push(TAG_INT);
        bytes.extend_from_slice(&[0u8; 3]); // only 3 of 8 needed bytes
        assert!(matches!(
            from_bytes(&bytes),
            Err(BdfError::Truncated { .. })
        ));
    }

    #[test]
    fn rejects_unknown_tag() {
        let mut bytes = BDF_MAGIC.to_vec();
        bytes.push(0xEE); // not a known tag
        assert!(matches!(
            from_bytes(&bytes),
            Err(BdfError::UnknownTag { tag: 0xEE, .. })
        ));
    }

    #[test]
    fn rejects_length_overflow() {
        let mut bytes = BDF_MAGIC.to_vec();
        bytes.push(TAG_STR);
        bytes.extend_from_slice(&u64::MAX.to_le_bytes()); // absurd length
        assert!(matches!(
            from_bytes(&bytes),
            Err(BdfError::LengthOverflow { .. })
        ));
    }

    #[test]
    fn rejects_invalid_utf8_string() {
        let mut bytes = BDF_MAGIC.to_vec();
        bytes.push(TAG_STR);
        bytes.extend_from_slice(&2u64.to_le_bytes());
        bytes.extend_from_slice(&[0xff, 0xfe]); // not valid UTF-8
        assert!(matches!(
            from_bytes(&bytes),
            Err(BdfError::InvalidUtf8 { .. })
        ));
    }

    #[test]
    fn rejects_invalid_bool_byte() {
        let mut bytes = BDF_MAGIC.to_vec();
        bytes.push(TAG_BOOL);
        bytes.push(0x02); // neither 0 nor 1
        assert!(matches!(
            from_bytes(&bytes),
            Err(BdfError::InvalidBool { byte: 0x02, .. })
        ));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = to_bytes(&BdfValue::Null);
        bytes.push(0x00); // extra byte after a complete value
        assert!(matches!(
            from_bytes(&bytes),
            Err(BdfError::TrailingBytes { .. })
        ));
    }

    // ---- ENVELOPE TESTS ----

    fn sample_message() -> BdfMessage {
        BdfMessage::new(
            ProducedBy {
                tool: "buildc".to_string(),
                tool_version: "1.0.0".to_string(),
            },
            vec![Capability::FileSystem, Capability::Console],
            "buildlang.demo/v0",
            nested_value(),
            vec!["abc123".to_string()],
            vec![NextAction {
                tool: "qkv".to_string(),
                action: "store".to_string(),
                reason: "persist the result".to_string(),
            }],
        )
    }

    #[test]
    fn message_json_round_trip() {
        let message = sample_message();
        let json = message.to_json().expect("to_json");
        let decoded = BdfMessage::from_json(&json).expect("from_json");
        assert_eq!(message, decoded);
    }

    #[test]
    fn message_binary_round_trip() {
        let message = sample_message();
        let bytes = message.to_bytes();
        let decoded = BdfMessage::from_bytes(&bytes).expect("from_bytes");
        assert_eq!(message, decoded);
    }

    #[test]
    fn message_stamps_schema_and_receipt() {
        let message = sample_message();
        assert_eq!(message.schema, BDF_MESSAGE_SCHEMA);
        assert_eq!(message.receipt.method, "bdf-canonical-sha256");
        // The receipt digest matches an independent recompute over the payload.
        assert_eq!(message.receipt.sha256, payload_digest_hex(&message.payload));
        assert_eq!(message.receipt.sha256.len(), 64);
    }

    #[test]
    fn message_rejects_unknown_schema_json() {
        let message = sample_message();
        let json = message.to_json().expect("to_json");
        let tampered = json.replace(BDF_MESSAGE_SCHEMA, "buildlang.bdf-message/v999");
        assert!(matches!(
            BdfMessage::from_json(&tampered),
            Err(BdfError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn message_rejects_unknown_capability() {
        let message = sample_message();
        let value = message.to_bdf_value();
        let json = value.to_json().expect("to_json");
        let tampered = json.replace("FileSystem", "Telepathy");
        let tampered_value = BdfValue::from_json(&tampered).expect("from_json");
        assert!(matches!(
            BdfMessage::from_bdf_value(&tampered_value),
            Err(BdfError::UnsupportedSchema { .. })
        ));
    }

    #[test]
    fn message_payload_digest_is_stable() {
        let m1 = sample_message();
        let m2 = sample_message();
        assert_eq!(m1.receipt.sha256, m2.receipt.sha256);
    }
}
