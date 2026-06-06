//! ABI stability smoke for the FFI surface (WI-S02-003 §8 ABI scenario).
//!
//! Drives the `extern "C"` entry points exactly as a cbindgen-generated
//! C header would expose them: opaque `*mut ClientVerifier`, raw
//! body/digest pointers + lengths, `i32` error codes. If any signature
//! drifts, this test fails to compile and CI rejects the PR.
//!
//! This test does NOT verify the cbindgen header text directly — that
//! lives in the workflow `cbindgen-header-stable` job, which runs
//! `cbindgen --output ...` and `git diff --exit-code` against the
//! committed `include/corelink_client_verify.h`.
//!
//! The whole file is gated behind `#[cfg(feature = "ffi")]` because
//! the FFI surface itself is gated that way (see `src/lib.rs`). The
//! workflow runs both `cargo test -p corelink-client-verify` (default
//! features) and `cargo test -p corelink-client-verify --features ffi`
//! so this file always executes in CI.

#![cfg(feature = "ffi")]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::missing_docs_in_private_items,
    clippy::panic,
    missing_docs,
    unsafe_code,
    reason = "ABI smoke test exercises the C-ABI surface; raw pointers are the contract"
)]

use core::ffi::CStr;

use corelink_client_verify::ffi::{
    corelink_verifier_free, corelink_verifier_new_default_on, corelink_verifier_new_disabled,
    corelink_verifier_verify, corelink_verify_error_code_to_str, corelink_verify_opt_out_total,
    COR_VERIFY_ERR_DISABLED, COR_VERIFY_ERR_INVALID_DIGEST, COR_VERIFY_ERR_INVALID_INPUT,
    COR_VERIFY_ERR_MISMATCH, COR_VERIFY_OK, DIGEST_BYTES, DIGEST_HEX_LEN,
};
use corelink_client_verify::Digest;

#[test]
fn ffi_default_on_round_trip_all_canonical_codes() {
    let body = b"hello world";
    let d = Digest::compute(body);
    let hex = d.to_hex();

    // 1. Happy path
    {
        let v = unsafe { corelink_verifier_new_default_on() };
        assert!(!v.is_null());
        let mut out: i32 = -1;
        let rc = unsafe {
            corelink_verifier_verify(
                v,
                body.as_ptr(),
                body.len(),
                hex.as_ptr(),
                hex.len(),
                &mut out,
            )
        };
        assert_eq!(rc, COR_VERIFY_OK);
        assert_eq!(out, COR_VERIFY_OK);
        unsafe { corelink_verifier_free(v) };
    }

    // 2. Mismatch
    {
        let wrong = Digest::compute(b"goodbye world");
        let wrong_hex = wrong.to_hex();
        let v = unsafe { corelink_verifier_new_default_on() };
        let mut out: i32 = -1;
        let rc = unsafe {
            corelink_verifier_verify(
                v,
                body.as_ptr(),
                body.len(),
                wrong_hex.as_ptr(),
                wrong_hex.len(),
                &mut out,
            )
        };
        assert_eq!(rc, COR_VERIFY_ERR_MISMATCH);
        assert_eq!(out, COR_VERIFY_ERR_MISMATCH);
        unsafe { corelink_verifier_free(v) };
    }

    // 3. Disabled (silent variant for clean test logs)
    {
        let v = unsafe { corelink_verifier_new_disabled(0) };
        let mut out: i32 = -1;
        let rc = unsafe {
            corelink_verifier_verify(
                v,
                body.as_ptr(),
                body.len(),
                hex.as_ptr(),
                hex.len(),
                &mut out,
            )
        };
        assert_eq!(rc, COR_VERIFY_ERR_DISABLED);
        assert_eq!(out, COR_VERIFY_ERR_DISABLED);
        unsafe { corelink_verifier_free(v) };
    }

    // 4. Null handle → INVALID_INPUT
    {
        let mut out: i32 = -1;
        let rc = unsafe {
            corelink_verifier_verify(
                core::ptr::null(),
                body.as_ptr(),
                body.len(),
                hex.as_ptr(),
                hex.len(),
                &mut out,
            )
        };
        assert_eq!(rc, COR_VERIFY_ERR_INVALID_INPUT);
        assert_eq!(out, COR_VERIFY_ERR_INVALID_INPUT);
    }

    // 5. Invalid digest hex (64 bytes but non-hex) → INVALID_DIGEST.
    //    We deliberately pass exactly 64 chars so the length check
    //    passes and the hex parser is exercised.
    {
        let bad = "G".repeat(64);
        let v = unsafe { corelink_verifier_new_default_on() };
        let mut out: i32 = -1;
        let rc = unsafe {
            corelink_verifier_verify(
                v,
                body.as_ptr(),
                body.len(),
                bad.as_ptr(),
                bad.len(),
                &mut out,
            )
        };
        assert_eq!(rc, COR_VERIFY_ERR_INVALID_DIGEST);
        assert_eq!(out, COR_VERIFY_ERR_INVALID_DIGEST);
        unsafe { corelink_verifier_free(v) };
    }
}

