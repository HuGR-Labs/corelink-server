---
id: "RUNBOOK-INDEX"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "index", "catalog", "r6-2"]
---

# Runbook Index — `specs/05_quality/runbooks/`

> **Purpose:** single catalog of all canonical operational runbooks (`RB-*`) for CoreLink. Each runbook documents Symptom, Detection, Immediate mitigation, Root-cause investigation, Rollback/recovery, Escalation path, Post-incident, Related.
>
> **Coverage audit:** see `specs/_audits/sealed/2026-05-14-runbook-coverage.md` for the SLO/Alert/FM → runbook mapping (last verified 2026-05-14).
>
> **Drill cadence:** P0/P1 runbooks subject to monthly dry-run via `corelink-runbook-tracker` (WI-S17-003); canonical catalog `specs/05_runbooks/RB-RUNBOOK-DRILL-INDEX.md`.

---

## 1. SLO-anchored runbooks (10)

| RB | Covers | Drill |
|----|--------|-------|
| [`RB-SLO-AVAIL-CP`](./RB-SLO-AVAIL-CP.md) | SLO-AVAIL-CP (control plane availability) | continuous (alert-driven) |
| [`RB-SLO-AVAIL-DATA-PLANE`](./RB-SLO-AVAIL-DATA-PLANE.md) | SLO-AVAIL-CAS-GET / CAS-PUT / AC / EXEC; resolves `RB-SLO-AVAIL-CAS-GET-FAST-BURN`/`-SLOW-BURN` aliases | quarterly |
| [`RB-SLO-LATENCY-INVESTIGATION`](./RB-SLO-LATENCY-INVESTIGATION.md) | SLO-LAT-CAS-GET / CAS-PUT / AC-HIT | quarterly |
| [`RB-SLO-CORRECT-VIOLATION`](./RB-SLO-CORRECT-VIOLATION.md) | SLO-CORRECT-CAS / CORRECT-ISO (zero-budget SEV-1) | quarterly |
| [`RB-SLO-DEDUP-DEGRADATION`](./RB-SLO-DEDUP-DEGRADATION.md) | SLO-DEDUP-RATIO + cache-hit anomaly | quarterly |
| [`RB-ADMIN-CONFIG-STALE`](./RB-ADMIN-CONFIG-STALE.md) | SLO-ADMIN-CONFIG-PROPAGATION | quarterly |
| [`RB-ADMIN-DUAL-APPROVAL-BREACH`](./RB-ADMIN-DUAL-APPROVAL-BREACH.md) | SLO-ADMIN-DUAL-APPROVAL-LATENCY | semestral |
| [`RB-ADMIN-ROTATION-GAP`](./RB-ADMIN-ROTATION-GAP.md) | SLO-ADMIN-ROTATION-OVERLAP | semestral |
| [`RB-REGION-FAILOVER`](./RB-REGION-FAILOVER.md) | SLO-RTO-REGION-FAILOVER + SLO-RPO-REGION (operational; real incidents) | semestral (paired with `RB-DR-DRILL`) |
| [`RB-DR-DRILL`](./RB-DR-DRILL.md) | SLO-RTO-REGION-FAILOVER + SLO-RPO-REGION (drill-only) | semestral (Jan 1 + Jul 1) |

## 2. FM-anchored runbooks (P0/P1) (23)

