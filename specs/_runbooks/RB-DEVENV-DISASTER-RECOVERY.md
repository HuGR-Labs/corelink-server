---
id: "RB-DEVENV-DISASTER-RECOVERY"
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
tags: ["runbook", "devenv", "p0", "sre", "dr", "wave-devenv"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-DEVENV-DISASTER-RECOVERY — Regional Cloudflare Outage & MicroVM Evacuation

> **Severity floor:** P0
> **Detect → Acknowledge → Engage signers:** Incident Commander + SRE Lead.
> **Companion docs:** `RB-INCIDENT-RESPONSE.md` · `RB-GA-LAUNCH-ROLLBACK.md`.

---

## 1. Symptoms
- Entire Cloudflare region experiences Container / microVM host outage.
- Multiple tenant DOs in the affected region become unresponsive.

## 2. Diagnosis
- Confirm regional Cloudflare status via Cloudflare status API.
- Verify that R2 multi-region replicate storage remains intact.

## 3. Resolution
1. Failover Worker ingress routing to secondary region.
2. Re-instantiate `RunnerDevEnvDO` instances in secondary region.
3. Automatically re-hydrate `/workspace` from latest CAS snapshots in R2.
4. Update public status page to `DEGRADED_OPERATION` during migration window.