#[test]
fn ffi_wrong_digest_length_rejected_before_dereference() {
    // A non-64-byte digest must be rejected by the length precheck
    // (BEFORE the slice is constructed). To prove the length
    // precheck does not read from the digest pointer, we pass a
    // pointer to a 1-byte buffer along with a claimed length of 5.
    // If the implementation built the slice naively from
    // (ptr, len), it would read past the allocation and trigger
    // UB / Miri sanitizer failures. The expected behavior is
    // immediate INVALID_DIGEST without any read.
    let v = unsafe { corelink_verifier_new_default_on() };
    let one_byte = [b'a'];
    let mut out: i32 = -1;
    let rc = unsafe {
        corelink_verifier_verify(
            v,
            core::ptr::null(),
            0,
            one_byte.as_ptr(),
            5, // claimed length larger than the allocation
            &mut out,
        )
    };
    assert_eq!(rc, COR_VERIFY_ERR_INVALID_DIGEST);
    assert_eq!(out, COR_VERIFY_ERR_INVALID_DIGEST);
    unsafe { corelink_verifier_free(v) };
}

#[test]
fn ffi_oversized_body_len_rejected_before_dereference() {
    // body_len > isize::MAX is rejected before slice creation.
    let v = unsafe { corelink_verifier_new_default_on() };
    let mut out: i32 = -1;
    let any_hex = "0".repeat(DIGEST_HEX_LEN);
    let rc = unsafe {
        corelink_verifier_verify(
            v,
            core::ptr::null(),
            (isize::MAX as usize).wrapping_add(1),
            any_hex.as_ptr(),
            any_hex.len(),
            &mut out,
        )
    };
    assert_eq!(rc, COR_VERIFY_ERR_INVALID_INPUT);
    assert_eq!(out, COR_VERIFY_ERR_INVALID_INPUT);
    unsafe { corelink_verifier_free(v) };
}

#[test]
fn ffi_null_out_error_code_does_not_panic() {
    // out_error_code is allowed to be null; the function must
    // still return the canonical tag.
    let body = b"x";
    let d = Digest::compute(body);
    let hex = d.to_hex();
    let v = unsafe { corelink_verifier_new_default_on() };
    let rc = unsafe {
        corelink_verifier_verify(
            v,
            body.as_ptr(),
            body.len(),
            hex.as_ptr(),
            hex.len(),
            core::ptr::null_mut(),
        )
    };
    assert_eq!(rc, COR_VERIFY_OK);
    unsafe { corelink_verifier_free(v) };
}

