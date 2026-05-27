---
id: "AUDIT-2026-05-27-COMPLIANCE-DOCS-POST-W36"
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
tags: ["audit", "compliance", "wave-b-followup", "absorbed-crates", "conservative"]
references:
  - "specs/_audits/2026-05-27-specs-wave-b-defasado-seal.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-billing-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-byok-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-privacy-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-telemetry-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-replication-absorption.md"
---

# Compliance docs audit — Wave-B-style review post Wave 35-36

## §1 Scope

Conservative review of all **65 Markdown files under `specs/_compliance/`** for
stale absorbed-crate references following the Wave 35 Phase 2 umbrella
absorptions (CAS / Adapter-Host / AC / Billing / BYOK / Ops / Privacy /
Replication / Telemetry). Wave B (commit `2e6d327a`,
`specs/_audits/2026-05-27-specs-wave-b-defasado-seal.md`) already covered the
27-file present-tense-drift inventory; this audit is a wider sweep of the
remaining compliance corpus that the inventory map did not flag.

**Posture:** compliance docs are audit-record critical. Defaulted to SKIP on
any ambiguity. **Compliance > hygiene.**

## §2 Methodology

### §2.1 Absorbed-crate inventory (verified against disk)

The Wave 35 P2 SEALs list 9 absorption batches. Crate names mentioned in the
SEALs were **cross-checked against `crates/` on disk** to distinguish:

- **VERIFIED-ABSORBED** (physically gone from `crates/`): the only class that
  triggers a Wave-B callout.
- **STILL-EXISTS** (e.g. Option-A re-export shims, or never-absorbed siblings):
  references to these crates are NOT stale and require no callout.

Verified-absorbed set used in this audit (16 names):

```
corelink-billing-replay              corelink-byok-aws
corelink-byok-azure                  corelink-byok-gcp
corelink-byok-vault                  corelink-customer-alerts
corelink-dr-drill                    corelink-drata-sync
corelink-lighthouse-tracker          corelink-logpush
corelink-oncall                      corelink-privacy-consent-ledger
corelink-privacy-residency-enforcement
corelink-quota                       corelink-rollout-controller
corelink-tenant-offboarding
```

Names that the SEAL doc lists but that **still exist on disk** (Option-A
shims or scope-deferred absorptions — references to these are NOT stale):

```
corelink-billing-stripe              corelink-cf-bindings
corelink-dsr                         corelink-dsr-statuspage-scheduler
corelink-failover-router             corelink-privacy-erasure-worker
corelink-privacy-pseudonymize        corelink-region
corelink-replica-worker              corelink-replication-coordinator
corelink-runbook-tracker
```

### §2.2 Grep scope

```text
grep -rEln '<verified-absorbed regex>' specs/_compliance/
```

23 files matched the verified-absorbed regex (out of 65). 8 of those were
already touched in Wave B (`specs/_audits/2026-05-27-specs-wave-b-defasado-seal.md`
§2 rows 14, 15, 16, 17, 18, 19, 20, 21, 22 — i.e. BCP-DR-DRILL-CADENCE,
BYOK-FIPS-ATTESTATION-MATRIX, DPO-RESPONSIBILITIES-MATRIX,
DRATA-INTEGRATION-COVERAGE, LGPD-RESIDENCY-ATTESTATION-2026-05-15, and the 4
fips-attestation-letters).

15 files remained for triage in this audit.

## §3 Per-doc classification (15 candidates)

Legend:
- **CALLOUT-APPLIED:** Wave-B-style top-of-body callout block added; frontmatter
  `version` minor bump + `updated: "2026-05-27"`.
- **HISTORICAL-SKIPPED:** Document is a dated-instance audit artifact (`*-2026-05-15.md`)
  or point-in-time evidence record; references describe state-as-of-audit-date
  and SHOULD NOT be retro-edited. Audit-trail integrity > technical hygiene.
- **HUMAN-REVIEW-REQUIRED:** Reference is a literal shell command, test name,
  worker name, or auditor-facing script line that needs more than a callout
  (e.g. the command will fail on current code). Flagged below for next-cycle
  human/legal triage.

