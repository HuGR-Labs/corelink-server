---
id: "AUDIT-2026-05-15-BYOK-REAL-PROVIDER-PATTERN"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: ["Crypto SME (TBD)", "VP-Sec"]
supersedes: null
superseded_by: null
inv: ["INV-BYOK-CRYPTO-SOVEREIGNTY", "INV-KEY-OVERLAP"]
gap: "GAP-02"
references:
  - "specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md"
  - "compliance/byok-fips-matrix.md"
  - "crates/corelink-byok-aws/src/real.rs"
tags: ["byok", "fips", "ga", "pattern", "canonical-reference", "aws-kms", "gcp-kms", "azure-kv", "vault"]
---

# BYOK Real Provider Pattern — Canonical Reference

> **Purpose:** lock in the contract every concrete BYOK provider must
> satisfy for GA. AWS KMS is the source-of-truth implementation
> (`crates/corelink-byok-aws/src/real.rs`); GCP, Azure, and Vault follow
> the same pattern with provider-specific bindings substituted in.
>
> **Scope:** envelope encryption path (`wrap_dek`, `unwrap_dek`,
> `check_access`, optional `generate_data_key`, optional `list_aliases`).

---

## 1. Pattern checklist (apply to every provider)

| # | Property | AWS KMS impl | GCP KMS impl | Azure KV impl | Vault impl |
|---|---|---|---|---|---|
| 1 | `#![forbid(unsafe_code)]` at crate root | yes | yes | yes | yes |
| 2 | `clippy::unwrap_used = "deny"` + `expect_used = "deny"` + `panic = "deny"` | yes | yes | yes | yes |
| 3 | `KmsProvider` trait impl with the 4 canonical methods | yes | yes | yes | yes |
| 4 | FIPS endpoint **enforced unconditionally** in constructor; URL exposed for test assertion | yes (`resolved_fips_endpoint()`) | yes (`resolved_fips_endpoint()`, regional FedRAMP via `with_endpoint`) | yes (`resolved_fips_endpoint()` + `resolve_fips_host` 7-suffix allowlist incl. USGov/China/Germany sovereign) | yes (operator-config; `resolved_fips_endpoint()` returns `VAULT_ADDR`; TLS 1.3 enforced) |
| 5 | AAD JCS canonicalization via `serde_jcs` before binding | yes (`canonicalize_aad_to_string_map`) | yes (`canonicalize_aad_to_string_map`) | yes (`canonicalize_aad_to_string_map`; JCS bytes are AAD into inner AES-GCM) | yes (`canonicalize_aad_to_string_map` → base64 → Vault `context`) |
| 6 | Audit-emit fail-CLOSED — `emit_audit(...)` BEFORE error bubbles | yes (target `corelink.byok.aws.audit`) | yes (target `corelink.byok.gcp.audit`) | yes (target `corelink.byok.azure.audit`) | yes (target `corelink.byok.vault.audit`) |
| 7 | Constant-time fingerprint compare on AAD (mock-mode tamper detection) | yes (`subtle::ConstantTimeEq`) | yes (`subtle::ConstantTimeEq`) | yes (`subtle::ConstantTimeEq`; production path delegates to AES-GCM tag) | yes (`subtle::ConstantTimeEq` on validated key-name path; Vault server-side enforces AAD via `context`) |
| 8 | Native-only `aws-sdk-kms` / equivalent gated by `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` | yes | yes (`reqwest`/`tokio`/`jsonwebtoken`/`base64`/`regex` all native-cfg-gated) | yes (`reqwest`/`regex`/`tokio` target-gated) | yes (`reqwest` + `tokio` + `regex` native-only) |
| 9 | `*WasmStub` linked on `target_arch = "wasm32"` with explicit `BYOKError::Provider("... unsupported on wasm32 ...")` | yes (`AwsKmsWasmStub`) | yes (`GcpKmsWasmStub`) | yes (`AzureKeyVaultWasmStub`) | yes (`VaultWasmStub`) |
| 13 | wasm32 build green (`cargo build --target wasm32-unknown-unknown`) | yes (R-prep BYOK-unblock 2026-05-15 (commit `4323d1c`)) | yes (R-prep BYOK-unblock 2026-05-15 (commit `4323d1c`)) | yes (R-prep BYOK-unblock 2026-05-15 (commit `4323d1c`)) | yes (R-prep BYOK-unblock 2026-05-15 (commit `4323d1c`)) |
| 10 | ≥ 8 unit tests + 1 prop test (deterministic AAD canonicalization roundtrip) | yes (14 + 1 prop) | yes (15 + 1 prop in `tests/real_unit.rs`) | yes (28 + 1 prop on JCS determinism) | yes (22 + 1 prop, AAD-as-`context`) |
| 11 | Optional `#[ignore]` live test against `*_TEST_KEY_ARN` env (CI nightly) | yes (`e2e_byok_aws_kms.rs`) | yes | yes | yes |
| 12 | Server feature flag `byok-<provider>-real` wires the real impl behind the `KmsProvider` trait object | yes (`byok-aws-real`) | yes (`byok-gcp-real` → `corelink-byok-gcp/production`) | yes (`byok-azure-real` → `corelink-byok-azure/real`) | yes (`byok-vault-real` → `corelink-byok-vault/real`) |

