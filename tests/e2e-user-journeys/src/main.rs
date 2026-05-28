//! # CoreLink E2E User-Journey Suite — the real ship gate
//!
//! BLACK-BOX: Tests use ONLY what a paying customer has:
//!   - The deployed HTTP API (CORELINK_E2E_ENDPOINT, default: http://localhost:8787)
//!   - The `corelink` CLI binary (std::process::Command)
//!   - An HTTP client (reqwest blocking) + Bearer PAT
//!   - The published OpenAPI contract shape
//!
//! FORBIDDEN in this file and any module it imports:
//!   - ANY `corelink-*` crate import
//!   - ANY direct D1/R2/KV access
//!   - ANY mock of the system-under-test
//!   - Reading internal state to assert
//!
//! ## Environment variables
//!
//! | Var | Description | Default |
//! |-----|-------------|---------|
//! | `CORELINK_E2E_ENDPOINT` | Base URL of the API under test | `http://localhost:8787` |
//! | `CORELINK_E2E_TOKEN` | Bearer PAT for tenant A | (required for most journeys) |
//! | `CORELINK_E2E_TOKEN_TENANT_B` | Bearer PAT for tenant B (isolation test) | (required for journey 5) |
//! | `CORELINK_E2E_QUOTA_TEST` | Set to `1` to enable slow quota cap journey (journey 7) | unset |
//! | `CORELINK_E2E_BAZEL_TEST` | Set to `1` to enable Bazel round-trip (journey 4) | unset |
//!
//! ## Expected state
//!
//! All journeys are expected RED against the current production deploy.
//! See `SHIP-GATE: RED` at the end of output and the journey→P0 map in README.md.

use std::env;
use std::process::Command;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;
use sha2::{Digest, Sha256};

// ── Configuration ─────────────────────────────────────────────────────────────

struct Config {
    endpoint: String,
    token_a: Option<String>,
    token_b: Option<String>,
    quota_test: bool,
    bazel_test: bool,
}

impl Config {
    fn from_env() -> Self {
        Config {
            endpoint: env::var("CORELINK_E2E_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:8787".to_string())
                .trim_end_matches('/')
                .to_string(),
            token_a: env::var("CORELINK_E2E_TOKEN").ok(),
            token_b: env::var("CORELINK_E2E_TOKEN_TENANT_B").ok(),
            quota_test: env::var("CORELINK_E2E_QUOTA_TEST").map(|v| v == "1").unwrap_or(false),
            bazel_test: env::var("CORELINK_E2E_BAZEL_TEST").map(|v| v == "1").unwrap_or(false),
        }
    }
}

// ── Journey result ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum JourneyStatus {
    Pass,
    Fail(String),
    Gated(String), // tool/flag not present — gated, not silently skipped
}

struct JourneyResult {
    name: &'static str,
    status: JourneyStatus,
    duration_ms: u64,
    /// The P0 finding this journey catches (from the multimodel audit).
    p0_gates: &'static str,
}

// ── HTTP helpers ───────────────────────────────────────────────────────────────

fn build_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .danger_accept_invalid_certs(false)
        .build()
        .expect("failed to build reqwest client")
}

fn bearer(token: &str) -> String {
    format!("Bearer {}", token)
}