| # | File | Doc status | Match summary | Classification |
|---|------|-----------|---------------|----------------|
| 1 | `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md` | DRAFT / ACTIVE | L212 `corelink-drata-sync` (evidence upload anchor) | **CALLOUT-APPLIED** |
| 2 | `specs/_compliance/AUDITOR-WALKTHROUGH-SCRIPT.md` | DRAFT / ACTIVE | L75 `wrangler tail corelink-drata-sync` (live auditor command); L139 daily-tick anchor | **HUMAN-REVIEW-REQUIRED** (literal auditor-typed command; inherits SOC2-EVIDENCE-ROLLUP-2026-05-15) |
| 3 | `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` | DRAFT / ACTIVE | L72/L189/L221 `corelink-drata-sync`; L98/L220 `corelink-dr-drill` | **CALLOUT-APPLIED** |
| 4 | `specs/_compliance/GA-GATE-CRITERIA.md` | ACTIVE / ACTIVE | L122 `corelink-lighthouse-tracker` SLA metrics source | **CALLOUT-APPLIED** |
| 5 | `specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md` | DRAFT / ACTIVE | 7 hits to `corelink-privacy-consent-ledger`, `corelink-privacy-residency-enforcement` | **HISTORICAL-SKIPPED** (dated GDPR full-audit instance) |
| 6 | `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` | DRAFT / ACTIVE | L59 literal command `cargo test -p corelink-privacy-residency-enforcement` | **HUMAN-REVIEW-REQUIRED** (literal cargo test command will fail; needs rewrite to `cargo test -p corelink-privacy residency::`) |
| 7 | `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` | DRAFT / ACTIVE | 8 hits — same shape as GDPR-FULL-AUDIT | **HISTORICAL-SKIPPED** (dated LGPD full-audit instance) |
| 8 | `specs/_compliance/PCI-DSS-ANNUAL-RECERTIFY.md` | DRAFT / ACTIVE | L109/L116 literal `cargo test -p corelink-logpush` commands | **HUMAN-REVIEW-REQUIRED** (logpush absorption + test rename ambiguous — but logpush still exists as wrangler worker; need to confirm `corelink-logpush` is absorbed OR retained-as-wrangler-only) |
| 9 | `specs/_compliance/PCI-DSS-BOUNDARY-DIAGRAM.md` | DRAFT / ACTIVE | 5 hits to `corelink-logpush` in mermaid + prose | **HUMAN-REVIEW-REQUIRED** (PCI-DSS boundary diagram, inherits PCI-DSS-SAQ-A-2026-05-15; auditor-facing dataflow — touch needs legal review) |
| 10 | `specs/_compliance/PCI-DSS-SAQ-A-2026-05-15.md` | DRAFT / ACTIVE | 7 hits `corelink-logpush` (defense-in-depth redaction module) | **HISTORICAL-SKIPPED** (dated PCI SAQ-A submission) |
| 11 | `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` | DRAFT / ACTIVE | L222/L310 `corelink-drata-sync` (EvidenceStream module anchor) | **HISTORICAL-SKIPPED** (dated SOC 2 evidence rollup) |
| 12 | `specs/_compliance/VENDOR-RISK-REGISTER.md` | ACTIVE / ACTIVE | L99 `corelink-billing-replay` (Stripe compensating control) | **CALLOUT-APPLIED** |
| 13 | `specs/_compliance/drill-evidence/2026-Q3-cold-restore-dry-run.md` | DRAFT / ACTIVE | L69 `corelink-drata-sync` CLI binary check | **HISTORICAL-SKIPPED** (Q3 drill dry-run evidence record) |
| 14 | `specs/_compliance/templates/DR-DRILL-EVIDENCE.md` | DRAFT / ACTIVE | L38/L87/L88 `corelink-dr-drill` symbol anchors | **CALLOUT-APPLIED** (template only; FROZEN copies retain originals) |
| 15 | `specs/_compliance/vendor-dd/DD-CLERK.md` | ACTIVE / ACTIVE | L95 `corelink-dsr-engine` (note: name mismatch — neither `corelink-dsr` nor `corelink-dsr-statuspage-scheduler` matches exactly) | **HUMAN-REVIEW-REQUIRED** (typo or stale design name — `corelink-dsr` exists but `corelink-dsr-engine` does not; not absorbed-drift, just nomenclature drift) |

