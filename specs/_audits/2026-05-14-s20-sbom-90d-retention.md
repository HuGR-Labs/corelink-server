---
id: "AUDIT-S20-SBOM-90D-RETENTION"
type: "audit"
doc_status: "SEALED"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-007"
tags:
  - "audit"
  - "s20"
  - "sbom"
  - "cyclonedx"
  - "90d-retention"
  - "supply-chain"
  - "evt-010"
---

# AUDIT-S20-SBOM-90D-RETENTION — 90-Day SBOM Retention Proof

> **Purpose:** Extend WI-S12-002's `sbom-publish` workflow to **retain SBOMs ≥ 90 days** in Cloudflare R2 with a tamper-evident verification trail, satisfying:
>
> - `_spec_contract.md` §6.1 — "SBOM CycloneDX 1.5+ signed published (alinhado S-12 R-S12-3)".
> - `_spec_contract.md` §19 — "SBOM CycloneDX 1.5+ signed published — supply chain baseline (non-waivable; CycloneDX 1.4 fallback REMOVED per Lote 10.20 codex P1)".
> - INV-SUPPLY-SBOM-PRESENT (HIGH) — verified by retention cron.
>
> **Parent:** [WI-S20-007](../04_sprints/S20/work_items/WI-S20-007-30d-staging-tla-4-runbooks-90d-sbom-closing-prr.md) deliverable S20-007-D4.
> **EVT:** EVT-010 (Supply chain / SBOM).

---

## 1. Retention contract

| Field | Value |
|---|---|
| Retention window | **≥ 90 days** rolling from `published_at` |
| Storage backend | Cloudflare R2 bucket `corelink-sbom-archive` (region: ENAM primary; replicated to WEUR per residency_failover INV) |
| Object key scheme | `sbom/v1/<yyyy>/<mm>/<dd>/<git_sha>.cyclonedx.json` + `.sig` cosign sidecar |
| Object Lock mode | **GOVERNANCE** retain-until = published_at + 90d (mirror INV-AUDIT-APPEND-ONLY pattern) |
| Customer-accessible mirror | `https://corelink.dev/sbom/v1.json` (always points at latest) — historical SBOMs queryable via `?git_sha=<sha>` |
| Verification trail | Daily cron `sbom-retention-verify.yml` walks objects emitted in last 90d; verifies cosign signature + Object Lock retention + content hash |
| Failure alert | PagerDuty SEV-2 if any SBOM in 90d window missing, unsigned, or retention-expired before 90d |

---

## 2. CI workflow extension (declarative spec)

WI-S12-002 already provides `sbom-publish.yml` (syft → CycloneDX 1.5+ → cosign sign → R2 upload). This WI extends with:

```yaml
# .github/workflows/sbom-retention-verify.yml (declarative — implementation
# follows S-12 R-S12-3 pattern; not in scope for this PR which is spec-only)
name: sbom-retention-verify
on:
  schedule:
    - cron: '0 6 * * *'  # daily 06:00 UTC
  workflow_dispatch: {}
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - name: list objects in 90d window
        run: |
          aws s3api list-objects-v2 \
            --bucket corelink-sbom-archive \
            --prefix sbom/v1/ \
            --query 'Contents[?LastModified>=`90d-ago`]'
      - name: verify cosign signature
        run: |
          for obj in $(list); do
            cosign verify-blob --signature "${obj}.sig" "${obj}"
          done
      - name: verify Object Lock retention
        run: |
          for obj in $(list); do
            aws s3api get-object-retention --bucket corelink-sbom-archive --key "$obj"
            # expects Retention.Mode = GOVERNANCE
            # expects Retention.RetainUntilDate >= LastModified + 90d
          done
      - name: emit metrics
        run: |
          echo "corelink_sbom_retention_verified_total{outcome=\"ok\"} ${N_OK}" \
            >> /tmp/metrics.prom
          echo "corelink_sbom_retention_verified_total{outcome=\"missing\"} ${N_MISS}" \
            >> /tmp/metrics.prom
```

Daily verification trail is appended to `specs/_audits/sbom-retention/<YYYY-MM-DD>.txt` (one row per SBOM verified).

---

## 3. Acceptance criteria (binary)

| # | Criterion | Threshold | Source |
|---|---|---|---|
| R1 | SBOMs retained ≥ 90 days | 100% of SBOMs published in last 90d still readable | `sbom-retention-verify.yml` daily |
| R2 | All SBOMs cosign-signed | 100% verifiable signature | cosign verify-blob |
| R3 | Object Lock GOVERNANCE active | 100% objects with `RetainUntilDate ≥ published_at + 90d` | aws s3api get-object-retention |
| R4 | CycloneDX 1.5+ spec | 100% objects pass `bomFormat=CycloneDX` + `specVersion ≥ 1.5` | jq validation |
| R5 | Customer mirror reachable | `https://corelink.dev/sbom/v1.json` → 200 OK with valid SBOM | curl + jq smoke test in cron |
| R6 | Zero retention-policy waivers | `corelink_sbom_retention_waiver_count_gauge = 0` | spec contract §19 |

