# CoreLink — Enterprise Technical Due Diligence

**Date:** 2026-06-28 · **Commit audited:** `c58cb208` (main) · **Method:** 6-domain read-only
codebase DD (security, supply-chain, data-protection/compliance, reliability/ops, billing/financial,
code-quality) + adversarial cold-verification of every Critical/High finding against raw code.
**Scope boundary:** code/config only — no live pen-test, no SOC2 attestation, no legal opinion (those
are flagged NEEDS-EXTERNAL). Findings below were each re-verified at file:line; imprecise framings from
the first pass were corrected (noted).

---

## Executive summary

CoreLink is a **genuinely production-grade revenue core wrapped in a large, partly-aspirational
enterprise/compliance shell.** ~60–65% of the 73 crates are live-wired into the request path; ~35–40%
are skeletons / in-memory fakes / dead façades. The split is clean:

- **WIRED & SOLID (the money path):** multi-tenant CAS/AC on real R2 (aws-sdk-s3) + D1-over-HTTP, all
  five cache surfaces (native, Bazel REAPI, Turborepo, sccache, OCI), PAT auth (constant-time
  HMAC + Argon2id), Clerk RS256/JWKS session verify, Stripe HMAC-verified webhooks + checkout, the
  quota/$-ceiling/tombstone gates. Fail-closed everywhere, constant-time compares, **exceptional panic
  hygiene** (workspace lints DENY unwrap/expect/panic, `unsafe` forbidden — ~0 panics in prod request
  code), and an **enterprise-grade supply chain** (100% SHA-pinned actions, SLSA-L3, hermetic build,
  0 active dependency advisories, clean licenses).
- **SKELETON / SOLD-NOT-DELIVERED (the enterprise shell):** BYOK encryption-at-rest, immutable/keyed
  audit trail, multi-region replication & failover, observability/alerting, DR/backup, GDPR
  access/portability/rectification, granular consent, signed erasure attestation, WebAuthn.

### Verdict

| Audience | Rating | Rationale |
|---|---|---|
| **Stated launch (self-serve SMB cache, $30/mo)** | 🟡 **CONDITIONAL GO** | The core is fit-for-purpose. **Condition:** descope the unbuilt enterprise/compliance claims (BYOK, immutable audit, multi-region residency, DSAR, SOC2) from all marketing + legal collateral, and close the operational gaps (backups, alerting). |
| **Enterprise deal / SOC2 / acquisition DD** | 🔴 **NO-GO (today)** | Multiple deal-blockers: no real backups + broken restore, no alerting/observability, encryption-at-rest sold-not-delivered, forgeable+unverified audit trail, GDPR-rights skeleton, bus-factor = 1, test/gate theater. All remediable; none are architectural dead-ends except the BYOK↔content-addressing tension. |

### Domain scorecard

| Domain | Rating | Headline |
|---|---|---|
| Supply-chain & build | 🟢 GREEN | Enterprise-grade; 0 advisories, SLSA-L3, 100% SHA-pinned |
| Security & threat model | 🟡 YELLOW | Core auth/isolation SOTA; risks in audit-integrity, internal-plane blast radius, OCI OOM |
| Billing & financial integrity | 🟡 YELLOW | Forgery-safe core; no live reconciliation, dunning-gap-conditional |
| Code quality & production-readiness | 🟡 YELLOW | Production-grade core; ~35-40% scaffold, test-gate theater, bus-factor 1 |
| Data protection & compliance | 🔴 RED | BYOK undelivered, audit forgeable, GDPR rights modeled-not-enforced |
| Reliability & operational readiness | 🔴 RED | No real backups, simulator DR, no alerting, broken restore |

---

## Deal-blocking findings (ranked, all code-verified)

1. **No real backups + broken restore — potential unrecoverable customer data loss [CRITICAL, DD-4].**
   `scripts/backup-daily.sh` is a capable D1-export/GPG/R2 script but is invoked by **no live scheduled
   workflow** — only by `backup-daily-verify.yml`, whose live lane literally refuses to run
   (`backup-daily-verify.sh:204-208` → `live_handler_not_yet_wired`, exit 2) and "verifies" hardcoded
   synthetic snapshot ages. Separately, restore is broken: migration `0064` `DROP TABLE` + tenant FK
   children → a fresh-apply/restore fails `FOREIGN KEY constraint` and strands the DB at 0063.
   **→ Wire a live scheduled backup against prod; make 0064 FK-safe; flip verify to non-dry-run.**

2. **Blind operations — no metrics, no external logs, no working alerts [CRITICAL, DD-4].**
   `corelink-slo` ships only `InMemoryPagerDutyDispatcher`; the "real" HTTPS dispatcher exists only as a
   doc-comment (`pagerduty.rs:138`, no impl) and the crate has **zero deployed reverse-deps**. No
   analytics-engine binding, Logpush off, worker-Sentry inert (DSN only on admin-ui), prod/regional
   envs lack even native log retention (`wrangler.toml:40-42` — one `[observability]` block, not
   re-declared per env). **They could not detect a CAS error spike, tenant leak, or latency regression,
   and nothing pages.** → Wire a real PD dispatcher behind a scheduled SLO evaluator; ship logs/metrics
   to a backend; verify a page lands end-to-end.