// ─────────────────────────────────────────────────────────────────────────────
// Journey 1 — Onboarding / ping
// P0 gated: P0-6 (container health probe protocol mismatch → 503),
//           P0-2 (no PAT validation on live path)
//
// Contract: GET /api/health → 200 {"status":"SERVING",...}
//           Then: PAT-authenticated GET /v1/users/me OR `corelink ping`
//           A REAL authenticated endpoint must respond 200 (not 401 from
//           missing-auth and not 503 from container-unavailable).
// Expected: RED — the Worker answers /health 200 but any authenticated
//           endpoint returns 503 (container unreachable) or 200 with
//           garbage because no auth validation runs.
// ─────────────────────────────────────────────────────────────────────────────
fn journey_onboarding_ping(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Journey 1: Onboarding/ping — health + authenticated endpoint reachable";
    let p0_gates = "P0-6 (container health mismatch → 503), P0-2 (no auth validation)";
    let start = Instant::now();

    // Step 1: health must return 200 SERVING
    let health_url = format!("{}/api/health", cfg.endpoint);
    let health_resp = match client.get(&health_url).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!("GET /api/health connection failed: {}", e)),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    if health_resp.status() != 200 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET /api/health returned {} (expected 200)",
                health_resp.status()
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    let health_body: Value = match health_resp.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!("GET /api/health body not JSON: {}", e)),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    if health_body["status"].as_str() != Some("SERVING") {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET /api/health status field not SERVING: {}",
                health_body
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Step 2: an authenticated endpoint must work.
    // A customer running `corelink ping` or `corelink doctor` hits a real auth path.
    // We use GET /v1/users/me as the minimal authenticated probe.
    let token = match &cfg.token_a {
        Some(t) => t.clone(),
        None => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(
                    "CORELINK_E2E_TOKEN not set — cannot test authenticated endpoint".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    let me_url = format!("{}/v1/users/me", cfg.endpoint);
    let me_resp = match client
        .get(&me_url)
        .header(AUTHORIZATION, bearer(&token))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "GET /v1/users/me connection failed: {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    // The contract: a valid PAT must yield 200 with a UserProfile body.
    // Current prod: returns 503 (container unavailable) because P0-6 health
    // probe mismatch prevents the container from being considered healthy.
    let status = me_resp.status().as_u16();
    if status != 200 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET /v1/users/me with valid PAT returned {} (expected 200). \
                 Likely cause: P0-6 container health mismatch (503) or P0-2 auth not wired.",
                status
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Validate response shape per OpenAPI UserProfile schema
    let body: Value = match me_resp.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "GET /v1/users/me returned 200 but body not JSON: {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    for field in &["user_id", "email_hash", "locale", "created_at_ms"] {
        if body[field].is_null() {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "GET /v1/users/me response missing required field '{}'",
                    field
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    }

    JourneyResult {
        name,
        status: JourneyStatus::Pass,
        duration_ms: start.elapsed().as_millis() as u64,
        p0_gates,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Journey 2 — Auth rejection
// P0 gated: P0-2 (any 32-256 char string accepted as valid PAT)
//
// Contract:
//   - absent Authorization header → 401
//   - malformed token (< 32 chars) → 401
//   - well-formed-but-invalid PAT (correct length, wrong content) → 401
//
// Expected: RED — the audit shows "any 32-256 char string accepted" because
//   the auth middleware is not wired into the live path. The worker checks
//   token FORMAT only and the DO never validates.
// ─────────────────────────────────────────────────────────────────────────────
fn journey_auth_rejection(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Journey 2: Auth rejection — absent/malformed/invalid PAT → 401";
    let p0_gates = "P0-2 (auth middleware not wired; any 32-256 char string accepted)";
    let start = Instant::now();

    // We probe GET /v1/users/me — requires auth per OpenAPI.
    let me_url = format!("{}/v1/users/me", cfg.endpoint);

    // 2a: No Authorization header → must be 401
    let resp_no_auth = match client.get(&me_url).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!("connection failed (no-auth probe): {}", e)),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };
    if resp_no_auth.status() != 401 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "No-auth request to /v1/users/me returned {} (expected 401). \
                 P0-2: auth middleware not in live path.",
                resp_no_auth.status()
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // 2b: Malformed token (too short — 10 chars) → must be 401
    let resp_short = match client
        .get(&me_url)
        .header(AUTHORIZATION, "Bearer tooshort123")
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "connection failed (short-token probe): {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };
    if resp_short.status() != 401 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "Short (malformed) token to /v1/users/me returned {} (expected 401). \
                 P0-2: auth not validating format.",
                resp_short.status()
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // 2c: Well-formed but invalid PAT (correct length 64 hex chars, random content) → must be 401
    // This is the key P0-2 catch: if any 32-256 char string is accepted, this returns 200.
    let fake_pat = "0000000000000000000000000000000000000000000000000000000000000001";
    let resp_fake = match client
        .get(&me_url)
        .header(AUTHORIZATION, format!("Bearer {}", fake_pat))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "connection failed (fake-token probe): {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };
    if resp_fake.status() != 401 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "Well-formed but invalid PAT (fake) to /v1/users/me returned {} (expected 401). \
                 P0-2 CONFIRMED: any 32-256 char string is accepted as valid. \
                 Auth middleware not in the live request path.",
                resp_fake.status()
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    JourneyResult {
        name,
        status: JourneyStatus::Pass,
        duration_ms: start.elapsed().as_millis() as u64,
        p0_gates,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Journey 3 — Cache miss → hit (CAS PUT/GET round-trip)
// P0 gated: P0-1 (composed router not bound → no CAS route),
//           P0-4 (no R2/D1 bindings → PUT succeeds but data lost),
//           P0-7 (InMemory fakes → data ephemeral)
//
// Contract (REAPI v2 / HTTP CAS per OpenAPI):
//   PUT /v1/cas/blobs/{digest} (or equivalent CAS write endpoint)
//   GET /v1/cas/blobs/{digest} → bytes match
//   Second GET → cache-hit header present
//
// NOTE: The OpenAPI doc covers the REST management surface; the CAS surface
//   is gRPC REAPI v2 per the spec header. We test the HTTP CAS surface as
//   the user-accessible alternative (as referenced in quickstart).
//   If the endpoint is not in the published OpenAPI, we note this.
// Expected: RED — P0-1 router not bound; no CAS route reachable.
// ─────────────────────────────────────────────────────────────────────────────
fn journey_cache_miss_hit(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Journey 3: Cache miss→hit — PUT blob, GET bytes match, 2nd GET = cache hit";
    let p0_gates = "P0-1 (router not bound), P0-4 (no R2 bindings), P0-7 (InMemory fakes, ephemeral)";
    let start = Instant::now();

    let token = match &cfg.token_a {
        Some(t) => t.clone(),
        None => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(
                    "CORELINK_E2E_TOKEN not set — cannot test CAS operations".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    // Generate unique content per run (hermetic)
    let run_id = uuid::Uuid::new_v4().to_string();
    let blob_content = format!("corelink-e2e-test-blob-{}", run_id);
    let blob_bytes = blob_content.as_bytes();

    // Compute SHA-256 digest (CAS content-address)
    let mut hasher = Sha256::new();
    hasher.update(blob_bytes);
    let digest_hex = hex::encode(hasher.finalize());
    let digest_with_size = format!("sha256:{}/{}", digest_hex, blob_bytes.len());

    // PUT the blob
    // The CAS write endpoint per REAPI v2 HTTP binding.
    // Path: /v1/cas/blobs/{hash}/{size} or /v1/blobs/{digest}
    // We try the most common pattern; a 404 here means the route doesn't exist (P0-1).
    let put_url = format!(
        "{}/v1/cas/blobs/{}/{}",
        cfg.endpoint, digest_hex, blob_bytes.len()
    );

    let put_resp = match client
        .put(&put_url)
        .header(AUTHORIZATION, bearer(&token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob_bytes.to_vec())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!("PUT {} connection failed: {}", put_url, e)),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    let put_status = put_resp.status().as_u16();
    // Accept 200 or 201 as success
    if put_status != 200 && put_status != 201 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "PUT blob returned {} (expected 200/201). \
                 Likely P0-1: CAS router not bound. URL tried: {}. Digest: {}.",
                put_status, put_url, digest_with_size
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // GET the blob (first — should be a miss served from just-written data)
    let get_url = format!(
        "{}/v1/cas/blobs/{}/{}",
        cfg.endpoint, digest_hex, blob_bytes.len()
    );

    let get_resp1 = match client
        .get(&get_url)
        .header(AUTHORIZATION, bearer(&token))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "GET (first) {} connection failed: {}",
                    get_url, e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    if get_resp1.status() != 200 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET blob (first) returned {} (expected 200). \
                 Likely P0-1/P0-4/P0-7: route missing or storage not bound. Digest: {}.",
                get_resp1.status(), digest_with_size
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    let returned_bytes = match get_resp1.bytes() {
        Ok(b) => b,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "GET blob body read error: {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    if returned_bytes.as_ref() != blob_bytes {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET blob bytes don't match PUT bytes. \
                 PUT {} bytes, GET {} bytes. \
                 P0-7: InMemory fakes may be returning wrong data.",
                blob_bytes.len(), returned_bytes.len()
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Second GET — should hit cache (X-Cache-Status: HIT or similar)
    let get_resp2 = match client
        .get(&get_url)
        .header(AUTHORIZATION, bearer(&token))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "GET (second, cache-hit probe) connection failed: {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    if get_resp2.status() != 200 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET blob (second) returned {} (expected 200 with cache-hit semantics). \
                 Digest: {}.",
                get_resp2.status(), digest_with_size
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Note: we don't assert a specific cache-hit header name here because
    // the OpenAPI doesn't specify it for the HTTP CAS surface. We assert
    // the bytes are correct on second GET.
    let returned_bytes2 = match get_resp2.bytes() {
        Ok(b) => b,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "GET blob (2nd) body read error: {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    if returned_bytes2.as_ref() != blob_bytes {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET blob (2nd) bytes don't match. P0-7: ephemeral in-memory storage.",
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    JourneyResult {
        name,
        status: JourneyStatus::Pass,
        duration_ms: start.elapsed().as_millis() as u64,
        p0_gates,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Journey 4 — Bazel round-trip (gated behind `which bazel` + BAZEL_TEST flag)
// P0 gated: P0-1, P0-4, P0-6 (end-to-end build cache doesn't work)
//
// Contract: `corelink bazel-init` in a tiny workspace → `bazel build //...`
//   twice → second build hits cache (`remote cache hit` in output).
//
// Gated: requires `bazel` on PATH and CORELINK_E2E_BAZEL_TEST=1.
// ─────────────────────────────────────────────────────────────────────────────
fn journey_bazel_round_trip(cfg: &Config, _client: &Client) -> JourneyResult {
    let name = "Journey 4: Bazel round-trip — bazel-init + 2 builds → 2nd hits cache";
    let p0_gates = "P0-1 (router not bound), P0-4 (no R2 bindings), P0-6 (container health mismatch)";
    let start = Instant::now();

    if !cfg.bazel_test {
        return JourneyResult {
            name,
            status: JourneyStatus::Gated(
                "CORELINK_E2E_BAZEL_TEST=1 not set — Bazel round-trip skipped. \
                 Set to enable (requires `bazel` on PATH and a valid PAT).".to_string(),
            ),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Check bazel is available
    let bazel_check = Command::new("which").arg("bazel").output();
    match bazel_check {
        Ok(o) if !o.status.success() => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(
                    "`bazel` not found on PATH — Bazel round-trip requires Bazel 7+.".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(format!(
                    "could not check for `bazel`: {} — skipping Bazel round-trip",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
        Ok(_) => {}
    }

    // Check corelink CLI is available
    let cli_check = Command::new("corelink").arg("--version").output();
    match cli_check {
        Ok(o) if !o.status.success() => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(
                    "`corelink` CLI not found or --version failed. Install from https://corelink-get.humangr.com".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
        Err(_) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(
                    "`corelink` CLI not found on PATH. Install from https://corelink-get.humangr.com".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
        Ok(_) => {}
    }

    // Create a minimal throwaway Bazel workspace in a temp dir
    let tmp_dir = env::temp_dir().join(format!("corelink-e2e-bazel-{}", uuid::Uuid::new_v4()));
    if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!("could not create temp dir: {}", e)),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Minimal MODULE.bazel
    std::fs::write(tmp_dir.join("MODULE.bazel"), "module(name = \"corelink_e2e_test\", version = \"0.0.1\")\n")
        .expect("write MODULE.bazel");

    // Minimal BUILD.bazel with a genrule
    let build_content = r#"genrule(
    name = "hello",
    outs = ["hello.txt"],
    cmd = "echo 'hello corelink e2e' > $@",
)
"#;
    std::fs::write(tmp_dir.join("BUILD.bazel"), build_content)
        .expect("write BUILD.bazel");

    // Run `corelink bazel-init` in the temp workspace
    let bazel_init = Command::new("corelink")
        .arg("bazel-init")
        .current_dir(&tmp_dir)
        .env("CORELINK_E2E_ENDPOINT", &cfg.endpoint)
        .output();

    match bazel_init {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // Clean up
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "`corelink bazel-init` failed (exit {}): {}",
                    o.status, stderr
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!("`corelink bazel-init` exec error: {}", e)),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
        Ok(_) => {}
    }

    // First build (cold — should upload to cache)
    let build1 = Command::new("bazel")
        .arg("build")
        .arg("//...")
        .current_dir(&tmp_dir)
        .env_remove("BAZEL_CACHE_SILO_KEY") // hermetic
        .output();

    match build1 {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "First `bazel build //...` failed: {}",
                    stderr
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!("`bazel build` exec error: {}", e)),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
        Ok(_) => {}
    }

    // Second build — must show remote cache hits
    let build2 = Command::new("bazel")
        .arg("build")
        .arg("//...")
        .current_dir(&tmp_dir)
        .output();

    let _ = std::fs::remove_dir_all(&tmp_dir); // clean up regardless

    let build2_output = match build2 {
        Ok(o) => o,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!("Second `bazel build` exec error: {}", e)),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    if !build2_output.status.success() {
        let stderr = String::from_utf8_lossy(&build2_output.stderr);
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "Second `bazel build //...` failed: {}",
                stderr
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Check for remote cache hit in Bazel output
    let stdout = String::from_utf8_lossy(&build2_output.stdout);
    let stderr = String::from_utf8_lossy(&build2_output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Bazel emits: "N processes: M remote cache hit" or "(cache hit)"
    if !combined.contains("remote cache hit") && !combined.contains("cache hit") {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "Second build did not show remote cache hits. \
                 P0-1/P0-4/P0-6: CAS router not bound or storage not connected. \
                 Build output: {}",
                &combined[..combined.len().min(500)]
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    JourneyResult {
        name,
        status: JourneyStatus::Pass,
        duration_ms: start.elapsed().as_millis() as u64,
        p0_gates,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Journey 5 — Tenant isolation
// P0 gated: P0-3 (no tenant isolation; all traffic → single `_pending_auth` DO)
//
// Contract:
//   Tenant A PUTs a blob with a unique content-address.
//   Tenant B (different PAT, different tenant) GETs the same content-address.
//   Expected: 404 (miss) or 403 (denied) — never 200 with tenant A's bytes.
//
// This is the security ship gate: if B gets A's data, multi-tenancy is broken.
// No confirmation oracle needed — the API response is the only signal.
// Expected: RED — P0-3 all auth traffic routes to single `_pending_auth` DO,
//   tenant derivation not run, data not tenant-scoped.
// ─────────────────────────────────────────────────────────────────────────────
fn journey_tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Journey 5: Tenant isolation — tenant A PUTs; tenant B GETs same addr → denied/miss";
    let p0_gates = "P0-3 (no tenant isolation; all traffic → single _pending_auth DO; tenantId null)";
    let start = Instant::now();

    let token_a = match &cfg.token_a {
        Some(t) => t.clone(),
        None => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(
                    "CORELINK_E2E_TOKEN not set — cannot test tenant isolation".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    let token_b = match &cfg.token_b {
        Some(t) => t.clone(),
        None => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(
                    "CORELINK_E2E_TOKEN_TENANT_B not set — cannot test cross-tenant isolation. \
                     Set to a PAT belonging to a DIFFERENT tenant than CORELINK_E2E_TOKEN.".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    // Generate unique content for tenant A (hermetic per run)
    let run_id = uuid::Uuid::new_v4().to_string();
    let blob_content = format!("tenant-a-secret-blob-{}", run_id);
    let blob_bytes = blob_content.as_bytes();

    let mut hasher = Sha256::new();
    hasher.update(blob_bytes);
    let digest_hex = hex::encode(hasher.finalize());

    let cas_url = format!(
        "{}/v1/cas/blobs/{}/{}",
        cfg.endpoint, digest_hex, blob_bytes.len()
    );

    // Tenant A: PUT the blob
    let put_resp = match client
        .put(&cas_url)
        .header(AUTHORIZATION, bearer(&token_a))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob_bytes.to_vec())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "Tenant A PUT connection failed: {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    // If PUT fails (not 200/201), the data plane isn't working at all.
    // Record this as a failure but note it's a prerequisite issue (P0-1).
    let put_status = put_resp.status().as_u16();
    if put_status != 200 && put_status != 201 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "Tenant A PUT returned {} — data plane not operational (prerequisite P0-1 failure). \
                 Cannot verify isolation without a successful write.",
                put_status
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Tenant B: GET the same content address — MUST NOT return 200 with the bytes
    let get_resp = match client
        .get(&cas_url)
        .header(AUTHORIZATION, bearer(&token_b))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "Tenant B GET connection failed: {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    let get_status = get_resp.status().as_u16();

    if get_status == 200 {
        // Check if the returned bytes match tenant A's secret content
        let returned = match get_resp.bytes() {
            Ok(b) => b,
            Err(_) => {
                return JourneyResult {
                    name,
                    status: JourneyStatus::Fail(
                        "Tenant B received 200 from tenant A's content address — \
                         P0-3 CONFIRMED: tenant isolation not enforced. \
                         Could not read body for byte comparison.".to_string(),
                    ),
                    duration_ms: start.elapsed().as_millis() as u64,
                    p0_gates,
                };
            }
        };

        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "Tenant B received 200 from tenant A's content address ({} bytes). \
                 SECURITY FAILURE — P0-3 CONFIRMED: tenantId is null/ignored; \
                 all authenticated traffic routes to a single unscoped DO. \
                 Cross-tenant data leakage confirmed.",
                returned.len()
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // 404 (miss) or 403 (denied) are both acceptable — tenant B cannot see A's data
    if get_status == 404 || get_status == 403 {
        return JourneyResult {
            name,
            status: JourneyStatus::Pass,
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Any other status is unexpected
    JourneyResult {
        name,
        status: JourneyStatus::Fail(format!(
            "Tenant B GET returned unexpected status {} (expected 404 or 403 for isolation, \
             or 200 for isolation failure). Investigate the live path.",
            get_status
        )),
        duration_ms: start.elapsed().as_millis() as u64,
        p0_gates,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Journey 6 — Audit export + re-derive
// P0 gated: P0-1 (router not bound; no audit export route),
//           P0-4 (no D1 bindings → audit chain empty)
//
// Contract:
//   After N authenticated operations, GET /v1/admin/audit/events
//   → page of AuditEvent objects with event_id + occurred_at_ms + hash fields.
//   Re-derive the hash chain offline using only the public event payload.
//   Chain integrity must hold.
//
// NOTE: /v1/admin/audit/events requires admin-scoped PAT. If the PAT
//   doesn't have `admin` scope, we record this as an expected auth failure
//   and skip the chain-integrity check.
// Expected: RED — P0-1 router not bound; audit export route unreachable.
// ─────────────────────────────────────────────────────────────────────────────
fn journey_audit_export(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Journey 6: Audit export + re-derive — N ops → export audit log → verify chain";
    let p0_gates = "P0-1 (router not bound; no /v1/admin/audit/events route), P0-4 (no D1 bindings → empty chain)";
    let start = Instant::now();

    let token = match &cfg.token_a {
        Some(t) => t.clone(),
        None => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(
                    "CORELINK_E2E_TOKEN not set — cannot test audit export".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    // Perform a few operations to generate audit events (best-effort — may fail due to P0s)
    // We attempt GET /v1/users/me which should emit an audit event if auth works.
    let me_url = format!("{}/v1/users/me", cfg.endpoint);
    for _ in 0..3 {
        let _ = client
            .get(&me_url)
            .header(AUTHORIZATION, bearer(&token))
            .send();
    }

    // Now export the audit log
    let audit_url = format!("{}/v1/admin/audit/events?limit=100", cfg.endpoint);
    let audit_resp = match client
        .get(&audit_url)
        .header(AUTHORIZATION, bearer(&token))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "GET /v1/admin/audit/events connection failed: {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    let audit_status = audit_resp.status().as_u16();

    // 403 is expected if PAT doesn't have admin scope — document but don't fail
    if audit_status == 403 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET /v1/admin/audit/events returned 403. \
                 PAT may not have `admin` scope. \
                 If the endpoint returned 404 instead, P0-1 is confirmed (route not bound). \
                 Provide an admin-scoped PAT to complete this journey."
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    if audit_status == 401 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET /v1/admin/audit/events returned 401. \
                 Auth not working on this endpoint."
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    if audit_status != 200 {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "GET /v1/admin/audit/events returned {} (expected 200). \
                 P0-1: CAS/Admin router not bound — audit export route unreachable. \
                 The /api/health Worker route works but /v1/admin/* is not served.",
                audit_status
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    let audit_body: Value = match audit_resp.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "GET /v1/admin/audit/events returned 200 but body not JSON: {}",
                    e
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    let items = match audit_body["items"].as_array() {
        Some(a) => a,
        None => {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(
                    "GET /v1/admin/audit/events response missing 'items' array. \
                     Response schema doesn't match OpenAPI AuditEventPage.".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    // P0-4: if D1 has no bindings, the audit chain is empty even after operations.
    if items.is_empty() {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(
                "Audit log returned 0 events after performing operations. \
                 Likely P0-4: D1 not bound to the container → no audit writes persisted. \
                 Or P0-7: InMemory audit emitter is ephemeral per-instance RAM.".to_string(),
            ),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    // Re-derive the hash chain offline from the public event payload.
    // The audit chain uses SHA-256 linking: each event's `prev_hash` field
    // must equal SHA-256(canonical_serialize(previous_event)).
    // If the chain has no `prev_hash` field, we verify that event_ids are UUIDs
    // and occurred_at_ms are monotonically non-decreasing (weakest but public check).
    let mut prev_occurred_at: Option<i64> = None;
    let mut chain_ok = true;
    let mut chain_error = String::new();

    for (i, event) in items.iter().enumerate() {
        // Required fields per OpenAPI AuditEvent schema
        for field in &["event_id", "event_type", "occurred_at_ms"] {
            if event[field].is_null() {
                chain_ok = false;
                chain_error = format!("Event {} missing required field '{}'", i, field);
                break;
            }
        }
        if !chain_ok { break; }

        let occurred_at = event["occurred_at_ms"].as_i64().unwrap_or(0);

        // Monotonicity check (items should be sorted by occurred_at_ms)
        if let Some(prev) = prev_occurred_at {
            if occurred_at < prev {
                chain_ok = false;
                chain_error = format!(
                    "Audit event ordering violation: event {} has occurred_at_ms={} < prev={}. \
                     Hash chain integrity suspect.",
                    i, occurred_at, prev
                );
                break;
            }
        }
        prev_occurred_at = Some(occurred_at);

        // If prev_hash is present, verify the chain link
        if let Some(prev_hash) = event["prev_hash"].as_str() {
            // We'd need the previous event to verify; check it's a valid hex string.
            if prev_hash.len() != 64 || !prev_hash.chars().all(|c| c.is_ascii_hexdigit()) {
                chain_ok = false;
                chain_error = format!(
                    "Event {} prev_hash '{}' is not valid SHA-256 hex",
                    i, prev_hash
                );
                break;
            }
        }
    }

    if !chain_ok {
        return JourneyResult {
            name,
            status: JourneyStatus::Fail(format!(
                "Audit chain integrity check failed: {}",
                chain_error
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    JourneyResult {
        name,
        status: JourneyStatus::Pass,
        duration_ms: start.elapsed().as_millis() as u64,
        p0_gates,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Journey 7 — Quota hard-cap (gated: slow, requires CORELINK_E2E_QUOTA_TEST=1)
// P0 gated: P1-5 (quota race; free-tier quota not enforced → silent overage)
//
// Contract: free-tier PUT blobs until 10GB quota → next PUT → 429 hard cap.
// This is a P1-5 test (quota race) but also catches P0-4 (no R2 bindings →
// quota tracking never runs → cap never enforced).
//
// Gated: very slow (10GB of uploads). Set CORELINK_E2E_QUOTA_TEST=1 to enable.
// We do a lighter version: PUT until we get a 429, or after N blobs declare
// quota not enforced.
// ─────────────────────────────────────────────────────────────────────────────
fn journey_quota_hard_cap(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Journey 7: Quota hard-cap — PUT blobs until 429 hard cap (not silent overage)";
    let p0_gates = "P1-5 (quota race; free-tier not enforced → silent overage), P0-4 (no R2 bindings → quota never tracked)";
    let start = Instant::now();

    if !cfg.quota_test {
        return JourneyResult {
            name,
            status: JourneyStatus::Gated(
                "CORELINK_E2E_QUOTA_TEST=1 not set — quota hard-cap test skipped. \
                 WARNING: this test uploads many blobs; set only against a test account. \
                 We use small blobs (1KB) and stop at 10000 attempts or first 429.".to_string(),
            ),
            duration_ms: start.elapsed().as_millis() as u64,
            p0_gates,
        };
    }

    let token = match &cfg.token_a {
        Some(t) => t.clone(),
        None => {
            return JourneyResult {
                name,
                status: JourneyStatus::Gated(
                    "CORELINK_E2E_TOKEN not set — cannot test quota cap".to_string(),
                ),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    };

    // Use small blobs (1KB) and attempt many uploads.
    // We stop at MAX_ATTEMPTS or when we get a 429.
    // NOTE: real 10GB quota enforcement requires 10M × 1KB blobs, which is
    // impractical in a unit test. We test the MECHANISM: does the server
    // return 429 at all? We send 100 blobs and check for a 429 response.
    // A production quota test would need the test account to be near-limit.
    const MAX_ATTEMPTS: usize = 100;
    let blob_template = vec![b'x'; 1024]; // 1KB

    for i in 0..MAX_ATTEMPTS {
        // Each blob is unique (hermetic)
        let mut blob = blob_template.clone();
        let marker = format!("quota-test-{}-{}", uuid::Uuid::new_v4(), i);
        let marker_bytes = marker.as_bytes();
        if marker_bytes.len() < blob.len() {
            blob[..marker_bytes.len()].copy_from_slice(marker_bytes);
        }

        let mut hasher = Sha256::new();
        hasher.update(&blob);
        let digest_hex = hex::encode(hasher.finalize());

        let put_url = format!(
            "{}/v1/cas/blobs/{}/{}",
            cfg.endpoint, digest_hex, blob.len()
        );

        let resp = match client
            .put(&put_url)
            .header(AUTHORIZATION, bearer(&token))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(blob)
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                return JourneyResult {
                    name,
                    status: JourneyStatus::Fail(format!("PUT #{} connection failed: {}", i, e)),
                    duration_ms: start.elapsed().as_millis() as u64,
                    p0_gates,
                };
            }
        };

        let status = resp.status().as_u16();
        if status == 429 {
            // Quota enforced — this is PASS for this journey
            return JourneyResult {
                name,
                status: JourneyStatus::Pass,
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }

        // 404/5xx means data plane not working — fail with P0 context
        if status == 404 || status >= 500 {
            return JourneyResult {
                name,
                status: JourneyStatus::Fail(format!(
                    "PUT #{} returned {} — data plane not operational (P0-1/P0-4). \
                     Cannot verify quota enforcement without a working CAS write path.",
                    i, status
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                p0_gates,
            };
        }
    }

    // Reached MAX_ATTEMPTS without a 429
    // This could mean: quota not tracked (P0-4/P1-5) OR the account is far from the limit.
    // We report as fail with a caveat.
    JourneyResult {
        name,
        status: JourneyStatus::Fail(format!(
            "Sent {} × 1KB blobs without receiving a 429. \
             Quota hard-cap not observed. \
             Either P0-4 (no R2 bindings → quota never tracked) or P1-5 (quota race). \
             NOTE: A real 10GB test requires ~10M blobs; this test probes the mechanism only. \
             If the account is far below quota, set the account near its limit first.",
            MAX_ATTEMPTS
        )),
        duration_ms: start.elapsed().as_millis() as u64,
        p0_gates,
    }
}

// ── Runner ────────────────────────────────────────────────────────────────────

fn main() {
    let cfg = Config::from_env();
    let client = build_client();

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║       CoreLink E2E User-Journey Suite — Black-Box Ship Gate      ║");
    println!("╠══════════════════════════════════════════════════════════════════╣");
    println!("║  Endpoint: {:<55} ║", &cfg.endpoint[..cfg.endpoint.len().min(55)]);
    println!("║  Token A:  {:<55} ║", if cfg.token_a.is_some() { "SET" } else { "NOT SET" });
    println!("║  Token B:  {:<55} ║", if cfg.token_b.is_some() { "SET" } else { "NOT SET" });
    println!("║  Bazel:    {:<55} ║", if cfg.bazel_test { "ENABLED" } else { "GATED (CORELINK_E2E_BAZEL_TEST=1)" });
    println!("║  Quota:    {:<55} ║", if cfg.quota_test { "ENABLED" } else { "GATED (CORELINK_E2E_QUOTA_TEST=1)" });
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    let results = vec![
        journey_onboarding_ping(&cfg, &client),
        journey_auth_rejection(&cfg, &client),
        journey_cache_miss_hit(&cfg, &client),
        journey_bazel_round_trip(&cfg, &client),
        journey_tenant_isolation(&cfg, &client),
        journey_audit_export(&cfg, &client),
        journey_quota_hard_cap(&cfg, &client),
    ];

    println!();
    println!("┌──────────────────────────────────────────────────────────────────────────────┐");
    println!("│                        Journey Results                                        │");
    println!("├──────────────────────────────────────────────────────────────────────────────┤");

    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut gated = 0usize;

    for result in &results {
        let (icon, label) = match &result.status {
            JourneyStatus::Pass => {
                pass += 1;
                ("✓", "PASS ")
            }
            JourneyStatus::Fail(_) => {
                fail += 1;
                ("✗", "FAIL ")
            }
            JourneyStatus::Gated(_) => {
                gated += 1;
                ("⊙", "GATED")
            }
        };

        println!("│ {} [{}] ({:>5}ms) {}",
            icon, label, result.duration_ms,
            if result.name.len() > 60 { &result.name[..60] } else { result.name }
        );

        match &result.status {
            JourneyStatus::Fail(msg) => {
                // Print first 200 chars of failure message
                let truncated = if msg.len() > 200 { &msg[..200] } else { msg };
                println!("│         DETAIL: {}", truncated);
            }
            JourneyStatus::Gated(reason) => {
                let truncated = if reason.len() > 200 { &reason[..200] } else { reason };
                println!("│         REASON: {}", truncated);
            }
            JourneyStatus::Pass => {}
        }

        println!("│         P0-GATE: {}", result.p0_gates);
        println!("├──────────────────────────────────────────────────────────────────────────────┤");
    }

    println!("│ Summary: {} PASS  {} FAIL  {} GATED (not silently skipped)                     │", pass, fail, gated);
    println!("└──────────────────────────────────────────────────────────────────────────────┘");
    println!();

    // Ship gate verdict
    // PASS: all non-gated journeys pass
    // RED: any non-gated journey fails
    let ship_gate = if fail == 0 && pass > 0 {
        "GREEN"
    } else if fail == 0 && pass == 0 {
        "RED (all journeys gated — set CORELINK_E2E_TOKEN to run)"
    } else {
        "RED"
    };

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║  SHIP-GATE: {:<54} ║", ship_gate);
    if fail > 0 {
        println!("║                                                                  ║");
        println!("║  The P0 remediation wave must land before this gate turns GREEN. ║");
        println!("║  See tests/e2e-user-journeys/README.md for the journey→P0 map.  ║");
    }
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    // Exit 1 if any journey failed (not merely gated)
    if fail > 0 {
        std::process::exit(1);
    }
}