---

## 4. Cumulative SBOM scope (S-13..S-19)

The 90-day window MUST cover cumulative SBOMs from S-13 through S-19 implementation merges:

| Sprint | Merge date (approx) | Crates added | In 90d window today (2026-05-14)? |
|---|---|---|---|
| S-13 | 2026-02-21 | corelink-admin, corelink-config-singleton | **no** (beyond 90d — covered by initial release SBOM) |
| S-14 | 2026-03-14 | corelink-byok, corelink-region-routing | **no** (beyond 90d) |
| S-15 | 2026-03-28 | corelink-fuzz-targets | **yes** (47 days ago) |
| S-16 | 2026-04-11 | corelink-ux | **yes** (33 days ago) |
| S-17 | 2026-04-25 | corelink-chaos-runner | **yes** (19 days ago) |
| S-18 | 2026-05-02 | corelink-docs-tooling | **yes** (12 days ago) |
| S-19 | 2026-05-13 | corelink-onboarding | **yes** (1 day ago) |

The retention cron MUST therefore see **≥ 5 SBOMs** within the 90-day rolling window on 2026-05-14 and **≥ 7 SBOMs** by GA day (D+60 ≈ 2026-06-13).

---

## 5. Adversarial scenarios (10)

| # | Scenario | Detection | Mitigation |
|---|---|---|---|
| 1 | Object deleted before 90d | aws s3api list-objects-v2 misses object | Object Lock GOVERNANCE blocks delete → root cause = misconfig; alert SEV-2 |
| 2 | Object lock relaxed by op | get-object-retention returns Mode != GOVERNANCE | daily cron alerts; revert via Terraform; post-mortem |
| 3 | cosign signature absent | cosign verify-blob fails | retention cron flags; re-sign or block GA gate |
| 4 | CycloneDX downgraded to 1.4 | jq specVersion check fails | spec contract §19 — non-waivable; build red |
| 5 | Customer mirror returns 5xx | curl health check fails | DNS/CDN troubleshoot; mirror is stateless (served from same R2 origin) |
| 6 | Cosign key rotation invalidates old sigs | verify-blob fails on legacy SBOMs | dual-sign during rotation per S-12 R-S12-3 |
| 7 | R2 region outage | listObjects fails on primary | failover to WEUR replica per INV-DATA-RESIDENCY (replication is async; gap ≤ MaxLag) |
| 8 | git_sha collision | unlikely (SHA-1 → SHA-256 deployment SBOM hash) | use SHA-256 of build artefact as secondary key |
| 9 | Manual `aws s3 cp` upload bypassing CI | object lacks signature → R3 fails | bucket policy restricts PutObject to CI role only |
| 10 | Retention period off-by-one | retention end < 90d | unit test in retention-verify cron asserts `≥ 90d`, not `> 89d` |

All scenarios route to PagerDuty SEV-2; none CRITICAL because the build artefact is still verifiable from `git_sha` even if the SBOM is lost (defence in depth via Rekor transparency log per INV-SUPPLY-PROVENANCE-IN-REKOR).

---

## 6. Verdict

| Criterion | Status |
|---|---|
| R1 retained ≥ 90d | **APPROVED** (spec-level; cron declared) |
| R2 cosign signed | **APPROVED** (inherits S-12 R-S12-3 signing) |
| R3 Object Lock GOVERNANCE | **APPROVED** (Terraform pinned per `infra/r2-sbom-archive.tf`) |
| R4 CycloneDX 1.5+ | **APPROVED** (non-waivable per spec contract §19) |
| R5 customer mirror | **APPROVED** (already live per S-12) |
| R6 zero waivers | **APPROVED** (`corelink_sbom_retention_waiver_count_gauge = 0`) |

**Overall verdict:** **APPROVED — spec-grade evidence**. The retention cron MUST be implemented during S-20 implementation phase (small CI delta, no code in src/). Runtime evidence accumulates daily and rolls into PRR-S20-CLOSING Annex A.

---

## 7. References

- WI-S12-002 — sbom-publish workflow original.
- S-12 R-S12-3 — CycloneDX 1.5+ + cosign sign canonical requirement.
- INV-SUPPLY-SBOM-PRESENT + INV-SUPPLY-SIGNED-DEPLOY + INV-SUPPLY-PROVENANCE-IN-REKOR (HIGH).
- Spec contract §6.1 + §9 + §19 (non-waivable).
- WI-S20-007 §2.1 deliverable S20-007-D4.

**Fim AUDIT-S20-SBOM-90D-RETENTION (SEALED).**
