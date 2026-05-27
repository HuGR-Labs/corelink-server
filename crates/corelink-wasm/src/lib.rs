//! CoreLink WASM module — wasm-bindgen entry point.
//!
//! Exposes `CoreLinkClient` to JS/TS with:
//! - `get(digest: string): Promise<Uint8Array>`
//! - `put(data: Uint8Array): Promise<string>`
//! - `stat(digest: string): Promise<StatResult>`
//!
//! Client-verify is **default-on** per CTRL-CAS-002, reusing the single
//! Rust truth in `corelink-client-verify` (S-02 SEAL). Opt-out requires
//! `clientVerify: false` in `ClientConfig` and logs a canonical warning.
//!
//! # Build
//!
//! ```sh
//! wasm-pack build --target web --release
//! # then optimize:
//! wasm-opt -O3 pkg/corelink_wasm_bg.wasm -o pkg/corelink_wasm_bg.wasm
//! ```
//!
//! # Bundle size gate
//!
//! CI asserts the wasm-opt'd `.wasm` file is ≤ 1MB (1_048_576 bytes).

#![deny(unsafe_code)]

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use corelink_client_verify::{ClientVerifier, Digest, VerifyConfig, VerifyError};

/// Configuration for `CoreLinkClient`.
///
/// Passed to `new CoreLinkClient(config)` from JS/TS.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    /// Personal Access Token.
    pub pat: String,
    /// Tenant scope.
    pub tenant_id: String,
    /// BLAKE3 verify after every `get()`. Defaults to `true` per CTRL-CAS-002.
    #[serde(default = "default_true")]
    pub client_verify: bool,
}

fn default_true() -> bool {
    true
}

/// CoreLink CAS client for JavaScript / TypeScript via WASM.
///
/// Wraps the Rust `corelink-client-verify` crate (single source of truth per
/// ADR-0016) via wasm-bindgen. All BLAKE3 integrity verification uses the
/// canonical Rust implementation.
///
/// ```typescript
/// import { CoreLinkClient } from "@corelink/client";
///
/// const client = new CoreLinkClient({
///   pat: process.env.CORELINK_PAT!,
///   tenantId: "acme-corp",
/// });
///
/// const digest = await client.put(new Uint8Array([0x68, 0x65, 0x6c, 0x6c, 0x6f]));
/// const data = await client.get(digest);
/// ```
#[wasm_bindgen]
#[derive(Debug)]
pub struct CoreLinkClient {
    /// PAT (never exposed to JS; CTRL-CRED-001).
    #[allow(dead_code)]
    pat: String,
    /// Tenant scope.
    #[allow(dead_code)]
    tenant_id: String,
    /// Single Rust truth verifier (corelink-client-verify S-02).
    verifier: ClientVerifier,
    /// Reflects `clientVerify` constructor flag for test inspection.
    client_verify_enabled: bool,
}

#[wasm_bindgen]
impl CoreLinkClient {
    /// Construct a new `CoreLinkClient` from a JS config object.
    ///
    /// # Arguments
    ///
    /// Accepts a JS object `{ pat, tenantId, clientVerify? }`.
    /// `clientVerify` defaults to `true` per CTRL-CAS-002.
    ///
    /// # Errors
    ///
    /// Returns a JS `Error` if `config` cannot be deserialized.
    #[wasm_bindgen(constructor)]
    pub fn new(config: JsValue) -> Result<CoreLinkClient, JsValue> {
        let cfg: ClientConfig = serde_wasm_bindgen::from_value(config)
            .map_err(|e| JsValue::from_str(&format!("invalid config: {e}")))?;

        let verify_config = if cfg.client_verify {
            VerifyConfig::new()
        } else {
            // Warn before constructing the verifier so the tracing::warn
            // fires before the counter ticks (test ordering is predictable).
            tracing::warn!(
                target: "corelink_wasm::client_verify",
                code = "COR_CAS_VERIFY_DISABLED",
                "client_verify=disabled; DISABLE NOT RECOMMENDED"
            );
            VerifyConfig::disabled()
        };

        Ok(CoreLinkClient {
            verifier: ClientVerifier::new(verify_config),
            client_verify_enabled: cfg.client_verify,
            pat: cfg.pat,
            tenant_id: cfg.tenant_id,
        })
    }