---

## 2. AAD canonicalization contract

The contract is fixed by INV-BYOK-CRYPTO-SOVEREIGNTY and applies to all
providers byte-identically:

1. **Top-level value MUST be a JSON object.** Anything else
   (`array`, `string`, `number`, `bool`, `null`) → reject with
   `BYOKError::EnvelopeError`.
2. **All values MUST be JSON strings.** AWS KMS only accepts
   `Map<String, String>` for `EncryptionContext`; GCP/Azure/Vault should
   apply the same restriction so the AAD wire shape is provider-portable.
3. **Keys are sorted lexicographically** (`BTreeMap`) so wire bytes are
   deterministic regardless of input ordering.
4. **JCS (RFC 8785) canonical bytes** of the sorted object are also
   returned for downstream digest / signature use. The crate uses
   `serde_jcs = "0.2"` workspace-pinned, same as `corelink-audit-chain`.

The same logical AAD MUST produce byte-identical canonical bytes
across native and wasm32 targets so a `WrappedDek.encryption_context`
stored on D1 can be unwrapped from either environment.

### Reference: `corelink-byok-aws::real::canonicalize_aad_to_string_map`

```rust
pub fn canonicalize_aad_to_string_map(
    aad: &Value,
) -> Result<(BTreeMap<String, String>, Vec<u8>), BYOKError>
```

Returns `(string_map_for_provider_api, jcs_canonical_bytes)`. Every
provider real impl calls this once per `wrap_dek` / `unwrap_dek` and
threads the result through provider-specific binding.

---

## 3. FIPS endpoint enforcement

| Provider | FIPS endpoint shape | Construction enforcement | Test assertion |
|---|---|---|---|
| AWS KMS | `kms-fips.<region>.amazonaws.com` | `AwsKmsRealProvider::new` always sets `use_fips(true)` | `resolved_fips_endpoint()` exposed; tests assert exact URL pattern + `starts_with("kms-fips.")` + `ends_with(".amazonaws.com")` for 4 regions |
| GCP KMS | `cloudkms.<region>.rep.googleapis.com` (FedRAMP regions) | `GcpKmsRealProvider::with_endpoint` accepts FIPS regional URL | TBD R-prep — same assertion shape |
| Azure KV | `<vault>.managedhsm.azure.net` (Managed HSM Level 3) | `endpoint_override` resolves Managed HSM | TBD R-prep — same assertion shape |
| Vault | customer-hosted; TLS 1.3 + server X.509 verify enforced (FIPS = operator-config, NOT endpoint-routable — Vault Enterprise FIPS build + `seal_type=pkcs11`) | `VaultRealProvider::from_env` rejects `VAULT_SKIP_VERIFY` + sets `min_tls_version(TLS_1_3)`; `resolved_fips_endpoint()` returns operator-supplied `VAULT_ADDR` | `resolved_fips_endpoint_returns_vault_addr` + `tls_strict_enforced_is_true` |

**Rule:** every provider crate MUST expose a `resolved_fips_endpoint() -> &str`
(or equivalent name) so the test suite can pin the exact URL string the
production client uses. This is what the audit cross-references at
`BYOK-FIPS-ATTESTATION-MATRIX.md` "Endpoint (FIPS)" column.

---

## 4. wasm32 strategy

Background: CoreLink Workers compile to `wasm32-unknown-unknown`. The
official AWS, GCP, and Azure SDKs do not target wasm32 (tokio + hyper +
OS-level TLS). For the production wasm path Workers proxy envelope
operations to the native server process via the internal control-plane
RPC.

The pattern is:

