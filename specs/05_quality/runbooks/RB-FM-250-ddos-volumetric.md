---
id: "RB-FM-250"
type: "runbook"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-05-02"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "ddos", "rate-limit", "availability", "wi-s08-006", "dry-run-executed"]
---

> **Dry-run executed 2026-05-02** (host-side; harness `scripts/rb_fm_250_dry_run.sh`; audit trace `specs/_audits/2026-05-02-rb-fm-250-dry-run.md`). Per WI-S08-006 §6.1.3 + sprint contract §6 DoD EVT-017. Flipped DRAFT → FROZEN at S-08 PRR ship gate ceremony.

# RB-FM-250 — DDoS Volumetric (single-tenant ou cross-tenant)

> **FM:** FM-250 (S=4, O=3, D=2, RPN=24, P1) | **INV:** INV-AVAIL-ISOLATION HIGH + CTRL-RATE-001 | **SLA:** detect ≤ 5 min, mitigate ≤ 15 min

## Detecção

- Cloudflare WAF metrics: `requests_per_sec` por origin spike > 10× baseline.
- Worker analytics `corelink_rate_limit_rejects_total` spike per-IP layer.
- SLO breach `SLO-AVAIL-CAS-PUT` por > 1 min.
- Alert `ddos_volumetric_active` (multi-burn-rate).

## Comunicação

- **SEV-1** se affecting > 1 tenant ou global SLO; **SEV-2** se single-tenant isolated.
- Page SRE on-call + Security lead.
- Status page: `degraded` (regional) ou `partial outage` (global).
- Customer notification template `customer-degraded-availability`.

## Mitigação imediata (≤ 15 min)

1. **Identify attack profile**: source IPs, ASN, geographic distribution, target paths (CAS write/read/AC).
2. **Cloudflare DDoS protection escalation**:
   - Verify CF auto-mitigation engaged (L3/L4 + Bot Management).
   - Manual rule: WAF rate-limit per-IP 100 req/min (vs default 1k) para attack window.
3. **Rate limit layer 4 (S-08)**: lower per-IP layer threshold dynamically via DO config sync.
4. **Geographic block** se attack concentrado em regiões não-customer (e.g., known botnets).
5. **Tenant tier protection**: if single-tenant DoS, isolate to per-tenant bulkhead (PAT-BULKHEAD-001) preventing SLO impact em outros tenants.

## Diagnóstico (≤ 1h)

1. Logs analysis: identify attack signature (UA, headers, payload pattern).
2. Cross-reference com threat intel (CF Threat Intelligence + custom feeds).
3. Determine target: customer-specific extortion, competitor sabotage, opportunistic.
4. Asset inventory: which Worker endpoints? Which storage tier hit?

## Resolução

- Short-term: WAF rules + rate-limit tightening; CF DDoS Pro upgrade if needed.
- Mid-term: per-IP layer rate-limit (S-08 layer 3) refinement; bot challenge for suspicious traffic.
- Long-term:
  - Improve INV-AVAIL-ISOLATION via stricter per-tenant bulkhead (PAT-BULKHEAD-001).
  - Adicionar CAPTCHA challenge para signup flows (S-13 admin plane).
  - Threat intel feed integration (paid tier).

## Post-incident

- Post-mortem dentro de 7d se SEV-1.
- Customer notification follow-up com root cause + remediation plan.
- Review WAF rules + rate-limit thresholds.
- Update CTRL-RATE-001 baselines.

## Evidence

- CF analytics dashboard screenshot.
- WAF event log export (R2 7y retention).
- Rate-limit reject distribution per-IP.
- Worker logs de attack window.

## References

- `failure_modes.md` FM-250 entry.
- `invariant_registry.md` INV-AVAIL-ISOLATION.
- `resilience_patterns.md` PAT-BULKHEAD-001.
- Cloudflare DDoS Protection: <https://www.cloudflare.com/ddos/>.
- `specs/04_sprints/S08/_spec_contract.md` (rate-limit 4-layer).
