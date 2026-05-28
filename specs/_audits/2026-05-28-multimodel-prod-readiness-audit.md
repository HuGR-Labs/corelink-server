---
id: "AUDIT-2026-05-28-MULTIMODEL-PROD-READINESS"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-28"
updated: "2026-05-28"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "production-readiness", "security", "multi-model", "do-not-ship", "wave-32"]
references:
  - "specs/_audits/2026-05-22-w32-closure.md"
  - "crates/corelink-container/src/main.rs"
  - "worker/src/index.ts"
  - "worker/src/durable_object.ts"
  - "wrangler.toml"
---

# Multi-Model Brutal Production-Readiness Audit (2026-05-28)

> **Question asked:** Is CoreLink safe to ship to production / have real
> paying customers hit tomorrow?
>
> **Panel:** 1 Opus + 1 Sonnet (holistic "safe to ship?") + 10 Haiku
> (brute-force security lenses). All READ-ONLY.
>
> **VERDICT: DO-NOT-SHIP.** The product data plane is not deployed —
> only a gRPC health-check + (conditionally) the Stripe webhook are
> served. The 2026-05-22 closure audit's "GREEN / ready" verdict
> mistook infrastructure liveness for functional correctness.

## §0 The headline (verified, not just claimed)

The deployed production container (`crates/corelink-container/src/main.rs`)
serves ONLY:
- `HealthServer` (gRPC health) — `main.rs:345`, always
- Stripe webhook HTTP route — `main.rs:334`, ONLY when
  `STRIPE_WEBHOOK_SECRET` is set

The real product router — CAS + Action-Cache + Admin + audit-export +
audit-analytics — is built at `main.rs:253` into `_composed_router`
(leading underscore = **discarded/unused**). The code comment is explicit:
*"The composed router is constructed but not yet bound to an HTTP
listener."*

**Consequence:** a customer running `bazel build` / `docker pull` /
`npm ci` against the cache gets NOTHING (no route). The 8/8 smoke gate
that passed (`pre-cutover-wave32-extension.sh`) only tests `/health`,
DNS, Pages liveness, migration table-count, secret presence, and
BetterStack probes — pure infrastructure. The script that exercises the
actual CAS PUT/GET + audit chain was NOT the gate.

**Honest status: the platform is provisioned; the product is not deployed.**

## §1 The crucial nuance

The 87-crate codebase is genuinely high quality. The problem is NOT that
the code is wrong — it is that the correct code is **not wired into the
deployed binary / Worker / DO path.** Two auditors looking at the same
system reached opposite-seeming conclusions, and both are right:

- **Haiku-10 (multi-tenant isolation)** audited the CRATE libraries
  (`corelink-tenant-path`, `corelink-meta`, `corelink-cas`,
  `corelink-worker` middleware) → "NO CRITICAL GAPS; defense-in-depth
  across 5 layers; no confirmation-oracle; parameterized queries."
- **Opus (holistic)** audited what is ACTUALLY DEPLOYED → "no data plane,
  no auth on the live path, single shared DO, no R2/D1 bindings on the
  container."

Both true. The libraries are solid; the deployment wires almost none of
them. **The gap is "wiring + deploy", not "rewrite".**

## §2 P0 — blocks ship (the product isn't there)

| # | Finding | Evidence | Fix |
|---|---|---|---|
| P0-1 | Container serves only Health (+ Stripe webhook). CAS/AC/Admin router discarded. | `crates/corelink-container/src/main.rs:253` (`_composed_router`), `:345` (only HealthServer served) | Bind the composed router into an HTTP/gRPC listener on the container. |
| P0-2 | No PAT/auth validation on the live request path. Worker checks token FORMAT only (`_pending`); DO never validates. The real auth middleware (`corelink-worker/src/middleware/auth.rs`, `corelink-pat`) exists + is correct but is NOT in the deployed path. | `worker/src/index.ts:244-285`, `durable_object.ts` (no auth/D1 refs) | Wire the existing auth middleware into the DO or container ingress. |
| P0-3 | No tenant isolation on the live path — all authenticated traffic → single `_pending_auth` DO. The tenant-derivation code is correct (Haiku-10) but unrouted. | `worker/src/index.ts:492-493`, `durable_object.ts:234` (tenantId hardcoded null) | Resolve tenant from validated PAT; derive DO id from real tenant id. |
| P0-4 | Container has no R2/D1/KV bindings — cannot read/write a blob even if routed. | `wrangler.toml` `[[env.prod.containers]]` (no storage bindings); container env = RUST_LOG+PORT only | Thread storage to the data plane (Worker-side handlers OR container bindings). |
| P0-5 | `enableInternet:false` on the container → BYOK cannot reach AWS/GCP/Azure/Vault KMS. Headline feature unreachable. | `worker/src/durable_object.ts:400` | Enable egress (scoped) OR move BYOK to the Worker side. |

## §3 P1 — real bugs (fix before/with the wiring)

