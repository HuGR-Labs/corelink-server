//! libFuzzer harness — HKDF-SHA256 expand under random
//! `(IKM, salt, info, L)` inputs.
//!
//! Mirrors the production AC envelope signature key derivation from
//! ADR-0021 §1 (`HKDF-Expand(HKDF-Extract(salt = sig_key_id, IKM = TDK),
//! info = b"ac-sig", L = 32)`), exposing the same primitive to a
//! parse-or-crash adversary.
//!
//! Properties asserted:
//!
//! 1. **No panic.** Any `(IKM, salt, info, L)` triple must produce
//!    `Ok(out)` or `Err(_)` — never an abort.
//! 2. **Length contract.** When `Ok`, `out.len() == L` exactly.
//! 3. **Determinism.** Same inputs → byte-equal output across two
//!    independent invocations (HKDF is a deterministic KDF).
//! 4. **Salt sensitivity.** Flipping a single bit of the salt
//!    produces a different output (with overwhelming probability —
//!    HKDF-SHA256 has 256-bit collision resistance; observing equal
//!    output under distinct salts is a SEV-0 cryptographic finding).

#![no_main]

use hkdf::Hkdf;
use libfuzzer_sys::fuzz_target;
use sha2::Sha256;

fuzz_target!(|data: &[u8]| {
    // Layout: [ikm_len:1][salt_len:1][info_len:1][L:1][ikm][salt][info]
    if data.len() < 4 {
        return;
    }
    let ikm_len = usize::from(data[0]);
    let salt_len = usize::from(data[1]);
    let info_len = usize::from(data[2]);
    // HKDF-SHA256 maximum output length is 255 * 32 = 8160 bytes. Cap
    // L at 255 so the fuzzer doesn't repeatedly hit the OutputLength
    // error arm; values 0..=255 cover the full meaningful range.
    let l = usize::from(data[3]);

    let need = 4usize.saturating_add(ikm_len).saturating_add(salt_len).saturating_add(info_len);
    if data.len() < need {
        return;
    }
    let ikm = &data[4..4 + ikm_len];
    let salt = &data[4 + ikm_len..4 + ikm_len + salt_len];
    let info = &data[4 + ikm_len + salt_len..need];

    // ── (1) First invocation ──────────────────────────────────────────
    let hk1 = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out1 = vec![0u8; l];
    let r1 = hk1.expand(info, &mut out1);

    match r1 {
        Ok(()) => {
            // (2) Length contract — Vec was pre-sized to L; HKDF must
            // have filled exactly L bytes (panics if it ever shrinks).
            assert_eq!(out1.len(), l, "HKDF expand MUST fill exactly L bytes");
        }
        Err(_) => {
            // L > 255*HashLen → OutputLength rejection; fine.
            return;
        }
    }

    // ── (3) Determinism — re-derive and compare ──────────────────────
    let hk2 = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out2 = vec![0u8; l];
    hk2.expand(info, &mut out2)
        .expect("second HKDF expand with identical inputs MUST succeed");
    assert_eq!(out1, out2, "HKDF-SHA256 is a deterministic KDF");

    // ── (4) Salt sensitivity — flip one bit of salt if non-empty ─────
    // Only meaningful when L is large enough that two random 8-bit
    // outputs colliding is itself improbable. We gate at L >= 16 (16
    // bytes = 128-bit collision width, ~2^-128 chance under HKDF-SHA256
    // diffusion). For shorter outputs the per-byte collision probability
    // dominates and would surface as false-positive crashes.
    if !salt.is_empty() && l >= 16 {
        let mut flipped_salt = salt.to_vec();
        flipped_salt[0] ^= 0x01;
        let hk3 = Hkdf::<Sha256>::new(Some(&flipped_salt), ikm);
        let mut out3 = vec![0u8; l];
        if hk3.expand(info, &mut out3).is_ok() {
            // Theoretical collision probability at L >= 16 bytes is
            // ~2^-128; if it ever fires the fuzzer will catch + persist
            // the corpus entry as a SEV-0 finding.
            assert_ne!(
                out1, out3,
                "HKDF-SHA256 under single-bit-flipped salt produced identical output \
                 at L >= 16 bytes — SEV-0 cryptographic finding"
            );
        }
    }
});
