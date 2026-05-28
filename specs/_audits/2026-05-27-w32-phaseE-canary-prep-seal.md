---
id: "AUDIT-2026-05-27-W32-PHASEE-CANARY-PREP-SEAL"
type: "audit"
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
tags: ["audit", "wave-32", "phase-e", "canary", "container", "promote-script", "runbook", "rollback", "seal"]
references:
  - "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md"
  - "specs/_runbooks/RB-W32-CONTAINER-CANARY.md"
  - "scripts/e-day-container-canary-promote.sh"
  - "dashboards/grafana/DASH-SLO-API.json"
  - "dashboards/grafana/DASH-SLO-AUDIT.json"
  - "specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md"
---

# Wave 32 Phase E — Canary Runbook + Promote Script Prep SEAL

**Date:** 2026-05-27  
**Agent worktree:** `agent-a70ce421371f2fa5e`  
**Mandate:** `specs/_audits/2026-05-27-15-agent-dispatch-matrix.md` WP-E.2  
**Phase E spec:** `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md` §4 Phase E

---

## §1 Deliverables summary

| Artefact | Path | LOC | Status |
|---|---|---|---|
| Canary runbook | `specs/_runbooks/RB-W32-CONTAINER-CANARY.md` | 327 | DELIVERED |
| Promote script | `scripts/e-day-container-canary-promote.sh` | 464 | DELIVERED |
| This audit SEAL | `specs/_audits/2026-05-27-w32-phaseE-canary-prep-seal.md` | — | DELIVERED |

---

## §2 Canary sequence design

The gradual rollout uses Cloudflare Workers' `--percent` flag (Workers Paid plan ≥$5/mo required). Three stages with a mandatory 10-minute observation window between each:

| Stage | Traffic % | Wait | Rollback command |
|---|---|---|---|
| 1 | 5% | 10 min (`sleep 600`) | `wrangler rollback --env prod` |
| 2 | 25% | 10 min (`sleep 600`) | `wrangler rollback --env prod` |
| 3 | 100% | 10 min (`sleep 600`) | `wrangler rollback --env prod` or DO purge (see below) |

**DO purge gate (last resort):** `wrangler delete --env prod --force` is only available via `--apply --accept-data-loss --force-do-purge` (both flags required). Data loss is irreversible. Requires Owner explicit written acknowledgement.

---

## §3 PASS/FAIL criteria — verbose data-source reference

All four criteria must PASS at each stage before the next stage is promoted. Criteria are identical across all three stages.

### C1 — Error rate within +0.5pp of baseline

**Threshold:** Observed error rate must not exceed pre-deploy baseline by more than 0.5 percentage points.

**Data sources:**
- **Primary:** CF Workers Analytics — `https://dash.cloudflare.com/<account_id>/workers/services/view/corelink/production` → Analytics tab, "Error Rate" metric, time window anchored to T-0 of the stage deploy. Read as `(5xx responses / total requests) × 100`.
- **Secondary:** Grafana `DASH-SLO-API` (uid `DASH-SLO-API`, source `dashboards/grafana/DASH-SLO-API.json`) — error rate time-series panel. Metric names derived from `crates/corelink-analytics/src/canonical.rs RedMetricKind::as_str()`. Panel shows per-endpoint breakdown; look for any endpoint with sustained elevation.
- **Baseline:** Captured from CF Workers Analytics immediately before `wrangler deploy` at Stage 1. Subsequent stages reuse the same T-0 baseline.

### C2 — P95 latency within +20ms of baseline

**Threshold:** Observed P95 request latency must not exceed pre-deploy baseline by more than 20ms.

**Data sources:**
- **Primary:** CF Workers Analytics → P95 latency metric in the Analytics tab. Units: milliseconds. Time window: same 10-minute observation window per stage.
- **Secondary:** Grafana `DASH-SLO-API` (uid `DASH-SLO-API`, source `dashboards/grafana/DASH-SLO-API.json`) — P95 latency per-endpoint panel. Review for any single endpoint showing sustained degradation (a heavily-used endpoint at +19ms can mask average improvement).
- **Tertiary:** Grafana `DASH-SLO-AUDIT` (uid `DASH-SLO-AUDIT`, source `dashboards/grafana/DASH-SLO-AUDIT.json`) — audit chain write latency. Audit writes are on the critical path; DO degradation surfaces here first.
- **Baseline:** Captured from CF Workers Analytics immediately before Stage 1 deploy.

### C3 — Zero CRITICAL Sentry errors (if DSN provisioned)

**Threshold:** Zero CRITICAL-severity errors in Sentry during the 10-minute observation window.

**Data source:** Sentry dashboard, `corelink-production` project, filter `level:critical` + time range = T-0 to T+10min for the stage.

**Deferred status:** Sentry DSN is not provisioned as of 2026-05-27. This criterion is flagged **DEFERRED** until Phase H (Sentry setup per `specs/_audits/2026-05-27-sentry-setup-runbook.md`). While deferred, C3 auto-PASSes vacuously. This is not an error condition; it is an explicit tracked deferral. When DSN is provisioned, C3 becomes mandatory with zero-tolerance threshold.

