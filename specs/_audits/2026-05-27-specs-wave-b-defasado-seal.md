---
id: "AUDIT-2026-05-27-SPECS-WAVE-B-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "specs-cleanup", "wave-b", "defasado", "seal"]
references:
  - "specs/_audits/2026-05-27-specs-inventory-cleanup-map.md"
  - "specs/_audits/2026-05-27-specs-wave-a-archival-seal.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-cas-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-telemetry-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-adapter-host-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-ac-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-billing-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-privacy-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-byok-absorption.md"
---

# Specs cleanup Wave B — defasado triage SEAL

## §1 Scope

Wave B executes the semantic triage prescribed by the 2026-05-27 specs inventory
cleanup map (`specs/_audits/2026-05-27-specs-inventory-cleanup-map.md` §3.2):

- **27 DEFASADO-CODE-DRIFT files** received a top-of-body absorbed-crate → umbrella
  callout linking back to the relevant W35 Phase 2 absorption SEAL.
- **8 DEFASADO-NOT-LANDED files** had their frontmatter flipped to
  `doc_status: "DEFASADO"` and received a §0 historical-record prose block.
- **0 inventory candidates skipped.** All two CHARTER documents flagged as possible
  false positives (`d1-schema-evolution.md`, `tenant-offboarding-spec.md`) were
  verified to use absorbed-crate paths in present tense without any historical
  marker; both received the standard callout.
- The `front_matter.schema.json` `doc_status` enum was extended with `DEFASADO`
  so the validator accepts the new value (validator stays GREEN).

## §2 Results — DEFASADO-CODE-DRIFT (27 callouts applied)

