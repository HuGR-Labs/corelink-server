---
id: "AUDIT-2026-05-15-BYOK-REAL-PROVIDER-PATTERN"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
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
| 4 | FIPS endpoint **enforced unconditionally** in constructor; URL exposed for test assertion | yes (`resolved_fips_endpoint()`) | (`with_endpoint`) | (`endpoint_override`) | yes (operator-config; `resolved_fips_endpoint()` returns `VAULT_ADDR`; TLS 1.3 enforced) |
| 5 | AAD JCS canonicalization via `serde_jcs` before binding | yes (`canonicalize_aad_to_string_map`) | TBD R-prep | TBD R-prep | yes (`canonicalize_aad_to_string_map` → base64 → Vault `context`) |
| 6 | Audit-emit fail-CLOSED — `emit_audit(...)` BEFORE error bubbles | yes (target `corelink.byok.aws.audit`) | TBD R-prep | TBD R-prep | yes (target `corelink.byok.vault.audit`) |
| 7 | Constant-time fingerprint compare on AAD (mock-mode tamper detection) | yes (`subtle::ConstantTimeEq`) | TBD R-prep | TBD R-prep | yes (`subtle::ConstantTimeEq` on validated key-name path; Vault server-side enforces AAD via `context`) |
| 8 | Native-only `aws-sdk-kms` / equivalent gated by `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` | yes | needs port | needs port | yes (`reqwest` + `tokio` + `regex` native-only) |
| 9 | `*WasmStub` linked on `target_arch = "wasm32"` with explicit `BYOKError::Provider("... unsupported on wasm32 ...")` | yes (`AwsKmsWasmStub`) | TBD R-prep | TBD R-prep | yes (`VaultWasmStub`) |
| 10 | ≥ 8 unit tests + 1 prop test (deterministic AAD canonicalization roundtrip) | yes (14 + 1 prop) | yes (mock-only) | yes (mock-only) | yes (22 + 1 prop, AAD-as-`context`) |
| 11 | Optional `#[ignore]` live test against `*_TEST_KEY_ARN` env (CI nightly) | yes (`e2e_byok_aws_kms.rs`) | yes | yes | yes |
| 12 | Server feature flag `byok-<provider>-real` wires the real impl behind the `KmsProvider` trait object | yes (`byok-aws-real`) | TBD R-prep | TBD R-prep | yes (`byok-vault-real`) |

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

**Known limitation (2026-05-15):** the underlying `corelink-byok` crate
itself does not yet build for `wasm32-unknown-unknown` (uses `getrandom`
without the `js` feature, `tokio` "full"). The wasm32 stub source code
in this crate is *target-shaped* so once `corelink-byok` is made wasm32-
compatible (a separate, larger PR), no further changes to the AWS
adapter are needed. Tracked under the broader CF-Worker BYOK proxy work
item.

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

### 6.1 GCP Cloud KMS (`crates/corelink-byok-gcp`)

- [ ] Move JCS canonicalization into the wrap/unwrap path
      (currently uses plain `serde_json::to_vec`).
- [ ] Add `resolved_fips_endpoint()` returning the regional FIPS URL
      (`cloudkms.<region>.rep.googleapis.com` for FedRAMP).
- [ ] Add audit-emit fail-CLOSED at every error site (target
      `corelink.byok.gcp.audit`).
- [ ] Gate `reqwest` + `serde_json` direct deps unchanged (already
      wasm-compatible); add `GcpKmsWasmStub` (returns
      `BYOKError::Provider("... unsupported on wasm32 ...")`).
- [ ] Add `byok-gcp-real` server feature flag (mirrors `byok-aws-real`).
- [ ] Add ≥ 8 unit + 1 prop test (`real_unit.rs`).

### 6.2 Azure Key Vault (`crates/corelink-byok-azure`)

- [ ] Same canonicalization migration as GCP.
- [ ] FIPS endpoint = Managed HSM URL (`*.managedhsm.azure.net`).
      Premium tier (vault.azure.net) is FIPS 140-2 Level 2; Managed HSM
      is Level 3. Constructor MUST accept only Managed HSM URLs for GA.
- [ ] Audit-emit + wasm stub + feature flag — same as GCP.

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

---

## 7. Acceptance gate

A provider crate is **GA-ready** only when:

1. All 12 pattern checklist items in §1 are green.
2. The vendor FIPS attestation letter is on file (see
   `BYOK-FIPS-ATTESTATION-MATRIX.md` — currently 1/4 attested).
3. The provider crate has a passing nightly `#[ignore]` live test
   against a staging CMK.
4. `python3 scripts/validate_specs.py` reports zero diagnostics for the
   crate's referenced INVs.
5. The server feature flag builds with the rest of the workspace
   (`cargo build -p corelink-server --features byok-<provider>-real`).

GA goal: 4/4 providers GA-ready by **2026-06-14** (D+30 cap from
`BYOK-FIPS-ATTESTATION-MATRIX.md`).
