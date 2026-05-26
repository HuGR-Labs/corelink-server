//! Go cgo bridge — C-ABI entry points consumed by `corelink-go/corelink.go`.
//!
//! All functions here follow the same null-safety + opaque-handle patterns as
//! `corelink-client-verify/ffi.rs`. The Go wrapper translates these into
//! idiomatic `(value, error)` returns.
//!
//! # Client-verify default-on
//!
//! `corelink_go_client_new(pat, pat_len, tenant_id, tenant_id_len,
//! client_verify)` takes `client_verify: u8` where `1` = enabled (default),
//! `0` = disabled (emits warning + ticks counter). This differs from the
//! Python pyO3 wrapper (where the default is enforced at the Go wrapper level
//! via `Config.ClientVerify` default `true`).
//!
//! Test inspection: `corelink_go_client_is_verify_enabled(handle)` returns
//! `1` when enabled, `0` when disabled.

#![allow(
    unsafe_code,
    reason = "C-ABI surface for Go cgo: opaque handle + raw pointer; same audit scope as \
              corelink-client-verify/ffi.rs. Every other module denies unsafe_code."
)]

use corelink_client_verify::ffi::{
    corelink_verifier_free, corelink_verifier_new_default_on, corelink_verifier_new_disabled,
    corelink_verifier_verify,
};
use corelink_client_verify::{ClientVerifier, Digest};

/// Opaque Go client handle. Owns a `ClientVerifier` + metadata.
///
/// Go cgo code works with `*CorelinkGoClient` (void * equivalent at the C
/// layer); the Go wrapper casts through `unsafe.Pointer`.
#[derive(Debug)]
pub struct CorelinkGoClient {
    /// The underlying verifier (single Rust truth; corelink-client-verify S-02).
    verifier: *mut ClientVerifier,
    /// Mirrors the `client_verify` flag for `IsClientVerifyEnabled`.
    client_verify_enabled: bool,
    /// PAT (never logged; CTRL-CRED-001).
    _pat: String,
    /// Tenant scope.
    _tenant_id: String,
}

// SAFETY: `CorelinkGoClient` is passed across the cgo boundary as an
// opaque pointer. Access is single-threaded per call from Go cgo (Go runtime
// pins goroutines during cgo calls). The raw pointer to `ClientVerifier` is
// owned exclusively by this struct.
unsafe impl Send for CorelinkGoClient {}
unsafe impl Sync for CorelinkGoClient {}

/// Allocate a new Go client handle.
///
/// # Inputs
///
/// - `pat_ptr / pat_len`: PAT bytes (UTF-8). Must not be null when `pat_len > 0`.
/// - `tenant_id_ptr / tenant_id_len`: tenant scope bytes (UTF-8).
/// - `client_verify`: `1` = enabled (canonical default), `0` = disabled.
///
/// # Returns
///
/// Non-null opaque handle. Pass to `corelink_go_client_free` exactly once.
/// Returns null if PAT or tenant_id bytes are not valid UTF-8.
///
/// # Safety
///
/// Caller must ensure pointers are valid for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn corelink_go_client_new(
    pat_ptr: *const u8,
    pat_len: usize,
    tenant_id_ptr: *const u8,
    tenant_id_len: usize,
    client_verify: u8,
) -> *mut CorelinkGoClient {
    // Validate + parse PAT
    if pat_len > 0 && pat_ptr.is_null() {
        return core::ptr::null_mut();
    }
    let pat_bytes = if pat_len == 0 {
        &[] as &[u8]
    } else {
        // SAFETY: caller invariant
        unsafe { core::slice::from_raw_parts(pat_ptr, pat_len) }
    };
    let Ok(pat) = core::str::from_utf8(pat_bytes) else {
        return core::ptr::null_mut();
    };

    // Validate + parse tenant_id
    if tenant_id_len > 0 && tenant_id_ptr.is_null() {
        return core::ptr::null_mut();
    }
    let tid_bytes = if tenant_id_len == 0 {
        &[] as &[u8]
    } else {
        // SAFETY: caller invariant
        unsafe { core::slice::from_raw_parts(tenant_id_ptr, tenant_id_len) }
    };
    let Ok(tenant_id) = core::str::from_utf8(tid_bytes) else {
        return core::ptr::null_mut();
    };

    // Construct verifier (single Rust truth; warn on opt-out)
    let verifier_ptr = if client_verify != 0 {
        // SAFETY: always returns non-null per corelink-client-verify contract
        unsafe { corelink_verifier_new_default_on() }
    } else {
        tracing::warn!(
            target: "corelink_go::client_verify",
            code = "COR_CAS_VERIFY_DISABLED",
            "client_verify=disabled; DISABLE NOT RECOMMENDED"
        );
        // warn=1 so the tracing::warn fires (already done above for the Go
        // layer; the Rust layer will also fire for completeness).
        // SAFETY: always returns non-null
        unsafe { corelink_verifier_new_disabled(1) }
    };

    let handle = CorelinkGoClient {
        verifier: verifier_ptr,
        client_verify_enabled: client_verify != 0,
        _pat: pat.to_string(),
        _tenant_id: tenant_id.to_string(),
    };
    Box::into_raw(Box::new(handle))
}