    /// Whether client-side BLAKE3 verify is enabled on this instance.
    ///
    /// Test inspection: `expect(client._clientVerifyEnabled).toBe(true)`.
    #[wasm_bindgen(getter = _clientVerifyEnabled)]
    pub fn client_verify_enabled(&self) -> bool {
        self.client_verify_enabled
    }

    /// Download a blob by BLAKE3 hex digest.
    ///
    /// Returns the raw bytes as `Uint8Array`. When `clientVerify: true`
    /// (default), the bytes are BLAKE3-verified before returning.
    ///
    /// # Errors
    ///
    /// Throws a JS `Error` on digest mismatch (`COR_CAS_DIGEST_MISMATCH`)
    /// or invalid digest format.
    pub fn get(&self, digest: String) -> Result<js_sys::Uint8Array, JsValue> {
        // Delegate to pure-Rust helper so tests can exercise verify logic
        // without a JS runtime.
        let body = self
            .get_inner(&digest)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(js_sys::Uint8Array::from(body.as_slice()))
    }

    /// Upload bytes to the CAS and return the BLAKE3 hex digest.
    ///
    /// The digest is computed locally using the single Rust truth
    /// (BLAKE3 via `corelink-client-verify`).
    ///
    /// # Errors
    ///
    /// Throws a JS `Error` on upload failure.
    pub fn put(&self, data: js_sys::Uint8Array) -> Result<String, JsValue> {
        let bytes = data.to_vec();
        Ok(self.put_inner(&bytes))
    }

    /// Return metadata for a stored blob.
    ///
    /// Returns a JS object `{ digest, sizeBytes, exists }`.
    ///
    /// # Errors
    ///
    /// Throws a JS `Error` on invalid digest or server error.
    pub fn stat(&self, digest: String) -> Result<JsValue, JsValue> {
        let result = self
            .stat_inner(&digest)
            .map_err(|e| JsValue::from_str(&e))?;
        serde_wasm_bindgen::to_value(&result)
            .map_err(|e| JsValue::from_str(&format!("serialization error: {e}")))
    }
}

/// Pure-Rust helpers — not exposed to wasm-bindgen; used by both the
/// `#[wasm_bindgen]` methods above and the native Rust unit tests below.
impl CoreLinkClient {
    /// Inner get: parse + verify, return owned bytes.
    fn get_inner(&self, digest: &str) -> Result<Vec<u8>, String> {
        let expected =
            Digest::from_hex(digest).map_err(|e| format!("invalid digest: {e}"))?;

        // Stub: production impl fetches from server.
        let body: Vec<u8> = Vec::new();

        if self.client_verify_enabled {
            self.verifier
                .verify(&body, &expected)
                .map_err(|e| match e {
                    VerifyError::DigestMismatch {
                        expected: exp,
                        computed: comp,
                    } => format!("COR_CAS_DIGEST_MISMATCH: expected={exp} computed={comp}"),
                    VerifyError::VerifyDisabled => "COR_CAS_VERIFY_DISABLED".to_string(),
                    _ => format!("verify error: {e}"),
                })?;
        }
        Ok(body)
    }

    /// Inner put: compute BLAKE3 of bytes, return hex digest.
    fn put_inner(&self, data: &[u8]) -> String {
        Digest::compute(data).to_hex()
    }

    /// Inner stat: validate digest, return StatResult.
    fn stat_inner(&self, digest: &str) -> Result<StatResult, String> {
        let _ = Digest::from_hex(digest).map_err(|e| format!("invalid digest: {e}"))?;
        Ok(StatResult {
            digest: digest.to_string(),
            size_bytes: 0,
            exists: false,
        })
    }
}

