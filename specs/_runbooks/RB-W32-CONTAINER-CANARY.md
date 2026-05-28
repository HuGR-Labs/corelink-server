---
id: "RB-W32-CONTAINER-CANARY"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "wave-32", "phase-e", "canary", "container", "deploy", "gradual-rollout", "rollback", "wrangler"]
references:
  - "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md"
  - "specs/_audits/2026-05-27-w32-phaseE-canary-prep-seal.md"
  - "dashboards/grafana/DASH-SLO-API.json"
  - "dashboards/grafana/DASH-SLO-AUDIT.json"
  - "scripts/e-day-container-canary-promote.sh"
---

# RB-W32-CONTAINER-CANARY — Wave 32 Phase E Container Canary Deploy Runbook

> **Purpose.** Walk the Owner through the 5% → 25% → 100% gradual canary rollout of the
> CoreLink container deploy (Wave 32 Phase E). Each stage has a 10-minute observation
> window with explicit PASS/FAIL criteria before the next promotion.
>
> **Scope:** Phase E only (container canary). Phase D (secrets/migrations) must be
> complete and Phase F+G (DNS, smoke) must be ready to execute after 100% promote.
>
> **Automation counterpart:** `scripts/e-day-container-canary-promote.sh` enforces the
> 10-minute wait and Owner ack between stages. This runbook is the manual companion
> for operators who prefer human pacing or who need to deviate from the happy path.
>
> **Workers Paid plan requirement.** Gradual rollouts (canary percentage routing) require
> the Cloudflare Workers Paid plan (≥$5/mo). Verify billing before executing §1.

---

## §0 Pre-deploy checklist

All items must be **GREEN** before invoking §1. Any RED blocks Phase E execution.

### §0.1 Phase D completion gate

| # | Check | Command / Verification | Status |
|---|---|---|---|
| 0.1.1 | D1 migrations applied (52 migrations) | `bash scripts/apply-d1-migrations-prod.sh --verify` → exit 0 | ☐ |
| 0.1.2 | Secrets deployed (55 wrangler secrets) | `bash scripts/verify-secrets-deployed.sh` → 0 missing | ☐ |
| 0.1.3 | Phase D SEAL committed to main | `git log --oneline \| grep "w32-phaseD"` | ☐ |

### §0.2 Phase E artefacts ready

| # | Check | Command / Verification | Status |
|---|---|---|---|
| 0.2.1 | Container image built locally | `docker images corelink-server:prod --format "{{.ID}}"` → non-empty | ☐ |
| 0.2.2 | Image pushed to CF Containers registry | `bash scripts/push-container-prod.sh --verify` → exit 0 | ☐ |
| 0.2.3 | Worker build artefact current | `ls -la worker/dist/index.js` → mtime within last 24h | ☐ |

### §0.3 Phase F+G readiness (post-E dependencies)

| # | Check | Status |
|---|---|---|
| 0.3.1 | DNS plan reviewed (`scripts/dns-prod-plan.sh` output saved) | ☐ |
| 0.3.2 | Smoke runbook read (`scripts/smoke-prod-corelink.sh --dry-run` passes) | ☐ |
| 0.3.3 | Rollback script tested in dry-run (`scripts/rollback-prod-corelink.sh --dry-run`) | ☐ |

### §0.4 Secrets bound

| # | Check | Status |
|---|---|---|
| 0.4.1 | `CF_API_TOKEN` set in shell (not in script) | `[[ -n "$CF_API_TOKEN" ]] && echo OK` | ☐ |
| 0.4.2 | `CF_ACCOUNT_ID` set in shell | `[[ -n "$CF_ACCOUNT_ID" ]] && echo OK` | ☐ |
| 0.4.3 | `wrangler whoami` returns the correct account | Verify account ID matches CF dashboard | ☐ |

### §0.5 Observability surfaces primed

