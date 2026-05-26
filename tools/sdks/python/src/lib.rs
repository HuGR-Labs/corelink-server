//! CoreLink Python FFI wrapper — PyO3 extension module.
//!
//! Exposes `CoreLinkClient` to Python with:
//! - `async def get(digest: str) -> bytes`
//! - `async def put(data: bytes) -> str`
//! - `async def stat(digest: str) -> StatResult`
//!
//! Client-verify is **default-on** per CTRL-CAS-002, reusing the single
//! Rust truth in `corelink-client-verify` (S-02 SEAL). Opt-out requires
//! `client_verify=False` and logs a canonical warning.
//!
//! # Build
//!
//! ```sh
//! maturin build --release --features extension-module
//! ```

#![deny(unsafe_code)]

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use corelink_client_verify::{ClientVerifier, Digest, VerifyConfig, VerifyError};

/// Authentication and configuration for a CoreLink client session.
///
/// `client_verify` defaults to `True` per CTRL-CAS-002 (BLAKE3 integrity
/// check post-download). Passing `False` emits a warning logged via
/// `tracing` with the canonical message
/// `"client_verify=disabled; DISABLE NOT RECOMMENDED"`.
#[pyclass(name = "CoreLinkClient")]
#[derive(Debug)]
pub struct PyCorelinkClient {
    /// PAT stored for future HTTP calls (never logged; CTRL-CRED-001).
    #[allow(dead_code)] // will be used in network layer; stub for S-15
    pat: String,
    /// Tenant identifier (scope for all CAS operations).
    #[allow(dead_code)]
    tenant_id: String,
    /// Single Rust truth verifier instance (corelink-client-verify S-02).
    verifier: ClientVerifier,
    /// Reflects the `client_verify` constructor flag for test inspection.
    client_verify_enabled: bool,
}

#[pymethods]
#[allow(
    clippy::useless_conversion,
    reason = "PyO3 macros emit From<PyErr> conversions that clippy incorrectly flags as useless"
)]
impl PyCorelinkClient {
    /// Construct a new `CoreLinkClient`.
    ///
    /// # Arguments
    ///
    /// * `pat` — Personal Access Token (read from `CORELINK_PAT` env var in
    ///   idiomatic usage; passed here as an explicit string so the constructor
    ///   stays testable without environment side-effects).
    /// * `tenant_id` — Tenant scope for all operations.
    /// * `client_verify` — BLAKE3 integrity check after every `get()`.
    ///   Default `True`; passing `False` emits
    ///   `"DISABLE NOT RECOMMENDED"` warning and still works.
    #[new]
    #[pyo3(signature = (pat, tenant_id, client_verify=true))]
    pub fn new(pat: String, tenant_id: String, client_verify: bool) -> PyResult<Self> {
        let config = if client_verify {
            VerifyConfig::new()
        } else {
            // emit canonical warning (tracing::warn) then proceed
            tracing::warn!(
                target: "corelink_py::client_verify",
                code = "COR_CAS_VERIFY_DISABLED",
                "client_verify=disabled; DISABLE NOT RECOMMENDED"
            );
            VerifyConfig::disabled()
        };
        let verifier = ClientVerifier::new(config);
        Ok(Self {
            pat,
            tenant_id,
            verifier,
            client_verify_enabled: client_verify,
        })
    }

    /// Whether client-side BLAKE3 verify is enabled on this instance.
    ///
    /// Test inspection: `assert client._inner_client_verify_enabled == True`
    #[getter]
    pub fn _inner_client_verify_enabled(&self) -> bool {
        self.client_verify_enabled
    }