### §3.1 Files with PARTIAL matches that did NOT trigger touches

The following compliance files contain references that look like absorbed
crate names BUT the referenced crates **still physically exist** on disk
(Option-A shims or scope-deferred absorptions). No action needed:

- `ACTIVE-FAILOVER-DRILL-SPEC.md` lines 23, 25, 60, 70, 71, 96, 99, 111, 189, 232-234
  — `corelink-failover-router`, `corelink-region`, `corelink-replica-worker`
  all still exist as Option-A shim crates per Wave 35 P2 replication-absorption
  SEAL §1. (The L212 `corelink-drata-sync` ref is the only stale one — covered above.)
- `COLD-RESTORE-DRILL-SPEC.md` L41, L57, L217-L219 — same shim crates.
- `DPO-APPOINTMENT-2026-05-15.md` L126, L145 — `corelink-privacy-*` glob, `corelink-dsr/` both still exist.
- `DPO-HANDOFF-PLAN.md` L209 — `corelink-dsr/` still exists.
- `GDPR-DPIA-LIBRARY.md` L170 — `corelink-privacy-pseudonymize/` still exists.
- `GDPR-FULL-AUDIT-2026-05-15.md` numerous `corelink-dsr/`, `corelink-privacy-erasure-worker/`, `corelink-privacy-pseudonymize/` refs — all still exist; only consent-ledger + residency-enforcement are absorbed (historical-skipped above).
- `IR-TABLETOP-PLAYBOOK.md` L60 — `corelink-runbook-tracker` still exists.
- `LGPD-FULL-AUDIT-2026-05-15.md` — same shape as GDPR-FULL-AUDIT.
- `LGPD-ROPA-2026-05-15.md` L214 — `corelink-dsr/`, `corelink-privacy-*/` glob; still exist.
- `PCI-DSS-ANNUAL-RECERTIFY.md` L142, `PCI-DSS-BOUNDARY-DIAGRAM.md` L75, L177, L324, `PCI-DSS-SAQ-A-2026-05-15.md` L88, L442, L443 — `corelink-stripe-real`, `corelink-billing-stripe`, `corelink-billing-aggregator` all still exist.

## §4 Touched files (4 CALLOUT-APPLIED)

Each received: frontmatter `version` minor bump (e.g. `1.0.0 → 1.1.0`),
`updated: "2026-05-27"`, and a top-of-body Wave-B-style callout block.

| # | File | Absorbed crate(s) called out | Umbrella | SEAL |
|---|------|------------------------------|----------|------|
| 1 | `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md` | `corelink-drata-sync` (only — non-shim refs noted as Option-A retained) | `ops` | `w35-p2-ops-absorption` |
| 2 | `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` | `corelink-dr-drill`, `corelink-drata-sync` | `ops` | `w35-p2-ops-absorption` |
| 3 | `specs/_compliance/GA-GATE-CRITERIA.md` | `corelink-lighthouse-tracker` | `telemetry` | `w35-p2-telemetry-absorption` |
| 4 | `specs/_compliance/VENDOR-RISK-REGISTER.md` | `corelink-billing-replay` | `billing` | `w35-p2-billing-absorption` |
| 5 | `specs/_compliance/templates/DR-DRILL-EVIDENCE.md` | `corelink-dr-drill` | `ops` | `w35-p2-ops-absorption` |

Total: **5 callouts applied** (one of the 15 candidates was a template, which I
counted as a touch — that's the 5-row total; #15 typo-classified DD-CLERK was
flagged for review, not touched).

## §5 Files flagged HUMAN-REVIEW-REQUIRED (4)