| RB | FM | Class | Drill |
|----|----|-------|-------|
| [`RB-FM-007-deserialize-rce`](./RB-FM-007-deserialize-rce.md) | FM-007 | P1 (S=5) | trimestral |
| [`RB-FM-051-r2-bit-rot`](./RB-FM-051-r2-bit-rot.md) | FM-051 | P1 (S=5) | trimestral |
| [`RB-FM-054-kv-stale`](./RB-FM-054-kv-stale.md) | FM-054 | P1 | anual |
| [`RB-FM-062-hash-collision`](./RB-FM-062-hash-collision.md) | FM-062 | P1 (S=5) | anual (tabletop) |
| [`RB-FM-100-dns-outage`](./RB-FM-100-dns-outage.md) | FM-100 | P1 (S=5) | semestral |
| [`RB-FM-101-cf-edge-outage`](./RB-FM-101-cf-edge-outage.md) | FM-101 | P1 (S=5) | semestral |
| [`RB-FM-156-dep-maintainer-malicioso`](./RB-FM-156-dep-maintainer-malicioso.md) | FM-156 | P1 (S=5) | semestral |
| [`RB-FM-202-runbook-stale`](./RB-FM-202-runbook-stale.md) | FM-202 | P1 | mensal (meta) |
| [`RB-FM-205-admin-mistake`](./RB-FM-205-admin-mistake.md) | FM-205 | P1 | anual (tabletop) |
| [`RB-FM-206-terraform-drift`](./RB-FM-206-terraform-drift.md) | FM-206 | P1 | mensal |
| [`RB-FM-253-cross-tenant-read`](./RB-FM-253-cross-tenant-read.md) | FM-253 | P1 (S=5) | **trimestral (highest)** |
| [`RB-FM-254-cache-poisoning`](./RB-FM-254-cache-poisoning.md) | FM-254 | P1 (S=5) | trimestral |
| [`RB-FM-258-insider-exfil`](./RB-FM-258-insider-exfil.md) | FM-258 | P1 (S=5) | anual (tabletop) |
| [`RB-FM-300-gc-refcount-bug`](./RB-FM-300-gc-refcount-bug.md) | FM-300 | P1 | semestral |
| [`RB-FM-302-billing-leak`](./RB-FM-302-billing-leak.md) | FM-302 | P1 | mensal |
| [`RB-FM-303-ac-cross-tenant`](./RB-FM-303-ac-cross-tenant.md) | FM-303 | P1 (S=5) | trimestral |
| [`RB-FM-400-retry-storm`](./RB-FM-400-retry-storm.md) | FM-400 | P1 | trimestral |
| [`RB-FM-403-container-leak`](./RB-FM-403-container-leak.md) | FM-403 | P1 | trimestral |
| [`RB-FM-404-gc-write-race`](./RB-FM-404-gc-write-race.md) | FM-404 | P1 (S=5) | semestral |
| [`RB-DSR-ERASURE-INCOMPLETE`](./RB-DSR-ERASURE-INCOMPLETE.md) | FM-450 | P1 (S=5) | trimestral |
| [`RB-DATA-RESIDENCY-LEAK`](./RB-DATA-RESIDENCY-LEAK.md) | FM-451 | **P0** | trimestral |
| [`RB-CONSENT-TAMPERING`](./RB-CONSENT-TAMPERING.md) | FM-452 | P1 (S=5) | trimestral |
| [`RB-SUB-PROCESSOR-BROADCAST-MISS`](./RB-SUB-PROCESSOR-BROADCAST-MISS.md) | FM-453 | P1 | trimestral |

## 3. FM-anchored runbooks (P2, kept for cross-link) (12)

| RB | FM | Notes |
|----|----|-------|
| [`RB-FM-057-neon-failover`](./RB-FM-057-neon-failover.md) | FM-057 | Neon failover; semestral |
| [`RB-FM-059-do-quota-exceeded`](./RB-FM-059-do-quota-exceeded.md) | FM-059 | DO storage quota |
| [`RB-FM-060-multipart-orphan`](./RB-FM-060-multipart-orphan.md) | FM-060 | R2 multipart orphan |
| [`RB-FM-105-region-replication-diverge`](./RB-FM-105-region-replication-diverge.md) | FM-105 | Inter-region latency |
| [`RB-FM-151-stripe-outage`](./RB-FM-151-stripe-outage.md) | FM-151 | Stripe API outage |
| [`RB-FM-153-grafana-cloud-outage`](./RB-FM-153-grafana-cloud-outage.md) | FM-153 | Grafana Cloud outage |
| [`RB-FM-156-dep-malicious`](./RB-FM-156-dep-malicious.md) | FM-156 (alt) | Dep maintainer compromise (variant) |
| [`RB-FM-157-typosquat`](./RB-FM-157-typosquat.md) | FM-157 | Typosquatting (forward) |
| [`RB-FM-160-auth-invalid-storm`](./RB-FM-160-auth-invalid-storm.md) | FM-160 | Auth invalid storm |
| [`RB-FM-201-config-change-ratelimit-drop`](./RB-FM-201-config-change-ratelimit-drop.md) | FM-201 | Config rate-limit drop |
| [`RB-FM-250-ddos-volumetric`](./RB-FM-250-ddos-volumetric.md) | FM-250 | DDoS volumetric |
| [`RB-FM-305-tombstone-lost`](./RB-FM-305-tombstone-lost.md) | FM-305 | GC tombstone lost |

## 4. Cross-cutting runbooks (16)