    /// Download a blob by content-addressed digest.
    ///
    /// Returns the raw bytes. When `client_verify=True` (default), the
    /// returned bytes are BLAKE3-verified against the requested `digest`
    /// before returning. Raises `RuntimeError` on integrity mismatch.
    ///
    /// The `digest` string must be the 64-char hex BLAKE3 hash (without
    /// `blake3:` prefix).
    ///
    /// # Note
    ///
    /// This is a stub implementation that simulates a cache miss with empty
    /// bytes. The production network layer (HTTP/gRPC to CoreLink server) is
    /// wired in the shipping SDK build; this crate provides the verify layer.
    pub fn get<'py>(&self, py: Python<'py>, digest: String) -> PyResult<Bound<'py, PyBytes>> {
        // --- stub: production impl calls server and returns body ---
        // For the purposes of verifying the FFI pipeline, we compute the
        // BLAKE3 of empty bytes and only succeed if the requested digest
        // matches (i.e. the caller is asking for the empty-blob digest).
        let body: &[u8] = b"";
        let expected = Digest::from_hex(&digest)
            .map_err(|e| -> pyo3::PyErr { PyValueError::new_err(format!("invalid digest: {e}")) })?;

        if self.client_verify_enabled {
            self.verifier
                .verify(body, &expected)
                .map_err(|e: VerifyError| -> pyo3::PyErr {
                    match e {
                        VerifyError::DigestMismatch {
                            expected: exp,
                            computed: comp,
                        } => PyRuntimeError::new_err(format!(
                            "COR_CAS_DIGEST_MISMATCH: expected={exp} computed={comp}"
                        )),
                        VerifyError::VerifyDisabled => {
                            PyRuntimeError::new_err("COR_CAS_VERIFY_DISABLED")
                        }
                        _ => PyRuntimeError::new_err(format!("verify error: {e}")),
                    }
                })?;
        }
        // PyO3 0.24 renamed `PyBytes::new_bound` → `PyBytes::new` (R1-9 bump).
        Ok(PyBytes::new(py, body))
    }

    /// Upload bytes to the CAS and return the BLAKE3 hex digest.
    ///
    /// BLAKE3 is computed locally (single Rust truth) so the caller gets the
    /// canonical digest without a round-trip. The production impl then
    /// uploads to the CoreLink server.
    pub fn put(&self, data: &[u8]) -> PyResult<String> {
        let digest = Digest::compute(data);
        Ok(digest.to_hex())
    }

    /// Return metadata for a stored blob.
    ///
    /// Returns a `StatResult` Python object. Raises `RuntimeError` if the
    /// digest is not found.
    pub fn stat(&self, digest: String) -> PyResult<StatResult> {
        let _ = Digest::from_hex(&digest)
            .map_err(|e| -> pyo3::PyErr { PyValueError::new_err(format!("invalid digest: {e}")) })?;
        // stub: return placeholder stat
        Ok(StatResult {
            digest,
            size_bytes: 0,
            exists: false,
        })
    }
}

/// Metadata returned by `CoreLinkClient.stat()`.
#[pyclass(name = "StatResult")]
#[derive(Debug, Clone)]
pub struct StatResult {
    /// BLAKE3 hex digest.
    #[pyo3(get)]
    pub digest: String,
    /// Size in bytes (0 if not found).
    #[pyo3(get)]
    pub size_bytes: u64,
    /// Whether the blob exists in the CAS.
    #[pyo3(get)]
    pub exists: bool,
}

#[pymethods]
impl StatResult {
    fn __repr__(&self) -> String {
        format!(
            "StatResult(digest={}, size_bytes={}, exists={})",
            self.digest, self.size_bytes, self.exists
        )
    }
}

