---
title: "OKF zero-counter round 1 — anti-drift re-grounding worklist"
type: "Audit"
status: "TO APPLY"
tags: ["okf","audit","anti-drift","remediation"]
---
# Zero-counter round 1 found the systemic defect: invariants cite //!/barrel/trait-sig/const/doc (the prose that NARRATES enforcement) instead of the executed enforcer line → C5 watches inert prose. UNIFORM FIX: for each invariant below, OPEN the file, find the executed enforcer (the `if`/SQL/`return`/`match`/fn-body that PERFORMS the check), and repoint the invariant's INLINE cite to it (confirm the line shows the enforcement). If a claim has NO code enforcer (deployment/topology/infra fact), mark it explicitly as such — do NOT fabricate a cite. Add any newly-cited file to source_files (C6b). Also apply the TRUTH fixes. Confirm every line in THIS worktree. Do NOT run okf_index.py / edit index.md / commit.

## TRUTH FIXES (do these precisely — they are wrong-vs-code, not just grounding)
- surfaces/native-cas: the type is `CasRouteState`, NOT `CasHandlerState` (which exists nowhere). Fix the name; fields real at `cas.rs:142-154`.
- storage/byok-envelope-encryption: "AWS factory enforces the FIPS endpoint UNCONDITIONALLY" overstates — `container/src/byok.rs:26-29` only DELEGATES to `AwsKmsRealProvider::new`; FIPS enforcement is in that other crate. Reword to "delegates to AwsKmsRealProvider (FIPS enforced there)". Also the "cargo-deny at workspace boundary" cite points at comment lines with no cargo-deny config — soften or cite the real deny.toml if it exists.
- crates/d1-config-db: "runs inside DO `storage.transaction()` for true atomicity" overstates — the crate ships only the InMemory store; there is no DO-transactional impl present. Reword to designed/InMemory reality.
- ops/perf-playbook: the deployed p99 gate is tiered **5% critical / 15% non-critical**, NOT a flat 10% — fix the number; cite `.github/workflows/perf-regression.yml` (add to source_files).
- surfaces/public-packages: the OCI charge-all-methods gate cite `oci.rs:842-849` is the COMMENT block; the real unconditional `gate.check(&tenant)` is `oci.rs:850-854` — repoint.
- auth/introspect: the vCPU-h fail-OPEN "wall-off" cite `auth_introspect.rs:382-394` is `decode_runner_cap` (concurrency, no vCPU logic) — repoint to `decode_runner_vcpu_h` (~`:426-454`, the `if raw.is_null() { return Ok(None) }`).
- security/pentest-learnings: the "live in prod `36d6a891-r1`" image is stale (prod cycled past, e.g. 90e914f6) — soften to "fixed + deployed" without pinning a stale image, or mark the image a point-in-time value.
- flows/cas-write: the per-tenant rate-limiter bounds ARRIVAL RATE, not in-flight concurrency/memory — only `DefaultBodyLimit` (per-request 10 MiB, `main.rs:459-464`) is a hard memory bound. Reword: batch peak memory is bounded per-request by DefaultBodyLimit (not unbounded), but the rate-limiter is not a concurrency cap; the residual is the missing hard CasPutGuard semaphore.

## RE-GROUND invariant inline cites to the EXECUTED enforcer (repoint each; the real enforcer line is named — confirm it):
### flows/surfaces (Z2)
- flows/billing-quota-check: atomic check-and-accrue → `tenant_quota.rs:1021-1060` (`:1031-1036` the `WHERE accrued+delta<=budget RETURNING` SQL), NOT the trait-sig `:250`; LeasedQuotaStore → `:658` impl (not the :366-451 doc); staged ON CONFLICT dedup → the real `store.stage()` SQL (find it), key is `idem_key`.
- surfaces/bazel-reapi: shared-store moat → `routes.rs:527` (`build_handlers_from(cas_read,cas_write,ac_lookup,ac_update)` passing the same Arc handlers from `:392/:451`), NOT `bazel_v2.rs:5-8` //!.
- surfaces/public-packages: "tenant from bearer, path never trusted" → the real resolvers `npm.rs:238-247`/`pip.rs`/`brew.rs` (PatVerifier.verify), not the module //!; "OCI per-tenant" → `oci.rs:534-536/:568-570` not `:188-192` doc.
- surfaces/turborepo: R2KvStore durable backing → `turbo_v8.rs:722-745/:754` (selection/fail-closed), not the `:705-708` /// doc.
- surfaces/sccache-cargo: trust-model → `cargo.rs:181` resolver (already co-cited — repoint the invariant inline too); nest_service → `:246` call, not the `:23-26` //!.
- planes/worker-edge: "only layer that reads pat.scope" → read `index.ts:1013-1014`, forward `:2501`, not the `:285-300` type doc.
- planes/container: rate-limit outer layer → `routes.rs:825` (the `.layer(rate_limit)`), not `:780-822`.
- flows/request-flow: "shared CAS/AC handlers" → AC build begins `routes.rs:481` (cite both CAS :392 and AC :481); proxy port → `durable_object.ts:367` (getTcpPort construction), not `:251-264`.
- flows/introspection-fabric: vCPU wall-off → `decode_runner_vcpu_h` (see truth fix), not `:382-394`.

