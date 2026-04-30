//! C-ABI surface for SDK FFI wrappers (Python pyO3, Go cgo).
//!
//! Per WI §6.1, this module exposes ONLY signatures that are
//! cbindgen-stable: opaque handle pattern, primitive in/out parameters,
//! `i32` error codes (no `Result<>`, no generics, no `impl Trait`).
//!
//! JS/TS WASM consumers use a separate wasm-bindgen pipeline (lib.rs
//! Rust-native types crossing the wasm-bindgen boundary directly), NOT
//! this FFI surface.
//!
//! # Default-on contract
//!
//! The constructor surface is split into TWO functions
//! ([`corelink_verifier_new_default_on`] +
//! [`corelink_verifier_new_disabled`]) on purpose. A previous design
//! exposed a single `corelink_verifier_new(uint8_t enabled, uint8_t
//! warn_optout)` where `0` opted out, but `0` is the ambient default
//! for zero-initialized C structs and Go cgo wrapper memory, so a
//! buggy or malicious caller could silently disable verify just by
//! forgetting to populate the field. The split surface fails closed:
//! a caller who does not call any constructor at all has a null
//! handle, which the verify entry point rejects with
//! `COR_VERIFY_ERR_INVALID_INPUT`. Opting out requires the caller to
//! deliberately reach for the `..._disabled` symbol, which is what
//! CTRL-CAS-002 asks for.
//!
//! # Safety
//!
//! All `extern "C"` functions are `unsafe` because they take raw
//! pointers across the FFI boundary. Caller obligations are documented
//! per-function. The crate denies `unsafe` *Rust* code via
//! `#![deny(unsafe_code)]` at lib.rs; the FFI surface is the single
//! intentional exception, scoped to this module via
//! `#![allow(unsafe_code)]` below.
//!
//! Even in this exception, no raw pointer is dereferenced except after
//! a length check + null check, and no buffer is read past its claimed
//! length. Each function returns a stable `i32` error code which the
//! SDK side maps onto language-idiomatic exceptions.

#![allow(
    unsafe_code,
    reason = "C-ABI surface: opaque handle + raw pointer is the contract; \
              deny(unsafe_code) on the rest of the crate stays in effect. \
              See WI-S02-003 §6.1 + §9.7."
)]

use crate::config::VerifyConfig;
use crate::digest::{Digest, COR_CAS_DIGEST_MISMATCH, COR_CAS_VERIFY_DISABLED, DIGEST_LEN};
use crate::error::VerifyError;
use crate::verifier::ClientVerifier;

/// Raw BLAKE3 digest length (32 bytes). Re-emitted as a numeric
/// literal in the cbindgen C header so downstream C / Go / Python
/// wrappers reference a single canonical constant rather than
/// hard-coding `32`. Pinned to a literal (not
/// `corelink_hash::DIGEST_LEN`) because cbindgen with `parse_deps =
/// false` cannot resolve constants from the workspace dep tree —
/// the static assertion below keeps this literal in lockstep with
/// the upstream constant.
pub const DIGEST_BYTES: usize = 32;

/// Hex-encoded digest length (`DIGEST_BYTES * 2` = 64). The C-ABI
/// verify entry point rejects any other length with
/// `COR_VERIFY_ERR_INVALID_DIGEST` before touching the digest
/// pointer's memory.
pub const DIGEST_HEX_LEN: usize = 64;

// Compile-time guards: if BLAKE3 ever stops being 32 bytes (would
// require a coordinated CTRL-CAS update), these assertions fire at
// build time so the FFI numeric contract cannot drift silently.
const _: () = assert!(DIGEST_BYTES == DIGEST_LEN);
const _: () = assert!(DIGEST_HEX_LEN == DIGEST_BYTES * 2);

/// `out_error_code` tag: success.
pub const COR_VERIFY_OK: i32 = 0;

/// `out_error_code` tag: digest mismatch.
pub const COR_VERIFY_ERR_MISMATCH: i32 = 1;

/// `out_error_code` tag: verifier was constructed via
/// [`corelink_verifier_new_disabled`] and the caller invoked verify
/// anyway. Production callers expecting opt-out should not invoke
/// verify at all; this tag exists so integration tests + audit logs
/// can route the explicit opt-out path through a stable code.
pub const COR_VERIFY_ERR_DISABLED: i32 = 2;

/// `out_error_code` tag: a required input pointer was null OR a length
/// was non-zero with a null pointer / inconsistent length. Returned by
/// the verify entry point before any hash work begins.
pub const COR_VERIFY_ERR_INVALID_INPUT: i32 = 3;

