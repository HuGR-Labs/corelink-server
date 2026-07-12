//! Bazel REAPI v2 journeys — `/bazel/v2/{instance}/...`.
//!
//! ## Journey inventory
//!
//! ### M7 — HTTP-level REAPI v2 ops (no bazel/corelink CLI dependency)
//!
//! These journeys drive the REAPI v2 HTTP surface directly via reqwest so they
//! run in every environment without the gated CLI binaries.
//!
//! Route shapes (from `routes/bazel_v2.rs`):
//!
//! | Op                 | Method | Path template                                               | Success |
//! |--------------------|--------|-------------------------------------------------------------|---------|
//! | CAS write          | PUT    | `/bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}`   | 204     |
//! | CAS read           | GET    | `/bazel/v2/{instance}/blobs/{hash}/{size}`                  | 200     |
//! | AC write           | PUT    | `/bazel/v2/{instance}/blobs/ac/{hash}/{size}`               | 204     |
//! | AC read            | GET    | `/bazel/v2/{instance}/blobs/ac/{hash}/{size}`               | 200     |
//! | findMissingBlobs   | POST   | `/bazel/v2/{instance}/findMissingBlobs`                     | 200     |
//!
//! Request/response JSON shapes:
//!
//! `POST findMissingBlobs` body:
//! ```json
//! {"blobDigests":[{"hash":"<sha256hex>","sizeBytes":<N>},...]}
//! ```
//! Response:
//! ```json
//! {"missingBlobDigests":[{"hash":"...","sizeBytes":N},...]}
//! ```
//!
//! **`instance` MUST equal the authenticated tenant-id** (the Worker injects
//! `x-corelink-tenant-id`; a mismatch → 403 CrossTenantDenied). The black-box
//! journeys therefore use `cfg.tenant` as the instance, resolving it from
//! `CORELINK_E2E_TENANT`.
//!
//! **Hash is SHA-256** (not blake3): REAPI v2 uses `sha256/` keyspace and the
//! route's `verify_sha256` boundary check rejects a wrong digest with 422.
//!
//! ### CLI round-trip (original, M4 / slow)
//!
//! `corelink bazel-init` in a throwaway workspace, then two `bazel build //...` —
//! the second must show a remote cache hit. Gated behind `corelink` + `bazel` on
//! PATH and `CORELINK_E2E_RUN_SLOW=1`.

use std::env;
use std::process::Command;
use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

use crate::harness::{
    bearer, expect_denied, expect_status, sha256_hex, unique_blob, url_bazel_ac,
    url_bazel_cas_read, url_bazel_cas_write, url_bazel_find_missing, Config, JourneyResult,
};
use crate::personas::Persona;

/// Run the Bazel journeys.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        // M7 — HTTP-level REAPI v2 (no CLI dependency)
        bazel_find_missing_blobs(cfg, client),
        bazel_ac_round_trip(cfg, client),
        bazel_cas_round_trip(cfg, client),
        bazel_instance_tenant_isolation(cfg, client),
        // Original CLI round-trip (slow, gated)
        bazel_round_trip(cfg, client),
    ]
}

// ── M7 helpers ────────────────────────────────────────────────────────────────

/// Resolve the primary tenant-id used as the REAPI `instance`. Gates if absent.
/// The instance MUST equal the authenticated tenant (the Worker enforces it via
/// the x-corelink-tenant-id header; mismatch → 403).
fn resolve_instance(cfg: &Config) -> Option<String> {
    cfg.tenant.clone()
}

/// Produce a stable upload UUID string for CAS write requests.
fn upload_uuid() -> String {
    format!("e2e-upload-{}", uuid::Uuid::new_v4())
}

// ── M7a — findMissingBlobs ────────────────────────────────────────────────────

