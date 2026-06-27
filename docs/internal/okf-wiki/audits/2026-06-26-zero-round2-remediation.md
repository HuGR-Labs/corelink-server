---
title: "OKF zero-counter round 2 — definitive invariant-inline re-ground + truth + ADR caveats"
type: "Audit"
status: "TO APPLY"
tags: ["okf","audit","anti-drift","remediation"]
---
# Round 2 (51 findings): truth holds; residue = invariant INLINE cites still on //!/sig/const (mostly mitigated by elsewhere-cite) + ~6 truth-mislabels + 5 ADRs-as-live + residual doc-only. DEFINITIVE FIX: for EVERY invariant in EACH listed concept, make its INLINE cite the EXECUTED enforcer (the if/SQL/return/match/acquire that PERFORMS it) — open, confirm, repoint. Add new files to source_files (C6b). Don't fabricate; mark genuine topology/infra/process facts as such. Confirm every line in THIS worktree. No okf_index.py / index.md / commit.

## TRUTH FIXES (wrong-vs-code — do precisely)
- ops/reproducible-build: the workflow is `schedule: cron '0 10 * * *'` + workflow_dispatch only (NOT v*-tag, NOT 04:00), on `[self-hosted,Linux,X64]` (NOT ubuntu-22.04 matrix) — fix the trigger+runner prose; ADD `.github/workflows/reproducible-build.yml` to source_files + cite `:20-25,46`.
- storage/cas-hot-path-latency: the no-row read path returns `Ok(None)` at billing_d1_http.rs:~313 (NOT a Transient arm); `:319` is the missing-`tier`-column arm. Fix the label; keep the `:113` `.map_err→Transient` cite.
- ops/audit-analytics-plane: `verify_ndjson_http.rs:104` is a Display doc-comment, NOT "a request builder that never includes the Bearer" — the real builder `:218-229` DOES set Authorization. Fix the claim/cite; ground the chain-head-anchor ct-match on `ascii_eq_ct` `:294,:343`.
- tenancy/storage-quota-header: `oci/auth.rs:285-303` is `mint`, not `encode_cap` (which is ~`:243-248`) — fix the symbol/range.
- ops/gc-eviction: `MAX_ENTERPRISE_TTL_DAYS=730` is at `tier.rs:44` (`:41` is a comment) — fix the line.
- storage/r2-ac-regional: the `R2_AC_BUCKET_PREFIX` env override is honored in ANY env (not "only non-prod" — that's doc-comment intent) — soften; AC fixed-set enforcer is `AC_REGIONS` at `adapter_r2_ac.rs:56` (iterated in list_and_delete_ac), repoint the fixed-set half off `:84-90`.

## ADR "Status vs shipped code" caveats (library-only narrated as live — add the same caveat the sibling ADRs carry; container depends on corelink-privacy-erasure-worker, NOT corelink-privacy)
- adr/adr-s14-002-region-pinning-enforcement: the 4-layer region-pinning stack is crate-only in corelink-privacy (container has NO dep; deployed path is the header-based 409 residency_guard); the `region_enforcer` DO does not exist. Add the caveat.
- adr/adr-s11-008-...: the sub-processor system is library-only (corelink-privacy/src/sub_processor.rs), no route mounted — the "30-day notice emails / broadcast seeds all tenants / consent_revoke live" Consequences need the not-deployed caveat.
- adr/adr-s11-011-region-migration-cooldown: library-only, `POST /v1/admin/tenant/region-migration` unmounted — add caveat.
- adr/adr-s14-005-byok-gcp-azure-vault: BYOK unwired (no live CAS blob BYOK-encrypted); the matrix is a WEEKLY cron not per-PR — add caveat.
- adr/adr-s14-001-multi-region-terraform-module: reality is US-only (all R2 ENAM); the module/binary exist but "four production regions stood up / 4 KV namespaces provisioned" is not live — add caveat.

## COMPLETE the doc-only grounding (add the real code file + cite the enforcer for the affirmative current-code claims)
- security/attack-surface-dataplane: add `routes/bazel_v2.rs` (adapter scoping), `routes/turbo_v8.rs` (teamId demote), the OCI digest-verify + moat re-hash enforcers; cite the live enforcer for the Bazel/Turbo/OCI/npm isolation claims (CAS/erasure/r2 already grounded).
- security/money-path-review: add the $-ceiling enforcer (`tenant_quota.rs` check-and-accrue) + the F-018 `requires_cache_write` billing-scope gate sources; cite them (only the subscription_state gate is grounded today).
- security/credential-handling: repoint the dedicated-key cites from `main.rs:482/:503` (comment) → the executed `if let Some(_) = build_state_from_env()` at `:486/:509`; ground PAT-plane-separation on /auth cross-links (acceptable) or the real argon2id/PatVerifier line.
- security/posture-overview: ground the Worker-authZ isolation layer on the real scope==tenant check (find it), not ARCHITECTURE.md.
- ops/cli-reference: add `tools/cli/src/telemetry.rs` cites (`:24,:27,:113-123`) for the telemetry domain/timeout/detached-POST claims.
- ops/secrets-lifecycle: add `corelink-clerk/src/env_config.rs:44` + `migrations/d1/0037_signup_orchestration.sql:102-103` for the CLERK_JWT_ISSUER + pat.scope claims.
- ops/perf-playbook: citation #15 → repoint to the executed env block `.github/workflows/perf-regression.yml:80-81` (NOT the `:11-12` comment).

## RE-GROUND remaining invariant INLINE cites to the executed enforcer (open each, confirm, repoint)
- surfaces/bazel-reapi: concurrency invariant const `bazel_v2.rs:112` → executed `if *count >= BAZEL_WRITE_CONCURRENCY_LIMIT { 429 }` `:183-184`; matchit `:name` //! `:38-41` → route strings `:278-300`.
- flows/pat-gauntlet: scope.rs fn-sig `:73/:93` → executed allowlist `matches!(t,"cas:rw"|...)` `:77/:94`.
- surfaces/public-packages: cross-tenant-dedup //! `pip.rs:32-37`/`brew.rs:27-30` → executed `moat.get/put(PUBLIC_NAMESPACE,…)` `pip.rs:117-118`/`brew.rs:90/104`; `.merge` //! `oci.rs:19-30` → `:753`.
- surfaces/action-cache: non-canonical-digest-400 → executed 400 returns `ac.rs:501-502/:558-559/:625-626`, not the predicate def `:460-462`.
- crates/container-platform: audit-before-mutation → `store.rs:~273` (`self.audit.emit(&entry)?` before insert ~:276), not the `:223-255` CAS-only range.
- storage/byok-envelope-encryption: zeroize/SecretString/ct → the real Drop/ct_eq enforcer in byok_core (find it), not the `lib.rs:64-90` //!.
- auth/pat-moat: Role "re-runs full verification" + uniform-rejection → the executed pipeline / the real VerifyError returns (already co-cited :543/:598/:650/:656 — repoint the Role/Invariant inline too).
- auth/hmac-fast-reject: uniform-401 → the rejection CALL-SITES (not the `unauthorized()` def :247-251); fingerprint-as-key → `let fp = fingerprint(token)` :165 (not the def :253-257); 256-array → the fixed Vec build in `new()` ~:118-120 (not the const decl).
- auth/argon2id-verify: admin-superset → `matches!(…"admin"…)` `scope.rs:77/:94` (not the //! :34-45).
- auth/d1-pat-store: unrecognized-fail-closed → the `Err(token)=>Err(Unauthorized)` arm `customer_d1.rs:325-327` (just outside the cited `:311-324`); list-handler-None → the list handler ~`:899+`.
- tenancy/dollar-ceiling: fail-CLOSE invariant → executed arms `tenant_quota.rs:745/:762/:799` (not the //! :18-30).
- tenancy/request-quota: fail-OPEN → `return None` arms `request_count.rs:258/:266/:270/:277` (not //! :31-48); bearer-keyed → `if let Some(tenant)=oci_bearer_tenant` `oci.rs:841` (not //! :50-56).
- tenancy/governance: per-tenant-bucket-keyed → `req.headers().get(TENANT_HEADER)` ~`:442-447` + `tenant_key_uuid` ~`:463` (not //! :24-35); NoOp-sinks → the field type `limiter: Arc<...>` ~`:203`.
- crates/billing-commerce: drop the redundant `billing-stripe/lib.rs:21-24` //! citation (the Invariant already cites the executed `ledger.rs:197-204`).
- flows/billing-quota-check: idem_key→request_id bind → the actual bind ~`billing_ingest.rs:285` (not the stage() fn-sig :250).