1. Real SDK deps go in `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`.
2. A wasm32 stub type (`Aws*WasmStub`, `Gcp*WasmStub`, etc.) implements
   `KmsProvider` and returns `BYOKError::Provider("AWS KMS real provider
   unsupported on wasm32; use CF Worker-side AWS SDK binding instead")`
   from every method.
3. A `pub type *RealProvider = *WasmStub;` re-export on wasm32 means
   downstream code can name `AwsKmsRealProvider` on both targets — on
   wasm32 the call surfaces the explicit error above.

**Resolved 2026-05-15 (R-prep BYOK-unblock, branch
`wt/r-prep-byok-wasm32-unblock`):** the underlying `corelink-byok` crate
now builds for `wasm32-unknown-unknown`. Two surgical changes were
required:

1. `getrandom = { version = "0.2", features = ["js"] }` declared under
   `[target.'cfg(target_arch = "wasm32")'.dependencies]` — without the
   `js` feature, `getrandom 0.2` emits `compile_error!("the
   wasm*-unknown-unknown targets are not supported by default ...")`.
   On native the workspace-pinned `getrandom = "0.2"` is unchanged
   (OS-level CSPRNG via `getrandom(2)` / `getentropy(3)` /
   `BCryptGenRandom`).
2. `tokio` split across `cfg(not(target_arch = "wasm32"))` (workspace
   `full` features for the server / native paths) and
   `cfg(target_arch = "wasm32")` (`default-features = false, features
   = ["sync"]`). The lib only touches `tokio::sync::Mutex`, so the
   wasm32 sync-only build is functionally complete; this excludes
   `mio` + `rt-multi-thread` + `net` from the wasm32 dep graph.

Verified by:

```
cargo build -p corelink-byok      --target wasm32-unknown-unknown    # green
cargo build -p corelink-byok-aws  --target wasm32-unknown-unknown    # green
cargo build -p corelink-byok-gcp  --target wasm32-unknown-unknown    # green
cargo build -p corelink-byok-azure --target wasm32-unknown-unknown   # green
cargo build -p corelink-byok-vault --target wasm32-unknown-unknown   # green (default features)
cargo build -p corelink-byok-vault --target wasm32-unknown-unknown --features real # green
```

Each provider also gained a `tests/wasm32_stub.rs` integration test
(gated `cfg(target_arch = "wasm32")`) that exercises the explicit-
error contract: `*WasmStub::new(...)` constructible + `KmsProvider`
trait satisfied + `wrap_dek(...).await` returns
`BYOKError::Provider` whose message contains `"unsupported on wasm32"`.
The async future is driven by `futures::executor::block_on` to avoid
pulling tokio's runtime on wasm32.

---

## 5. Audit fail-CLOSED ordering

Every KMS error path MUST emit a structured `tracing` event BEFORE
returning the error. The orchestrator subscribes to
`target = "corelink.byok.<provider>.audit"` and turns each event into a
BLAKE3-linked audit record (see `corelink-audit-chain`).

Reference helper:

```rust
fn emit_audit(op: &str, key: &str, reason: &str) {
    warn!(
        target: "corelink.byok.aws.audit",
        audit = true,
        op = op,
        key = key,
        reason = reason,
        "BYOK AWS KMS audit event"
    );
}
```

The `audit = true` field is the canonical marker the subscriber uses to
distinguish audit events from regular `tracing` output. Each `reason`
string is from a closed vocabulary so analytics joins remain stable:

- `wrong_provider` — cross-provider key_id mismatch.
- `aad_missing` — `encryption_context` not provided.
- `malformed_arn` — ARN validation failed.
- `access_denied` → `BYOKError::CmkRevoked`.
- `invalid_ciphertext_aad_mismatch` → `BYOKError::AadMismatch`.
- `kms_invalid_state` → `BYOKError::CmkRevoked`.
- `throttled` → `BYOKError::Provider("... throttled")`.
- `not_found` → `BYOKError::Provider("... NotFoundException")`.
- `missing_plaintext` / `missing_ciphertext` — provider response shape error.
- `dek_length_invalid` → `BYOKError::DekLengthInvalid`.
- `provider_error` — fallback / unknown SDK error.

---

## 6. Replication checklist for GCP / Azure / Vault (next 3 providers)

Each provider gets a parallel R-prep work item. The diff from AWS KMS:

### 6.1 GCP Cloud KMS (`crates/corelink-byok-gcp`) — SEALED 2026-05-15

- [x] Move JCS canonicalization into the wrap/unwrap path
      (`canonicalize_aad_to_string_map`, BTreeMap-sorted, JCS bytes
      passed as `additionalAuthenticatedData`).