### auth/tenancy (Z3)
- tenancy/storage-quota-header: accrue/release/from_env → the executed impls BELOW `byte_accounting.rs:130` (find accrue/release/byte_accountant_from_env), not the `:1-64` //! + sql-illustration.
- tenancy/governance: "/_health,/_internal/* never rate-limited" → the route-merge order enforcer in main.rs (find it), not the `ratelimit_layer.rs:15-22` //!.
- tenancy/dollar-ceiling: LeasedQuotaStore refill/debit-chunk → the impl below `tenant_quota.rs:459` (the lease impl / refill override), not the `:366-451` /// + const.
- auth/hmac-fast-reject: prod-FATAL predicate → `main.rs:78` (the `prod && !gate_present` body), not the `:77` fn-sig (the :253 call site stays cited).
- auth/argon2id-verify: dummy-burn shared bucket → the routing enforcer (~`adapter_pat.rs:586`), not the const-def `:220-229`; per-tenant fairness sub-cap → the acquire site (~`:638-641`), not the const `:172-199`.
- tenancy/isolation: "cargo/sccache PRIVATE per-tenant" → the cargo per-tenant namespacing enforcer (find it in cargo.rs), not the `:92` /// doc.
- auth/introspect: "not reachable from public internet" → NO code enforcer (deployment topology) — mark it explicitly a deployment/topology fact, not a code invariant.

### storage/crates (Z4) — repoint each invariant's INLINE cite to the named real enforcer
- crates/d1-config-db: INV-TENANT-ISOLATION → `customer_d1.rs:58+` (the real WHERE-tenant SQL); fail-CLOSED→500 → the `Internal` mapping (find it); deny_unknown_fields → the `#[serde(deny_unknown_fields)]` submodule line; VersionConflict CAS → `ConfigSingletonStore::update` submodule line. (Add the submodule files to source_files.)
- crates/chunk-manifest-buckets: INV-MULTIPART-PATH-TENANT-SCOPED → the compose body `object_key.rs:85-91`, not `:24-32`/`:54-58` docs; cross-tenant upload_id reject → `adapter.rs` CrossTenantUpload logic; #[non_exhaustive] → `types.rs` def lines.
- storage/byok-envelope-encryption: zeroize/SecretString/ct → the byok_core discipline lines, not the `lib.rs:64-90` //!.
- storage/cas-hot-path-latency: fail-CLOSED → `.map_err(Transient)` at `billing_d1_http.rs:113`, not the `:40-51` //! (the WP-2/GDPR perf-doc cites are inherently doc — leave, they're about the perf doc).
- storage/r2-cas-bucket: INV-TENANT-ISOLATION → `r2_s3.rs:453-458/:573-601` (tenant_prefix/blob_key), not the `:1-19` //!.
- crates/adapter-hosts: "adapters free of SPI imports" → no positive code enforcer (Cargo.toml dep-graph) — mark as dep-graph/negative guard.
- crates/audit-analytics: "never re-canonicalize at link time" → `link_hash.rs` logic, not `corelink-audit/lib.rs:66-70` //!.
- crates/auth-pat: charter (RLS/ct/SecretString) → the auth-schema/pat enforcer lines, not `corelink-auth/lib.rs:76-91` //!.
- crates/billing-commerce: INV-AUDIT-EMIT-ATOMIC → `ledger.rs:197-204` (already co-cited — repoint the invariant inline), not `billing-stripe/lib.rs:21-24` //!.
- crates/container-platform: "DO speaks only HTTP / gRPC removed" → main() axum serve + Cargo.toml dep-absence (mark dep-graph); storage-backing → the real `static`/fn, not the `:11-17`/`:48-55` //!.
- crates/privacy-compliance: MFA "None on destructive→Required" → `endpoint.rs:354-367` (already co-cited — repoint inline), not the `mfa.rs:80-103` trait /// .

### security (Z5) + ops (Z6) — ADD the real code files to source_files so the gate can WATCH them (these are currently doc-only)
- security/attack-surface-dataplane: add `crates/corelink-container/src/routes/cas.rs` + `cas_erase.rs` (+ the surfaces) and cite the real `TombstoneGatedCasHandler`/`R2CasHandler` lines for the defense claims.
- security/credential-handling: add the worker+container gate files (`worker/src/lib/internal_auth.ts`, the constant-time compare, the dedicated-key resolvers) + cite.
- security/money-path-review: add `worker/src/lib/quota.ts` (subscription_state gate :152), `customer_d1.rs:799` (accrue), `oci_cap.rs:131` + cite.
- security/posture-overview: add the real isolation enforcer (tenant-path prefix) + hedge "BYOK envelope encryption" as a DESIGNED (unwired) posture layer, not live.
- ops/perf-playbook: add `.github/workflows/perf-regression.yml` (truth fix above).
- ops/deploy-runbook: add the deploy-gate script/workflow if it makes a current-CI claim (find it); else mark the secret-gate claim as ADR-0025 process.
- ops/engineering-onboarding: add the `.github/workflows` gate set / pre-commit config, or mark the PR-gate list a process statement.