/// M7a: `POST /findMissingBlobs` — the FIRST op every real Bazel build executes.
///
/// Protocol:
/// 1. PUT one blob (CAS write) so it is known to the server.
/// 2. POST `findMissingBlobs` with two digests: the known one + a random unknown.
/// 3. Assert the response lists ONLY the unknown digest in `missingBlobDigests`.
///
/// This is the most important coverage gap: `findMissingBlobs` is on the hot
/// path of every Bazel build (Bazel calls it before uploading to skip redundant
/// uploads), yet was never exercised by the previous CLI-only suite.
fn bazel_find_missing_blobs(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Bazel M7a: findMissingBlobs — known absent, only unknown reported";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    let instance = match resolve_instance(cfg) {
        Some(i) => i,
        None => {
            return JourneyResult::gated(
                name,
                "CORELINK_E2E_TENANT not set (needed as REAPI instance)",
            )
        }
    };

    // Step 1: PUT a known blob (so we can prove it is NOT reported missing).
    let known_blob = unique_blob("bazel-find-missing-known");
    let known_hash = sha256_hex(&known_blob);
    let known_size = known_blob.len();

    let write_url = url_bazel_cas_write(cfg, &instance, &upload_uuid(), &known_hash, known_size);
    let put = match client
        .put(&write_url)
        .header(AUTHORIZATION, bearer(token))
        .body(known_blob)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("CAS PUT seed: {e}")),
    };
    if let Err(m) = expect_status("bazel CAS seed PUT", put.status().as_u16(), 204) {
        return JourneyResult::fail(name, ms(start), format!("{m} (url={write_url})"));
    }

    // Step 2: Compose a findMissingBlobs request with [known, unknown].
    let unknown_blob = unique_blob("bazel-find-missing-unknown");
    let unknown_hash = sha256_hex(&unknown_blob);
    let unknown_size = unknown_blob.len();

    let body = serde_json::json!({
        "blobDigests": [
            {"hash": known_hash, "sizeBytes": known_size},
            {"hash": unknown_hash, "sizeBytes": unknown_size}
        ]
    })
    .to_string();

    let find_url = url_bazel_find_missing(cfg, &instance);
    let resp = match client
        .post(&find_url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("POST findMissingBlobs: {e}"))
        }
    };

    let status = resp.status().as_u16();
    if let Err(m) = expect_status("findMissingBlobs", status, 200) {
        return JourneyResult::fail(name, ms(start), format!("{m} (url={find_url})"));
    }

    // Step 3: Assert only the unknown digest is listed as missing.
    let body_bytes = match resp.bytes() {
        Ok(b) => b,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("findMissingBlobs body read: {e}"))
        }
    };
    let json: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("findMissingBlobs JSON parse: {e}"),
            )
        }
    };

    let missing_arr = match json.get("missingBlobDigests").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "findMissingBlobs: response missing 'missingBlobDigests' array — got: {json}"
                ),
            )
        }
    };

    // Must have exactly 1 missing entry.
    if missing_arr.len() != 1 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "findMissingBlobs: expected 1 missing digest (the unknown), got {} — \
                 missingBlobDigests={missing_arr:?}",
                missing_arr.len()
            ),
        );
    }

    // The single missing entry must be the unknown hash.
    let reported_hash = missing_arr[0]
        .get("hash")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if reported_hash != unknown_hash {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "findMissingBlobs: reported missing hash {reported_hash:?} ≠ unknown hash \
                 {unknown_hash:?} — the KNOWN blob was incorrectly reported missing or wrong entry"
            ),
        );
    }

    JourneyResult::pass(name, ms(start))
}

// ── M7b — AC write → read round-trip ─────────────────────────────────────────