/// Free a client handle previously returned by `corelink_go_client_new`.
///
/// # Safety
///
/// `handle` must be a valid, non-freed pointer from `corelink_go_client_new`
/// or null (null is a no-op, consistent with `free(NULL)`).
#[no_mangle]
pub unsafe extern "C" fn corelink_go_client_free(handle: *mut CorelinkGoClient) {
    if handle.is_null() {
        return;
    }
    // SAFETY: caller invariant
    let client = unsafe { Box::from_raw(handle) };
    // Free the inner verifier.
    // SAFETY: verifier was created by corelink_verifier_new_*
    unsafe { corelink_verifier_free(client.verifier) };
    // `client` is dropped here, freeing the CorelinkGoClient box.
}

/// Return `1` if client-verify is enabled, `0` otherwise.
///
/// Test inspection: Go calls `client.IsClientVerifyEnabled()` which wraps
/// this symbol.
///
/// # Safety
///
/// `handle` must be non-null and valid.
#[no_mangle]
pub unsafe extern "C" fn corelink_go_client_is_verify_enabled(
    handle: *const CorelinkGoClient,
) -> u8 {
    if handle.is_null() {
        return 0;
    }
    // SAFETY: caller invariant
    let client = unsafe { &*handle };
    u8::from(client.client_verify_enabled)
}

/// Compute BLAKE3 of `body` and write the 64-char hex digest into
/// `out_digest_hex[..64]`.
///
/// This is the "Put" path: the Go wrapper calls this to get the canonical
/// digest before uploading to the server. Using the single Rust truth here
/// ensures the digest matches what the server + download path expects.
///
/// # Returns
///
/// `0` on success; non-zero on input validation failure (null pointers,
/// `body_len > isize::MAX`, null `out_digest_hex`).
///
/// # Safety
///
/// - `body_ptr[..body_len]` valid readable bytes (or `body_len == 0`).
/// - `out_digest_hex` must point to a writable buffer of at least 64 bytes.
#[no_mangle]
pub unsafe extern "C" fn corelink_go_client_put(
    body_ptr: *const u8,
    body_len: usize,
    out_digest_hex: *mut u8,
) -> i32 {
    const OK: i32 = 0;
    const ERR: i32 = 1;

    if out_digest_hex.is_null() {
        return ERR;
    }
    if body_len > isize::MAX as usize {
        return ERR;
    }
    let body: &[u8] = if body_len == 0 {
        &[]
    } else {
        if body_ptr.is_null() {
            return ERR;
        }
        // SAFETY: caller invariant
        unsafe { core::slice::from_raw_parts(body_ptr, body_len) }
    };

    let digest = Digest::compute(body);
    let hex = digest.to_hex();
    let hex_bytes = hex.as_bytes();
    // Exactly 64 bytes
    // SAFETY: out_digest_hex writable 64+ bytes per caller contract
    unsafe { core::ptr::copy_nonoverlapping(hex_bytes.as_ptr(), out_digest_hex, 64) };
    OK
}