3. **Encryption-at-rest "BYOK / your key" is sold but not delivered [CRITICAL, DD-3].**
   The CAS write path hands bytes to R2 with **no envelope-encrypt call** (`storage/r2_s3.rs`); the
   `byok_envelope` table is **read-only** (never written); the BYOK orchestrator is **never called** on
   the data plane. *Precise:* the `corelink-byok` crate has **real AES-256-GCM envelope code**
   (`byok_core/envelope.rs`) but it is unwired, and the default KMS provider is a dev XOR fake; data is
   protected only by **Cloudflare-managed R2 encryption**, not a customer key. Content-addressing
   (key = plaintext digest) is structurally incompatible with storing ciphertext → wiring BYOK is a
   **redesign, not a hook-up**. **→ Pull "BYOK / customer-held keys / crypto-shred" from all
   sales/DPA collateral until built; honest claim = "encrypted at rest with provider-managed keys."**
   *(This is a material-misrepresentation risk if currently marketed — the single most likely
   deal-killer in an enterprise security questionnaire.)*

4. **Audit trail is forgeable and never verified in prod [HIGH, DD-1/DD-3/DD-6 — confirmed by 4 lenses].**
   The live chain hash is **plain un-keyed BLAKE3** (`corelink-audit-chain/chain.rs:152-193`, no
   `keyed_hash`/HMAC/signature), sealed only at an hourly drain over **mutable D1 rows**, with R2
   Object-Lock **unwired**. An insider with D1 write (per CLAUDE.md, `.env.local`'s `CLOUDFLARE_API_TOKEN`
   has it) can rewrite any suffix and recompute a self-consistent chain + head; the verifier passes.
   The verifier additionally reads an empty in-memory exporter, so nothing is verified end-to-end. The
   SOC2 CC7 "tamper-evident / append-only" claim is not deliverable. *(Filed to engineering as CF-6:
   key/sign the head, chain at write-time, wire Object-Lock.)*

5. **GDPR rights surface is a skeleton; consent is modeled-not-enforced [HIGH, DD-3].**
   Only erasure has a live executor. `/_internal/dsr/*` registers **only `erase` + `verify`**
   (`routes/dsr.rs:351-352`) — access/portability/rectification routes are not wired; the DSR export
   gathers nothing (in-memory endpoint returns a receipt, no payload). `ConsentPurpose` (12 variants)
   has **zero production call sites** — nothing gates processing on consent. Erasure itself is now
   complete (CF-1, 60/60 tables) and physically sound; the rest of the rights surface will not survive
   a GDPR audit. **→ Implement gather-and-export + register the routes; wire consent checks or stop
   offering granular consent.**

6. **Sold multi-region / residency is logical-only; physical residency is single-bucket US [HIGH].**
   One physical CAS bucket (`corelink-cas-prod`, US) serves every region except EU/`lhr`; residency
   "enforcement" is a stateless HTTP-409 header-match guard (`routes/residency.rs:62`) over a metadata
   column, never the physical R2 location. The replication/failover plane is a closed island (5 crates,
   0 deployed callers). A `sam`/LGPD tenant's bytes physically land in the US. **→ Provision real
   per-region buckets or block non-EU/US tenants at request time; stop selling multi-region/failover.**

7. **Bus factor = 1 + test/gate theater [DD-6].** One founder authored ~2,605 of 2,623 commits at
   ~40/day (AI-driven velocity) — a single point of key-person + mental-model failure; ~25 prod secrets
   live only in a gitignored `.env.local` ("must be backed up", no vault). The headline "463/0" and
   "concepts PASS" numbers are **markdown/YAML doc-linters** (`validate_specs.py`), **not behavioral
   tests**; coverage is nightly + **ungated** (no fail-under); integration tests run on **in-memory/SQLite
   fakes**, not real D1/R2; the only real-prod e2e gate is **RED and off-CI**. A green PR does not prove
   the wired core stays correct end-to-end. **→ Second committer / secret vault; gate coverage; promote
   a real-environment smoke gate onto CI.**

---

## Notable single findings (below deal-blocker, real)

- **OCI blob upload OOM DoS [HIGH, DD-1]** — `oci/server/handlers.rs:491` buffers the chunk with
  `to_bytes(body, usize::MAX)` (no cap), while the manifest path correctly caps at 4 MiB. Any
  authenticated tenant drives multi-GB single-allocation heap pressure. **→ cap `to_bytes`.**
- **Any-tenant admin-PAT mint with no tenant authz [HIGH-conditional, DD-1]** — `/_internal/pat/mint`
  takes `tenant_id`+scopes from the body and can mint `SCOPE_ADMIN_ALL`; the gate is a dedicated
  `CORELINK_PAT_MINT_AUTH_KEY` that **falls back to the shared `CORELINK_INTERNAL_AUTH_KEY`** if unset.
  *Conditional:* full-compromise blast radius applies only if the dedicated key is not provisioned in
  prod (operator-controlled, unreadable). **→ Service-binding-only + per-tenant authz + confirm
  dedicated-key provisioning; the compare is already constant-time.**
