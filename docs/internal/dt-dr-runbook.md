# DT DR Runbook — Dependency-Track Disaster Recovery (WI-S12-005)

**Runbook ID:** RB-DT-001  
**Status:** ⚠️ NOT EXECUTABLE AS WRITTEN — see verification note below. Was marked
ACTIVE but never drilled ("_pending first quarterly DR test_" below, unchanged since
authoring) and every command in this document targets a host that does not resolve.  
**Owner:** SRE Lead  
**Last tested:** _pending first quarterly DR test_ (still true — this runbook has
never actually been run)  
**Next test due:** Q3 2026  

> ⚠️ **Verified NXDOMAIN, no live equivalent documented anywhere in this repo.**
> Every procedure below targets `dt.corelink.humangr.com`, which does not resolve
> (`dig dt.corelink.humangr.com` returns no answer). The `DT_API_URL` /
> `DT_API_KEY` / `DT_WEBHOOK_SECRET` / `DT_PROJECT_UUID` secrets referenced by
> `crates/corelink-dt-reconcile` / `crates/corelink-dt-cli` / `.github/workflows/sbom.yml`
> are declared in `docs/internal/secrets-checklist.md` (#44-48) but **none of them
> are actually set as GitHub secrets on this repo** (`gh secret list` shows zero
> `DT_*` entries) — so the Dependency-Track self-hosted instance this runbook
> assumes appears to have never been provisioned, or was decommissioned without
> this runbook being updated. **Do not treat this document as executable until an
> operator confirms whether Dependency-Track is still in service and, if so,
> supplies the current instance URL** — this document does not invent one. If DT
> is no longer in service, the correct fix is to retire this runbook (and the SBOM
> workflow's DT dependency) rather than patch a host name.

---

## 1. Overview

This runbook covers recovery of the Dependency-Track self-hosted instance
(`https://dt.corelink.humangr.com` — **UNVERIFIED / does not currently resolve; see
the warning above**) from the following failure modes:

| Failure | RTO | RPO | Recovery path |
|---------|-----|-----|---------------|
| DT instance corruption | ≤ 1h | ≤ 1h | Neon PITR restore + redeploy |
| VPS failure | ≤ 1h | ≤ 1h | VPS snapshot restore |
| Neon Postgres failure | ≤ 1h | ≤ 1h | Neon automatic failover + PITR |
| DT API outage > 4h | Alert SEV-3 | N/A | OSS Index fallback + DLQ replay |
| Webhook delivery failure | SEV-2 alert | N/A | DLQ replay via corelink-dt-reconcile |

---

## 2. Decision Tree

```
DT instance unreachable?
  ├── Yes, < 4h
  │     └── Monitor; ping #supply-chain-cve-alerts; no action required yet.
  ├── Yes, ≥ 4h
  │     ├── Alert SEV-3 'dt_outage' fires automatically.
  │     ├── Trigger OSS Index fallback: corelink-dt-cli ossindex-fallback
  │     └── Begin DR procedure § 3.
  └── No → check webhook delivery (§ 4).
```

---

## 3. DT Instance Recovery Procedure

### 3.1 Verify failure scope

```bash
# SSH to VPS
ssh ops@<vps-ip>

# Check container status
docker compose -f /opt/dt/docker-compose.yml ps

# Check Neon Postgres connectivity
psql $NEON_POSTGRES_URL -c "SELECT NOW();"
```

### 3.2 Neon Postgres PITR restore

1. Log in to [Neon Console](https://console.neon.tech).
2. Navigate to the `dt_db` project → **Branches** → **Restore**.
3. Select a restore point no older than 1h before the failure.
4. Click **Restore to new branch**, then promote it to main.
5. Update `NEON_POSTGRES_URL` in `/opt/dt/.env` if the endpoint changes.

### 3.3 Redeploy DT stack

```bash
# Pull pinned images (verify digests match docker-compose.yml).
docker compose -f /opt/dt/docker-compose.yml pull

# Restart stack.
docker compose -f /opt/dt/docker-compose.yml up -d

# Verify health.
curl -sf https://dt.corelink.humangr.com/api/version | jq .version
```

### 3.4 Verify SBOM history retained

```bash
# Query DT API for recent project SBOMs.
curl -H "X-Api-Key: $DT_API_KEY" \
  "https://dt.corelink.humangr.com/api/v1/project?pageSize=10" | jq '.[].name'
```

Expected: all workspace members (corelink-server, corelink-cli, corelink-worker) present.

### 3.5 Verify CVE matching resumes

```bash
# Trigger a manual NVD sync.
curl -X POST -H "X-Api-Key: $DT_API_KEY" \
  "https://dt.corelink.humangr.com/api/v1/vulnerability/source/NVD/refresh"

# Watch logs for sync completion.
docker logs dt-apiserver --follow | grep "NVD sync"
```

### 3.6 Validate E2E alert path

```bash
# Run mock CVE injection to verify the full alert path.
DT_MOCK_INJECTION_ENABLED=true \
DT_WEBHOOK_SECRET=$DT_WEBHOOK_SECRET \
DT_PROJECT_UUID=<staging-project-uuid> \
corelink-dt-cli inject-mock --severity critical
```

Expected: exit code 0 + Slack message in `#supply-chain-cve-alerts` within 15 min.

---

## 4. Webhook Delivery Failure Recovery

### 4.1 Check DLQ size

```bash
# Query current DLQ size via reconcile job (dry run).
DT_API_URL=https://dt.corelink.humangr.com/api/v1 \
DT_API_KEY=$DT_API_KEY \
DT_WEBHOOK_SECRET=$DT_WEBHOOK_SECRET \
corelink-dt-reconcile
```

### 4.2 Manual DLQ replay

If the reconcile job fails to auto-replay:

```bash
# Run reconcile with verbose logging.
RUST_LOG=debug corelink-dt-reconcile
```

### 4.3 Alert fatigue DLQ drain

If DLQ exceeds 1 000 events (SEV-2 alert fires):

1. Audit DLQ contents for legitimate alerts vs noise.
2. If noise: clear DLQ entries with `SKIP_NOISE=true` flag (custom env var).
3. If legitimate: escalate to Security Lead.

---

## 5. OSS Index Fallback (DT outage > 4h)

```bash
# Manual trigger — queries Sonatype OSS Index for CVEs against current Cargo.lock.
DT_WEBHOOK_SECRET=$DT_WEBHOOK_SECRET corelink-dt-cli ossindex-fallback
```

Rate limit: 1 000 lookups/day on free tier. Use sparingly.

---

## 6. Quarterly DR Test Procedure

1. **Pre-test notification**: notify #supply-chain-cve-alerts 48h ahead; schedule 2h maintenance.
2. **Simulate Postgres corruption**: rename the Neon main branch in console.
3. **Execute recovery**: follow § 3 above.
4. **Measure RTO**: time from "corruption simulated" to "CVE matching resumed".
5. **Document**: commit report to `specs/_audits/dt-dr-test-<YYYY>-Q<N>.md`.
6. **Pass criteria**: RTO ≤ 1h, SBOM history retained, mock CVE injection green.

---

## 7. Escalation Matrix

| Condition | Severity | On-call action |
|-----------|----------|----------------|
| DT down < 4h | INFO | Monitor only |
| DT down ≥ 4h | SEV-3 | Page SRE on-call; trigger OSS Index fallback |
| DLQ > 1 000 | SEV-2 | Page Security Lead; audit DLQ |
| Mock CVE injection fails 24h | SEV-3 | Page SRE; debug E2E path |
| Reconciliation gap > 5/day | SEV-2 | Page Security Lead + SRE |
| TLS cert expires in < 30d | SEV-3 | Renew via Let's Encrypt (Caddy auto-renews) |
| HMAC secret leak | CRITICAL | Rotate secret + audit all webhook events |

---

## 8. References

- WI-S12-005 spec: `specs/04_sprints/_sealed/S12/work_items/WI-S12-005-dependency-track-self-host-cve-alerts.md`
- ADR-0024: `specs/03_architecture/adrs/ADR-0024-dependency-track-self-host.md`
- RB-FM-156 (dep malicious scenario): `specs/04_sprints/_sealed/S12/work_items/WI-S12-007-rb-fm-156-rb-fm-157-prr-ship-gate.md`
- Neon PITR docs: <https://neon.tech/docs/introduction/point-in-time-restore>
- DT API docs: <https://docs.dependencytrack.org/integrations/rest-api/>
