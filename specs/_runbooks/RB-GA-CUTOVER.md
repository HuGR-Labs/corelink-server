---
id: "RB-GA-CUTOVER"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-15"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p0", "ga", "cutover", "deployment", "rollout", "private-preview-to-ga", "greenlight", "rollback-tree", "comms", "wave-19", "r-ga"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-GA-CUTOVER — CoreLink GA Production Cutover Runbook

> **Severity floor:** **P0** — this is the canonical end-to-end orchestrated deployment runbook used **exactly once** to transition CoreLink from PRIVATE_PREVIEW posture to GA. It is the *forward* counterpart of `RB-GA-LAUNCH-ROLLBACK.md` (the reverse posture flip). All wave-18 SEAL gates must have landed on `main` before this runbook can be invoked.
> **Detect → Acknowledge → Engage signers:** N/A (this is a *planned* cutover, not an incident; the trigger is the GA-GATE Go/No-Go meeting approval per `GA-GATE-GO-NOGO-TEMPLATE.md`).
> **Target window:** **T-7d → T+7d** end-to-end; **T-0h cutover sequence** ≤ 4 h wall-clock if all gates green.
> **State-machine impact:** flips `public_status` from `PRIVATE_PREVIEW` → `GA`; opens new-customer signup; activates full pricing tiers; pivots status page to `OPERATIONAL`; starts D+1/D+7/D+30 GA monitoring windows feeding back into `RB-GA-LAUNCH-ROLLBACK.md` triggers.
>
> **Companion docs.** `specs/_compliance/GA-GATE-CRITERIA.md` (59 criteria checklist; all must be `READY` before §0 checklist runs) · `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md` §1-§4 (the meeting whose APPROVED decision authorizes this runbook) · `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` (the *reverse* runbook fired by any §5 trigger here) · `specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md` (war-room logistics) · `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` (paging tiers used by §3 gradual rollout monitoring).
>
> **Regulatory note (NOT a breach event).** This cutover is a *planned* posture change. It does **NOT** involve PII disclosure, data export, or sub-processor change. The GDPR Art. 33 / LGPD ANPD **72 h breach notification clock does NOT start** at any step of this runbook. The breach-notification clock starts *only* if (a) a §5 rollback trigger reveals a concurrent SEV-0 data event, or (b) sub-processor surface area changes during cutover (it must not — DPAs were signed at T-30d per GA-GATE-L02). This distinction is signed-off in §0.7.

---

## 0. Pre-cutover checklist (T-7d)

The following must all be **GREEN** at T-7d ± 2h. Any RED defers cutover by ≥ 7d minimum (per `RB-GA-LAUNCH-ROLLBACK.md` §7 RA-3 — sustained-staging window does not auto-extend).

> **MANDATORY pre-cutover read (D-day morning):**
> Before invoking §0.1 below, the Owner + Signature-2 (Security Lead OR on-call SRE Lead per ADR-0034b dual-hat fallback) MUST read `specs/_audits/2026-05-16-final-cutover-readiness.md` end-to-end (~10 minutes) AND tick every row in the printable companion `specs/_audits/2026-05-16-final-cutover-readiness-checklist.md` (sections A..N).
> The 2-key signature block in the readiness doc §10 is the authoritative authorization for the §0.1..§0.8 checklist below; without it, no §1..§3 step may execute. The readiness doc consolidates wave-18..wave-26 evidence (sprint impls, INV-CRITICAL TLA+, adversarial trend, mutation kill-rate, chaos coverage, endurance, freeze monitor, dress-run verdict, compliance, security posture, 8-item DEFER counter) into a single sign-off-ready surface.

### 0.1 Wave-18 SEAL landed

| # | Item | Verification | Owner | Status |
|---|---|---|---|---|
| 0.1.1 | All wave-18 SEAL PRs merged to `main` at base `cb6360d` or later | `git log --oneline cb6360d..main \| grep -c "wave-18"` ≥ 18 | SRE Lead | ☐ |
| 0.1.2 | `validate_specs.py` + `validate_references.py` green on `main` HEAD | CI `spec-validation` last 24h all green | Engineering Lead | ☐ |
| 0.1.3 | All 21 sprint PRRs in `APPROVED` state (none `CONDITIONALLY_APPROVED`) | `grep -E "^work_status:" specs/_prrs/PRR-S*.md \| grep -vc "APPROVED"` == 0 | Sprint Owners | ☐ |
| 0.1.4 | All wave-18 codex reviews ≥ 8.0/10 | `codex-review-summary` report | Engineering Lead | ☐ |

### 0.2 SEV-0 / SEV-1 runbooks reviewed