/// M7b: Bazel Action Cache write→read round-trip.
///
/// `PUT /bazel/v2/{instance}/blobs/ac/{hash}/{size}` → 204 (no content).
/// `GET /bazel/v2/{instance}/blobs/ac/{hash}/{size}` → 200 + original bytes.
///
/// The AC stores opaque ActionResult blobs keyed by a SHA-256 digest. Unlike
/// CAS writes, the server does NOT re-hash the body for AC entries (the hash is
/// opaque — it is the client's ActionResult digest, not a content hash of the
/// bytes). We still use `sha256_hex` of a unique blob as the key, which is a
/// valid 64-char hex string.
fn bazel_ac_round_trip(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Bazel M7b: AC write→read round-trip — bytes match (happy)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    let instance = match resolve_instance(cfg) {
        Some(i) => i,
        None => return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set"),
    };

    // Use a unique blob as the ActionResult payload. The hash is derived from it
    // so each run addresses a fresh key (no stale-hit false positives).
    let payload = unique_blob("bazel-ac-e2e");
    let hash = sha256_hex(&payload);
    let size = payload.len();

    let ac_url = url_bazel_ac(cfg, &instance, &hash, size);

    // AC write (PUT) — success is 204 (no content).
    let put = match client
        .put(&ac_url)
        .header(AUTHORIZATION, bearer(token))
        .body(payload.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("AC PUT: {e}")),
    };
    if let Err(m) = expect_status("bazel AC PUT", put.status().as_u16(), 204) {
        return JourneyResult::fail(name, ms(start), format!("{m} (url={ac_url})"));
    }

    // AC read (GET) — success is 200 + the raw bytes we PUT.
    let get = match client
        .get(&ac_url)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("AC GET: {e}")),
    };
    if let Err(m) = expect_status("bazel AC GET", get.status().as_u16(), 200) {
        return JourneyResult::fail(name, ms(start), format!("{m} (url={ac_url})"));
    }

    match get.bytes() {
        Ok(b) if b.as_ref() == payload.as_slice() => {}
        Ok(b) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "AC round-trip bytes mismatch: PUT {} bytes, GET {} bytes",
                    payload.len(),
                    b.len()
                ),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("AC GET body read: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}

// ── M7c — CAS blob upload → read with SHA-256 + byte-equality ─────────────────

/// M7c: Bazel CAS blob upload→read with SHA-256 addressing and byte-equality.
///
/// `PUT /bazel/v2/{instance}/uploads/{uuid}/blobs/{sha256}/{size}` → 204.
/// `GET /bazel/v2/{instance}/blobs/{sha256}/{size}` → 200 + original bytes.
///
/// The server **verifies** the SHA-256 of the body against the digest in the
/// URL (`verify_sha256` at the route boundary) — a mismatched digest returns 422.
/// This journey proves the correct address (sha256_hex of the body) goes through
/// end-to-end and the stored bytes are returned verbatim.
fn bazel_cas_round_trip(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Bazel M7c: CAS PUT→GET sha256 + byte-equality (happy)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    let instance = match resolve_instance(cfg) {
        Some(i) => i,
        None => return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set"),
    };

    let blob = unique_blob("bazel-cas-e2e");
    let hash = sha256_hex(&blob);
    let size = blob.len();

    // CAS write (PUT with upload UUID).
    let write_url = url_bazel_cas_write(cfg, &instance, &upload_uuid(), &hash, size);
    let put = match client
        .put(&write_url)
        .header(AUTHORIZATION, bearer(token))
        .body(blob.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("CAS PUT: {e}")),
    };
    if let Err(m) = expect_status("bazel CAS PUT", put.status().as_u16(), 204) {
        return JourneyResult::fail(name, ms(start), format!("{m} (url={write_url})"));
    }

    // CAS read (GET) — address the same sha256/size.
    let read_url = url_bazel_cas_read(cfg, &instance, &hash, size);
    let get = match client
        .get(&read_url)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("CAS GET: {e}")),
    };
    if let Err(m) = expect_status("bazel CAS GET", get.status().as_u16(), 200) {
        return JourneyResult::fail(name, ms(start), format!("{m} (url={read_url})"));
    }

    match get.bytes() {
        Ok(b) if b.as_ref() == blob.as_slice() => {}
        Ok(b) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "CAS round-trip bytes mismatch: PUT {} bytes, GET {} bytes",
                    blob.len(),
                    b.len()
                ),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("CAS GET body read: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}

// ── M7d — instance / tenant isolation ─────────────────────────────────────────

/// M7d: Bazel instance/tenant isolation.
///
/// The `instance` path segment MUST equal the authenticated tenant-id. If a
/// PAT authenticated for tenant A sends a request with `instance = "evil-other"`,
/// the server returns 403 (CrossTenantDenied) — the instance check is the
/// cross-tenant gate on the Bazel surface. This journey verifies that the gate
/// is active and correctly rejecting a mismatched instance.
///
/// Uses P1's token but supplies a deliberately wrong instance so the
/// cross-tenant check fires. This is an adversarial probe that must see 403.
fn bazel_instance_tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Bazel M7d: instance/tenant isolation — wrong instance → 403 (adversarial)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    // Gate: we need a known tenant to prove it ISN'T the wrong instance.
    if cfg.tenant.is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_TENANT not set (needed to craft a mismatched instance)",
        );
    }

    // Deliberately wrong instance — must not match the authenticated tenant.
    let wrong_instance = "00000000-0000-0000-0000-000000000000";

    // A CAS read probe with the wrong instance — any hash/size, expecting 403.
    let dummy_hash = "a".repeat(64);
    let probe_url = url_bazel_cas_read(cfg, wrong_instance, &dummy_hash, 1);

    let resp = match client
        .get(&probe_url)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("cross-instance GET: {e}")),
    };

    let status = resp.status().as_u16();
    if status == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            "SECURITY: wrong-instance request returned 200 — cross-tenant gate is NOT active"
                .to_string(),
        );
    }
    // 403 is the expected active-gate reject; 401 is also acceptable (no tenant
    // auth at all). A 404 here means the route or the gate is absent.
    if let Err(m) = expect_denied("bazel cross-instance", status) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}

