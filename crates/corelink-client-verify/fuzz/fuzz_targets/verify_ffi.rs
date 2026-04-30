//! libFuzzer harness for the FFI surface — exercise `extern "C"`
//! functions with random byte ranges to verify no panic / abort and
//! that error tags match the Rust-API truth.
//!
//! Coverage plan (codex round 2):
//! - Both constructors (`new_default_on` + `new_disabled`) reached.
//! - Null `handle`, null `out_error_code`, oversized `body_len`, and
//!   body-pointer / length inconsistency are all driven by fuzzer
//!   control bits, not unit-test-only.
//! - Digest is fed through FOUR codecs in parallel: happy hex,
//!   wrong-length (guaranteed != 64), non-hex but correct length,
//!   non-UTF-8 correct length.
//! - Each leg has an oracle that asserts the canonical surface tag.

#![no_main]

use corelink_client_verify::ffi::{
    corelink_verifier_free, corelink_verifier_new_default_on, corelink_verifier_new_disabled,
    corelink_verifier_verify, COR_VERIFY_ERR_DISABLED, COR_VERIFY_ERR_INVALID_DIGEST,
    COR_VERIFY_ERR_INVALID_INPUT, COR_VERIFY_ERR_MISMATCH, COR_VERIFY_OK, DIGEST_HEX_LEN,
};
use corelink_client_verify::Digest;
use libfuzzer_sys::fuzz_target;

/// Codec enum + its scratch buffer kept alive for the duration of
/// the FFI call so the slice the C function reads from has a stable
/// backing.
enum Claim {
    OkHex(String),                 // exactly 64 ASCII hex chars
    WrongLen(Vec<u8>),             // length deliberately != 64
    NonHexCorrectLen(Vec<u8>),     // length 64, ASCII but not hex
    NonUtf8CorrectLen(Vec<u8>),    // length 64, non-UTF-8 bytes
}

