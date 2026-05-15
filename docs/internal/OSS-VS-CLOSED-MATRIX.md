---
id: "OSS-VS-CLOSED-MATRIX"
type: "governance_policy"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["oss", "licensing", "boundary", "DEBT-002"]
---

# OSS vs Closed-Source Crate Matrix

Closes DEBT-002 (OSS prep). Per-crate decision: which crates are
dual-licensed (MIT OR Apache-2.0) and ship to crates.io vs which
stay proprietary in the closed CoreLink server repo.

## Decision boundary

Crates that meet ALL of the following are **OSS** (dual MIT/Apache):

1. Customer-facing surface (CLI, SDK FFI, type defs of REAPI shapes).
2. Zero proprietary algorithm or business logic; only protocols /
   types / verifiers that customers must run locally.
3. No secrets, no server-side stateful logic.
4. Versioned semver-major; cleanly testable in isolation.

Everything else (server-side handlers, BYOK real-providers, audit
chain server, billing pipeline, rate-limit + abuse heuristics, DSR
worker, replica orchestration) stays **closed**.

## Matrix

| Crate | Decision | Rationale |
|---|---|---|
| `corelink-cli` | OSS | Customer-facing binary; surface must be inspectable to be trustable. |
| `tenant-path` | OSS | Pure HMAC primitive; no secrets; trust depends on customer reproducibility. |
| `corelink-hash` | OSS | BLAKE3 wrapper + VerifiedBody; customer-side verification primitive. |
| `corelink-openapi` | OSS | API contract; consumers need it to generate clients. |
| `corelink-py` | OSS | Python FFI wrapper for CLI. |
| `corelink-go` | OSS | Go cgo wrapper. |
| `corelink-wasm` | OSS | WASM bundle for browser audit-chain verification. |
| `corelink-client-verify` | OSS | Customer-side proof verifier. |
| `corelink-rate-headers` | OSS | Customer-facing 429 response shape + parser. |
| `corelink-audit` (CloudEvents schema only) | OSS — schema crate split | Schema customers parse; server impl stays closed. |
| `corelink-ac-schema` | OSS | Action cache wire schema. |
| `corelink-auth-schema` | OSS | Auth wire schema. |
| `corelink-multipart-schema` | OSS | Multipart wire schema. |
| `corelink-byok-*` (all 5: byok / aws / gcp / azure / vault) | **closed** | Server-side KMS orchestration; trade secret. |
| `corelink-byok-matrix-test` | **closed** | Internal QA harness. |
| `corelink-audit-chain` | **closed** | Server-side Merkle append impl + retention engine. |
| `corelink-audit-chain-replica` | **closed** | Server-side; depends on audit-chain. |
| `corelink-stripe-real` | **closed** | Server-side Stripe orchestration. |
| `corelink-tier-selection` | **closed** | Pricing logic. |
| `corelink-billing-*` (5 crates) | **closed** | Server-side billing pipeline. |
| `corelink-dsr` | **closed** | DSR orchestration impl. |
| `corelink-privacy-*` (8 crates) | **closed** | Server-side privacy pipeline. |
| `corelink-ratelimit` + `corelink-rate-limit` | **closed** | Abuse heuristics; trade secret. |
| `corelink-quota` + `corelink-quota-*` | **closed** | Server-side quota. |
| `corelink-clerk` + `corelink-clerk-cf` | **closed** | Server-side session validation. |
| `corelink-pat` + `corelink-webauthn` | **closed** | Server-side credential issuance + verify. |
| `corelink-admin-*` | **closed** | Internal admin. |
| `corelink-failover-router` + `corelink-region` + `corelink-replica-worker` | **closed** | Server-side replication. |
| `corelink-tracing` + `corelink-slo` + `corelink-logpush` | **closed** | Internal observability. |
| `corelink-survey` | **closed** | Customer feedback infra. |
| `corelink-tenant-offboarding` | **closed** | Tenant state machine. |
| `corelink-handler-{cas,ac,admin}` | **closed** | Server-side handlers. |
| `corelink-backup-verify` | **closed** | Backup orchestration. |
| `corelink-cf-bindings` | **closed** | CF Worker bindings; wasm32-only; tightly coupled to deployment. |
| `corelink-drata-sync` | **closed** | Compliance evidence collection. |
| `corelink-otel-export` | **closed** | Customer telemetry forwarding (config-by-customer; impl by us). |
| `corelink-d1-migrations` | **closed** | Migration replay harness. |
| `corelink-chaos-*` + `corelink-dr-drill` + `corelink-oncall` | **closed** | Ops infra. |
| `tests/e2e-*` (all 9 E2E crates) | **closed** | Internal test harnesses. |

## Coverage summary

- **OSS**: 13 crates (CLI, schema, primitives, FFI wrappers)
- **Closed**: ~60 crates (server-side, ops, billing, privacy, auth)

## Repo split (post-GA)

Today everything lives in `humangr-labs/corelink-server` (closed).
Post-GA we **split**:

| Repo | Visibility | Contents |
|---|---|---|
| `humangr-labs/corelink` | **PUBLIC** | OSS crates (above) + customer-facing docs + SDK examples. |
| `humangr-labs/corelink-server` | **PRIVATE** | Closed crates + server deployment + compliance docs + internal runbooks. |

Migration WI: `WI-OSS-SPLIT-2026` — scheduled R-8 launch (T-7d). Until then, OSS-flagged crates carry `license = "MIT OR Apache-2.0"` in their `Cargo.toml` so any one-off publish-to-crates.io step is unblocked.

## Cargo.toml license tag rollout

Per-crate `license` field bumped in this commit for the 13 OSS-flagged
crates. Closed crates retain `license-file` pointer or omit (no
crates.io publish path expected).

Verify post-bump:

```bash
cargo metadata --format-version 1 --no-deps \
  | jq '.packages[] | select(.name | startswith("corelink") or . == "tenant-path") | {name, license}'
```

## DEBT-002 closure evidence

- LICENSE-APACHE-2.0 + LICENSE-MIT — stub-with-canonical-URL form (full text fetched via `scripts/fetch-license-texts.sh` for SBOM / cargo package).
- CONTRIBUTING.md — DCO sign-off requirement; first-PR walkthrough; review process.
- CODE_OF_CONDUCT.md — Contributor Covenant v2.1 adoption.
- .github/workflows/dco-check.yml — Per-commit Signed-off-by trailer enforcement.
- This matrix — per-crate OSS-vs-closed decision.
- 13 OSS crates tagged `license = "MIT OR Apache-2.0"` in Cargo.toml.

Update debt register: DEBT-002 → CLOSED 2026-05-15 (commit pending).