- **No live billing reconciliation [HIGH, DD-5]** — `corelink-billing-reconcile` is InMemory + deferred
  (WI-S10-007); no usage→Stripe drift detection runs. Combined with the forgeable audit chain, the
  company cannot cryptographically prove what it billed in a chargeback. **→ Wire a reconcile cron.**
- **Dunning downgrade may not fire [HIGH-conditional, DD-5]** — two live activation writers; the
  container materializer is grant-only (never downgrades), the signup-worker holds the dunning logic.
  If the live Stripe endpoint targets the container alone, a lapsed tenant keeps paid access
  ($15-149/tenant/mo). **→ Confirm the live endpoint targets the downgrade authority (Stripe dashboard
  — NEEDS-EXTERNAL).**
- **BYOK AEAD binds no AAD on the warm-DEK path [MED, DD-1]** — even in the (unwired) crypto,
  `byok_core/envelope.rs` skips the AAD-match on a cache hit, so a ciphertext could be moved between
  blobs within a tenant. Fix before wiring.
- **MEDs:** un-salted audit email hash (rainbow-attackable); Sentry message bodies unscrubbed; raw
  Clerk user_id on the D1 config plane; in-process (not global) rate limiter (N× under fan-out); no
  internal-auth secret rotation overlap; no captcha/PoW on signup (cost-amplification); cargo PUT
  buffer-then-check DoS; physical-delete GC has no entrypoint (erased bytes not reclaimed).

---

## What's genuinely strong (do not lose in the noise)

- The **revenue-generating core is production-grade**: real R2/D1, fail-closed, constant-time, ~0
  prod-path panics, no fail-open path found across auth/quota/billing/residency.
- **Tenant isolation + PAT crypto are SOTA** (HMAC fast-reject before Argon2id, cryptographically-bound
  prefix, constant-time everywhere) — re-confirmed across multiple passes.
- **Supply chain / build integrity is enterprise-grade** (SLSA-L3, 100% SHA-pinned, hermetic, 0
  advisories, clean licenses).
- **Stripe billing is forgery-safe** (constant-time HMAC, replay window, idempotency keys).
- The team **actively polices its own designed-vs-wired drift** (internal audits + CHANGELOG correct
  the overstatements) — the gaps are disclosed in-repo, not hidden.

---

## Boundaries — requires an external firm (cannot be closed from code)

- **Live penetration test:** the internet-reachability + blast radius of `/_internal/*`, the
  audit-chain forgery against the real store, the CF edge WAF / rate-limit posture, timing side-channels.
- **SOC2:** posture is solo-founder self-attested, pre-audit; needs a Type I/II auditor (Schellman/
  Drata-class).
- **Legal (GDPR/LGPD):** every legal instrument is template / `PENDING_LEGAL_REVIEW` — the DPA, SCCs,
  TIA/LIA, Schrems II transfer assessment, and the BYOK/residency marketing claims need counsel.
- **Operational drills:** a real cold-restore DR drill (will surface the 0064 FK failure), a real
  failover/chaos drill, and a load/soak test (cold-start tail, D1-over-HTTP hops, DO contention) — none
  have been run.
- **Secret provisioning:** confirm dedicated per-consumer internal-auth keys + Clerk Bot-Protection are
  actually set in prod (CF secrets are write-only / unreadable from code).

---

## Remediation roadmap (priority order)

**P0 — before any enterprise conversation, and most before SMB launch:**
1. Wire a live scheduled backup + fix the 0064 restore path. (data-loss)
2. Wire real alerting + ship logs/metrics to a backend. (blind ops)
3. Descope unbuilt enterprise claims (BYOK, immutable audit, multi-region, DSAR, SOC2) from
   marketing/legal — or build them. (misrepresentation)
4. Key/sign the audit chain + wire the verifier + Object-Lock. (CF-6)
5. Confirm the live Stripe endpoint targets the downgrade authority. (revenue leak)

**P1 — before enterprise GA:**
6. Cap OCI/cargo upload buffers; service-binding-only + per-tenant authz on `/_internal/pat/mint`.
7. Wire billing reconciliation; salt the audit email hash; tighten Sentry scrub.
8. Implement GDPR access/portability/rectification + wire consent, or descope.
9. Second committer + secret vault; gate coverage; promote a real-env smoke gate to CI.

**P2 — hygiene:** finish/abandon the Wave-33/35 crate-façade migration (delete ~5 orphan crates +
the dead SHA-256 audit crate); prune stale cargo-deny ignores; reconcile the SLSA version label.

---

*This DD audited the codebase, not the running system or the legal/contractual posture. The core
product is real and well-built; the risk is the gap between what the running data plane does and what
the enterprise/compliance/marketing surface claims it does. Every finding above is reproducible at the
cited file:line; the verdict is honest, not alarmist — several first-pass "Critical" framings were
deliberately corrected to their precise, defensible form during verification.*