#[test]
fn ffi_non_utf8_digest_returns_invalid_digest() {
    // Length matches DIGEST_HEX_LEN, but bytes are non-UTF-8 → from_utf8
    // rejects, INVALID_DIGEST, no panic.
    let bytes = [0xFFu8; DIGEST_HEX_LEN];
    let v = unsafe { corelink_verifier_new_default_on() };
    let mut out: i32 = -1;
    let rc = unsafe {
        corelink_verifier_verify(
            v,
            core::ptr::null(),
            0,
            bytes.as_ptr(),
            bytes.len(),
            &mut out,
        )
    };
    assert_eq!(rc, COR_VERIFY_ERR_INVALID_DIGEST);
    assert_eq!(out, COR_VERIFY_ERR_INVALID_DIGEST);
    unsafe { corelink_verifier_free(v) };
}

#[test]
fn ffi_disabled_with_warn_returns_disabled_tag() {
    // Exercise the warn=1 leg of the disabled constructor (silent
    // path is covered above). Tracing subscriber is unset in test
    // by default → warn is a no-op surface, but the symbol must
    // still produce a usable handle and the canonical tag.
    let body = b"hi";
    let d = Digest::compute(body);
    let hex = d.to_hex();
    let v = unsafe { corelink_verifier_new_disabled(1) };
    let mut out: i32 = -1;
    let rc = unsafe {
        corelink_verifier_verify(
            v,
            body.as_ptr(),
            body.len(),
            hex.as_ptr(),
            hex.len(),
            &mut out,
        )
    };
    assert_eq!(rc, COR_VERIFY_ERR_DISABLED);
    assert_eq!(out, COR_VERIFY_ERR_DISABLED);
    unsafe { corelink_verifier_free(v) };
}

#[test]
fn ffi_error_code_lookup_table_is_populated() {
    for (code, expected) in [
        (COR_VERIFY_OK, "COR_VERIFY_OK"),
        (COR_VERIFY_ERR_MISMATCH, "COR_CAS_DIGEST_MISMATCH"),
        (COR_VERIFY_ERR_DISABLED, "COR_CAS_VERIFY_DISABLED"),
        (COR_VERIFY_ERR_INVALID_INPUT, "COR_VERIFY_ERR_INVALID_INPUT"),
        (
            COR_VERIFY_ERR_INVALID_DIGEST,
            "COR_VERIFY_ERR_INVALID_DIGEST",
        ),
    ] {
        let p = unsafe { corelink_verify_error_code_to_str(code) };
        assert!(!p.is_null(), "code {code} must resolve");
        let s = unsafe { CStr::from_ptr(p) }.to_str().expect("utf8");
        assert_eq!(s, expected, "code {code} maps to wrong string");
    }
    let p = unsafe { corelink_verify_error_code_to_str(0xDEAD_BEEF_u32 as i32) };
    assert!(p.is_null(), "unknown codes return null");
}

#[test]
fn ffi_null_digest_pointer_with_correct_len_returns_invalid_input() {
    // Even when digest_hex_len is exactly 64 (length precheck passes),
    // a null digest_hex_ptr must be rejected with INVALID_INPUT
    // before the slice is constructed. This is the "after the
    // length passes" branch in src/ffi.rs.
    let v = unsafe { corelink_verifier_new_default_on() };
    let mut out: i32 = -1;
    let rc = unsafe {
        corelink_verifier_verify(
            v,
            core::ptr::null(),
            0,
            core::ptr::null(),
            DIGEST_HEX_LEN,
            &mut out,
        )
    };
    assert_eq!(rc, COR_VERIFY_ERR_INVALID_INPUT);
    assert_eq!(out, COR_VERIFY_ERR_INVALID_INPUT);
    unsafe { corelink_verifier_free(v) };
}

