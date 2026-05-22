//! libFuzzer harness — feed arbitrary bytes through `WrappedDek` JSON
//! deserialization (the wire shape persisted in D1 `byok_envelope`) +
//! exercise the `EncryptedBlob` parser surface.
//!
//! Properties asserted:
//!
//! 1. **Never panic.** Any byte stream → `Ok(WrappedDek)` or `Err(_)`.
//!    Indexing, length asserts, UTF-8 decodes inside serde must never
//!    abort.
//! 2. **Serialize roundtrip.** If parsing succeeds, re-serializing the
//!    `WrappedDek` MUST succeed (so the canonical wire form is total over
//!    the constructable in-memory set). Mismatched re-parse signals
//!    a serde drift that would corrupt the D1 mirror on the next read.
//! 3. **`encryption_context` invariant.** The Option<Value> field is
//!    deserialized verbatim — we don't reject `None` here (the envelope
//!    decrypt path is responsible for `EncryptionContextMissing`), but
//!    we DO assert that re-encoding preserves Some/None polarity.
//!
//! This pins INV-BYOK-CRYPTO-SOVEREIGNTY (envelope wire form stability)
//! against a parse-or-crash adversary.

#![no_main]

use corelink_byok_core::envelope::EncryptedBlob;
use corelink_byok_core::WrappedDek;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // ── (1) WrappedDek JSON parse ─────────────────────────────────────
    if let Ok(parsed) = serde_json::from_slice::<WrappedDek>(data) {
        // Roundtrip via serde_json must succeed for any in-memory value
        // that came from `from_slice`. A drift here = D1 corruption on
        // the next read.
        let reserialized = serde_json::to_vec(&parsed)
            .expect("WrappedDek -> serde_json::to_vec must be total for any parsed value");

        let reparsed: WrappedDek = serde_json::from_slice(&reserialized)
            .expect("WrappedDek serde roundtrip must hold");

        // AAD presence polarity stable across roundtrip.
        assert_eq!(
            parsed.encryption_context.is_some(),
            reparsed.encryption_context.is_some(),
            "encryption_context Some/None polarity drifted across serde roundtrip — \
             attacker could swap envelopes if this ever flips"
        );

        // Provider + region preserved (the key_id binds the CMK and the
        // region; a drift would let a cross-region key be substituted).
        assert_eq!(parsed.provider, reparsed.provider);
        assert_eq!(parsed.key_id.region, reparsed.key_id.region);
        assert_eq!(parsed.key_id.key_arn_or_id, reparsed.key_id.key_arn_or_id);
    }

    // ── (2) EncryptedBlob JSON parse ──────────────────────────────────
    // The blob shape stored in R2 (split between R2 ciphertext + D1
    // wrapped DEK at production); fuzz the JSON wire form for parse-
    // or-crash bugs at boundary.
    if let Ok(blob) = serde_json::from_slice::<EncryptedBlob>(data) {
        let reserialized = serde_json::to_vec(&blob)
            .expect("EncryptedBlob -> serde_json::to_vec must be total for any parsed value");
        let _: EncryptedBlob = serde_json::from_slice(&reserialized)
            .expect("EncryptedBlob serde roundtrip must hold");
    }
});