| RB | Purpose | Drill |
|----|---------|-------|
| [`RB-BREACH-NOTIF`](./RB-BREACH-NOTIF.md) | Data breach notification (GDPR/LGPD 72h) | semestral tabletop |
| [`RB-KEY-COMPROMISE`](./RB-KEY-COMPROMISE.md) | Cryptographic key compromise | semestral |
| [`RB-HSM-UNAVAILABLE`](./RB-HSM-UNAVAILABLE.md) | HSM outage | semestral |
| [`RB-BYOK-REVOKE`](./RB-BYOK-REVOKE.md) | BYOK kill switch | semestral |
| [`RB-GDPR-ERASURE-HOLD`](./RB-GDPR-ERASURE-HOLD.md) | Legal hold vs DSR conflict | anual |
| [`RB-DSR-INTAKE-FAILURE`](./RB-DSR-INTAKE-FAILURE.md) | DSR intake pipeline failure | trimestral |
| [`RB-PRIVACY-NOTICE-LATE-PUBLICATION`](./RB-PRIVACY-NOTICE-LATE-PUBLICATION.md) | Privacy notice delay | semestral |
| [`RB-BILLING-001`](./RB-BILLING-001.md) | Billing pipeline forensic replay | semestral |
| [`RB-BILLING-002`](./RB-BILLING-002.md) | Billing reconciliation mismatch | mensal |
| [`RB-FM-AC-BUCKET-LEAK`](./RB-FM-AC-BUCKET-LEAK.md) | AC bucket cross-tenant leak (forward) | trimestral |
| [`RB-FM-AC-MIGRATION-BUG`](./RB-FM-AC-MIGRATION-BUG.md) | AC migration bug | semestral |
| [`RB-FM-AC-TTL-DRIFT`](./RB-FM-AC-TTL-DRIFT.md) | AC TTL drift | trimestral |
| [`RB-FM-AC-TTL-STORM`](./RB-FM-AC-TTL-STORM.md) | AC TTL invalidation storm | trimestral |
| [`RB-FM-SIGNUP-FAILED`](./RB-FM-SIGNUP-FAILED.md) | Signup pipeline failure | mensal |
| [`RB-OBS-CARDINALITY-001`](./RB-OBS-CARDINALITY-001.md) | Cardinality budget breach | mensal |
| [`RB-ROLLOUT-STUCK`](./RB-ROLLOUT-STUCK.md) | Progressive rollout stuck | trimestral |
| [`RB-SUPPLY-REKOR-OUTAGE`](./RB-SUPPLY-REKOR-OUTAGE.md) | Rekor transparency log outage | semestral |
| [`RB-TLA-COUNTEREXAMPLE`](./RB-TLA-COUNTEREXAMPLE.md) | TLA+ model checker finds counterexample | per-occurrence |
| [`RB-region-leak`](./RB-region-leak.md) | Region scoping leak (legacy, see DATA-RESIDENCY-LEAK) | trimestral |

## 5. Auxiliary runbook locations

- `specs/_runbooks/` — process / governance runbooks (RB-ONCALL-POLICY, RB-CHAOS-CATALOG, RB-POSTMORTEM-PROCESS, RB-PENTEST-FINDING-RESPONSE, RB-TABLETOP-TEMPLATE, RB-DRATA-SYNC-FAILURE, RB-BACKUP-VERIFICATION, RB-D1-MIGRATION-APPLY, RB-DPA-CHANGE, RB-LIGHTHOUSE-CUSTOMER-INCIDENT, RB-SYNTHETIC-PAGE-DRILL, RB-SYSTEM-CMK-ROTATION).
- `specs/05_runbooks/` — RB-RUNBOOK-DRILL-INDEX (canonical drill catalog), RB-BYOK-REVOKE (sibling), RB-region (legacy).
- `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` — **Incident-Response tabletop master playbook** (NIST 800-61 Rev.2 aligned; quarterly cadence; 6 scenarios under `specs/_compliance/ir-scenarios/TT-01..TT-06`; auditor-ready evidence form at `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md`; 2026 schedule at `specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md`). SOC 2 CC7.3 + CC7.4 + CC7.5. Closes GAP-03. Companion to BCP/DR cadence (operational rehearsal); IR tabletops focus on full human-IR program (IC + Legal + Comms + Privacy + Customer Comms seats).

> `validate_references.py` indexes only `specs/05_quality/runbooks/` for canonical RB definitions; cross-refs to RBs in `_runbooks/` and `05_runbooks/` resolve via path-relative links but do not satisfy the registry. New canonical runbooks SHOULD live here.

## 6. Maintenance

- Adding a new runbook:
  1. Place in `specs/05_quality/runbooks/RB-<ID>.md` with frontmatter (`id`, `type: "runbook"`, `doc_status`, `audit_status`, `version`, etc).
  2. Include the 7 mandatory sections: Symptom, Detection, Immediate mitigation, Root-cause investigation, Rollback/recovery, Escalation path, Post-incident. Plus a Related section.
  3. Append a row to the appropriate table above.
  4. If P0/P1: append to `specs/05_runbooks/RB-RUNBOOK-DRILL-INDEX.md` §3 catalog.
  5. Run `python3 scripts/validate_specs.py` — frontmatter must validate.
  6. Run `python3 scripts/validate_references.py` — cross-refs must resolve (or be intentional forward-stubs).

- Updating a runbook after a real incident:
  1. Bump patch version.
  2. Update `updated:` date.
  3. Add post-mortem reference in the body.
  4. Record fresh dry-run in `runbook_drills` D1 table (within 7d of merge).

---

**Fim RUNBOOK-INDEX.**
