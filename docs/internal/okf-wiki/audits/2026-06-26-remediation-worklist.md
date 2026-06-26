---
title: "OKF Wiki Truth-Audit — Remediation Worklist"
type: "Audit"
description: "Exact, transcribe-only fix list for the 5 MAJOR + ~20 MINOR findings from the 2026-06-26 truth audit. An agent applies each verbatim; the lead cold-checks M1/M2 (prose truth)."
status: "TO APPLY"
tags: ["okf", "audit", "remediation"]
---

# Remediation worklist (apply each fix EXACTLY — these are pre-decided, transcribe-only)

All paths are concept files under `docs/knowledge/`. Citations are `path:line`. Do NOT change any code
file. After all fixes: run `python3 scripts/okf_index.py` then `python3 scripts/validate_okf.py` → must
stay GREEN (citations still resolve). Keep `checkpoint_sha` unchanged.

## MAJOR (truth-critical — M1/M2 are prose rewrites, use the EXACT corrected text)

### M1 — surfaces/action-cache.md  [TRUTH]
Find the claim that cross-tenant attempts return 403 "with a LookupDenied/UpdateDenied audit row".
REPLACE that claim (and its `ac.rs:33-37` citation) with this corrected text + citation:
> Cross-tenant attempts are rejected **HTTP 403** by the route-level tenant check
> (`crates/corelink-container/src/routes/ac.rs:496-497`, `:554-555`, `:621-622`), which returns
> **before** `lookup`/`update` runs — so this reject path itself writes **no** audit row. (The
> `LookupDenied`/`UpdateDenied` audit rows are emitted by the lookup/update handlers on
> authorized-but-denied paths, not by this route-level cross-tenant 403.)
Ensure `# Citations` lists `ac.rs:496-497` (and `:554-555`/`:621-622`) and drops the `33-37` doc-comment cite.

### M2 — surfaces/public-packages.md  [TRUTH]
Find the intro thesis implying public **npm** bytes are deduped cross-tenant. REPLACE with EXACT text:
> Public upstream bytes for **pip, brew, and OCI** are stored once under the shared `_public` namespace
> and deduped **cross-tenant** — the network-effect moat (`crates/corelink-container/src/routes/pip.rs:115-118`,
> `crates/corelink-container/src/routes/brew.rs:85-95`). **npm is per-tenant today:** npm tarball bytes
> are namespaced per-tenant (`crates/corelink-container/src/routes/npm.rs:40-46`); only npm *metadata*
> splits public/private, and cross-tenant npm tarball dedup is a tracked, not-yet-built enhancement.
Update `# Citations` + `source_files` so npm.rs/pip.rs/brew.rs are all cited where claimed.