| # | Finding | Evidence |
|---|---|---|
| P1-1 | **BYOK non-canonical AAD serialization** — Azure/GCP/Vault use `serde_json::to_vec()` not `serde_jcs::to_vec()` (RFC 8785). Cross-arch (native↔wasm32) AAD bytes can differ → GCM tag verify fails → DEK unrecoverable → **data loss**. AWS adapter does it correctly. | `crates/corelink-byok/src/byok_{azure,gcp,vault}.rs` (~lines 410/187/226) |
| P1-2 | **Svix webhook replay window missing** — `svix-timestamp` captured but never validated. Old signed `user.created` webhook replayable → tenant re-provision. MATTERS NOW (the webhook is one of the only live surfaces). | `apps/signup-worker/src/webhooks/clerk.ts:270-294` |
| P1-3 | **Audit-chain verifier non-constant-time** — chain-break compare uses `!=` (timing oracle); exporter + archive correctly use `subtle::ConstantTimeEq`, verifier does not. | `crates/corelink-audit-chain/src/verifier.rs:162` |
| P1-4 | **PAT token_id compare non-constant-time** — `!=` short-circuit → token-id enumeration oracle. Violates the module's own INV-AUTH-CONSTANT-TIME-COLD-PAD. | `crates/corelink-pat/src/verify.rs:54` |
| P1-5 | **Rate-limit / quota gaps** — unbounded per-IP buckets (cleanup deferred), no per-tenant concurrency cap, free-tier quota race (concurrent writes both pass check). Cost-amplification + free-tier abuse on metered CF infra. | `migrations/d1/0010`, `crates/corelink-billing/src/quota/cas/cas.rs` |
| P1-6 | **Unauth DO admin endpoints** — `/_do/stop` (container shutdown) + `/_do/health` reachable without auth IF the Worker forwards `/_do/*` externally. Needs verification of external routability. | `worker/src/durable_object.ts:264,267` |
| P1-7 | **Broken D1 index** — `idx_quota_cas_attempts_tenant_now_ms` references non-existent column `now_ms` → silent full-scan under load (DoS). | `migrations/d1/0012_quota_cas_attempts.sql` |
| P1-8 | **No decompression-bomb / body-size limit** on the Worker ingress → CPU/memory DoS. | Worker router (no DefaultBodyLimit found) |

## §4 P2 — debt (track, not ship-blocking)

- Worker API responses missing X-Content-Type-Options / Referrer-Policy / Permissions-Policy (Haiku-CSP).
- CSP wildcard `*.algolia.*` / `*.ingest.sentry.io`; missing upgrade-insecure-requests (Haiku-CSP).
- `export_audit_log` no retention policy; `audit_outbox` UNIQUE too loose (Haiku-D1).
- smoke-install token via CLI arg (test-only, read:ping scope) (Haiku-secrets).
- Mutation-testing baseline deferred for byok + audit-chain (only hash measured).
- BYOK mock-mode XOR fingerprint collisions + silent `unwrap_or_default()` on AAD serialize (Haiku-BYOK).
- DO idle-stop uses `setTimeout` not `storage.setAlarm` (unreliable under DO eviction).
- Sentry no-op until DSN set → no error observability today.

## §5 What's genuinely STRONG (don't lose this in the gloom)

- **Tenant isolation libraries** — 5-layer defense-in-depth, HMAC tenant-path,
  parameterized D1, tenant-scoped dedup (no confirmation oracle),
  constant-time cross-tenant header rejection. (Haiku-10: no critical gaps.)
- **Secrets management** — mature: 138-row checklist, drift CI gate, zero
  real secrets in git, stdin-only `wrangler secret put`, SecretString
  redaction. (Haiku-secrets: EXCELLENT.)
- **JWT validation** — RS256-only, rejects alg=none/HS256, constant-time
  aud/iss, PAT HMAC+Argon2id two-stage. (Haiku-Clerk.)
- **Supply chain** — digest-pinned base images, non-root, minimal final
  image, deny.toml strict allowlist, dry-run-default deploy scripts. (Haiku-container.)
- **Audit-chain core** — canonicalization + linking + append-only
  cryptographically sound (verifier timing-bug aside). (Haiku-audit.)
- **D1 schema** — zero SQL injection, tenant-leftmost PKs, additive-only
  governance. (Haiku-D1.)

## §6 Remediation order (to actually reach shippable)

1. **Wire the data plane** (P0-1): bind `_composed_router` to a listener;
   thread R2/D1/KV to it (P0-4).
2. **Wire auth + tenant routing** (P0-2, P0-3): the middleware exists —
   put it on the live path; derive DO id from validated tenant.
3. **Enable BYOK egress** (P0-5) OR relocate BYOK to Worker.
4. **Fix the data-loss + auth bugs** (P1-1 BYOK JCS, P1-2 webhook replay,
   P1-3/P1-4 constant-time) — these would corrupt data / enable abuse
   the moment the data plane is live.
5. **Re-run the REAL smoke** — `scripts/smoke-prod-corelink.sh` CAS
   PUT/GET + audit checks (3/4/19/20) against the deployed binary, not
   just `/health`. THAT passing is the true ship gate.
6. Then rate-limit/quota hardening (P1-5..P1-8), then P2 debt.

## §7 Correction to the record

The `2026-05-22-w32-closure.md` (filled 2026-05-28) claimed L9 = GREEN,
"Worker→DO→Container is the real path." **That is false.** This audit
supersedes that verdict. The closure doc + memory
`corelink_prod_deploy_sealed.md` must be amended to reflect: infrastructure
provisioned + health/webhook live; product data plane NOT deployed;
status = NOT-SHIPPABLE pending §6 remediation.

## §8 DCO

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
(Panel: Opus + Sonnet + 10 Haiku, 2026-05-28.)