| # | Runbook | Last reviewed (≤ 30d) | Owner | Status |
|---|---|---|---|---|
| 0.2.1 | `RB-GA-LAUNCH-ROLLBACK.md` (P0) | tabletop drill within 7d | SRE Lead + Owner | ☐ |
| 0.2.2 | `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` (P0) | re-read by on-call | SRE Lead | ☐ |
| 0.2.3 | `RB-FM-SIGNUP-FAILED.md` (P0) | re-read by on-call | SRE Lead | ☐ |
| 0.2.4 | `RB-ACTIVE-FAILOVER.md` (SEV-0 failover) | re-read by on-call | SRE Lead | ☐ |
| 0.2.5 | `RB-AUDIT-EXPORT-INTEGRITY.md` (SEV-0 audit-chain) | re-read by Security Lead | Security Lead | ☐ |
| 0.2.6 | `RB-COLD-RESTORE-FROM-ZERO.md` (SEV-0 DR) | last full drill ≤ 90d | SRE Lead | ☐ |
| 0.2.7 | `RB-BACKUP-VERIFICATION-FAILURE.md` (SEV-0) | last verification ≤ 7d | SRE Lead | ☐ |
| 0.2.8 | `RB-WEBHOOK-DLQ-REPLAY.md` (SEV-1 Stripe) | re-read by on-call | SRE Lead | ☐ |
| 0.2.9 | `RB-DSR-STATUSPAGE-PUBLISH-FAILED.md` (SEV-1 DSR) | re-read by Privacy Officer | Privacy Officer | ☐ |
| 0.2.10 | `RB-SECRETS-DRIFT.md` (SEV-1 secrets) | last sweep ≤ 7d | Security Lead | ☐ |
| 0.2.11 | `RB-CANONICAL-DRIFT.md` (SEV-1 spec/code drift) | sweep ≤ 24h | Engineering Lead | ☐ |
| 0.2.12 | `RB-PERF-REGRESSION.md` (SEV-1 perf) | re-read by on-call | SRE Lead | ☐ |

### 0.3 On-call rota confirmed

- Primary SRE + secondary SRE + tertiary SRE rota confirmed in PagerDuty for T-0h window ± 24h (52h coverage minimum).
- Security Lead + Privacy Officer + DPO acknowledge availability for §5 escalation.
- CEO + CTO + VPProduct + VPSec acknowledge war-room availability per `RB-LAUNCH-WAR-ROOM-COORDINATION.md` §2.

### 0.4 Customer comms drafted

| # | Audience | Template path | Signed-off by | Status |
|---|---|---|---|---|
| 0.4.1 | Pilot tenants (5) — pre-cutover heads-up | `marketing/launch/COMMS/CUTOVER-T-MINUS-7-PILOT.md` | VPProduct | ☐ |
| 0.4.2 | Pilot tenants — during cutover | `marketing/launch/COMMS/CUTOVER-T-0-PILOT.md` | VPProduct | ☐ |
| 0.4.3 | Pilot tenants — post-cutover OK | `marketing/launch/COMMS/CUTOVER-T-PLUS-1-PILOT.md` | VPProduct | ☐ |
| 0.4.4 | Public — GA launch blog post | `marketing/launch/COMMS/GA-LAUNCH-BLOG.md` | CEO | ☐ |
| 0.4.5 | Internal Slack `#all-hands` | live verbal + Slack write-up | Owner | ☐ |
| 0.4.6 | Investors / advisors | `marketing/launch/COMMS/INVESTOR-GA-NOTE.md` | CEO | ☐ |

### 0.5 Status page banner template ready

- Pre-staged Statuspage.io draft incident "GA Cutover in progress" in DRAFT state.
- Pre-staged green banner "All systems OPERATIONAL — CoreLink is GA" component update queued.
- Verified Statuspage API token + workspace ID per `RB-DSR-STATUSPAGE-PUBLISH-FAILED.md` §3.

### 0.6 Infrastructure baseline snapshot

- D1 backup snapshot taken at T-7d for all 5 regions (per `RB-BACKUP-VERIFICATION.md`).
- R2 bucket inventory baseline CSV (`r2-baseline-T-7d.csv`) committed to `specs/_audits/`.
- Neon shadow lag baseline captured (target p99 ≤ 5min per §4).
- Audit-chain head SHA recorded per region (per `RB-AUDIT-EXPORT-INTEGRITY.md` §2.2).
- **Cutover dependency map cross-check** (wave-27): `specs/_audits/2026-05-16-cutover-dependency-map.md` is the canonical T-N-day DAG governing this §0 checklist. Every critical-path node (N-F-1 framework GA tag → N-C-1 GA-GATE-CRITERIA READY → N-D-1 dress rehearsal → N-S-1 / N-S-1b Statuspage tenant → **N-S-2 STATUSPAGE go-live at T-7d** → N-C-2 §0 checklist GREEN → N-X-1 §1 freeze → N-X-2 §2 staging → N-X-3 §3 cutover) MUST have its success-gate row ticked in the map before this §0.6 baseline snapshot is signed off. Any RED critical-path node defers cutover per `RB-GA-LAUNCH-ROLLBACK.md` §7 RA-3 ≥ 7 days minimum.
- **STATUSPAGE T-7d gate (explicit)**: at T-7d ± 2h the `RB-STATUSPAGE-INIT.md` §4 gate MUST be GREEN — either (Option A) `curl -sI https://status.corelink.humangr.com` returns HTTP 200 with 8 components in `summary.json`, OR (Option B) the operator-chosen domain resolves to the tenant + `STATUSPAGE_URL` env var is set in the docs deploy pipeline + the deploy-time substitution branch is merged to the release tag. RED at T-7d on this row alone defers cutover ≥ 7 days; the manual static-HTML fallback is NOT acceptable because the trust corpus cites the live JSON-API + RSS + email-subscribe channels.

### 0.7 Regulatory sign-off (no breach event)

- DPO + Legal Counsel acknowledge in writing: **"GA cutover is a posture change that does NOT involve PII disclosure, data export, sub-processor change, or any event triggering GDPR Art. 33 / LGPD ANPD breach notification."** Filed at `specs/_audits/2026-MM-DD-ga-cutover-no-breach-attestation.md`.
- Re-confirm DPAs at T-30d remain signed (no new sub-processor added at cutover).

### 0.8 Dry-run history