/// `out_error_code` tag: hex parse of the claimed digest failed
/// (length != 64 OR non-hex char).
pub const COR_VERIFY_ERR_INVALID_DIGEST: i32 = 4;

/// Allocate a new default-on opaque `ClientVerifier`. Pass the
/// returned handle back to [`corelink_verifier_verify`] and finally
/// release it via [`corelink_verifier_free`].
///
/// This is the canonical SDK construction path: verify is enabled,
/// no warning fires (because nothing is being opted out), and the
/// `opt_out_total` counter does NOT tick.
///
/// # Errors / safety
///
/// Always returns a non-null `*mut ClientVerifier`. Allocation failure
/// is not modeled (the verifier is `Sized`, no per-instance dynamic
/// allocation beyond the heap box for FFI ownership transfer). If a
/// future change introduces a fallible path, this function will
/// return null and update its docs.
///
/// # Safety
///
/// Caller MUST eventually call [`corelink_verifier_free`] exactly once
/// on the returned handle. Passing the handle to anything other than
/// the documented FFI entry points is undefined behavior.
#[no_mangle]
pub unsafe extern "C" fn corelink_verifier_new_default_on() -> *mut ClientVerifier {
    let v = ClientVerifier::new(VerifyConfig::default());
    Box::into_raw(Box::new(v))
}

/// Allocate a new **explicitly disabled** opaque `ClientVerifier`.
///
/// Distinct C symbol from [`corelink_verifier_new_default_on`] so that
/// opt-out cannot happen by zero-initialization or argument-flag
/// confusion: the caller must deliberately reach for *this* symbol.
///
/// # Inputs
///
/// - `warn`: non-zero ⇒ emit `tracing::warn!`
///   `client_verify=disabled; proceed at own risk` on construction.
///   Zero ⇒ counter still ticks but no log fires (used by Python
///   pyO3 `__del__`-driven teardown paths that cannot afford an
///   extra log).
///
/// # Errors / safety
///
/// Always returns a non-null `*mut ClientVerifier`. Same notes as
/// [`corelink_verifier_new_default_on`] apply.
///
/// # Safety
///
/// Caller MUST eventually call [`corelink_verifier_free`] exactly once
/// on the returned handle.
#[no_mangle]
pub unsafe extern "C" fn corelink_verifier_new_disabled(warn: u8) -> *mut ClientVerifier {
    // The Rust public API exposes only `VerifyConfig::disabled()`
    // (warn=true). The silent variant is `pub(crate)`. Routing the
    // FFI silent path through the crate-private constructor is
    // intentional: the public Rust API stays opinionated (default-on
    // or warn-on-optout), and the silent escape hatch is gated to
    // explicit FFI callers + the property-test layer.
    let cfg = if warn == 0 {
        VerifyConfig::disabled_silent()
    } else {
        VerifyConfig::disabled()
    };
    let v = ClientVerifier::new(cfg);
    Box::into_raw(Box::new(v))
}

/// Free an opaque verifier handle previously returned by either
/// [`corelink_verifier_new_default_on`] or
/// [`corelink_verifier_new_disabled`].
///
/// # Safety
///
/// `handle` MUST be a pointer previously returned by one of the
/// `corelink_verifier_new_*` constructors and not yet freed. Passing
/// any other pointer is undefined behavior. A null pointer is treated
/// as a no-op (consistent with `free(NULL)` C semantics).
#[no_mangle]
pub unsafe extern "C" fn corelink_verifier_free(handle: *mut ClientVerifier) {
    if handle.is_null() {
        return;
    }
    // SAFETY: invariant from caller — `handle` came from `Box::into_raw`
    // in one of the constructor symbols and is not aliased.
    drop(unsafe { Box::from_raw(handle) });
}