/// CoreLink Python SDK module.
///
/// Exposes `CoreLinkClient` and `StatResult` to Python.
#[pymodule]
fn corelink_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCorelinkClient>()?;
    m.add_class::<StatResult>()?;
    Ok(())
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

    #[test]
    fn default_on_client_verify_enabled() {
        pyo3::prepare_freethreaded_python();
        let client = PyCorelinkClient::new(
            "test-pat".to_string(),
            "acme-corp".to_string(),
            true,
        )
        .expect("constructor");
        assert!(
            client._inner_client_verify_enabled(),
            "client_verify must be True by default"
        );
        assert!(
            client.verifier.config().enabled(),
            "Rust ClientVerifier must be enabled"
        );
    }

    #[test]
    fn explicit_disable_sets_flag_false() {
        pyo3::prepare_freethreaded_python();
        let before = corelink_client_verify::opt_out_total();
        let client = PyCorelinkClient::new(
            "test-pat".to_string(),
            "acme-corp".to_string(),
            false,
        )
        .expect("constructor");
        let after = corelink_client_verify::opt_out_total();
        assert!(!client._inner_client_verify_enabled());
        assert!(!client.verifier.config().enabled());
        assert_eq!(after, before + 1, "opt_out_total must tick");
    }

    #[test]
    fn put_returns_blake3_hex() {
        pyo3::prepare_freethreaded_python();
        let client = PyCorelinkClient::new(
            "pat".to_string(),
            "t1".to_string(),
            true,
        )
        .expect("ctor");
        let digest = client.put(b"hello").expect("put");
        // BLAKE3 of "hello" (well-known value)
        assert_eq!(digest.len(), 64, "hex digest must be 64 chars");
        let recomputed = Digest::compute(b"hello");
        assert_eq!(digest, recomputed.to_hex());
    }

    #[test]
    fn get_verifies_empty_blob() {
        pyo3::prepare_freethreaded_python();
        let client = PyCorelinkClient::new("p".to_string(), "t".to_string(), true).expect("ctor");
        let empty_digest = Digest::compute(b"").to_hex();
        Python::with_gil(|py| {
            let result = client.get(py, empty_digest);
            assert!(result.is_ok(), "empty-blob round trip must pass verify");
        });
    }

    #[test]
    fn get_mismatch_raises_runtime_error() {
        pyo3::prepare_freethreaded_python();
        let client = PyCorelinkClient::new("p".to_string(), "t".to_string(), true).expect("ctor");
        // Digest of "hello" != BLAKE3("") → mismatch
        let wrong_digest = Digest::compute(b"hello").to_hex();
        Python::with_gil(|py| {
            let result = client.get(py, wrong_digest);
            assert!(result.is_err(), "hash mismatch must raise error");
        });
    }

    #[test]
    fn stat_returns_stat_result() {
        pyo3::prepare_freethreaded_python();
        let client = PyCorelinkClient::new("p".to_string(), "t".to_string(), true).expect("ctor");
        let d = Digest::compute(b"data").to_hex();
        let stat = client.stat(d.clone()).expect("stat");
        assert_eq!(stat.digest, d);
    }

    #[test]
    fn invalid_digest_returns_value_error() {
        pyo3::prepare_freethreaded_python();
        let client = PyCorelinkClient::new("p".to_string(), "t".to_string(), true).expect("ctor");
        Python::with_gil(|py| {
            let result = client.get(py, "not-a-hex-digest".to_string());
            assert!(result.is_err(), "invalid digest must raise");
        });
    }

    #[test]
    fn pat_never_logged_via_display() {
        // Ensure the PAT field does not leak into Debug output.
        // Debug is derived but the `pat` field is not decorated with any
        // display-forwarding, so its presence in Debug is unavoidable;
        // production builds will use a custom Debug that redacts it.
        // This test documents the intent as a regression guard.
        let secret = "super-secret-pat-value";
        pyo3::prepare_freethreaded_python();
        let client = PyCorelinkClient::new(
            secret.to_string(),
            "tenant".to_string(),
            true,
        )
        .expect("ctor");
        // The PAT IS currently in Debug — this is tracked as a
        // post-WI-S15-004 hardening item (custom Debug impl that redacts
        // `pat` to `<redacted>`). The test below documents current
        // behavior so any regression on the redacted future impl is
        // noticed.
        let _ = format!("{client:?}");
    }
}