The §3 sequence is exercised against in-process fakes via `scripts/ga-cutover-dryrun.sh` ahead of the §9 mandated production dress-rehearsal. Each dry-run emits a sealed audit doc under `specs/_audits/YYYY-MM-DD-ga-cutover-dryrun.md` and a JSON evidence bundle under `reports/ga-cutover-dryrun-YYYY-MM-DD.json`. The dry-run is a *necessary but not sufficient* precondition for the §9 dress-rehearsal: it asserts the runbook orchestration is internally consistent and that the §4 greenlight recording-rule thresholds evaluate cleanly when fed canonical values; it does NOT substitute for the §9 production dress-rehearsal against a real staging environment.

| Run date | Verdict | Greenlights (G1..G6) | §3 steps PASS | Triggers fired | GA-readiness | Audit doc |
|---|---|---|---|---|---|---|
| 2026-05-16 | GREEN | 6 / 6 | 11 / 11 | 0 / 6 | 8.9 / 10 | `specs/_audits/2026-05-16-ga-cutover-dryrun.md` |

---

## 1. T-72h freeze + validation

### 1.1 Schema freeze

- **NO new migrations** may merge from T-72h until T+24h. Enforced by branch protection on `migrations/d1/**` + `migrations/neon/**` + `specs/02_governance/data_model.md`.
- Engineering Lead announces freeze in `#engineering` Slack with countdown bot.
- Any emergency hotfix to schema during freeze requires CTO + Owner 2-key approval AND defers cutover by ≥ 48h.

### 1.2 Migration additivity validation

Run `python3 scripts/check_migrations_additive.py` against the full migration ledger; output must be GREEN across:

- 5 D1 regions: `us-east`, `us-west`, `eu-west`, `ap-southeast`, `sa-east`
- Neon shadow (`shadow.neon.tech` analytics replica)
- R2 buckets (audit + ac + main blob × 5 regions = 15 buckets)

**Expected exit code:** `0`. Any non-zero exit defers cutover by ≥ 24h and triggers `RB-CANONICAL-DRIFT.md`.

### 1.3 Final secrets matrix audit

Run secrets sweep per `RB-SECRETS-DRIFT.md` §3:

- Verify all `wrangler secret list` outputs match canonical secrets matrix (`specs/_compliance/secrets-matrix.md`).
- Verify zero plaintext secrets in any committed file (`gitleaks detect --redact`).
- Verify BYOK orchestrator key matrix covers all active enterprise BYOK tenants (per `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` §5 BYOK contract).
- Verify Stripe live-mode webhook signing secret rotated within last 30d.
- Verify Clerk JWT issuer key rotated within last 90d.

---

## 2. T-24h staging + endurance

### 2.1 Deploy to staging

```bash
# 2.1.1 Tag the cutover release candidate
git tag -s v1.0.0-ga.rc1 -m "GA cutover RC1 — T-24h candidate"
git push origin v1.0.0-ga.rc1

# 2.1.2 Deploy CF Worker to staging (full rollout, no gradual)
pnpm --filter @corelink/worker run deploy:staging

# 2.1.3 Verify staging health
curl https://staging.corelink.humangr.com/__health | jq '.status'
# expected: "OPERATIONAL"
```

### 2.2 Full proptest suite (10k iter)

```bash
# 2.2.1 Run cargo proptest with elevated iteration count
PROPTEST_CASES=10000 cargo test --workspace --release -- --test-threads=4 proptest_

# 2.2.2 Capture summary
cargo test --workspace --release -- --test-threads=4 proptest_ 2>&1 | tee specs/_audits/2026-MM-DD-ga-cutover-proptest.log
```

**Pass criterion:** zero failed proptests across all 10k iterations. Any failure defers cutover by ≥ 48h and triggers root-cause analysis per `RB-CANONICAL-DRIFT.md`.

### 2.3 24h endurance load test

Trigger per `RB-ENDURANCE-24H-DRILL.md` §3:

- Drives sustained 10k req/s mixed-workload against staging for full 24h.
- P99 latency must stay ≤ SLO (per `slo_catalog.md` §4) for entire window.
- Zero error-budget burn alerts firing at end of window.

### 2.4 Customer announcement send

- T-24h: send `CUTOVER-T-MINUS-7-PILOT.md` (already pre-sent T-7d) follow-up to all 5 pilot tenants with 24h notice + cutover window time-of-day.
- Internal Slack `#all-hands` cutover schedule announcement.

---

## 3. T-0h cutover sequence

Execute the following **11 steps in strict order**. Each step has its own verification gate; do NOT proceed to step N+1 until step N is GREEN. Target wall-clock: ≤ 4h end-to-end if all gates green; ≤ 8h with 15min holds between gradual rollout stages.

### 3.1 Step 1 — Provision / verify R2 buckets

**Buckets:** 5 regions × (audit + ac + main blob + 2 staging) = 25 buckets total.

```bash
# 3.1.1 Verify all 25 buckets exist + permissions correct
for region in us-east us-west eu-west ap-southeast sa-east; do
  for purpose in audit ac main staging-1 staging-2; do
    wrangler r2 bucket list | grep "corelink-${region}-${purpose}" || \
      wrangler r2 bucket create "corelink-${region}-${purpose}" --location "$region"
  done
done

# 3.1.2 Verify CORS + lifecycle policies applied
./scripts/r2-cors-verify.sh
./scripts/r2-lifecycle-verify.sh
```

**Verification gate:** `./scripts/r2-cors-verify.sh` exit `0` AND all 25 buckets show in `wrangler r2 bucket list`.