| # | Surface | URL / Path | Status |
|---|---|---|---|
| 0.5.1 | Grafana API SLO dashboard | `dashboards/grafana/DASH-SLO-API.json` — import or verify live | ☐ |
| 0.5.2 | Grafana Audit SLO dashboard | `dashboards/grafana/DASH-SLO-AUDIT.json` — import or verify live | ☐ |
| 0.5.3 | CF Workers Analytics tab | `https://dash.cloudflare.com/<account_id>/workers/services/view/corelink/production` | ☐ |
| 0.5.4 | BetterStack status page | Page ID `247652` — `https://status.corelink.humangr.com` operational | ☐ |
| 0.5.5 | BetterStack 7 synthetic probes | All 7 probes show GREEN before deploy starts | ☐ |
| 0.5.6 | Sentry DSN | **DEFERRED** — Sentry integration is a no-op until DSN is provisioned (not an error; Phase H). Zero CRITICAL alerts expected vacuously; revisit post-Phase-H. | — |

---

## §1 Stage 1 — 5% canary

### §1.1 Promote command

```bash
# Verify wrangler version supports --percent flag
npx wrangler@latest --version

# Deploy at 5% traffic percentage (Workers Paid plan required)
npx wrangler@latest deploy \
  --env prod \
  --percent 5 \
  --message "Wave 32 Phase E — canary stage 1 (5%)"
```

> **Automated alternative:** `bash scripts/e-day-container-canary-promote.sh --apply`
> starts at 5% and enforces the full sequence with built-in waits and Owner acks.

### §1.2 What to monitor (10-minute observation window)

Start the 10-minute clock the moment the deploy command completes successfully.

**Observability sources:**

| Source | What to watch | Dashboard / URL |
|---|---|---|
| CF Workers Analytics | Error rate (p99 errors/total requests), P95 latency | `https://dash.cloudflare.com/<account_id>/workers/services/view/corelink/production` → Analytics tab |
| Grafana DASH-SLO-API | P95 request latency per endpoint, error rate time-series | `dashboards/grafana/DASH-SLO-API.json` (uid `DASH-SLO-API`) |
| Grafana DASH-SLO-AUDIT | Audit chain error rate, write latency | `dashboards/grafana/DASH-SLO-AUDIT.json` (uid `DASH-SLO-AUDIT`) |
| BetterStack page `247652` | All 7 synthetic probes must remain GREEN | `https://status.corelink.humangr.com` |

**Checklist (tick each at T+10min):**

- [ ] CF Workers Analytics: error rate baseline established (note value: _____%)
- [ ] CF Workers Analytics: P95 latency baseline established (note value: _____ ms)
- [ ] Grafana DASH-SLO-API: no anomalous spikes in error rate panel
- [ ] Grafana DASH-SLO-AUDIT: no anomalous spikes in audit write panel
- [ ] BetterStack: all 7 probes remain GREEN throughout the 10-min window
- [ ] No CRITICAL alerts fired in any monitoring surface

### §1.3 PASS criteria (all four required)

| # | Criterion | Data source | Threshold | Result |
|---|---|---|---|---|
| C1 | Error rate within +0.5pp of pre-deploy baseline | CF Workers Analytics + Grafana DASH-SLO-API error rate panel | Δ ≤ +0.5 percentage points | ☐ PASS / ☐ FAIL |
| C2 | P95 latency within +20ms of pre-deploy baseline | CF Workers Analytics + Grafana DASH-SLO-API p95 panel | Δ ≤ +20ms | ☐ PASS / ☐ FAIL |
| C3 | Zero CRITICAL Sentry errors (if DSN provisioned) | Sentry dashboard — DEFERRED until DSN set; auto-PASS if DSN absent | 0 CRITICAL in window | ☐ PASS / ☐ N/A |
| C4 | Zero BetterStack probe failures | BetterStack page `247652` all-probes view | 0 failures in 10-min window | ☐ PASS / ☐ FAIL |

**Decision:**
- All criteria PASS → proceed to §1.4 (promote to 25%).
- Any criterion FAIL → execute §1.5 (rollback at 5%) immediately.

### §1.4 Promote to 25% (after PASS)

Proceed to §2.

### §1.5 Rollback at 5%