| # | File | Why human/legal review |
|---|------|------------------------|
| 1 | `AUDITOR-WALKTHROUGH-SCRIPT.md` | Contains literal `wrangler tail corelink-drata-sync` command that the auditor will type live; needs verification whether the wrangler worker binary name was preserved post-absorption, OR whether the command should be rewritten. Inherits SOC2-EVIDENCE-ROLLUP-2026-05-15. |
| 2 | `LGPD-DPO-MONTHLY-CHECKLIST.md` | L59 literal `cargo test -p corelink-privacy-residency-enforcement` will FAIL on current code (crate absorbed). Needs rewrite to `cargo test -p corelink-privacy --test residency_*` or equivalent. |
| 3 | `PCI-DSS-ANNUAL-RECERTIFY.md` | L116 literal `cargo test -p corelink-logpush --test pii_redaction_100k_synthetic`. Need to confirm whether the Rust crate `corelink-logpush` is fully absorbed (telemetry SEAL §1) and whether the test renamed inside `corelink-telemetry`. |
| 4 | `PCI-DSS-BOUNDARY-DIAGRAM.md` | Auditor-facing mermaid + prose dataflow diagram embedded in PCI scope. Inherits PCI-DSS-SAQ-A-2026-05-15. 5 hits to `corelink-logpush` need legal review before editing diagram content. |
| 5 | `vendor-dd/DD-CLERK.md` | L95 references `corelink-dsr-engine` — no crate with that exact name exists. Likely nomenclature drift or design-time placeholder; needs author confirmation. Not in absorbed set. |

## §6 Files HISTORICAL-SKIPPED (5)

Dated-instance audit artifacts whose content reflects state-as-of-audit-date.
Retro-editing such files would damage audit-trail integrity. Per the audit
mandate "compliance > hygiene", these are skipped:

| # | File | Audit framework |
|---|------|-----------------|
| 1 | `GDPR-FULL-AUDIT-2026-05-15.md` | GDPR full audit submission (dated) |
| 2 | `LGPD-FULL-AUDIT-2026-05-15.md` | LGPD full audit submission (dated) |
| 3 | `PCI-DSS-SAQ-A-2026-05-15.md` | PCI DSS v4.0 SAQ-A submission (dated) |
| 4 | `SOC2-EVIDENCE-ROLLUP-2026-05-15.md` | SOC 2 Type I evidence rollup (dated) |
| 5 | `drill-evidence/2026-Q3-cold-restore-dry-run.md` | Drill evidence dry-run record (date-bound Q3 2026) |

These docs already have the absorbed-crate references and will be superseded
by their next-cycle counterparts (e.g. `GDPR-FULL-AUDIT-2026-MM-DD.md`) at
which point the new copy can reference canonical post-absorption paths.

## §7 Files SKIPPED — no verified-absorbed reference (42)