/// Verify `body[..body_len]` against the hex-encoded digest in
/// `digest_hex[..digest_hex_len]`.
///
/// Writes the canonical error tag into `*out_error_code` (see the
/// `COR_VERIFY_*` constants). The function return value mirrors
/// `*out_error_code` for callers that prefer not to thread an
/// out-pointer (Python, JS) — they are guaranteed to agree.
///
/// # Inputs
///
/// - `handle`: opaque `*const ClientVerifier` from
///   [`corelink_verifier_new_default_on`] /
///   [`corelink_verifier_new_disabled`].
/// - `body_ptr`, `body_len`: raw body to hash. `body_len == 0` is
///   permitted (BLAKE3 is defined for empty input); in that case
///   `body_ptr` MAY be null. `body_len` must be `<= isize::MAX`
///   (slice creation precondition); larger lengths are rejected
///   with `COR_VERIFY_ERR_INVALID_INPUT` *before* any pointer
///   dereference.
/// - `digest_hex_ptr`, `digest_hex_len`: ASCII hex of the claimed
///   digest. Must be exactly [`DIGEST_HEX_LEN`] (= 64) bytes.
///   Any other length is rejected with
///   `COR_VERIFY_ERR_INVALID_DIGEST` before the pointer is read,
///   so a malformed length cannot trigger UB.
/// - `out_error_code`: must point to a writable `i32`. Receives the
///   same value returned by the function. If the pointer is null,
///   the function still returns the tag but cannot write through
///   it (callers are advised to always supply a writable cell).
///
/// # Safety
///
/// Caller MUST ensure:
/// - `handle` is a live verifier pointer (or null — null is
///   handled as `COR_VERIFY_ERR_INVALID_INPUT`).
/// - `body_ptr[..body_len]` is a valid readable slice (or
///   `body_len == 0`).
/// - `digest_hex_ptr[..DIGEST_HEX_LEN]` is a valid readable slice
///   of ASCII bytes when `digest_hex_len == DIGEST_HEX_LEN`. Other
///   lengths are rejected without reading from the pointer.
/// - `out_error_code` is null OR points to a writable `i32`.
#[no_mangle]
pub unsafe extern "C" fn corelink_verifier_verify(
    handle: *const ClientVerifier,
    body_ptr: *const u8,
    body_len: usize,
    digest_hex_ptr: *const u8,
    digest_hex_len: usize,
    out_error_code: *mut i32,
) -> i32 {
    /// Helper: write `tag` through `out_error_code` if non-null.
    ///
    /// SAFETY contract is the same as the outer function: the caller
    /// guarantees `out_error_code` is null or points to a writable
    /// `i32`. The helper is `unsafe` to keep the audit surface
    /// scoped.
    #[inline]
    unsafe fn write_out(out_error_code: *mut i32, tag: i32) {
        if !out_error_code.is_null() {
            // SAFETY: caller-side invariant — out_error_code points
            // to a writable i32 when non-null.
            unsafe { *out_error_code = tag };
        }
    }

    // 1. Validate every input length BEFORE touching any pointer's
    //    memory. `from_raw_parts` requires `len <= isize::MAX`; we
    //    enforce that with a generous integer check so a malicious
    //    or buggy caller cannot produce undefined behavior just by
    //    passing a huge `body_len`.
    if body_len > isize::MAX as usize {
        unsafe { write_out(out_error_code, COR_VERIFY_ERR_INVALID_INPUT) };
        return COR_VERIFY_ERR_INVALID_INPUT;
    }

    // 2. Handle: null → INVALID_INPUT.
    if handle.is_null() {
        unsafe { write_out(out_error_code, COR_VERIFY_ERR_INVALID_INPUT) };
        return COR_VERIFY_ERR_INVALID_INPUT;
    }

    // 3. Body pointer / length consistency. body_len==0 with a null
    //    pointer is permitted (BLAKE3 of empty input).
    if body_len > 0 && body_ptr.is_null() {
        unsafe { write_out(out_error_code, COR_VERIFY_ERR_INVALID_INPUT) };
        return COR_VERIFY_ERR_INVALID_INPUT;
    }

    // 4. Digest length MUST be exactly DIGEST_HEX_LEN (= 64). We
    //    check before dereference so any other length is rejected
    //    without reading from `digest_hex_ptr`. This collapses what
    //    used to be a dereference-then-validate ordering into the
    //    safer reject-then-dereference order.
    if digest_hex_len != DIGEST_HEX_LEN {
        unsafe { write_out(out_error_code, COR_VERIFY_ERR_INVALID_DIGEST) };
        return COR_VERIFY_ERR_INVALID_DIGEST;
    }
    if digest_hex_ptr.is_null() {
        unsafe { write_out(out_error_code, COR_VERIFY_ERR_INVALID_INPUT) };
        return COR_VERIFY_ERR_INVALID_INPUT;
    }

    // 5. Now safe to construct slices.
    // SAFETY: body_len is bounded by isize::MAX (checked above) and
    // either body_len==0 (we use an empty static slice) or
    // body_ptr is non-null (checked above). Caller-side invariant
    // covers underlying allocation lifetime.
    let body: &[u8] = if body_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(body_ptr, body_len) }
    };

    // SAFETY: digest_hex_ptr is non-null + length is exactly 64
    // (both checked above). 64 ≤ isize::MAX trivially.
    let hex_bytes: &[u8] =
        unsafe { core::slice::from_raw_parts(digest_hex_ptr, DIGEST_HEX_LEN) };
    let Ok(hex_str) = core::str::from_utf8(hex_bytes) else {
        unsafe { write_out(out_error_code, COR_VERIFY_ERR_INVALID_DIGEST) };
        return COR_VERIFY_ERR_INVALID_DIGEST;
    };
    let Ok(expected) = Digest::from_hex(hex_str) else {
        unsafe { write_out(out_error_code, COR_VERIFY_ERR_INVALID_DIGEST) };
        return COR_VERIFY_ERR_INVALID_DIGEST;
    };

    // SAFETY: caller obligation — `handle` came from a documented
    // `corelink_verifier_new_*` and has not been freed.
    let verifier: &ClientVerifier = unsafe { &*handle };

    let tag = match verifier.verify(body, &expected) {
        Ok(()) => COR_VERIFY_OK,
        Err(VerifyError::DigestMismatch { .. }) => COR_VERIFY_ERR_MISMATCH,
        Err(VerifyError::VerifyDisabled) => COR_VERIFY_ERR_DISABLED,
    };
    unsafe { write_out(out_error_code, tag) };
    tag
}