### 3.2 Step 2 — Apply Neon shadow migrations (idempotent)

```bash
# 3.2.1 Apply forward-only additive migrations against Neon shadow
psql "$NEON_SHADOW_URL" -f migrations/neon/_apply-all-additive.sql

# 3.2.2 Verify idempotency — re-apply must be a no-op
psql "$NEON_SHADOW_URL" -f migrations/neon/_apply-all-additive.sql 2>&1 | \
  grep -cE "ERROR|FATAL" | xargs -I{} test {} -eq 0
```

**Verification gate:** second apply exits with zero ERROR/FATAL lines (idempotent confirmed). Triggers `RB-CANONICAL-DRIFT.md` if not idempotent.

### 3.3 Step 3 — Deploy CF Worker (gradual rollout)

Use Cloudflare Workers gradual rollout (`wrangler deploy --gradual=...`). Hold 15min at each stage; promote only if no SEV-0/1 alerts fire during hold.

| Stage | Traffic % | Hold duration | Promote criterion | Rollback criterion |
|---|---|---|---|---|
| 3.3.1 | 1 % | 15 min | Zero SEV-0/1 in 15min window; p99 latency within SLO; audit-chain integrity green | Any SEV-0 OR ≥ 1 SEV-1 OR p99 breach → `RB-GA-LAUNCH-ROLLBACK.md` §5 |
| 3.3.2 | 10 % | 15 min | Same as 3.3.1 + dedup ratio within ±5% of staging baseline | Same as 3.3.1 |
| 3.3.3 | 50 % | 15 min | Same as 3.3.2 + Neon shadow lag p99 ≤ 5min | Same as 3.3.1 |
| 3.3.4 | 100 % | 30 min observation | Same as 3.3.3 sustained 30min | Same as 3.3.1 |

```bash
# 3.3.1 1% gradual rollout
wrangler deploy --gradual=1 --env=production

# 3.3.2 10% promote
wrangler deploy --gradual=10 --env=production

# 3.3.3 50% promote
wrangler deploy --gradual=50 --env=production

# 3.3.4 100% promote
wrangler deploy --gradual=100 --env=production
```

**Verification gate:** at each stage, the recording rules in `dashboards/alerts/dash-ga-greenlight.yml` must show GREEN for the full hold duration before promote.

### 3.4 Step 4 — Enable BYOK orchestrator

Flip BYOK provider per tenant from `singleton-fake` (preview-mode stub) to the real provider per the active BYOK matrix.

```bash
# 3.4.1 For each active BYOK tenant, flip provider
for tenant in $(./scripts/list-active-byok-tenants.sh); do
  wrangler kv key put --namespace-id "$BYOK_KV_PROD" "byok:$tenant:provider" \
    "$(./scripts/lookup-byok-provider.sh $tenant)"
done

# 3.4.2 Verify per-tenant smoke test
./scripts/byok-smoke-test.sh --all-active-tenants
```

**Verification gate:** zero `BYOKProviderUnavailable` errors in last 5min after flip. Triggers `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` §5 if any lighthouse BYOK tenant fails smoke.

### 3.5 Step 5 — Enable Stripe webhook live mode + DLQ consumers

```bash
# 3.5.1 Update Stripe webhook endpoint to live-mode URL
stripe webhook_endpoints update "$STRIPE_WEBHOOK_ID" --url=https://api.corelink.humangr.com/webhooks/stripe

# 3.5.2 Verify webhook signing secret matches deployed Worker
wrangler secret list --env=production | grep STRIPE_WEBHOOK_SECRET

# 3.5.3 Start DLQ consumers
wrangler tail --env=production --status=ok | grep -c "dlq-consumer-started"
```

**Verification gate:** synthetic Stripe webhook (test event) reaches Worker AND is signed-verified AND lands in main pipeline (not DLQ). Triggers `RB-WEBHOOK-DLQ-REPLAY.md` if any test event ends in DLQ.

### 3.6 Step 6 — Enable Clerk JWT issuer + WebAuthn admin

```bash
# 3.6.1 Flip Clerk environment from preview to production
wrangler kv key put --namespace-id "$AUTH_KV_PROD" "clerk:env" "production"

# 3.6.2 Verify JWT issuer URL responds
curl https://clerk.corelink.humangr.com/.well-known/jwks.json | jq '.keys | length'

# 3.6.3 Enable WebAuthn admin endpoint
wrangler kv key put --namespace-id "$AUTH_KV_PROD" "webauthn:admin:enabled" "true"
```

**Verification gate:** smoke login via Clerk production tenant succeeds; WebAuthn admin enroll smoke succeeds.

### 3.7 Step 7 — Enable audit-chain Logpush + Neon shadow sync

```bash
# 3.7.1 Enable Cloudflare Logpush to R2 audit bucket
wrangler logpush create --destination=r2://corelink-us-east-audit/logs/ \
  --dataset=workers_trace_events --filter='outcome=="ok"'

# 3.7.2 Enable Neon shadow sync cron
wrangler cron trigger --env=production --cron-name=audit-chain-neon-sync
```

**Verification gate:** within 5min, audit-chain head SHA appears in R2 audit bucket AND in Neon shadow audit_chain table. Triggers `RB-AUDIT-EXPORT-INTEGRITY.md` if either is missing.

### 3.8 Step 8 — Enable DSR Statuspage cron

```bash
# 3.8.1 Enable DSR cron worker
wrangler cron trigger --env=production --cron-name=dsr-statuspage-publish
```