```bash
npx wrangler@latest rollback --env prod
# Verify rollback complete:
npx wrangler@latest deployments list --env prod | head -5
```

Document failure in `specs/_audits/` before retrying. Do NOT retry without Owner root-cause sign-off.

---

## §2 Stage 2 — 25% canary

### §2.1 Promote command

```bash
npx wrangler@latest deploy \
  --env prod \
  --percent 25 \
  --message "Wave 32 Phase E — canary stage 2 (25%)"
```

### §2.2 What to monitor (10-minute observation window)

Same sources as §1.2. Start 10-minute clock from deploy completion.

| Source | What to watch |
|---|---|
| CF Workers Analytics | Error rate vs. baseline established at 5% stage |
| Grafana DASH-SLO-API | P95 latency per endpoint, error rate time-series |
| Grafana DASH-SLO-AUDIT | Audit chain write latency + error rate |
| BetterStack page `247652` | All 7 synthetic probes remain GREEN |

**Checklist (tick each at T+10min):**

- [ ] Error rate: still within +0.5pp of baseline
- [ ] P95 latency: still within +20ms of baseline
- [ ] Grafana DASH-SLO-API: no sustained spikes
- [ ] Grafana DASH-SLO-AUDIT: no audit chain errors
- [ ] BetterStack: all 7 probes GREEN throughout window
- [ ] No CRITICAL alerts in any surface

### §2.3 PASS criteria (all four required)

| # | Criterion | Data source | Threshold | Result |
|---|---|---|---|---|
| C1 | Error rate within +0.5pp of baseline | CF Workers Analytics + Grafana DASH-SLO-API | Δ ≤ +0.5pp | ☐ PASS / ☐ FAIL |
| C2 | P95 latency within +20ms of baseline | CF Workers Analytics + Grafana DASH-SLO-API | Δ ≤ +20ms | ☐ PASS / ☐ FAIL |
| C3 | Zero CRITICAL Sentry errors (if DSN provisioned) | Sentry — DEFERRED; auto-PASS if no DSN | 0 CRITICAL | ☐ PASS / ☐ N/A |
| C4 | Zero BetterStack probe failures | BetterStack page `247652` | 0 failures | ☐ PASS / ☐ FAIL |

**Decision:**
- All criteria PASS → proceed to §2.4 (promote to 100%).
- Any criterion FAIL → execute §2.5 (rollback at 25%) immediately.

### §2.4 Promote to 100% (after PASS)

Proceed to §3.

### §2.5 Rollback at 25%

```bash
npx wrangler@latest rollback --env prod
npx wrangler@latest deployments list --env prod | head -5
```

Document failure in `specs/_audits/` before retrying. Do NOT retry without Owner root-cause sign-off.

---

## §3 Stage 3 — 100% promote

### §3.1 Promote command

```bash
npx wrangler@latest deploy \
  --env prod \
  --message "Wave 32 Phase E — full promote (100%)"
# Note: omitting --percent flag (or passing --percent 100) promotes fully.
```

### §3.2 What to monitor (10-minute observation window)

Same sources as §1.2 and §2.2. Start 10-minute clock from deploy completion.

**Checklist (tick each at T+10min):**

- [ ] Error rate: within +0.5pp of baseline across all traffic
- [ ] P95 latency: within +20ms of baseline across all traffic
- [ ] Grafana DASH-SLO-API: stable time-series, no degradation trend
- [ ] Grafana DASH-SLO-AUDIT: audit chain healthy
- [ ] BetterStack: all 7 probes GREEN throughout window
- [ ] No CRITICAL alerts in any surface

### §3.3 PASS criteria (all four required)

| # | Criterion | Data source | Threshold | Result |
|---|---|---|---|---|
| C1 | Error rate within +0.5pp of baseline | CF Workers Analytics + Grafana DASH-SLO-API | Δ ≤ +0.5pp | ☐ PASS / ☐ FAIL |
| C2 | P95 latency within +20ms of baseline | CF Workers Analytics + Grafana DASH-SLO-API | Δ ≤ +20ms | ☐ PASS / ☐ FAIL |
| C3 | Zero CRITICAL Sentry errors (if DSN provisioned) | Sentry — DEFERRED; auto-PASS if no DSN | 0 CRITICAL | ☐ PASS / ☐ N/A |
| C4 | Zero BetterStack probe failures | BetterStack page `247652` | 0 failures | ☐ PASS / ☐ FAIL |