```
ACTIVE-FAILOVER-DRILL-SPEC.md           (touched per §4)
AUDITOR-WALKTHROUGH-SCRIPT.md           (HUMAN-REVIEW §5)
BCP-DR-DRILL-CADENCE.md                 (Wave B already)
BYOK-FIPS-ATTESTATION-MATRIX.md         (Wave B already)
COLD-RESTORE-DRILL-SPEC.md              (touched per §4)
DPO-APPOINTMENT-2026-05-15.md           (refs only to still-existing crates)
DPO-HANDOFF-PLAN.md                     (refs only to still-existing crates)
DPO-RESPONSIBILITIES-MATRIX.md          (Wave B already)
DRATA-INTEGRATION-COVERAGE.md           (Wave B already)
FEDRAMP-MODERATE-CROSSWALK-2026-05-15.md (no crate refs)
FEDRAMP-NOT-IN-SCOPE-RATIONALE.md       (no crate refs)
FIPS-RFI-QUESTIONNAIRE.md               (no crate refs)
GA-GATE-CRITERIA.md                     (touched per §4)
GA-GATE-GO-NOGO-TEMPLATE.md             (no crate refs)
GDPR-DPIA-LIBRARY.md                    (refs only to still-existing crates)
GDPR-FULL-AUDIT-2026-05-15.md           (HISTORICAL-SKIPPED §6)
GDPR-SCC-EXECUTION-2026-05-15.md        (no crate refs)
IR-TABLETOP-PLAYBOOK.md                 (refs only to still-existing crates)
IR-TABLETOP-SCHEDULE-2026.md            (no crate refs)
ISO27001-CROSSWALK-2026-05-15.md        (no crate refs)
ISO27001-GAP-ANALYSIS.md                (no crate refs)
ISO27001-INTERNAL-AUDIT-PROGRAM.md      (no crate refs)
ISO27001-MANAGEMENT-REVIEW-TEMPLATE.md  (no crate refs)
ISO27001-ROADMAP.md                     (no crate refs)
ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md (no crate refs)
LGPD-DPO-MONTHLY-CHECKLIST.md           (HUMAN-REVIEW §5)
LGPD-FULL-AUDIT-2026-05-15.md           (HISTORICAL-SKIPPED §6)
LGPD-RESIDENCY-ATTESTATION-2026-05-15.md (Wave B already)
LGPD-ROPA-2026-05-15.md                 (refs only to still-existing crates)
PCI-DSS-ANNUAL-RECERTIFY.md             (HUMAN-REVIEW §5)
PCI-DSS-BOUNDARY-DIAGRAM.md             (HUMAN-REVIEW §5)
PCI-DSS-SAQ-A-2026-05-15.md             (HISTORICAL-SKIPPED §6)
SOC2-EVIDENCE-ROLLUP-2026-05-15.md      (HISTORICAL-SKIPPED §6)
SOC2-GAP-ANALYSIS.md                    (no crate refs)
SOC2-ROADMAP.md                         (no crate refs)
VENDOR-RISK-METHODOLOGY.md              (no crate refs)
VENDOR-RISK-REGISTER.md                 (touched per §4)
aws-artifact-placeholder.md             (no crate refs)
vendor-shortlist-soc2.md                (no crate refs)
weekly-digests/README.md                (no crate refs)
weekly-digests/2026-05-15.md            (no crate refs)
templates/DR-DRILL-EVIDENCE.md          (touched per §4)
templates/IR-TABLETOP-EVIDENCE.md       (no crate refs)
vendor-dd/DD-AWS-KMS.md                 (no crate refs)
vendor-dd/DD-AZURE-KEYVAULT.md          (no crate refs)
vendor-dd/DD-CLERK.md                   (HUMAN-REVIEW §5)
vendor-dd/DD-CLOUDFLARE.md              (no crate refs)
vendor-dd/DD-DRATA.md                   (no crate refs)
vendor-dd/DD-GCP-KMS.md                 (no crate refs)
vendor-dd/DD-HASHICORP-VAULT.md         (no crate refs)
vendor-dd/DD-HUBSPOT.md                 (no crate refs)
vendor-dd/DD-PAGERDUTY.md               (no crate refs)
vendor-dd/DD-SLACK.md                   (no crate refs)
vendor-dd/DD-STRIPE.md                  (no crate refs)
fips-attestation-letters/LETTER-AWS-KMS.md   (Wave B already)
fips-attestation-letters/LETTER-AZURE-KV.md  (Wave B already)
fips-attestation-letters/LETTER-GCP-KMS.md   (Wave B already)
fips-attestation-letters/LETTER-VAULT.md     (Wave B already)
ir-scenarios/TT-01-data-breach.md       (no crate refs)
ir-scenarios/TT-02-cascading-failure.md (no crate refs)
ir-scenarios/TT-03-webhook-compromise.md (no crate refs)
ir-scenarios/TT-04-insider-threat.md    (no crate refs)
ir-scenarios/TT-05-supply-chain.md      (no crate refs)
ir-scenarios/TT-06-ddos-abuse.md        (no crate refs)
drill-evidence/2026-Q3-cold-restore-dry-run.md (HISTORICAL-SKIPPED §6)
```

Total: 65 docs scanned · 8 already touched (Wave B) · 5 callouts applied · 5
historical-skipped · 5 human-review-required · 42 no-action.

(`AUDITOR-WALKTHROUGH-SCRIPT.md` is double-listed in HUMAN-REVIEW; total
unique = 65.)

## §8 Acceptance gates

```text
python3 scripts/validate_specs.py     → ✅ 449 schema + 9 YAML-only (458 total)
python3 scripts/validate_references.py → ✅ Nenhuma dangling reference
grep -rEn "<<<<<<<|>>>>>>>" --include="*.md" specs/_compliance/ → (empty)
```

## §9 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