// ── CLI round-trip (original) ─────────────────────────────────────────────────

/// Bazel CLI round-trip → second build hits the remote cache.
fn bazel_round_trip(cfg: &Config, _client: &Client) -> JourneyResult {
    let name = "Bazel: CLI round-trip — bazel-init + 2 builds → 2nd hits cache";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    if !cfg.run_slow {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_RUN_SLOW=1 not set — Bazel CLI round-trip skipped (requires bazel + corelink CLI + a valid PAT)",
        );
    }
    if Persona::P1ReadWrite.resolve(cfg).is_err() {
        return JourneyResult::gated(name, "CORELINK_E2E_PAT_RW not set");
    }

    // Require both binaries on PATH.
    if !cmd_ok("which", &["bazel"]) {
        return JourneyResult::gated(name, "`bazel` not found on PATH (Bazel 7+ required)");
    }
    if !cmd_ok("corelink", &["--version"]) {
        return JourneyResult::gated(
            name,
            "`corelink` CLI not found on PATH (https://corelink-get.humangr.com)",
        );
    }

    // Throwaway workspace.
    let tmp = env::temp_dir().join(format!("corelink-e2e-bazel-{}", uuid::Uuid::new_v4()));
    if let Err(e) = std::fs::create_dir_all(&tmp) {
        return JourneyResult::fail(name, ms(start), format!("mkdir temp: {e}"));
    }
    let cleanup = || {
        let _ = std::fs::remove_dir_all(&tmp);
    };
    if let Err(e) = std::fs::write(
        tmp.join("MODULE.bazel"),
        "module(name = \"corelink_e2e_test\", version = \"0.0.1\")\n",
    ) {
        cleanup();
        return JourneyResult::fail(name, ms(start), format!("write MODULE.bazel: {e}"));
    }
    let build = r#"genrule(
    name = "hello",
    outs = ["hello.txt"],
    cmd = "echo 'hello corelink e2e' > $@",
)
"#;
    if let Err(e) = std::fs::write(tmp.join("BUILD.bazel"), build) {
        cleanup();
        return JourneyResult::fail(name, ms(start), format!("write BUILD.bazel: {e}"));
    }

    // corelink bazel-init.
    let init = Command::new("corelink")
        .arg("bazel-init")
        .current_dir(&tmp)
        .env("CORELINK_E2E_ENDPOINT", &cfg.endpoint)
        .output();
    match init {
        Ok(o) if !o.status.success() => {
            let err = String::from_utf8_lossy(&o.stderr).to_string();
            cleanup();
            return JourneyResult::fail(name, ms(start), format!("bazel-init failed: {err}"));
        }
        Err(e) => {
            cleanup();
            return JourneyResult::fail(name, ms(start), format!("bazel-init exec: {e}"));
        }
        Ok(_) => {}
    }

    // First (cold) build.
    let b1 = Command::new("bazel")
        .args(["build", "//..."])
        .current_dir(&tmp)
        .env_remove("BAZEL_CACHE_SILO_KEY")
        .output();
    match b1 {
        Ok(o) if !o.status.success() => {
            let err = String::from_utf8_lossy(&o.stderr).to_string();
            cleanup();
            return JourneyResult::fail(name, ms(start), format!("first build failed: {err}"));
        }
        Err(e) => {
            cleanup();
            return JourneyResult::fail(name, ms(start), format!("first build exec: {e}"));
        }
        Ok(_) => {}
    }

    // Second build — expect remote cache hit.
    let b2 = Command::new("bazel")
        .args(["build", "//..."])
        .current_dir(&tmp)
        .output();
    cleanup();
    let b2 = match b2 {
        Ok(o) => o,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("second build exec: {e}")),
    };
    if !b2.status.success() {
        let err = String::from_utf8_lossy(&b2.stderr);
        return JourneyResult::fail(name, ms(start), format!("second build failed: {err}"));
    }
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&b2.stdout),
        String::from_utf8_lossy(&b2.stderr)
    );
    if !combined.contains("remote cache hit") && !combined.contains("cache hit") {
        let tail = &combined[..combined.len().min(500)];
        return JourneyResult::fail(
            name,
            ms(start),
            format!("second build showed no remote cache hit. output: {tail}"),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// `true` if the command exists and exits 0.
fn cmd_ok(bin: &str, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