/// Metadata returned by `CoreLinkClient.stat()`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatResult {
    /// BLAKE3 hex digest.
    pub digest: String,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Whether the blob exists.
    pub exists: bool,
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
    use wasm_bindgen_test::wasm_bindgen_test;

    // These tests run in wasm-bindgen-test framework (Node.js or browser).
    // The non-wasm tests below use direct Rust calls for local CI.

    // ---- Rust-side unit tests (run with `cargo test`) ----

    fn make_client(client_verify: bool) -> CoreLinkClient {
        let config = ClientConfig {
            pat: "test-pat".to_string(),
            tenant_id: "acme-corp".to_string(),
            client_verify,
        };
        let verifier = ClientVerifier::new(if client_verify {
            VerifyConfig::new()
        } else {
            VerifyConfig::disabled()
        });
        CoreLinkClient {
            pat: config.pat,
            tenant_id: config.tenant_id,
            verifier,
            client_verify_enabled: config.client_verify,
        }
    }

    #[test]
    fn default_on_verify_enabled() {
        let client = make_client(true);
        assert!(
            client.client_verify_enabled(),
            "_clientVerifyEnabled must be true by default"
        );
        assert!(client.verifier.config().enabled());
    }

    #[test]
    fn explicit_disable_sets_flag() {
        let before = corelink_client_verify::opt_out_total();
        let client = make_client(false);
        let after = corelink_client_verify::opt_out_total();
        assert!(!client.client_verify_enabled());
        assert!(!client.verifier.config().enabled());
        assert_eq!(after, before + 1, "opt_out_total must tick");
    }

    #[test]
    fn put_returns_blake3_hex() {
        let client = make_client(true);
        let digest = client.put_inner(b"hello world");
        assert_eq!(digest.len(), 64);
        assert_eq!(digest, Digest::compute(b"hello world").to_hex());
    }

    #[test]
    fn get_passes_for_empty_blob() {
        let client = make_client(true);
        let empty_hex = Digest::compute(b"").to_hex();
        let result = client.get_inner(&empty_hex);
        assert!(result.is_ok(), "empty-blob digest must pass verify");
    }

    #[test]
    fn get_mismatch_errors() {
        let client = make_client(true);
        let wrong = Digest::compute(b"something else").to_hex();
        let result = client.get_inner(&wrong);
        assert!(result.is_err(), "mismatch must return Err");
    }

    #[test]
    fn invalid_digest_errors() {
        let client = make_client(true);
        let result = client.get_inner("not-hex");
        assert!(result.is_err(), "invalid digest must error");
    }

    #[test]
    fn stat_returns_result() {
        let client = make_client(true);
        let d = Digest::compute(b"data").to_hex();
        let result = client.stat_inner(&d);
        assert!(result.is_ok());
    }

    // ---- Property-based invariants (WI-PROPTEST-FU-W33-002) ----
    //
    // Density follow-up per `specs/_audits/sealed/proptest-followup-tickets.md`.
    // These exercise the pure-Rust helpers (`*_inner`) which back the
    // `#[wasm_bindgen]` methods — covering invariants that must hold across
    // arbitrary inputs at the JS/TS boundary. `PROPTEST_CASES` env var
    // overrides the default per workspace convention (S-07 P1-2).
    //
    // Why colocate (not `tests/prop_*.rs`): integration tests cannot call
    // the `JsValue`-based public constructor on the host target, and the
    // pure-Rust `*_inner` helpers are crate-private. Colocation also matches
    // the existing in-module `wasm_bindgen_test` pattern.

    use proptest::prelude::*;

    fn proptest_cases() -> u32 {
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(256)
    }

    /// Strategy: arbitrary byte vector up to 4 KiB.
    ///
    /// Bounded so a release-mode `PROPTEST_CASES=256` stress run stays well
    /// under the workspace per-test budget while still exercising chunk
    /// boundaries of the underlying BLAKE3 implementation.
    fn arb_bytes() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 0..=4096)
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: proptest_cases(),
            ..ProptestConfig::default()
        })]

        /// INV-WASM-PUT-HEX64: `put_inner` ALWAYS returns a 64-character
        /// lowercase hex digest (BLAKE3 → 32 bytes → 64 hex). Asserts both
        /// length AND that every char is in `[0-9a-f]` (catches a future
        /// regression that swaps in uppercase or base64).
        #[test]
        fn prop_put_inner_emits_lowercase_hex64(data in arb_bytes()) {
            let client = make_client(true);
            let hex = client.put_inner(&data);
            prop_assert_eq!(hex.len(), 64, "BLAKE3 hex digest must be 64 chars");
            prop_assert!(
                hex.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                "digest must be lowercase hex: got {hex}"
            );
        }

        /// INV-WASM-PUT-DETERMINISTIC: `put_inner` is a pure function of its
        /// input bytes — two calls with identical input MUST yield identical
        /// digests. Catches accidental non-determinism (e.g. salted hash,
        /// time-based seed) in any future swap of the hash backend.
        #[test]
        fn prop_put_inner_is_deterministic(data in arb_bytes()) {
            let client_a = make_client(true);
            let client_b = make_client(false);
            let h1 = client_a.put_inner(&data);
            let h2 = client_a.put_inner(&data);
            // Determinism holds across BOTH client_verify=on/off because
            // verify config is independent of the hash function.
            let h3 = client_b.put_inner(&data);
            prop_assert_eq!(&h1, &h2, "same bytes, same client → same digest");
            prop_assert_eq!(&h1, &h3, "client_verify flag must not affect put");
        }

        /// INV-WASM-PUT-ROUNDTRIP: encode → parse → re-encode is identity.
        /// `Digest::from_hex(put_inner(x)).to_hex() == put_inner(x)`. Guards
        /// the hex codec at the WASM↔JS boundary against silent corruption.
        #[test]
        fn prop_put_inner_roundtrip_hex(data in arb_bytes()) {
            let client = make_client(true);
            let hex1 = client.put_inner(&data);
            let parsed = Digest::from_hex(&hex1)
                .map_err(|e| TestCaseError::fail(format!("from_hex failed: {e}")))?;
            let hex2 = parsed.to_hex();
            prop_assert_eq!(hex1, hex2, "hex encode/decode must round-trip");
        }

        /// INV-WASM-GET-VERIFY-MATCH: with `client_verify=true`, `get_inner`
        /// returns Ok iff the supplied digest matches the BLAKE3 of the
        /// (currently stubbed) empty body, and otherwise returns the
        /// `COR_CAS_DIGEST_MISMATCH` error string. Exercises the production
        /// verify path on the WASM critical path.
        #[test]
        fn prop_get_inner_verify_match_or_mismatch(data in arb_bytes()) {
            let client = make_client(true);
            let empty_hex = Digest::compute(b"").to_hex();
            let candidate_hex = Digest::compute(&data).to_hex();
            let result = client.get_inner(&candidate_hex);
            if candidate_hex == empty_hex {
                // arb_bytes can produce the empty Vec — verify must succeed.
                match result {
                    Ok(body) => prop_assert!(
                        body.is_empty(),
                        "stub body for empty-digest must be empty"
                    ),
                    Err(e) => prop_assert!(
                        false,
                        "empty-blob digest must verify Ok, got Err: {e}"
                    ),
                }
            } else {
                match result {
                    Err(e) => prop_assert!(
                        e.starts_with("COR_CAS_DIGEST_MISMATCH"),
                        "mismatch error must use canonical taxonomy code, got: {e}"
                    ),
                    Ok(_) => prop_assert!(
                        false,
                        "non-empty digest must NOT verify against empty stub"
                    ),
                }
            }
        }

        /// INV-WASM-GET-PANIC-FREE: `get_inner` must NEVER panic, regardless
        /// of the shape of the digest string at the JS boundary (arbitrary
        /// UTF-8, oversized, mixed case, control chars, etc.). Either Ok or
        /// a typed Err — never a panic. This is the canonical panic-free
        /// invariant required of any FFI entry point.
        #[test]
        fn prop_get_inner_panic_free(digest in ".*") {
            let client = make_client(true);
            // Any panic here will be caught by proptest and reported as a
            // failing case — exactly what we want.
            let _ = client.get_inner(&digest);
        }

        /// INV-WASM-STAT-ECHO: `stat_inner` echoes the input digest into
        /// `StatResult.digest` whenever the digest parses; size=0 and
        /// exists=false in the stub. Matches AND asserts each named field
        /// (per S-08 P1-1 lesson — no `matches!` anti-pattern).
        #[test]
        fn prop_stat_inner_echoes_valid_digest(data in arb_bytes()) {
            let client = make_client(true);
            let hex = client.put_inner(&data);
            let result = client.stat_inner(&hex);
            match result {
                Ok(stat) => {
                    prop_assert_eq!(&stat.digest, &hex, "stat must echo digest");
                    prop_assert_eq!(stat.size_bytes, 0, "stub size_bytes must be 0");
                    prop_assert!(!stat.exists, "stub exists must be false");
                }
                Err(e) => prop_assert!(
                    false,
                    "valid hex digest must parse for stat, got Err: {e}"
                ),
            }
        }
    }

    // ---- wasm-bindgen-test (runs in Node.js / browser via wasm-pack test) ----

    #[wasm_bindgen_test]
    fn wasm_put_produces_digest() {
        use js_sys::Uint8Array;

        // Build a plain JS object via js_sys::Object to avoid serde_json dep
        // in tests (serde_json is not a workspace dep; serde-wasm-bindgen
        // handles serialization of the ClientConfig at runtime).
        let obj = js_sys::Object::new();
        js_sys::Reflect::set(&obj, &"pat".into(), &"test".into()).unwrap();
        js_sys::Reflect::set(&obj, &"tenantId".into(), &"t1".into()).unwrap();
        js_sys::Reflect::set(&obj, &"clientVerify".into(), &true.into()).unwrap();
        let config: wasm_bindgen::JsValue = obj.into();

        let client = CoreLinkClient::new(config).expect("client");
        assert!(client.client_verify_enabled());

        let data = Uint8Array::from(&[0x68u8, 0x65, 0x6c, 0x6c, 0x6f] as &[u8]);
        let digest = client.put(data).expect("put");
        assert_eq!(digest.len(), 64);
    }

    #[wasm_bindgen_test]
    fn wasm_client_verify_default_on() {
        let obj = js_sys::Object::new();
        js_sys::Reflect::set(&obj, &"pat".into(), &"p".into()).unwrap();
        js_sys::Reflect::set(&obj, &"tenantId".into(), &"t".into()).unwrap();
        let config: wasm_bindgen::JsValue = obj.into();
        let client = CoreLinkClient::new(config).expect("client");
        assert!(
            client.client_verify_enabled(),
            "clientVerify must default to true"
        );
    }

    #[wasm_bindgen_test]
    fn wasm_client_verify_opt_out() {
        let obj = js_sys::Object::new();
        js_sys::Reflect::set(&obj, &"pat".into(), &"p".into()).unwrap();
        js_sys::Reflect::set(&obj, &"tenantId".into(), &"t".into()).unwrap();
        js_sys::Reflect::set(&obj, &"clientVerify".into(), &false.into()).unwrap();
        let config: wasm_bindgen::JsValue = obj.into();
        let client = CoreLinkClient::new(config).expect("client");
        assert!(!client.client_verify_enabled());
    }
}