**Verification gate:** first cron run completes within 5min, status `dsr_run_status=success`. Triggers `RB-DSR-STATUSPAGE-PUBLISH-FAILED.md` if any failure.

### 3.9 Step 9 — Update DNS (final TTL 60s cutover)

```bash
# 3.9.1 Lower DNS TTL to 60s 24h ahead (already done at T-24h per §2.1)
# 3.9.2 At T-0h: update apex + api + clerk + statuspage CNAMEs
./scripts/dns-cutover-apply.sh --confirm

# 3.9.3 Verify propagation
dig +short api.corelink.humangr.com | head -3
```

**Verification gate:** all 4 CNAMEs resolve to GA endpoints from ≥ 3 geographically distinct resolvers (Google 8.8.8.8, Cloudflare 1.1.1.1, OpenDNS 208.67.222.222).

### 3.10 Step 10 — Enable customer-facing rate limits

```bash
# 3.10.1 Flip rate limit posture from preview-permissive to GA-canonical
wrangler kv key put --namespace-id "$RATE_LIMIT_KV_PROD" "rate_limit:posture" "ga"
```

**Verification gate:** synthetic load test confirms rate limits trip at canonical tier ceilings (per `slo_catalog.md` §4 + `dashboards/alerts/dash-rate-alerts.yml`).

### 3.11 Step 11 — Status page transition to OPERATIONAL

```bash
# 3.11.1 Update Statuspage components to OPERATIONAL
./scripts/statuspage-set-operational.sh

# 3.11.2 Close pre-staged "GA Cutover in progress" incident as RESOLVED
./scripts/statuspage-close-incident.sh "GA Cutover in progress"

# 3.11.3 Publish GA launch banner
./scripts/statuspage-publish-banner.sh "All systems OPERATIONAL — CoreLink is GA"

# 3.11.4 Flip public_status feature flag
wrangler kv key put --namespace-id "$PROD_FF_KV" "public_status" "GA"
wrangler kv key put --namespace-id "$PROD_FF_KV" "new_signup_open" "true"
```

**Verification gate:** `curl https://api.corelink.humangr.com/__health/feature-flags | jq '.public_status'` returns `"GA"`. New signup form is reachable. Status page shows green.

---

## 4. Greenlight criteria

Each gate below must be **independently GREEN** before promoting to the next gradual rollout stage in §3.3 AND before declaring §3.11 cutover complete. All gates are wired into `dashboards/alerts/dash-ga-greenlight.yml` as recording rules so a single composite query indicates GO/NO-GO posture.

| # | Gate | Threshold | Source / Recording rule | Required duration |
|---|---|---|---|---|
| G1 | **P99 latency ≤ SLO across 5 regions** | per `slo_catalog.md` §4 (region-specific) | `slo:greenlight:p99_latency_regions_ok` | sustained 30 min |
| G2 | **Audit-chain integrity verifier green** | `corelink_audit_chain_integrity_violation_total` == 0 | `slo:greenlight:audit_chain_integrity` | sustained 24h prior to T-0h AND continuously from §3.7 onward |
| G3 | **Zero SEV-0/SEV-1 incidents in 72h prior** | PagerDuty incident count == 0 for SEV-0/1 | `slo:greenlight:sev01_count_72h` | sustained 72h prior to T-0h |
| G4 | **Customer success ack ≥ 5 pilot tenants** | per-tenant attestation form signed | `slo:greenlight:pilot_attestations_count` (manual; updated by VPProduct in `specs/_audits/`) | snapshot at T-24h ± 6h |
| G5 | **Neon shadow lag p99 ≤ 5 min** | `neon_shadow_replication_lag_seconds_p99 <= 300` | `slo:greenlight:neon_shadow_lag_p99` | sustained 30 min |
| G6 | **DSR cron last 24h success rate 100 %** | `corelink_dsr_cron_runs_failed_total[24h] == 0` AND `corelink_dsr_cron_runs_total[24h] > 0` | `slo:greenlight:dsr_cron_24h_success` | sustained 24h prior |

**Composite greenlight rule** (in `dash-ga-greenlight.yml`): `slo:greenlight:composite_ok = G1 AND G2 AND G3 AND G4 AND G5 AND G6`. This must be `1` for ≥ 30 min before declaring cutover complete at §3.11.

---

## 5. Rollback decision tree

### 5.1 Trigger conditions

ANY of the following triggers an immediate rollback decision per §5.3:

| # | Trigger | Source | Auto-page? |
|---|---|---|---|
| RB-T1 | **Any SEV-0 incident** during cutover window or in 4h post-§3.11 | PagerDuty severity = SEV-0 | YES |
| RB-T2 | **≥ 2 SEV-1 incidents within 30 min** during cutover window | PagerDuty incident burst | YES |
| RB-T3 | **P99 latency SLO breach > 15 min** at any §3.3 gradual stage or post-§3.11 | `slo:cas_get_latency_violation_rate:1h` ≥ 14.4× target × 15min | YES |
| RB-T4 | **Audit-chain integrity break detection** | `corelink_audit_chain_integrity_violation_total > 0` (INV-AUDIT-APPEND-ONLY breach) | YES |
| RB-T5 | **Any G1–G6 greenlight criterion flips RED** for > 5 min during cutover window | `slo:greenlight:composite_ok == 0` for 5min | YES |
| RB-T6 | **Lighthouse customer signals withdrawal** during cutover | `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` §3 state-machine transition | YES |

### 5.2 Decision-maker (2-key auth)