/// Read the process-wide opt-out counter (mirror of
/// [`crate::opt_out_total`] in the C-ABI surface).
///
/// SDK FFI wrappers (Python pyO3, Go cgo) call this to populate the
/// canonical `corelink_client_verify_optout_total{lang}` Grafana
/// metric without having to duplicate the accounting per language.
/// The counter is monotonic non-decreasing and never reset.
///
/// # Safety
///
/// Always safe to call. Returns the current u64 value at the moment
/// of the call; concurrent constructions of disabled verifiers may
/// race, so callers should treat the return as a sample rather than
/// a barrier-synchronized observation. `Ordering::Relaxed` is used
/// internally — see `verifier.rs` for rationale.
#[no_mangle]
pub unsafe extern "C" fn corelink_verify_opt_out_total() -> u64 {
    crate::verifier::opt_out_total()
}

/// Map an FFI error code to its canonical
/// `error_taxonomy.md` string. Returns a static null-terminated
/// pointer; callers MUST NOT free the returned pointer.
///
/// # Safety
///
/// Always safe to call. Returns `core::ptr::null()` for unknown codes
/// so SDK callers can detect a future-extended taxonomy without
/// crashing.
#[no_mangle]
pub unsafe extern "C" fn corelink_verify_error_code_to_str(code: i32) -> *const core::ffi::c_char {
    let s: &'static str = match code {
        COR_VERIFY_OK => "COR_VERIFY_OK",
        COR_VERIFY_ERR_MISMATCH => COR_CAS_DIGEST_MISMATCH,
        COR_VERIFY_ERR_DISABLED => COR_CAS_VERIFY_DISABLED,
        COR_VERIFY_ERR_INVALID_INPUT => "COR_VERIFY_ERR_INVALID_INPUT",
        COR_VERIFY_ERR_INVALID_DIGEST => "COR_VERIFY_ERR_INVALID_DIGEST",
        _ => return core::ptr::null(),
    };
    // NUL-terminated static byte ranges — every constant string used
    // here is `&'static str`, but the C ABI wants `*const c_char` to a
    // NUL-terminated buffer. We pre-allocate a NUL terminator by using
    // the static map below.
    let bytes: &'static [u8] = match s {
        "COR_VERIFY_OK" => b"COR_VERIFY_OK\0",
        "COR_CAS_DIGEST_MISMATCH" => b"COR_CAS_DIGEST_MISMATCH\0",
        "COR_CAS_VERIFY_DISABLED" => b"COR_CAS_VERIFY_DISABLED\0",
        "COR_VERIFY_ERR_INVALID_INPUT" => b"COR_VERIFY_ERR_INVALID_INPUT\0",
        "COR_VERIFY_ERR_INVALID_DIGEST" => b"COR_VERIFY_ERR_INVALID_DIGEST\0",
        // Unreachable given the outer match, but the type system
        // does not know that. Return null defensively.
        _ => return core::ptr::null(),
    };
    bytes.as_ptr().cast::<core::ffi::c_char>()
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code; panic on assertion failure is the contract"
)]
mod tests {
    use super::*;
    use core::ffi::CStr;