/// Verify `body[..body_len]` against `digest_hex[..64]`.
///
/// Delegates to the underlying `ClientVerifier` (single Rust truth).
///
/// # Returns
///
/// Same error codes as `corelink_verifier_verify`:
/// - `0` = OK
/// - `1` = mismatch
/// - `2` = verify disabled
/// - `3` = invalid input
/// - `4` = invalid digest
///
/// # Safety
///
/// Same as `corelink_verifier_verify` — see that function's safety docs.
#[no_mangle]
pub unsafe extern "C" fn corelink_go_client_verify_get(
    handle: *const CorelinkGoClient,
    body_ptr: *const u8,
    body_len: usize,
    digest_hex_ptr: *const u8,
    digest_hex_len: usize,
) -> i32 {
    if handle.is_null() {
        return 3; // COR_VERIFY_ERR_INVALID_INPUT
    }
    // SAFETY: caller invariant
    let client = unsafe { &*handle };
    // SAFETY: delegates to corelink_verifier_verify with same safety contract
    unsafe {
        corelink_verifier_verify(
            client.verifier,
            body_ptr,
            body_len,
            digest_hex_ptr,
            digest_hex_len,
            core::ptr::null_mut(), // out_error_code unused; return value is sufficient
        )
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::*;
    use corelink_client_verify::Digest;

    #[test]
    fn new_default_on_verify_enabled() {
        let pat = b"test-pat";
        let tid = b"acme-corp";
        let handle = unsafe {
            corelink_go_client_new(
                pat.as_ptr(),
                pat.len(),
                tid.as_ptr(),
                tid.len(),
                1, // client_verify=true
            )
        };
        assert!(!handle.is_null());
        let enabled = unsafe { corelink_go_client_is_verify_enabled(handle) };
        assert_eq!(enabled, 1, "verify must be enabled by default");
        unsafe { corelink_go_client_free(handle) };
    }

    #[test]
    fn disabled_sets_flag() {
        let before = corelink_client_verify::opt_out_total();
        let pat = b"p";
        let tid = b"t";
        let handle = unsafe {
            corelink_go_client_new(pat.as_ptr(), pat.len(), tid.as_ptr(), tid.len(), 0)
        };
        assert!(!handle.is_null());
        let after = corelink_client_verify::opt_out_total();
        // opt_out_total ticks (once from our warn path + once from the
        // corelink_verifier_new_disabled call with warn=1)
        assert!(after > before, "opt_out_total must tick on disable");
        let enabled = unsafe { corelink_go_client_is_verify_enabled(handle) };
        assert_eq!(enabled, 0);
        unsafe { corelink_go_client_free(handle) };
    }

    #[test]
    fn put_produces_blake3_hex() {
        let body = b"hello world";
        let mut out = [0u8; 64];
        let rc = unsafe { corelink_go_client_put(body.as_ptr(), body.len(), out.as_mut_ptr()) };
        assert_eq!(rc, 0);
        let hex = core::str::from_utf8(&out).expect("utf8");
        let expected = Digest::compute(body).to_hex();
        assert_eq!(hex, expected);
    }

    #[test]
    fn verify_get_happy_path() {
        let pat = b"p";
        let tid = b"t";
        let body = b"test data";
        let handle = unsafe {
            corelink_go_client_new(pat.as_ptr(), pat.len(), tid.as_ptr(), tid.len(), 1)
        };
        let digest = Digest::compute(body).to_hex();
        let rc = unsafe {
            corelink_go_client_verify_get(
                handle,
                body.as_ptr(),
                body.len(),
                digest.as_ptr(),
                digest.len(),
            )
        };
        assert_eq!(rc, 0, "verify must pass for matching data");
        unsafe { corelink_go_client_free(handle) };
    }

    #[test]
    fn verify_get_mismatch() {
        let pat = b"p";
        let tid = b"t";
        let body = b"real data";
        let handle = unsafe {
            corelink_go_client_new(pat.as_ptr(), pat.len(), tid.as_ptr(), tid.len(), 1)
        };
        let wrong_digest = Digest::compute(b"different data").to_hex();
        let rc = unsafe {
            corelink_go_client_verify_get(
                handle,
                body.as_ptr(),
                body.len(),
                wrong_digest.as_ptr(),
                wrong_digest.len(),
            )
        };
        assert_eq!(rc, 1, "mismatch must return COR_VERIFY_ERR_MISMATCH=1");
        unsafe { corelink_go_client_free(handle) };
    }

    #[test]
    fn null_handle_is_invalid_input() {
        let body = b"x";
        let d = Digest::compute(body).to_hex();
        let rc = unsafe {
            corelink_go_client_verify_get(
                core::ptr::null(),
                body.as_ptr(),
                body.len(),
                d.as_ptr(),
                d.len(),
            )
        };
        assert_eq!(rc, 3);
    }

    #[test]
    fn free_null_is_noop() {
        unsafe { corelink_go_client_free(core::ptr::null_mut()) };
    }

    #[test]
    fn put_empty_body_is_valid() {
        let mut out = [0u8; 64];
        let rc = unsafe { corelink_go_client_put(core::ptr::null(), 0, out.as_mut_ptr()) };
        assert_eq!(rc, 0);
        let hex = core::str::from_utf8(&out).expect("utf8");
        assert_eq!(hex, Digest::compute(b"").to_hex());
    }
}