Each file received a top-of-body block of the form:

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-<absorbed>` was absorbed
> into `corelink-<umbrella>` via inline `mod <name>;` per SEAL
> `specs/_audits/sealed/2026-05-26-w35-p2-<umbrella>-absorption.md`. Canonical
> consumer path is now `corelink_<umbrella>::*`.

Frontmatter was bumped `version` minor (e.g. `1.0.0 → 1.1.0`) and `updated → 2026-05-27`.

| # | File | Absorbed crate(s) | Umbrella(s) |
|---|------|-------------------|-------------|
| 1 | `specs/03_architecture/d1-schema-evolution.md` | `d1-migrations` | `ops` |
| 2 | `specs/03_architecture/tenant-offboarding-spec.md` | `admin-dry-run`, `byok-revocation`, `privacy-erasure-worker`, `tenant-offboarding` | `byok`, `ops`, `privacy` |
| 3 | `specs/04_sprints/S08/work_items/WI-S08-003-quota-checker-middleware-atomic-cas.md` | `quota`, `quota-cas` | `billing` |
| 4 | `specs/04_sprints/S08/work_items/WI-S08-004-abuse-detection-heuristica-scoring.md` | `abuse` | `billing` |
| 5 | `specs/04_sprints/S10/work_items/WI-S10-006-replay-forensic-endpoint-role-audit-trail.md` | `billing-replay`, `quota` | `billing` |
| 6 | `specs/04_sprints/S11/work_items/WI-S11-003-consent-ledger-d1-proof-of-informed-symmetric-revoke.md` | `privacy-consent-ledger` | `privacy` |
| 7 | `specs/04_sprints/S11/work_items/WI-S11-007-residency-e2e-custom-domain-routing-property-test-20k.md` | `privacy-residency-enforcement` | `privacy` |
| 8 | `specs/04_sprints/S13/PRR-S13.md` | `admin-api`, `admin-dry-run`, `config-api`, `rollout-controller`, `rotation-worker` | `adapter-host`, `ops` |
| 9 | `specs/04_sprints/S13/work_items/WI-S13-003-secret-rotation-tdk-pat-audit-chain-byok.md` | `rotation-worker` | `ops` |
| 10 | `specs/04_sprints/S13/work_items/WI-S13-005-progressive-rollout-error-budget-auto-rollback.md` | `rollout-controller` | `adapter-host` |
| 11 | `specs/04_sprints/S14/work_items/WI-S14-005-byok-gcp-azure-vault-16-combination-matrix-test.md` | `byok-azure`, `byok-gcp`, `byok-vault` | `byok` |
| 12 | `specs/04_sprints/S14/work_items/WI-S14-006-cmk-revocation-kill-switch-5min-chaos-drill.md` | `byok-revocation`, `customer-alerts` | `byok`, `ops` |
| 13 | `specs/05_quality/runbooks/RB-FM-AC-MIGRATION-BUG.md` | `ac-schema` | `ac` |
| 14 | `specs/_compliance/BCP-DR-DRILL-CADENCE.md` | `dr-drill`, `oncall` | `ops` |
| 15 | `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` | `byok-aws`, `byok-azure`, `byok-gcp`, `byok-vault` | `byok` |
| 16 | `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md` | `privacy-consent-ledger`, `privacy-erasure-worker`, `privacy-residency-enforcement` | `privacy` |
| 17 | `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` | `drata-sync` | `ops` |
| 18 | `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` | `privacy-residency-enforcement` | `privacy` |
| 19 | `specs/_compliance/fips-attestation-letters/LETTER-AWS-KMS.md` | `byok-aws` | `byok` |
| 20 | `specs/_compliance/fips-attestation-letters/LETTER-AZURE-KV.md` | `byok-azure` | `byok` |
| 21 | `specs/_compliance/fips-attestation-letters/LETTER-GCP-KMS.md` | `byok-gcp` | `byok` |
| 22 | `specs/_compliance/fips-attestation-letters/LETTER-VAULT.md` | `byok-vault` | `byok` |
| 23 | `specs/_lighthouse/lighthouse-customer-program.md` | `lighthouse-tracker` | `telemetry` |
| 24 | `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` | `oncall` | `ops` |
| 25 | `specs/_runbooks/RB-LIGHTHOUSE-PHASE-MANAGEMENT.md` | `lighthouse-tracker` | `telemetry` |
| 26 | `specs/_runbooks/RB-ONCALL-POLICY.md` | `oncall`, `synthetic-pager` | `ops`, `telemetry` |
| 27 | `specs/_runbooks/RB-TENANT-OFFBOARDING.md` | `abuse`, `byok-revocation`, `tenant-offboarding`, `webauthn` | `ac`, `billing`, `byok`, `ops` |

### §2.1 CHARTER false-positive verification

The inventory map flagged two CHARTER documents as potential false positives.
Both were re-read end-to-end before edits:

- `specs/03_architecture/d1-schema-evolution.md` references
  `crates/corelink-d1-migrations` in present tense at L46, L104, L149 with no
  historical framing. The crate was confirmed absorbed (no
  `crates/corelink-d1-migrations/` directory in the workspace). → Callout applied.
- `specs/03_architecture/tenant-offboarding-spec.md` references
  `crates/corelink-tenant-offboarding`, `corelink-byok-revocation`,
  `corelink-admin-dry-run`, and `corelink-privacy-erasure-worker` in present
  tense across L27, L36, L104, L181, L289, etc. All four directories absent
  from the workspace. → Callout applied.

Neither charter doc received any body modification beyond the inserted
top-of-body callout, the frontmatter `version` minor bump, and `updated` field.

### §2.2 Partial-update note for RB-ONCALL-POLICY

`specs/_runbooks/RB-ONCALL-POLICY.md` already contained a per-section absorption
note for `corelink-oncall` at L80 ("absorbed Wave 35 P2 from `corelink-oncall`
into `corelink-ops` umbrella"). The file still referenced
`corelink-synthetic-pager` without a historical marker at L248. The Wave B
callout was applied at the top because it documents both absorbed crates
(`oncall` + `synthetic-pager`) under both umbrellas (`ops` + `telemetry`),
superseding the partial L80 note as the authoritative pointer.

## §3 Results — DEFASADO-NOT-LANDED (8 frontmatter flips)

Each file received:

1. Frontmatter `doc_status` updated to `"DEFASADO"` (was `"DRAFT"`).
2. Frontmatter `version` bumped minor (e.g. `0.1.0 → 0.2.0`).
3. Frontmatter `updated: "2026-05-27"`.
4. Top-of-body §0 prose block:

> **DEFASADO 2026-05-27 — never landed.** This proposal/draft was scoped but
> did not advance to implementation. Preserved as historical record; no current
> code references it. See
> `specs/_audits/2026-05-27-specs-inventory-cleanup-map.md` §3.2 for the
> inventory triage decision.

| # | File | Previous `doc_status` |
|---|------|----------------------|
| 1 | `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md` | `DRAFT` |
| 2 | `specs/_proposals/2026-05-16-framework-reviewer-roles.md` | `DRAFT` |
| 3 | `specs/_proposals/adapters/README.md` | `DRAFT` |
| 4 | `specs/_proposals/adapters/brew.md` | `DRAFT` |
| 5 | `specs/_proposals/adapters/cargo.md` | `DRAFT` |
| 6 | `specs/_proposals/adapters/npm.md` | `DRAFT` |
| 7 | `specs/_proposals/adapters/oci.md` | `DRAFT` |
| 8 | `specs/_proposals/adapters/pip.md` | `DRAFT` |

All 8 are under `specs/_proposals/` (the inventory map's classification rule
for NOT-LANDED) and none have associated landed commits, tags, or live
code that would contradict the DEFASADO classification.

## §4 Schema extension

`specs/_schemas/front_matter.schema.json` `doc_status` enum was extended from

```
["DRAFT", "REVIEW", "FROZEN", "THAWED", "SUPERSEDED", "DEPRECATED", "SEALED", "ACTIVE"]
```

to

```
["DRAFT", "REVIEW", "FROZEN", "THAWED", "SUPERSEDED", "DEPRECATED", "SEALED", "ACTIVE", "DEFASADO"]
```

Semantically `DEFASADO` is distinct from `DEPRECATED`: it labels a document that
was once meaningful (proposal / spec referencing crates that existed) but is
now out-of-sync with the codebase in a way that the maintainer chose to
preserve rather than rewrite or supersede. `DEPRECATED` typically implies a
replacement exists; `DEFASADO` does not assert one.

## §5 Acceptance gates

```text
python3 -c "yaml.safe_load(each frontmatter)"       → 35/35 OK
python3 scripts/validate_specs.py                    → ✅ 449 schema + 9 YAML-only (458 total)
python3 scripts/validate_references.py               → ✅ Nenhuma dangling reference
grep -rEn "<<<<<<<|>>>>>>>" --include="*.md" specs/  → only documentation strings inside other audit/seal files; no real conflict markers
```

## §6 Files NOT touched

- Wave A archival map (`specs/_audits/2026-05-27-specs-wave-a-archival-seal.md`) — out of scope.
- Wave C duplicate-consolidation candidates (4 files in `§4.8 DUPLICATE`) — deferred to Wave C.
- SCAFFOLD templates (`§4.9`) — by design.
- REFERENCE / SPRINT-WI-SEALED / CHARTER docs not on the 27+8 inventory list.
- `specs/_runbooks/RB-ONCALL-POLICY.md` L80 in-section absorption note retained
  in place (not removed) to preserve the per-section provenance trail.

## §7 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
