---
title: Validator Status Report
status: CURRENT
date: 2026-05-07
audited_by: hardening-sprint-agent-3
---

# Validator Scripts — Status Report

Audited 2026-05-07. Run from repo root: `python3 scripts/<name>.py`.
Python 3.x; no extra venv required (pure stdlib + `pyyaml` + `jsonschema`).

## Status Table

| Script | What it checks | Before | After | What was fixed |
|---|---|---|---|---|
| `validate_specs.py` | YAML front-matter structure + JSON Schema (draft-2020-12) on all `specs/*.md` | PASS (exit 0) | PASS (exit 0) | No fix needed |
| `validate_references.py` | Cross-doc ID integrity (EVT/CTRL/PAT/FM/INV/FF-HR/SLO/RB/ADR) — dangling + orphan detection | FAIL (exit 1, 17 false-positive RB IDs + 5 false-positive INV IDs due to regex bugs) | FAIL (exit 1, only real spec findings remain) | Fixed `RB` regex (`[A-Z0-9-]+` → `[A-Z0-9-]*[A-Z0-9]`) to stop greedy trailing-dash capture from runbook filenames; added 5 INV plural/line-wrap short-forms to `WHITELIST_IDS` |
| `validate_dashboards.py` | Grafana dashboard JSON structure: canonical 12 count, panel count ≥8, required vars, DS_PROMETHEUS datasource, tags, lastUpdated + cardinality_budget annotations | PASS (exit 0) | PASS (exit 0) | No fix needed |
| `validate_inv_promotion.py` | INVs declared in sprint WIs exist in `invariant_registry.md §3.X` | FAIL (exit 1) | FAIL (exit 1, real spec findings) | No script bug — correctly detecting 4 INVs in S09 WIs not in registry; see spec findings below |
| `check_cost_regression.py` | Cost regression gate documented in sprint contracts for S07/S08/S09/S10/S14 | PASS (exit 0) | PASS (exit 0) | No fix needed |
| `check_error_taxonomy.py` | `error_taxonomy.md` schema instantiation, domain coverage (10 canonical + onboarding), observability consistency | PASS (exit 0) | PASS (exit 0) | No fix needed |
| `check_migrations_additive.py` | SQL migrations in `migrations/` are additive (no DROP TABLE/COLUMN/INDEX, TRUNCATE, RENAME, etc.) | PASS (exit 0) | PASS (exit 0) | No fix needed |
| `check_tla_obligations.py` | CRITICAL/HIGH invariants in `invariant_registry.md §3` have obligation entries in `§4` | FAIL (exit 1) | FAIL (exit 1, real spec findings) | No script bug — correctly detecting 99 INVs added to §3 (S03–S06 sprints) missing §4 obligation entries; see spec findings below |

## PR-Gate Readiness

| Script | PR-gate-ready? | Notes |
|---|---|---|
| `validate_specs.py` | YES | Stable; exits 0 on clean spec set |
| `validate_references.py` | YES (gates on real findings) | After fix, no false positives; exits 1 when there are genuine dangling IDs — appropriate for PR gate |
| `validate_dashboards.py` | YES | Exits 0; all 12 canonical + 3 legacy dashboards pass all checks |
| `validate_inv_promotion.py` | YES (gates on real findings) | Exits 1 when WI-declared INVs are absent from registry; that is correct behavior |
| `check_cost_regression.py` | YES | Exits 0; all 5 required sprints (S07/S08/S09/S10/S14) have cost gate documented |
| `check_error_taxonomy.py` | YES | Exits 0; 71 error codes, 16 domains — all required domains covered |
| `check_migrations_additive.py` | YES | Exits 0; 21 migration files scanned, all additive |
| `check_tla_obligations.py` | YES (gates on real findings) | Exits 1 when CRITICAL/HIGH INVs lack §4 obligation entry; that is correct behavior |

---

## Real Spec-Side Findings Caught by Validators

These are genuine spec gaps detected by working validators. **Do not modify the validators** to suppress these — fix the specs during the relevant sprint implementation.

### Finding 1 — `validate_references.py`: 71 dangling cross-doc references

