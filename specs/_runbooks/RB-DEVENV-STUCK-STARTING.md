---
id: "RB-DEVENV-STUCK-STARTING"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "devenv", "p1", "sre", "wave-devenv"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-DEVENV-STUCK-STARTING — DevEnv Container Boot & Health Timeout

> **Severity floor:** P1
> **Detect → Acknowledge → Engage signers:** SRE on-call (pager duty alerts on `devenv_start_timeout_total > 5`).
> **Companion docs:** `RB-INCIDENT-RESPONSE.md` · `specs/_compliance/GA-GATE-CRITERIA.md`.

---

## 1. Symptoms
- Tenant UI displays persistent `starting` spinner (> 60s).
- Alert: `RunnerDevEnvDO` health check `/ping` failing on port 9090.
- Worker logs: `RunnerDevEnvDO: start failed with timeout awaiting container ready`.

## 2. Diagnosis
1. Query DO state via Cloudflare Workers dashboard or admin API:
   ```bash
   curl -H "Authorization: Bearer $ADMIN_TOKEN" https://api.corelink.humangr.com/internal/v1/devenv/tenant/<TENANT_ID>/status
   ```
2. Check container boot logs for gVisor/supervisord failure:
   - Inspect `/var/log/supervisord.log` within the container instance.
   - Verify `corelink-check-exec-server` listening on `127.0.0.1:9090`.

## 3. Resolution
### Option A: Force Restart & Transition Reset
Trigger an administrative reset to transition the DO back to `stopped`:
```bash
curl -X POST -H "Authorization: Bearer $ADMIN_TOKEN" \
  https://api.corelink.humangr.com/internal/v1/devenv/tenant/<TENANT_ID>/force-reset
```

### Option B: Container Eviction
Evict the stuck microVM instance to spawn a clean replacement.
