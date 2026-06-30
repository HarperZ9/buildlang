// ===============================================================================
// BUILDLANG COMPILER - LOSSLESS FLOAT SERDE
// ===============================================================================
// Copyright (c) 2022-2026 Zain Dana Harper. MIT License.
// ===============================================================================

//! Lossless `f64` serde: store the raw IEEE-754 bit pattern as a `u64`.
//!
//! `serde_json` serializes non-finite floats (`NaN`, `±∞`) as JSON `null`,
//! which does not round-trip, and even finite floats can suffer from
//! formatter-dependent shortest-representation choices. Encoding the bit
//! pattern guarantees every `f64` (including `NaN` payloads, `±∞`, `-0.0`,
//! and subnormals) round-trips bit-exact.
//!
//! This is the single source of truth for the lossless-float discipline shared
//! by the MIR interlingua (`buildlang.mir/v0`) and the Build Data Format
//! (`buildlang.bdf/v0`). Both formats must encode `f64` the same way so that a
//! float embedded in either wire form is byte-for-byte reconcilable.

use serde::{Deserialize, Deserializer, Serializer};

/// Serialize an `f64` as its raw IEEE-754 bit pattern (a `u64`).
pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_u64(value.to_bits())
}

/// Deserialize an `f64` from its raw IEEE-754 bit pattern (a `u64`).
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let bits = u64::deserialize(deserializer)?;
    Ok(f64::from_bits(bits))
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct Wrapper(#[serde(with = "super")] f64);

    fn round_trips_bit_exact(value: f64) -> bool {
        let json = serde_json::to_string(&Wrapper(value)).expect("serialize");
        let back: Wrapper = serde_json::from_str(&json).expect("deserialize");
        back.0.to_bits() == value.to_bits()
    }

    #[test]
    fn preserves_finite_and_special_floats_bit_exact() {
        assert!(round_trips_bit_exact(0.0));
        assert!(round_trips_bit_exact(-0.0));
        assert!(round_trips_bit_exact(1.0));
        assert!(round_trips_bit_exact(-1.5));
        assert!(round_trips_bit_exact(f64::INFINITY));
        assert!(round_trips_bit_exact(f64::NEG_INFINITY));
        assert!(round_trips_bit_exact(f64::NAN));
        assert!(round_trips_bit_exact(f64::MIN_POSITIVE)); // smallest normal
        assert!(round_trips_bit_exact(f64::from_bits(1))); // smallest subnormal
        assert!(round_trips_bit_exact(f64::MAX));
    }

    #[test]
    fn distinguishes_positive_and_negative_zero() {
        // The whole point: -0.0 must not collapse to 0.0.
        let json_pos = serde_json::to_string(&Wrapper(0.0)).expect("serialize");
        let json_neg = serde_json::to_string(&Wrapper(-0.0)).expect("serialize");
        assert_ne!(json_pos, json_neg);
    }
}