- [x] Add `resolved_fips_endpoint()` returning the regional FIPS URL
      (`cloudkms.<region>.rep.googleapis.com` for FedRAMP-High; default
      construction resolves to `cloudkms.googleapis.com` — FIPS 140-2 L1
      via BoringCrypto).
- [x] Add audit-emit fail-CLOSED at every error site (target
      `corelink.byok.gcp.audit`).
- [x] Gate native deps (`reqwest`, `tokio`, `jsonwebtoken`, `base64`,
      `regex`) via `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`;
      add `GcpKmsWasmStub` (returns `BYOKError::Provider("... unsupported
      on wasm32 ...")`) + `pub type GcpKmsRealProvider = GcpKmsWasmStub`
      on `target_arch = "wasm32"`.
- [x] Add `byok-gcp-real` server feature flag (mirrors `byok-aws-real`;
      pulls in `corelink-byok-gcp/production`).
- [x] Add ≥ 14 unit + 1 prop test (`tests/real_unit.rs`).

**Resolved 2026-05-15 (R-prep BYOK-unblock, branch
`wt/r-prep-byok-wasm32-unblock`):** see §4 — `corelink-byok` now builds
for `wasm32-unknown-unknown` (`getrandom` `js` feature + tokio sync-only
gate). Confirmed via `cargo build -p corelink-byok-gcp --target
wasm32-unknown-unknown` — green. Stub smoke test landed at
`crates/corelink-byok-gcp/tests/wasm32_stub.rs`.

### 6.2 Azure Key Vault (`crates/corelink-byok-azure`) — LANDED 2026-05-15

- [x] JCS canonicalization (`canonicalize_aad_to_string_map`) lands the
      same `(BTreeMap, Vec<u8>)` contract as AWS; the JCS bytes are passed
      into the inner AES-256-GCM cipher as AAD so the composite-key
      binding is byte-stable across architectures.