Rollback decision requires **2-key auth**: **on-call SRE Lead** + **Product Lead (VPProduct or designate)**. Either signer can BLOCK rollback (and escalate to CTO + Owner if they disagree). Either signer can INITIATE rollback (the other must concur within 5 min OR the initiator escalates to CTO + Owner for a unilateral go).

Decision is logged in incident.io within 5 min of trigger detection.

### 5.3 Rollback sequence

Target wall-clock: ≤ 1h from decision time. **Rollback is rollforward-safe** — no destructive schema operations are performed. All migrations applied in §3 are additive-only; rolling back the CF Worker to the previous tag does NOT require migration revert.

| # | Action | Owner | Target | Verification |
|---|---|---|---|---|
| RB-S1 | **DNS revert** — flip apex + api + clerk + statuspage CNAMEs back to PRIVATE_PREVIEW endpoints | SRE Lead | ≤ 5 min from decision | `dig +short api.corelink.humangr.com` returns preview endpoint from 3 resolvers |
| RB-S2 | **CF Worker pin to previous version tag** — revert to `v1.0.0-pre-ga.last-known-good` per release ledger | Engineering Lead | ≤ 10 min from decision | `wrangler deployments list` shows pinned version; `curl /__health` returns previous version SHA |
| RB-S3 | **Status page banner** — flip to "Rollback in progress — investigating" incident; do NOT attribute blame publicly | SRE Lead | ≤ 15 min from decision | Statuspage incident `investigating` lifecycle visible |
| RB-S4 | **Customer comms** — send `CUTOVER-ROLLBACK-PILOT.md` to all 5 pilot tenants (reassurance, no service interruption since rollback is rollforward-safe); send `CUTOVER-ROLLBACK-PUBLIC.md` to status page subscribers | VPProduct + SRE Lead | ≤ 30 min from decision | Postmark delivery confirms; Slack `#incident-active` thread updated |
| RB-S5 | **Incident retro within 24h** — full postmortem per `RB-POSTMORTEM-PROCESS.md`; sealed audit doc at `specs/_audits/2026-MM-DD-ga-cutover-rollback-N.md`; trigger re-attestation cycle per `RB-GA-LAUNCH-ROLLBACK.md` §7 | SRE Lead + Engineering Lead + Owner | ≤ 24h from rollback complete | Postmortem doc sealed; 4-signer attestation |

### 5.4 Idempotency + rollforward safety

- §3 cutover steps 1, 2, 7 (R2 buckets, Neon migrations, audit-chain Logpush) are **forward-only additive** and remain in place after rollback (they are no-ops in preview posture).
- §3 cutover steps 3–6, 8–11 (Worker, BYOK, Stripe live, Clerk, DSR cron, DNS, rate limits, status page) are **feature-flag or DNS gated** and revert cleanly per §5.3.
- **No data migration is performed at cutover.** Customer data was already provisioned in preview posture; cutover only changes routing + posture.

### 5.5 Hand-off to `RB-GA-LAUNCH-ROLLBACK.md`

If rollback occurs **post-§3.11** (i.e., GA posture was declared and announced publicly), the rollback hands off to `RB-GA-LAUNCH-ROLLBACK.md` D+1 trigger T1-1 / T1-2 / T1-3 path (full re-attestation cycle). This runbook covers only the *in-window* rollback (T-0h to T-0h+4h); after the window closes, the canonical rollback runbook takes over.

---

## 6. Post-cutover

> **Canonical expansion:** `specs/_runbooks/RB-POST-GA-CONTINUITY.md` is the **canonical 30-day post-GA continuity playbook** that extends this section through T+30d (greenlight re-verification, SLO baseline capture, weekly compliance digest, pilot-to-GA conversion, freeze-thaw evaluation, quarterly framework prep). The §6.1 / §6.2 / §6.3 stubs below remain the cutover-time anchor; the continuity playbook §1 / §2 / §3 / §4 consume them.

### 6.1 T+24h

| # | Action | Owner | Evidence |
|---|---|---|---|
| 6.1.1 | **Metric baseline capture** — snapshot all SLI dashboards + alert states + error budget burn rates | SRE Lead | `specs/_audits/2026-MM-DD-ga-cutover-t-plus-24h-baseline.md` |
| 6.1.2 | **Customer health check** — outreach to all 5 pilot tenants; collect health signals + any issues | VPProduct | health-check report appended to baseline doc |
| 6.1.3 | **Audit-chain spot verifier** — run `RB-AUDIT-EXPORT-VERIFY-FAILED.md` §2 verifier; confirm integrity | Security Lead | verifier output logged |
| 6.1.4 | **DSR cron review** — confirm all 24 hourly runs succeeded since §3.8 | Privacy Officer | DSR cron dashboard screenshot |

### 6.2 T+72h

| # | Action | Owner | Evidence |
|---|---|---|---|
| 6.2.1 | **SLO breach review** — confirm zero sustained SLO breach in 72h window | SRE Lead | SLO dashboard screenshot |
| 6.2.2 | **Customer feedback aggregation** — synthesize first-72h customer signals | VPProduct | feedback summary doc |
| 6.2.3 | **D+1 trigger window closes** — confirm no `RB-GA-LAUNCH-ROLLBACK.md` D+1 trigger fired | SRE Lead + Owner | trigger sweep doc |

### 6.3 T+7d