fuzz_target!(|data: &[u8]| {
    // Need at least: 2 control bytes + 32 raw "claimed digest" bytes.
    if data.len() < 34 {
        return;
    }
    let control = data[0];
    let mut_control = data[1];
    let raw_claim = &data[2..34];
    let body = &data[34..];

    // Seven fuzzer-driven control bits covering every dispatch path.
    let use_default_on = (control & 0b0000_0001) != 0;
    let digest_codec = (control & 0b0000_0110) >> 1; // 0..=3
    let null_handle = (control & 0b0000_1000) != 0;
    let null_out = (control & 0b0001_0000) != 0;
    let null_digest_ptr_with_correct_len = (control & 0b0010_0000) != 0;
    let oversized_body = (mut_control & 0b0000_0001) != 0;
    let null_body_with_nonzero_len = (mut_control & 0b0000_0010) != 0;
    let force_warn_on_disabled = (mut_control & 0b0000_0100) != 0;

    // SAFETY: ctor symbols return a heap box; null is impossible per
    // current implementation.
    let v = if null_handle {
        core::ptr::null_mut()
    } else if use_default_on {
        unsafe { corelink_verifier_new_default_on() }
    } else if force_warn_on_disabled {
        unsafe { corelink_verifier_new_disabled(1) }
    } else {
        unsafe { corelink_verifier_new_disabled(0) }
    };

    // Build the digest buffer four different ways. The "wrong-length"
    // leg uses fuzzer-provided low bits but explicitly avoids 64 so
    // the oracle below is unambiguous.
    let claim = match digest_codec {
        0 => {
            let hex: String = raw_claim.iter().map(|b| format!("{b:02x}")).collect();
            Claim::OkHex(hex)
        }
        1 => {
            // Length must NOT be 64 to keep the leg disjoint from
            // OkHex. Pick a length in 0..=200 minus the {64} carve-out.
            let raw_len_byte = body.first().copied().unwrap_or(3);
            let mut len = (raw_len_byte as usize) % 200; // 0..=199
            if len == DIGEST_HEX_LEN {
                len = if len == 0 { 1 } else { len - 1 };
            }
            // Keep `b'a'`-fill so the buffer is allocated even at len 0.
            let mut buf = vec![b'a'; len];
            for (i, b) in raw_claim.iter().take(len).enumerate() {
                buf[i] = *b;
            }
            Claim::WrongLen(buf)
        }
        2 => {
            let mut buf = vec![b'g'; DIGEST_HEX_LEN]; // 'g' is ASCII but not a hex digit
            // Mix raw bytes but mask back into ASCII non-hex range so
            // the leg stays "non-hex correct-length", not "non-UTF-8".
            for (i, b) in raw_claim.iter().enumerate() {
                let mixed = (*b & 0x3F) | 0x40; // ASCII range 0x40..=0x7F
                buf[i] = if matches!(mixed, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F') {
                    b'g'
                } else {
                    mixed
                };
            }
            Claim::NonHexCorrectLen(buf)
        }
        _ => {
            // High-bit-set bytes guarantee invalid UTF-8 (continuation
            // bytes without a leading byte).
            let mut buf = vec![0xFFu8; DIGEST_HEX_LEN];
            for (i, b) in raw_claim.iter().enumerate() {
                buf[i] = *b | 0x80;
            }
            Claim::NonUtf8CorrectLen(buf)
        }
    };

    let (digest_ptr, digest_len): (*const u8, usize) = if null_digest_ptr_with_correct_len {
        // Null pointer with a "valid" length: the verify entry point's
        // post-length-check null guard (step 5 of the precedence
        // tree) must reject this with INVALID_INPUT before any
        // dereference. Slot is mutually exclusive with other claim
        // variants; oracle below picks INVALID_INPUT.
        (core::ptr::null(), DIGEST_HEX_LEN)
    } else {
        match &claim {
            Claim::OkHex(s) => (s.as_ptr(), s.len()),
            Claim::WrongLen(b)
            | Claim::NonHexCorrectLen(b)
            | Claim::NonUtf8CorrectLen(b) => (b.as_ptr(), b.len()),
        }
    };

    // Body pointer / length permutations.
    let (body_ptr, body_len): (*const u8, usize) = if oversized_body {
        // body_len > isize::MAX → must be rejected before slice
        // construction.
        (core::ptr::null(), (isize::MAX as usize).wrapping_add(1))
    } else if null_body_with_nonzero_len {
        (core::ptr::null(), 1) // canonical INVALID_INPUT trigger
    } else {
        (body.as_ptr(), body.len())
    };

    // out_error_code permutations.
    let mut out_storage: i32 = -1;
    let out_ptr: *mut i32 = if null_out {
        core::ptr::null_mut()
    } else {
        &mut out_storage
    };

    let rc = unsafe {
        corelink_verifier_verify(v, body_ptr, body_len, digest_ptr, digest_len, out_ptr)
    };

    // Invariant 1: rc and out tag agree when out_ptr is non-null.
    if !null_out {
        assert_eq!(rc, out_storage, "rc and out tag must agree");
    }

    // Invariant 2: tag matches the documented decision tree. The
    // verify entry point validates inputs in the following order
    // (see src/ffi.rs::corelink_verifier_verify); the oracle below
    // mirrors that exact precedence so it never has two clauses
    // claim the same input:
    //
    //   1. body_len > isize::MAX                → INVALID_INPUT
    //   2. handle is null                       → INVALID_INPUT
    //   3. body_len > 0 && body_ptr is null     → INVALID_INPUT
    //   4. digest_hex_len != 64                 → INVALID_DIGEST
    //   5. digest_hex_ptr is null               → INVALID_INPUT
    //   6. digest hex parse fails               → INVALID_DIGEST
    //   7. disabled verifier                    → DISABLED
    //   8. content compare                      → OK / MISMATCH
    if oversized_body {
        assert_eq!(rc, COR_VERIFY_ERR_INVALID_INPUT, "oversized body must INVALID_INPUT");
    } else if null_handle {
        assert_eq!(rc, COR_VERIFY_ERR_INVALID_INPUT, "null handle must INVALID_INPUT");
    } else if null_body_with_nonzero_len {
        assert_eq!(
            rc, COR_VERIFY_ERR_INVALID_INPUT,
            "null body with nonzero len must INVALID_INPUT"
        );
    } else if !null_digest_ptr_with_correct_len && matches!(claim, Claim::WrongLen(_)) {
        // WrongLen only fires when we actually passed the
        // wrong-length buffer. With null_digest_ptr_with_correct_len
        // the digest length is forced to 64 instead.
        assert_eq!(
            rc, COR_VERIFY_ERR_INVALID_DIGEST,
            "wrong digest length must INVALID_DIGEST"
        );
    } else if null_digest_ptr_with_correct_len {
        assert_eq!(
            rc, COR_VERIFY_ERR_INVALID_INPUT,
            "null digest pointer with correct length must INVALID_INPUT"
        );
    } else if matches!(claim, Claim::NonHexCorrectLen(_) | Claim::NonUtf8CorrectLen(_)) {
        assert_eq!(
            rc, COR_VERIFY_ERR_INVALID_DIGEST,
            "non-hex / non-UTF-8 correct-length digest must INVALID_DIGEST"
        );
    } else if !use_default_on {
        // Disabled ctor reached the verifier-state arm with valid
        // inputs.
        assert_eq!(rc, COR_VERIFY_ERR_DISABLED, "disabled verifier short-circuit");
    } else if let Claim::OkHex(s) = &claim {
        // Default-on, valid hex, valid body — outcome is OK or
        // MISMATCH per the BLAKE3 truth.
        let claimed = Digest::from_hex(s).expect("OkHex must parse");
        let computed = Digest::compute(body);
        if computed.verify_constant_time(&claimed) {
            assert_eq!(rc, COR_VERIFY_OK, "computed==claimed must surface OK");
        } else {
            assert_eq!(
                rc, COR_VERIFY_ERR_MISMATCH,
                "computed!=claimed must surface MISMATCH"
            );
        }
    }

    // Free even after a null-handle path: the free function tolerates
    // null and was exercised in the abi_smoke tests.
    unsafe { corelink_verifier_free(v) };
});