- [x] FIPS endpoint enforced via `resolve_fips_host` allowlist
      (`*.vault.azure.net`, `*.managedhsm.azure.net`, plus US-Gov, China,
      and Germany sovereign-cloud TLDs). `resolved_fips_endpoint()` and
      `fips_tier_suffix()` accessors exposed for test assertion. Premium
      HSM = FIPS 140-2 L2 (CMVP #3516); Managed HSM = logical L3 in the
      compliance matrix — the orchestrator owns tier policy.
- [x] Audit-emit fail-CLOSED at every error site (target
      `corelink.byok.azure.audit`, `audit = true`).
- [x] Constant-time AAD fingerprint compare in mock mode
      (`subtle::ConstantTimeEq`). Production path delegates to the inner
      AES-GCM authentication tag.
- [x] Native-only `reqwest`/`regex`/`tokio` deps target-gated via
      `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`.
- [x] `AzureKeyVaultWasmStub` linked on `target_arch = "wasm32"`; returns
      `BYOKError::Provider("Azure Key Vault real provider unsupported on
      wasm32 ...")` from every method. The wasm32 stub source code is
      target-shaped per §4. **Wasm32 toolchain blocker on `corelink-byok`
      resolved 2026-05-15 (R-prep BYOK-unblock, commit `4323d1c`)** — `cargo build -p
      corelink-byok-azure --target wasm32-unknown-unknown` is green;
      stub smoke test landed at `crates/corelink-byok-azure/tests/wasm32_stub.rs`.
- [x] Server feature flag `byok-azure-real` (`apps/server/Cargo.toml`)
      pulls in `corelink-byok-azure` with the `real` (alias for
      `production`) feature on.
- [x] 28 unit + 1 prop test in `tests/real_unit.rs`: FIPS-host pattern
      assertions for Premium HSM / Managed HSM / US-Gov, wrap/unwrap
      roundtrip (mock + wiremock), AAD JCS order-independence, mock-mode
      constant-time tamper detection, missing-AAD rejection, wrong-
      provider rejection, malformed-URI rejection, Entra ID token cache
      (1 POST across 5 wraps), workload-identity federated-token-file
      flow, 401/403/404/429 mapping, `RSA-OAEP-256` wire-shape pin, API
      version `7.4` pin, prop-test on JCS determinism (96 cases by
      default; `PROPTEST_CASES` env override).

### 6.3 HashiCorp Vault (`crates/corelink-byok-vault`)

- [x] Same canonicalization migration; Vault Transit `context` field is
      base64-encoded so the AAD path passes the JCS bytes through b64
      (`real::canonicalize_aad_to_string_map` → `encode_context_for_vault`).
- [x] FIPS handling = customer-hosted `$VAULT_ADDR` (operator-config, not
      endpoint-routable like AWS/GCP); the adapter enforces TLS 1.3
      minimum + server X.509 verification and rejects `VAULT_SKIP_VERIFY`
      at construction. FIPS attestation is operator-supplied metadata
      (Vault Enterprise FIPS build + `seal_type=pkcs11`).
- [x] Audit-emit (target `corelink.byok.vault.audit`) + `VaultWasmStub` +
      `byok-vault-real` server feature flag — landed wave 14.
- [x] Four auth modes (token / AppRole / JWT / Kubernetes) plumbed via
      `auth::VaultAuth`; tokens never appear in error messages.
- [x] **Wasm32 toolchain blocker on `corelink-byok` resolved 2026-05-15
      (R-prep BYOK-unblock, commit `4323d1c`).** `cargo build -p corelink-byok-vault
      --target wasm32-unknown-unknown` is green (default features); the
      `feature = "real"` variant is also green after gating `mod
      key_name` to `cfg(all(feature = "real", not(target_arch =
      "wasm32")))` (the wasm32 stub never touches regex-validated key
      names). Stub smoke test landed at
      `crates/corelink-byok-vault/tests/wasm32_stub.rs`.

---

## 7. Server orchestrator wiring (production singleton)

**Status:** LANDED 2026-05-15 (wave 15 — Server BYOK orchestrator
production wiring). Module path:
`apps/server/src/byok_orchestrator.rs`.

The orchestrator is the canonical entrypoint the server boot path
calls to obtain the singleton `Arc<dyn KmsProvider>` threaded through
every BYOK-aware code path (envelope encrypt / decrypt, kill-switch
poller, erasure attestation).

### 7.1 Dispatch table

| `byok-*-real` flag | Concrete type | Active-provider label | Audit target |
|---|---|---|---|
| (none) | `byok_orchestrator::InMemoryFake` | `in_memory_fake` | `corelink.byok.in_memory.audit` |
| `byok-aws-real` | `corelink_byok_aws::AwsKmsRealProvider` | `aws` | `corelink.byok.aws.audit` |
| `byok-gcp-real` | `corelink_byok_gcp::GcpKmsRealProvider` | `gcp` | `corelink.byok.gcp.audit` |
| `byok-azure-real` | `corelink_byok_azure::AzureKeyVaultRealProvider` | `azure` | `corelink.byok.azure.audit` |
| `byok-vault-real` | `corelink_byok_vault::VaultRealProvider` | `vault` | `corelink.byok.vault.audit` |

`corelink-byok` (the trait + foundation crate) is a **non-optional**
dependency of `corelink-server` so the orchestrator always compiles —
the default `cargo build -p corelink-server` produces the
`InMemoryFake` path with zero KMS network deps linked. Real provider
adapter crates remain optional and gated by the respective feature.

### 7.2 Multi-flag mutual-exclusion guard

Enabling two or more `byok-*-real` flags simultaneously is a **HARD
compile error** — the orchestrator is a singleton trait object and
having two production providers in one binary is not a supported
deployment shape. The guard expands one `compile_error!` per pairwise
overlap (6 macro invocations total) so the diagnostic names the exact
two flags in conflict, e.g.:

```
error: BYOK orchestrator: features `byok-aws-real` AND `byok-gcp-real`
       are mutually exclusive — only one BYOK real provider may be
       enabled at a time (the orchestrator is a singleton trait
       object). See specs/_audits/2026-05-15-byok-real-provider-pattern.md §7.
```

### 7.3 Configuration via environment

Provider constructors read the following environment variables when
the matching feature is enabled. Missing required vars fail CLOSED
with `BYOKError::Provider` (audit-emit before the error returns).

| Feature | Variable | Required? | Default | Purpose |
|---|---|---|---|---|
| `byok-aws-real` | `AWS_REGION` | optional | `us-east-1` | KMS region |
| `byok-gcp-real` | `GCP_REGION` | optional | `us-east1` | Cloud KMS region |
| `byok-azure-real` | `CORELINK_BYOK_AZURE_VAULT_URL` | **required** | — | Premium / Managed HSM base URL |
| `byok-azure-real` | `CORELINK_BYOK_AZURE_REGION` | optional | `eastus2` | Azure region |
| `byok-vault-real` | `VAULT_ADDR` | **required** | — | Vault cluster URL (consumed by `VaultRealProvider::from_env`) |
| `byok-vault-real` | `CORELINK_BYOK_VAULT_REGION` | optional | `customer-hosted` | Logical region label |

Provider-specific credentials (AWS SDK chain, GCP ADC, Entra ID,
Vault auth methods) are resolved by each provider's native
credential layer — see the provider crate docs. Every variable
listed above is mirrored in `docs/internal/secrets-checklist.md`
(rows 25 — `AWS_REGION` —, plus new rows 109–112 for the four
orchestrator-level configuration vars).

### 7.4 Audit emission

The orchestrator emits ONE structured `tracing` event at boot:

```
target = "corelink.byok.orchestrator.audit"
audit  = true
op     = "boot"
provider = <label>     // "aws" | "gcp" | "azure" | "vault" | "in_memory_fake"
```

Per-operation audit events (`wrap_dek`, `unwrap_dek`, `check_access`)
are emitted by the concrete provider crates with
`target = "corelink.byok.<provider>.audit"` and the closed `reason`
vocabulary documented in §5. The orchestrator does NOT transform
those events — they flow through the server's global `tracing`
subscriber to the audit sink.

### 7.5 InMemoryFake (dev / test path)

`InMemoryFake` is the default-path provider used when no
`byok-*-real` feature is enabled. It:

- Reports `provider_kind() == KmsProviderKind::AwsKms` so downstream
  code (which validates `key_id.provider == self.provider_kind()`)
  works uniformly in dev.
- Reports `fips_level() == FipsLevel::None` — the load-bearing
  assertion that distinguishes the fake from any real provider.
- Wraps the DEK by XOR-masking with a module-private 32-byte
  constant; ciphertext is NOT byte-identical to plaintext, which
  guards against regressions in the matrix tests that round-trip
  via the orchestrator.
- Enforces the AAD-mandatory contract: `wrap_dek` and `unwrap_dek`
  both reject `encryption_context: None`. This mirrors the
  production providers' contract so handler code that targets the
  fake in CI catches AAD-omission bugs before they reach a real
  provider.

`InMemoryFake` is **NOT for production**. Any deployment that ships
the default `cargo build` configuration MUST explicitly opt out of
BYOK at the customer / tenant level — the orchestrator does not
attempt to gate this at runtime (cargo features are the gate).

### 7.6 Integration tests

`apps/server/tests/byok_orchestrator.rs` covers:

- Default-path dispatch returns `InMemoryFake` (assertions on
  `region()`, `fips_level()`, and `active_provider()`).
- Each `byok-*-real` flag wires the matching concrete type
  (`provider_kind()` discriminator + `Arc::strong_count` reachability
  check). The AWS path constructs the real provider in CI because
  `AwsKmsRealProvider::new` does not require live credentials at
  construction time; GCP / Azure / Vault paths skip when their
  staging-credential env vars are absent.
- `InMemoryFake` `wrap_dek` → `unwrap_dek` round-trip preserves the
  32-byte DEK exactly.
- `InMemoryFake` rejects `wrap_dek` / `unwrap_dek` without
  `encryption_context` (AAD-mandatory contract).
- `check_access` returns `Ok`.
- Trait-object reachability via `Arc<dyn KmsProvider>`.

Six tests total, plus the four `#[cfg(feature = ...)]`-gated
real-provider dispatch tests (one per feature). Quality gate:
`cargo test -p corelink-server --test byok_orchestrator` passes
under all five build configurations (default + 4 flags).

---

## 8. Acceptance gate

A provider crate is **GA-ready** only when:

1. All 12 pattern checklist items in §1 are green.
2. The vendor FIPS attestation letter is on file (see
   `BYOK-FIPS-ATTESTATION-MATRIX.md` — currently 1/4 attested).
3. The provider crate has a passing nightly `#[ignore]` live test
   against a staging CMK.
4. `python3 scripts/validate_specs.py` reports zero diagnostics for the
   crate's referenced INVs.
5. The server feature flag builds with the rest of the workspace
   (`cargo build -p corelink-server --features byok-<provider>-real`)
   AND the orchestrator integration test passes
   (`cargo test -p corelink-server --features byok-<provider>-real
   --test byok_orchestrator`).

GA goal: 4/4 providers GA-ready by **2026-06-14** (D+30 cap from
`BYOK-FIPS-ATTESTATION-MATRIX.md`).