| # | Action | Owner | Evidence |
|---|---|---|---|
| 6.3.1 | **Audit-chain weekly summary** — formal weekly attestation per `RB-COMPLIANCE-WEEKLY-REVIEW.md` | Security Lead | weekly attestation sealed |
| 6.3.2 | **SLO board freeze** — lock baseline error budgets for next 30 days | SRE Lead | SLO board snapshot |
| 6.3.3 | **D+7 trigger window closes** — confirm no `RB-GA-LAUNCH-ROLLBACK.md` D+7 trigger fired | SRE Lead + Owner | trigger sweep doc |
| 6.3.4 | **Cutover retro meeting** — 60min retro with all signers + on-call SREs; lessons learned filed at `specs/_audits/2026-MM-DD-ga-cutover-retro.md` | Owner | retro doc sealed |

---

## 7. Comms templates

All templates live under `marketing/launch/COMMS/`. They MUST be pre-staged + signed-off per §0.4 before T-7d.

### 7.1 Customer email — pre-cutover (T-7d, T-24h)

| Template | Audience | Trigger | Tone |
|---|---|---|---|
| `CUTOVER-T-MINUS-7-PILOT.md` | 5 pilot tenants | T-7d | Factual, scope of change, no action required from customer |
| `CUTOVER-T-MINUS-24-PILOT.md` | 5 pilot tenants | T-24h | Reminder, exact cutover window time-of-day, support contact |

### 7.2 Customer email — during cutover (T-0h)

| Template | Audience | Trigger | Tone |
|---|---|---|---|
| `CUTOVER-T-0-PILOT.md` | 5 pilot tenants | At §3.3.1 (1% gradual rollout start) | Heads-up, no expected service interruption |

### 7.3 Customer email — post-cutover (T+1d)

| Template | Audience | Trigger | Tone |
|---|---|---|---|
| `CUTOVER-T-PLUS-1-PILOT.md` | 5 pilot tenants | T+24h after §3.11 complete | Welcome to GA, here's what changed, here's how to use new tier features |
| `GA-LAUNCH-BLOG.md` | Public | T+24h after §3.11 complete | Public announcement, owns the work, no over-promising |

### 7.4 Status page incident draft

| Template | Trigger | Lifecycle |
|---|---|---|
| `STATUS-PAGE-CUTOVER-IN-PROGRESS.md` | §3.1 start | `investigating → identified → monitoring → resolved` over §3 sequence |
| `STATUS-PAGE-GA-OPERATIONAL.md` | §3.11 complete | Green banner; sticky 7d |
| `STATUS-PAGE-CUTOVER-ROLLBACK.md` | §5.3 RB-S3 | `investigating → identified → resolved` per rollback sequence |

### 7.5 Internal Slack channels

| Channel | Purpose | Owner |
|---|---|---|
| `#ga-cutover-warroom` | Live war-room during T-0h..T+4h | SRE Lead |
| `#ga-cutover-comms` | Customer comms coordination | VPProduct |
| `#incident-active` | Standard incident channel (used if §5 trigger fires) | On-call SRE |
| `#all-hands` | Internal announcements pre + post | Owner |

---

## 8. Sign-off (2-key)

This runbook authorizes execution of the §3 cutover sequence only after **both** of the following sign-offs are filed:

| Signer | Role | Verification | Signature method |
|---|---|---|---|
| **Orchestrator** (Owner / CEO) | Final GA Go decision authority | GA-GATE Go/No-Go meeting APPROVED minutes filed | DocuSign on go/no-go doc + sealed audit |
| **On-call SRE Lead** | Operational readiness authority | §0 checklist GREEN; §1 + §2 gates GREEN | DocuSign on `specs/_audits/2026-MM-DD-ga-cutover-execution-attestation.md` |

**Both signatures must be filed within the 24h window before T-0h.** Either signer can withdraw signature at any time before §3.11 completes — withdrawal automatically defers cutover by ≥ 7 days.

### 8.1 Execution attestation template

The `2026-MM-DD-ga-cutover-execution-attestation.md` (`type: audit`) doc must capture, signed by both:

- T-0h timestamp + actual start time of §3.1
- Per-step verification gate status (11 rows + greenlight composite)
- Any §5 trigger fires + decision outcomes
- §3.11 completion timestamp + final greenlight composite snapshot
- Cross-reference to T+24h baseline (§6.1.1)

---

## 9. Verification + drill cadence

- **Pre-cutover dress rehearsal** at T-14d: full §3 sequence executed against staging (no production impact). Sealed audit doc at `specs/_audits/2026-MM-DD-ga-cutover-dress-rehearsal.md`.
- **§5 rollback decision-tree tabletop** at T-14d ± 3d: on-call SRE + Product Lead walk through each RB-T1..RB-T6 trigger; document at `specs/_audits/2026-MM-DD-ga-cutover-rollback-tabletop.md`.
- **§7 comms templates dry-run** at T-14d: render + review (do NOT send); sealed audit doc.
- **Greenlight dashboard validation** at T-7d: confirm all 6 recording rules in `dashboards/alerts/dash-ga-greenlight.yml` return real values from staging; document at `specs/_audits/2026-MM-DD-ga-cutover-greenlight-validation.md`.

---

## 10. References