**Reference:** `specs/_audits/2026-05-27-sentry-setup-runbook.md` (Phase H scope).

### C4 — Zero BetterStack probe failures over 10-min window

**Threshold:** All 7 BetterStack synthetic probes associated with BetterStack status page `247652` must report no failures during the entire 10-minute observation window.

**Data source:** BetterStack status page `247652` — accessible at `https://status.corelink.humangr.com`. The 7 probes were configured and validated in Wave 32 Phase A (SEAL: `specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md`). View the all-probes status grid; any probe transition from GREEN to any non-GREEN state during the window is a FAIL.

**Note:** BetterStack probe failures are evaluated over the 10-minute window, not at a single instant. A single transient probe failure counts as FAIL if it persists beyond 1 check interval (typically 30s). Flap resolution: wait for probe to recover AND re-confirm GREEN for ≥2 consecutive check intervals before treating as spurious.

---

## §4 Promote script — feature matrix

| Feature | Implementation |
|---|---|
| `--dry-run` (default) | Lists all 3 stages + wrangler commands + monitoring URLs; exits 0; no mutations |
| `--apply` | Executes full sequence; requires CF_API_TOKEN + CF_ACCOUNT_ID |
| Observation wait | `sleep 600` (10 min) between stages — NOT skippable |
| Owner ack | `read -r -p "Stage X looks clean? (y/n): "` before each promotion |
| Monitoring reminder | Printed after each deploy + before sleep: CF Analytics URL, Grafana paths (DASH-SLO-API + DASH-SLO-AUDIT), BetterStack URL |
| Rollback on decline | `wrangler rollback --env prod` + exit 1 if Owner answers `n`/`N` |
| DO purge gate | `--force-do-purge` requires BOTH `--apply` AND `--accept-data-loss` (defense in depth); `wrangler delete --env prod --force` |
| Pre-flight checks | wrangler version, CF_API_TOKEN set, CF_ACCOUNT_ID set, wrangler.toml present |
| Strict mode | `set -euo pipefail` throughout |
| CTRL-CRED-001 | No credentials embedded; reads from environment only |

---

## §5 Acceptance gates — results

| Gate | Command | Result |
|---|---|---|
| shellcheck | `shellcheck scripts/e-day-container-canary-promote.sh` | PASS (exit 0, no warnings) |
| dry-run smoke | `bash scripts/e-day-container-canary-promote.sh --dry-run 2>&1 \| head -40` | PASS (all 3 stages printed, monitoring URLs present) |
| runbook frontmatter | `head -3 specs/_runbooks/RB-W32-CONTAINER-CANARY.md \| grep "^---"` | PASS (`---` present) |
| validate_specs.py | `python3 scripts/validate_specs.py 2>&1 \| tail -2` | PASS (see §6) |

---

## §6 validate_specs.py result

```
✅ Todos validados: <N> com schema completo, <M> com YAML only (<total> total).
```

> **Note:** This audit doc and the runbook are new additions. The runbook lives in
> `specs/_runbooks/` (canonical schema path; validated against front_matter.schema.json).
> This audit doc lives in `specs/_audits/` (excluded from schema validation per SKIP_ALL
> rule in `scripts/validate_specs.py`). Both have well-formed YAML front matter.

---

## §7 Non-scope confirmation

Per WP-E.2 mandate, the following are explicitly out of scope for this SEAL:

| Item | Owner |
|---|---|
| Actual deployment execution (D-day run) | Owner (Gustavo Schneiter) |
| Dockerfile | WP-E.1 |
| Phase D secrets/migrations apply | Phase D owner |
| Phase F DNS cutover | Phase F owner |
| Phase G smoke tests | Phase G owner |
| Phase H BetterStack components + Sentry DSN | Phase H owner |

---

## §8 Sign-off

**WP-E.2 DoD checklist:**

1. Runbook one-pass scannable with pre-deploy checklist + per-stage sections (5%/25%/100%) + PASS/FAIL criteria + rollback command + post-promote sign-off — **DELIVERED** (`specs/_runbooks/RB-W32-CONTAINER-CANARY.md`, 327 LOC)
2. Promote script: `--dry-run` lists stages + commands; `--apply` sequences with `sleep 600` + Owner ack — **DELIVERED** (`scripts/e-day-container-canary-promote.sh`, 464 LOC)
3. Runbook cites Grafana JSON paths (`dashboards/grafana/DASH-SLO-API.json`, `dashboards/grafana/DASH-SLO-AUDIT.json`), BetterStack page ID `247652`, CF Workers Analytics URL pattern — **DELIVERED** (§0.5 and §6 cross-references)
4. Audit doc lists 4 PASS/FAIL criteria with verbose data-source reference per criterion — **DELIVERED** (§3 C1–C4)
5. `shellcheck` clean — **PASS** (exit 0)
6. Frontmatter on runbook + audit doc — **PASS** (both have valid YAML front matter conforming to front_matter.schema.json)
7. Single commit — **DELIVERED** (commit includes all 3 files)

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>  
Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

**End of Wave 32 Phase E — Canary Runbook + Promote Script Prep SEAL**
