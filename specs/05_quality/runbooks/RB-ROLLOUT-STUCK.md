---
id: "RB-ROLLOUT-STUCK"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "operational", "progressive-rollout", "admin-plane", "s13"]
---

# RB-ROLLOUT-STUCK — Progressive Rollout Stuck Mid-Stage

> **WI:** WI-S13-005 | **PAT:** PAT-PROGRESSIVE-ROLLOUT-001 | **SLA:** detect ≤ 1h; resolve manual override ≤ 30 min

## Detecção

- Metric `corelink_admin_rollout_stage_gauge{outcome="active"}` stays at same stage > 1h with no probe advancement.
- Cloudflare gradual deploy API returns 5xx/timeout for > 30 consecutive probe cycles (30 min).
- SEV-3 alert fires (stage stuck > 1h): `[SEV-3] Rollout stuck at stage {stage} > 1h — CF API unavailable`.

## Causa raiz provável

1. Cloudflare gradual deploy API outage (upstream CF platform incident).
2. Rollout controller DO crashed / lost alarm binding (no probe ticks).
3. D1 write failure during stage advance (state stuck at old stage).
4. Cloudflare network partition isolating DO from D1.

## Comunicação

- Slack `#oncall` SEV-3 auto-posted via alert pipeline.
- Tag platform-on-call for Cloudflare status check.
- Customer impact: partial rollout at stuck stage% traffic; NOT a full outage (original version still serving remaining traffic).

## Diagnóstico

```bash
# 1. Check Cloudflare status
curl https://www.cloudflarestatus.com/api/v2/status.json | jq '.status.indicator'

# 2. Check rollout state via admin API
curl -H "Authorization: Bearer $ADMIN_TOKEN" \
  "https://api.corelink.HumanGuardrail.io/v1/admin/rollout/state?env=staging"

# 3. Check CF gradual deploy API
curl -H "Authorization: Bearer $CF_API_TOKEN" \
  "https://api.cloudflare.com/client/v4/accounts/$CF_ACCOUNT/workers/scripts/$WORKER/deployments"
```

## Remediação

### Opção A — CF API recovered (normal flow)
1. Wait for Cloudflare API to recover.
2. Controller will automatically resume probing at next alarm tick (60s).
3. If alarm binding lost: re-trigger via admin API `POST /v1/admin/rollout/probe?handle_id=$HANDLE_ID`.

### Opção B — Manual rollback (CF API stuck > 2h)
```bash
# Roll back to previous version via wrangler
wrangler rollback \
  --env $WRANGLER_ENV \
  --version-id $PREVIOUS_VERSION_ID

# Then abort the stuck rollout handle via admin API
curl -X POST -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "X-Dual-Approver: $APPROVER_USER_ID" \
  -H "X-Approver-Signature: $HMAC_SIG" \
  "https://api.corelink.HumanGuardrail.io/v1/admin/ops" \
  -d '{"op_type": "RolloutAbort", "handle_id": "'$HANDLE_ID'"}'
```

### Opção C — Manual advance (CF API recovered but state diverged)
```bash
# Manually trigger probe via admin API (force re-evaluate gate criteria)
curl -X POST -H "Authorization: Bearer $ADMIN_TOKEN" \
  "https://api.corelink.HumanGuardrail.io/v1/admin/rollout/probe?handle_id=$HANDLE_ID"
```

## SLO-ADMIN-ROLLBACK-RECOVERY breach handling (≤ 60s p99)

SLO-ADMIN-ROLLBACK-RECOVERY (`slo_catalog.md §4.17`) targets auto-rollback firing within 60s of error-budget breach. When this SLO is breached (auto-rollback fails or is delayed > 60s p99), the runbook applies:

1. **Confirm auto-rollback eligibility**: error budget breach detected by `INV-ROLLOUT-AUTO-ROLLBACK` (S-13)?
   ```promql
   corelink_admin_rollback_recovery_ms{outcome="completed"} p99 > 60000
   ```
2. **If auto-rollback didn't fire**: manual rollback per §Opção B; investigate why CTRL-ADMIN-007 didn't trigger (controller stuck per §Opção A, OR signal not propagated per `RB-ADMIN-CONFIG-STALE`).
3. **If auto-rollback fired but slow**: failover gateway (CF API) is the bottleneck; coordinate with `RB-FM-101` if CF degraded.
4. **SEV-1 escalation**: breach of SLO-ADMIN-ROLLBACK-RECOVERY = SEV-1 per slo_catalog §4.17.

## Escalation path

| Time elapsed   | Who                                        | Criteria                                        |
|----------------|--------------------------------------------|-------------------------------------------------|
| 0              | Primary SRE (Slack `#oncall`)              | SEV-3 stage stuck > 1h                          |
| 0              | Primary SRE + SRE Lead (PagerDuty)         | SEV-1 (SLO-ADMIN-ROLLBACK-RECOVERY breach)      |
| 30 min         | Platform on-call + Architect               | CF API outage sustained OR auto-rollback failed |
| 60 min         | VP Engineering + Comms Lead                | Customer impact (partial rollout traffic at stuck stage) |
| 2h             | CEO                                        | Sustained stuck rollout impacting GA            |

**Comms template (Slack `#oncall`):**

```
SEV-{1|3} — Progressive rollout stuck mid-stage
Handle: {handle_id}; stage: {N}% traffic on new version.
CF API status: {operational | degraded | outage}.
Auto-rollback fired: {yes | no}; if no, root cause: {...}.
Mitigation: {Opção A | B | C} in progress.
Owner: @{handle}. Next update: +30 min.
```

## Post-mortem

- If stuck > 1h sustained: 5-Why post-mortem com SRE lead + platform team.
- If CF API outage: check Cloudflare runbook `RB-FM-101` (CF edge outage).
- D1 failure: check D1 replication status + metrics.
- If SLO-ADMIN-ROLLBACK-RECOVERY breached: did CTRL-ADMIN-007 fire? What was the latency between budget breach and rollback completion? Capture in `dr_outage_log` even though this is a rollout (not a region) incident — RTO-style metric applies.

## Related

- **SLO:** SLO-ADMIN-ROLLBACK-RECOVERY (`slo_catalog.md §4.17`).
- **Invariant:** INV-ROLLOUT-AUTO-ROLLBACK CRITICAL (S-13).
- **CTRLs:** CTRL-ADMIN-007 (auto-rollback), CTRL-ADMIN-002 (dual-approval for abort).
- **FMs:** FM-101 (CF edge), FM-200 (deploy regression), FM-201 (config change rate-limit drop).
- **Patterns:** PAT-PROGRESSIVE-ROLLOUT-001, PAT-CIRCUIT-001.
- **Sister runbooks:** `RB-FM-101`, `RB-FM-201`, `RB-ADMIN-CONFIG-STALE`, `RB-SLO-AVAIL-DATA-PLANE`.
- **WI:** WI-S13-005.

## Prevenção

- Controller alarm binding should be self-healing (DO alarm re-registered on wakeup).
- Terraform drift detection (WI-S13-004) catches unexpected D1 schema divergence.
- Chaos test: simulate CF API outage sustained → verify SEV-3 fires + manual override path available.