### M3 — surfaces/native-cas.md  [GROUNDING]
The `cas:rw` write-scope-gate claim cites `cas.rs:741-743` (that's the PAT-possession gate). Repoint the
scope-gate claim + its citation to `crates/corelink-container/src/routes/cas.rs:738-739`. (If 741-743 is
also referenced for the possession gate, keep it labeled as the possession gate.) MINOR also: the 415
content-type claim cites `cas.rs:538-545` (the `content_type_is` predicate) — add the batch-handler
call-site line that actually returns 415.

### M4 — tenancy/isolation.md  [GROUNDING]
The `idFromName(tenant_id)` routing claim cites `worker/src/index.ts:56` (a comment about EVENT_LOG_DO).
Repoint to `worker/src/index.ts:2465-2467`.

### M5 — tenancy/isolation.md  [SOURCE_FILES]
Add `crates/tenant-path/src/prefix.rs` to this concept's `source_files`. Repoint the HMAC-prefix /
newtype claims from `tenant-path/src/lib.rs:5-7,27-29` to: derive_prefix `prefix.rs:148-166`,
private-field newtype `prefix.rs:91-92`, `TENANT_PREFIX_LEN=16` `prefix.rs:15`. Also MINOR: `TenantCtx::new`
signature is `new(tdk, tenant_id, region)` — add the missing `region` arg.

## MINOR (citation precision / source_files completeness)

- planes/worker-edge.md: (a) replace "fail-CLOSED by default" with "request-count fail-CLOSED; storage verb-aware (reads fail-open for availability, byte-adding writes fail-closed)"; (b) add cite `worker/src/index.ts:2584-2596` for the Sentry header-scrub-list claim.
- flows/cas-write.md: citation #2 calls the function `cas_handler_from_env` — the real symbol is `build_handlers` (`cas.rs:399`). Rename the label.
- auth/hmac-fast-reject.md: add `crates/corelink-container/src/main.rs` to `source_files` (the prod-fatal backstop lives at `main.rs:77`,`:253`).
- auth/d1-pat-store.md: the scope NULL→"" fail-closed claim cites `customer_d1.rs:319-323` (the dashboard inverse-mapper). Repoint to `crates/corelink-container/src/adapter_pat.rs:144-150` + `scope.rs:73-95`.
- auth/argon2id-verify.md: (a) permit-acquired-after-HMAC ordering cites `adapter_pat.rs:160-170` (the const); repoint to `adapter_pat.rs:542-543` (HMAC) + `:616-629` (acquire); (b) the "empty/unrecognized scope grants nothing" invariant cites `scope.rs:34-45` (module doc); repoint to `scope.rs:73-95`.
- storage/r2-cas-bucket.md: scope the "CAS is ONE bucket" claim to the native S3 adapter; add a note that `corelink-region::Region::r2_bucket_name()` (`crates/corelink-region/src/region.rs:66-70`) defines per-region `corelink-cas-{region}` names.
- storage/byok-envelope-encryption.md: (a) "boxed trait object"/"boxed `dyn KmsProvider`" → "`Arc<dyn KmsProvider>`" (factory returns `Arc::new`); (b) the compile_error guard cites `corelink-byok/src/lib.rs:16-21` (doc) — repoint to `:105-139`; the `#![forbid(unsafe_code)]` cite → line `93`.
- tenancy/governance.md: (a) `/v1/users/me` AuthTenant claim cites `users.rs:75-80` (the `router()` fn) — repoint to the `handle_me` handler signature (below line 85); (b) the "rate limiter wired as ONE `.layer()`" claim is grounded only in the module doc-comment — cite the actual `.layer()` wiring site in `routes.rs` (and add it to source_files if needed).
- crates/cas-ac-core.md: add `crates/corelink-hash/src/verified_body.rs` + `crates/corelink-hash/src/digest.rs` to `source_files`; cite `verified_body.rs:33-44` (compute-then-`verify_constant_time`, `Err(HashMismatch)`) and `digest.rs:78-79` for the BLAKE3 integrity invariant (currently grounded only on the `lib.rs` rustdoc).
- crates/adapter-hosts.md: the BYOK compile-time-guard claim cites `corelink-byok/src/lib.rs:16-32` (rustdoc) — repoint to `:105-150`. (The `INV-BAZEL-NO-GROPC` code typo is a CODE bug tracked separately — leave the concept's correct `INV-BAZEL-NO-GRPC` id, but soften to note the code currently spells it `GROPC`.)
- crates/audit-analytics.md: the intro presents `sha256(prev||content_hash)` as the cluster-wide chain spine — soften to note the two chain crates differ: `corelink-audit` links via SHA-256 (`audit/src/lib.rs:25`), `corelink-audit-chain` via BLAKE3 (`audit-chain/src/lib.rs:31`,`:36`).
- crates/operations.md: the RFC-6585 Retry-After clamp claim cites `corelink-ratelimit/src/lib.rs:28-43` — extend to `:11-18,28-43` (the floor/ceiling line is at 16-18).

## Code bug to track separately (NOT in this docs PR)
- `crates/corelink-bazel-bridge/src/lib.rs:48-50`: invariant id is misspelled `INV-BAZEL-NO-GROPC` (should be `GRPC`). One-char code fix — file as its own tiny PR.