    #[test]
    fn ffi_round_trip_happy() {
        let body = b"hello world";
        let d = Digest::compute(body);
        let hex = d.to_hex();
        // SAFETY: pointers point to live, properly-sized data.
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

    #[test]
    fn ffi_mismatch_returns_mismatch_tag() {
        let body = b"hello world";
        let wrong = Digest::compute(b"goodbye");
        let hex = wrong.to_hex();
        let v = unsafe { corelink_verifier_new_default_on() };
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
        assert_eq!(rc, COR_VERIFY_ERR_MISMATCH);
        assert_eq!(out, COR_VERIFY_ERR_MISMATCH);
        unsafe { corelink_verifier_free(v) };
    }

    #[test]
    fn ffi_disabled_returns_disabled_tag_silent() {
        let body = b"hello world";
        let d = Digest::compute(body);
        let hex = d.to_hex();
        // Silent disabled to keep test logs quiet.
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

    #[test]
    fn ffi_null_handle_returns_invalid_input() {
        let body = b"hello";
        let hex = "00".repeat(32);
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

    #[test]
    fn ffi_invalid_digest_returns_invalid_digest_tag() {
        let body = b"hello";
        // 64 ASCII bytes but not all hex chars → reaches from_hex
        // and is rejected.
        let hex = "G".repeat(64);
        let v = unsafe { corelink_verifier_new_default_on() };
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
        assert_eq!(rc, COR_VERIFY_ERR_INVALID_DIGEST);
        assert_eq!(out, COR_VERIFY_ERR_INVALID_DIGEST);
        unsafe { corelink_verifier_free(v) };
    }

    #[test]
    fn ffi_wrong_digest_length_rejected_without_dereference() {
        // Pass a non-null but deliberately wrong-length digest. The
        // length check must trip BEFORE the slice is constructed,
        // so even an undersized buffer cannot trigger UB.
        let body = b"hello";
        let short = "ab"; // 2 bytes — must reject.
        let v = unsafe { corelink_verifier_new_default_on() };
        let mut out: i32 = -1;
        let rc = unsafe {
            corelink_verifier_verify(
                v,
                body.as_ptr(),
                body.len(),
                short.as_ptr(),
                short.len(),
                &mut out,
            )
        };
        assert_eq!(rc, COR_VERIFY_ERR_INVALID_DIGEST);
        assert_eq!(out, COR_VERIFY_ERR_INVALID_DIGEST);
        unsafe { corelink_verifier_free(v) };
    }

    #[test]
    fn ffi_oversized_body_len_rejected() {
        // body_len exceeds isize::MAX → rejected before any slice
        // is built (which would otherwise be UB).
        let v = unsafe { corelink_verifier_new_default_on() };
        let mut out: i32 = -1;
        let any_hex = "0".repeat(64);
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
    fn ffi_null_out_error_code_returns_tag_without_panic() {
        let body = b"hi";
        let d = Digest::compute(body);
        let hex = d.to_hex();
        let v = unsafe { corelink_verifier_new_default_on() };
        // Caller didn't pass an out cell — function must still
        // return the canonical tag (and not crash).
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
    fn ffi_non_utf8_digest_rejected_as_invalid_digest() {
        // Exactly 64 bytes but invalid UTF-8 → from_utf8 rejects.
        let bytes = [0xFFu8; 64];
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
    fn ffi_empty_body_with_null_ptr_is_ok() {
        // BLAKE3 of empty body.
        let d = Digest::compute(b"");
        let hex = d.to_hex();
        let v = unsafe { corelink_verifier_new_default_on() };
        let mut out: i32 = -1;
        let rc = unsafe {
            corelink_verifier_verify(v, core::ptr::null(), 0, hex.as_ptr(), hex.len(), &mut out)
        };
        assert_eq!(rc, COR_VERIFY_OK);
        unsafe { corelink_verifier_free(v) };
    }

    #[test]
    fn ffi_free_null_is_noop() {
        unsafe { corelink_verifier_free(core::ptr::null_mut()) };
    }

    #[test]
    fn ffi_error_code_to_str_known() {
        let p = unsafe { corelink_verify_error_code_to_str(COR_VERIFY_OK) };
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().expect("utf8");
        assert_eq!(s, "COR_VERIFY_OK");

        let p = unsafe { corelink_verify_error_code_to_str(COR_VERIFY_ERR_MISMATCH) };
        let s = unsafe { CStr::from_ptr(p) }.to_str().expect("utf8");
        assert_eq!(s, COR_CAS_DIGEST_MISMATCH);
    }

    #[test]
    fn ffi_error_code_to_str_unknown_returns_null() {
        let p = unsafe { corelink_verify_error_code_to_str(9999) };
        assert!(p.is_null());
    }
}