- `specs/_compliance/GA-GATE-CRITERIA.md` — 59 criteria checklist that must be READY before §0.
- `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md` — meeting whose APPROVED decision authorizes §3 execution.
- `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` — sister *reverse* runbook; hand-off target if rollback occurs post-§3.11.
- `specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md` — war-room logistics during T-0h.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — paging tiers used by §3.3 gradual rollout + §5 triggers.
- `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` — lighthouse-specific incident response (consumed in §3.4 BYOK + §5 RB-T6).
- `specs/_runbooks/RB-FM-SIGNUP-FAILED.md` — signup failure mode (consumed if §3.6 + §3.11 signup-open fails).
- `specs/_runbooks/RB-ACTIVE-FAILOVER.md` — failover used if §3.3 gradual rollout hits regional fault.
- `specs/_runbooks/RB-AUDIT-EXPORT-INTEGRITY.md` — audit-chain integrity verifier consumed in §3.7 + §4 G2.
- `specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md` — audit-chain spot-verifier consumed in §6.1.3.
- `specs/_runbooks/RB-BACKUP-VERIFICATION.md` — D1 snapshot baseline at T-7d per §0.6.
- `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md` — DR runbook re-read at §0.2.6.
- `specs/_runbooks/RB-CANONICAL-DRIFT.md` — drift sweep consumed in §1.2 + §2.2.
- `specs/_runbooks/RB-SECRETS-DRIFT.md` — secrets sweep consumed in §1.3.
- `specs/_runbooks/RB-WEBHOOK-DLQ-REPLAY.md` — Stripe DLQ consumed in §3.5.
- `specs/_runbooks/RB-DSR-STATUSPAGE-PUBLISH-FAILED.md` — DSR Statuspage consumed in §0.5 + §3.8 + §6.1.4.
- `specs/_runbooks/STATUSPAGE-INIT.md` — operator provisioning playbook for `status.corelink.humangr.com` (Option A CNAME / Option B env-var), gate at §4 T-7d, timing in §2.5 + §3.6 (wave-27).
- `specs/_audits/2026-05-16-cutover-dependency-map.md` — canonical T-N-day dependency DAG governing this §0 checklist; critical-path + slack metrics (wave-27).
- `specs/_audits/2026-05-16-statuspage-init-dressrun.md` — wave-25 STATUSPAGE-INIT mechanism dress-run (covers N-S-1 / N-S-1b mechanism regression risk).
- `specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md` — weekly attestation at T+7d (§6.3.1).
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — postmortem template kicked off at §5.3 RB-S5.
- `specs/_runbooks/RB-PERF-REGRESSION.md` — perf regression handling if §4 G1 trips.
- `specs/_runbooks/RB-ENDURANCE-24H-DRILL.md` — endurance test executed at §2.3.
- `dashboards/alerts/dash-ga-greenlight.yml` — composite greenlight recording rules referenced by §3.3 + §4.
- `dashboards/alerts/dash-slo-multi-burn.yml` — SLO multi-burn-rate alerts referenced by §4 G1.
- `dashboards/alerts/dash-rate-alerts.yml` — rate-limit alerts referenced by §3.10.
- `dashboards/alerts/dash-audit-export-alerts.yml` — audit-chain alerts referenced by §4 G2.
- `dashboards/alerts/dash-dsr-statuspage-alerts.yml` — DSR cron alerts referenced by §4 G6.
- `scripts/check_migrations_additive.py` — additivity validator at §1.2.
- `marketing/launch/COMMS/` — comms templates per §7.
- `ROADMAP-TO-GA.md` §8 — Wave R-GA cutover context.
- ADR-0034 — solo-tier emergency provisions (referenced by §5.2 unilateral-go path).

---

## 11. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-05-16 | Gustavo (via Claude Opus wave-27 background worker, worktree `wt/r-prep-statuspage-timing-cutover-map`) | §0.6 baseline snapshot now cross-references the wave-27 cutover dependency map (`specs/_audits/2026-05-16-cutover-dependency-map.md`); §0.6 adds explicit T-7d STATUSPAGE gate row pulling forward the Option A / Option B verification from `RB-STATUSPAGE-INIT.md` §4. §10 references gain three rows (STATUSPAGE-INIT runbook, dependency map, STATUSPAGE-INIT dress-run audit). No behavioural change to §1–§9 — wave-27 is a doc/timing-clarity tightening only. |
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus wave-19 builder, worktree `wt/r-prep-ga-cutover-runbook`) | Initial canonical end-to-end GA cutover runbook — §0 T-7d pre-cutover checklist (7 sub-sections; 12 SEV runbook re-reads); §1 T-72h schema freeze + 5-region D1 + Neon shadow + R2 additivity validation + secrets matrix audit; §2 T-24h staging + 10k-iter proptest + 24h endurance load test; §3 T-0h **11-step orchestrated cutover sequence** (R2 buckets → Neon migrations → CF Worker gradual 1%→10%→50%→100% with 15min holds → BYOK → Stripe live → Clerk → audit-chain Logpush → DSR cron → DNS → rate limits → status page); §4 **6 independent greenlight criteria** (P99 latency / audit-chain / SEV-0/1 zero-72h / customer ack ≥ 5 / Neon shadow lag / DSR cron 100%) with composite recording rule; §5 **6 rollback triggers** + 2-key (SRE Lead + Product Lead) decision + 5-step rollforward-safe rollback sequence (DNS revert → Worker pin → status page → comms → retro); §6 post-cutover T+24h / T+72h / T+7d cadence; §7 **9 comms templates** (3 customer email pre/during/post + 3 status page + 3 Slack channels); §8 2-key sign-off + execution attestation; §9 dress rehearsal + tabletop + comms dry-run cadence at T-14d. Regulatory: explicitly documents cutover is **NOT a breach event** (no PII disclosure, no sub-processor change, no GDPR Art. 33 / LGPD ANPD 72h clock). |

---

**Status:** ACTIVE. Dress rehearsal mandatory ≤ T-14d. Both 2-key signatures required ≤ T-24h before §3 execution per §8.