**Decision:**
- All criteria PASS → proceed to §4 (post-promote sign-off).
- Any criterion FAIL → execute §3.4 (rollback at 100%) immediately.

### §3.4 Rollback at 100%

```bash
# Standard rollback (preferred — reverts to previous version)
npx wrangler@latest rollback --env prod
npx wrangler@latest deployments list --env prod | head -5
```

> **⚠ DATA LOSS WARNING — Durable Object purge (last resort only).**
>
> If `wrangler rollback` is insufficient because Durable Object (DO) state has been
> corrupted (e.g., schema-incompatible writes during the canary window), the Owner may
> authorize a full DO purge:
>
> ```bash
> # REQUIRES --apply AND --accept-data-loss both present.
> # DO NOT run without explicit Owner verbal + written ack.
> bash scripts/e-day-container-canary-promote.sh \
>   --apply \
>   --accept-data-loss \
>   --force-do-purge
> # Or directly:
> npx wrangler@latest delete --env prod --force
> ```
>
> **This is irreversible.** All in-flight DO state (active sessions, pending writes) is
> permanently lost. Requires Owner explicit written acknowledgement before execution.
> Document decision in `specs/_audits/` with timestamp and Owner signature.

---

## §4 Post-promote sign-off checklist

Complete after all three stages PASS and 100% promote is stable for 10 minutes.

| # | Item | Status |
|---|---|---|
| 4.1 | Phase E SEAL audit committed to `specs/_audits/` | ☐ |
| 4.2 | Deployment version recorded in SEAL doc | ☐ |
| 4.3 | BetterStack page `247652` shows all green (no historical incidents from canary) | ☐ |
| 4.4 | Grafana dashboards showing stable baseline (screenshot attached to SEAL doc) | ☐ |
| 4.5 | Phase F (DNS cutover) owner notified — Phase E gate is OPEN | ☐ |
| 4.6 | Phase G (smoke test) owner notified — ready to execute post-DNS | ☐ |
| 4.7 | Any canary-window anomalies (even non-blocking) documented in SEAL doc | ☐ |

---

## §5 Rollback ladder summary

| Stage | Trigger | Command | Data loss? |
|---|---|---|---|
| 5% — FAIL | C1-C4 any FAIL in §1.3 | `npx wrangler@latest rollback --env prod` | No |
| 25% — FAIL | C1-C4 any FAIL in §2.3 | `npx wrangler@latest rollback --env prod` | No |
| 100% — FAIL (standard) | C1-C4 any FAIL in §3.3 | `npx wrangler@latest rollback --env prod` | No |
| 100% — DO state corrupted | Owner explicit ack + `--apply --accept-data-loss` | `npx wrangler@latest delete --env prod --force` | **YES — irreversible** |

---

## §6 Cross-references

- Wave 32 Phase E spec: `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md` §4 Phase E
- Promote automation script: `scripts/e-day-container-canary-promote.sh`
- Phase E canary prep SEAL: `specs/_audits/2026-05-27-w32-phaseE-canary-prep-seal.md`
- General rollback runbook: `scripts/rollback-prod-corelink.sh`
- Grafana API SLO dashboard: `dashboards/grafana/DASH-SLO-API.json` (uid `DASH-SLO-API`)
- Grafana Audit SLO dashboard: `dashboards/grafana/DASH-SLO-AUDIT.json` (uid `DASH-SLO-AUDIT`)
- BetterStack status page `247652`: `https://status.corelink.humangr.com`
- Phase A SEAL (BetterStack setup): `specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md`
- CF Workers Analytics: `https://dash.cloudflare.com/<account_id>/workers/services/view/corelink/production`

---

**End of RB-W32-CONTAINER-CANARY v1.0.0**