References used in sprint WIs/contracts that have no canonical definition in their source doc.

| Type | Count | Key examples |
|---|---|---|
| CTRL | 5 | `CTRL-CHAOS-001` (S17), `CTRL-DATA-RESIDENCY-001` (S14), `CTRL-KEY-013/014` (S14), `CTRL-MULTIPART-002` (S05) |
| PAT | 3 | `PAT-PRIV-001` (S17), `PAT-SAGA-001` (S19), `PAT-SAGA-ATOMIC-001` (S20) |
| FM | 1 | `FM-249` (S03 PRR — not in `failure_modes.md`) |
| INV | 4 | `INV-AVAIL-DOS`, `INV-CAS-DIGEST-INTEGRITY`, `INV-EXEC-IDEMPOTENT`, `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` (all in S09 WIs) |
| SLO | 26 | `SLO-ADMIN-*` (S13, 5 SLOs), `SLO-BYOK-*` (S14, 6 SLOs), `SLO-ONBOARD-*` (S19, 5 SLOs), `SLO-AVAIL/LAT/FRESH` (S20) |
| RB | 32 | `RB-ABUSE-*` (S08, 5 RBs), `RB-AUDIT-*` (S09, 3 RBs), `RB-GLOBAL-CIRCUIT-*` (S08, 3 RBs), `RB-QUOTA-*` (S08, 3 RBs), `RB-BYOK-*` (S14), `RB-ONBOARD-*` (S19/S20), etc. |

**Resolution**: These IDs are forward-declared in sprint contracts but their canonical source files (runbooks, SLO catalog entries, CTRL/PAT entries) have not yet been created. Add during respective sprint implementation.

### Finding 2 — `validate_inv_promotion.py`: 4 INVs in S09 WIs missing from registry

| INV ID | WI file |
|---|---|
| `INV-AVAIL-DOS` | `S09/work_items/WI-S09-002-logpush-r2-loki-log-schema-pii-redaction.md` |
| `INV-CAS-DIGEST-INTEGRITY` | `S09/work_items/WI-S09-005-12-grafana-dashboards-as-code.md` |
| `INV-EXEC-IDEMPOTENT` | `S09/work_items/WI-S09-005-12-grafana-dashboards-as-code.md` |
| `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` | `S09/work_items/WI-S09-005-12-grafana-dashboards-as-code.md` |

**Resolution**: Add these 4 INVs to `specs/03_architecture/invariant_registry.md §3.X` in the appropriate domain section during S09 implementation.

### Finding 3 — `check_tla_obligations.py`: 99 CRITICAL/HIGH INVs in §3 missing §4 obligation entries

The registry §3 was significantly expanded during Lotes 10.2–10.7 (sprints S03–S07), adding ~100 new CRITICAL/HIGH INVs across the AUTH, AUDIT, AC, MULTIPART, GC, EVICT, QUOTA, BILLING, and NEG-CACHE domains. The §4 TLA+ obligation matrix was not updated to match.

Key domains affected:
- **INV-AUTH-*** (30 INVs): JWT validation, PAT hash, TenantCtx, revocation, WebAuthn, schema, DSR
- **INV-AUDIT-*** (5 INVs): PII-free, emit atomicity, chain hash, event type, retention
- **INV-AC-*** (15 INVs): Merkle tree, signing, eviction, path keys, TTL
- **INV-MULTIPART-*** (14 INVs): idempotency, manifest signing, streaming, state transitions
- **INV-GC-*** (21 INVs): mark/sweep phases, grace period, reconcile, CI gate
- **Others**: `INV-NEG-CACHE-MONOTONIC`, `INV-NO-BODY-IN-LOGS`, `INV-NO-PII-IN-LOGS`, `INV-BILLING-APPEND-ONLY`, eviction/quota/LRU group

**Resolution**: Update `specs/03_architecture/invariant_registry.md §4` to add obligation entries for each. Each new INV should be placed in §4.1 (GREEN), §4.2 (PLANNED with sprint owner + TLA+ filename), or §4.3 (non-distributed/algorithmic — no TLA+ required). This is pre-condition for S-20 GA gate (`check_tla_obligations.py --pre-ga`).