#[test]
fn ffi_opt_out_total_increments_on_disabled_construction() {
    // The C-ABI accessor mirrors the Rust `opt_out_total()` and is
    // the canonical metric source for SDK FFI wrappers.
    let before = unsafe { corelink_verify_opt_out_total() };
    let v = unsafe { corelink_verifier_new_disabled(0) };
    let after = unsafe { corelink_verify_opt_out_total() };
    assert!(
        after > before,
        "opt_out_total must strictly increment on disabled ctor (before={before}, after={after})"
    );
    unsafe { corelink_verifier_free(v) };
}

/// FFI: `digest_hex_ptr == NULL && digest_hex_len == DIGEST_HEX_LEN` must
/// reject with `COR_VERIFY_ERR_INVALID_INPUT` *before* dereference.
///
/// Closes the codex r3 finding "raw-pointer validation branch in the FFI
/// entry point is still untested" — the null-then-length-passes-check
/// branch at `ffi.rs::corelink_verifier_verify` (post the length gate
/// short-circuits to invalid-input rather than UB).
#[test]
fn ffi_null_digest_hex_ptr_with_valid_length_rejects() {
    let body = b"hello world";
    let v = unsafe { corelink_verifier_new_default_on() };
    assert!(!v.is_null());
    let mut out: i32 = -1;
    let rc = unsafe {
        corelink_verifier_verify(
            v,
            body.as_ptr(),
            body.len(),
            core::ptr::null(),
            DIGEST_HEX_LEN, // exactly 64 — the length check passes
            &mut out,
        )
    };
    assert_eq!(rc, COR_VERIFY_ERR_INVALID_INPUT);
    assert_eq!(out, COR_VERIFY_ERR_INVALID_INPUT);
    unsafe { corelink_verifier_free(v) };
}

/// FFI: `digest_hex_ptr == NULL && digest_hex_len == 0` rejects via the
/// length gate (different code path from the length-passes-then-null
/// branch above).
#[test]
fn ffi_null_digest_hex_ptr_with_zero_length_rejects() {
    let body = b"hello world";
    let v = unsafe { corelink_verifier_new_default_on() };
    let mut out: i32 = -1;
    let rc = unsafe {
        corelink_verifier_verify(v, body.as_ptr(), body.len(), core::ptr::null(), 0, &mut out)
    };
    // Length 0 != 64 trips the length gate first.
    assert_eq!(rc, COR_VERIFY_ERR_INVALID_DIGEST);
    assert_eq!(out, COR_VERIFY_ERR_INVALID_DIGEST);
    unsafe { corelink_verifier_free(v) };
}

#[test]
fn ffi_signatures_have_expected_layout() {
    // Compile-time function-pointer checks: if any of these signatures
    // change, this assignment fails to compile and the ABI gate
    // rejects the PR.
    let _new_default_on: unsafe extern "C" fn() -> *mut corelink_client_verify::ClientVerifier =
        corelink_verifier_new_default_on;
    let _new_disabled: unsafe extern "C" fn(u8) -> *mut corelink_client_verify::ClientVerifier =
        corelink_verifier_new_disabled;
    let _free: unsafe extern "C" fn(*mut corelink_client_verify::ClientVerifier) =
        corelink_verifier_free;
    let _verify: unsafe extern "C" fn(
        *const corelink_client_verify::ClientVerifier,
        *const u8,
        usize,
        *const u8,
        usize,
        *mut i32,
    ) -> i32 = corelink_verifier_verify;
    let _err_str: unsafe extern "C" fn(i32) -> *const core::ffi::c_char =
        corelink_verify_error_code_to_str;
    let _opt_out_total: unsafe extern "C" fn() -> u64 = corelink_verify_opt_out_total;
    // DIGEST_BYTES and DIGEST_HEX_LEN are part of the public ABI
    // surface; pin their values at compile time so a future change
    // cannot silently alter the contract.
    const _: () = assert!(DIGEST_BYTES == 32);
    const _: () = assert!(DIGEST_HEX_LEN == 64);
    const _: () = assert!(DIGEST_HEX_LEN == DIGEST_BYTES * 2);
}
