---
id: "AUDIT-2026-05-27-RUNBOOK-AUDIT-POST-W36"
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
tags: ["audit", "runbook-audit", "post-w36", "wave-b-defasado", "absorbed-crate", "callout", "seal"]
references:
  - "specs/_audits/2026-05-27-specs-wave-b-defasado-seal.md"
  - "specs/_audits/2026-05-27-specs-wave-a-archival-seal.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-telemetry-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-billing-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-privacy-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md"
  - "specs/_audits/sealed/2026-05-26-w35-p2-byok-absorption.md"
---

# Runbook audit — Wave-B-style callouts for absorbed-crate references (post Wave 33-36)

## §1 Scope + rationale

Wave A archival sealed audits + sprint contracts; Wave B applied
absorbed-crate → umbrella callouts to 27 DEFASADO-CODE-DRIFT files +
flipped 8 NOT-LANDED proposals to `DEFASADO`. The
`specs/_runbooks/` tree was deliberately left in-place by Wave A
(REFERENCE, live operational docs), so Wave B only touched 4 of the 55
runbooks that appeared on its inventory cleanup map (rows 24-27 of
the Wave B table):

- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`
- `specs/_runbooks/RB-LIGHTHOUSE-PHASE-MANAGEMENT.md`
- `specs/_runbooks/RB-ONCALL-POLICY.md`
- `specs/_runbooks/RB-TENANT-OFFBOARDING.md`

This audit closes the remaining drift surface across all 55 runbooks
by scanning for residual absorbed-crate references that Wave B's
inventory pass didn't enumerate, classifying each as present-tense
canonical (callout applied) vs historical/operational identifier
(skip), and applying the Wave-B-style callout where applicable.

The acceptance gate is unchanged: `validate_specs.py` GREEN, no git
conflict markers, no touching of OPERATIONAL CONTENT (steps,
commands, escalation paths), only the callout + frontmatter version
+ updated date.

## §2 Method

Per the Wave B template (`specs/_audits/2026-05-27-specs-wave-b-defasado-seal.md` §2):

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-<absorbed>` was absorbed
> into `corelink-<umbrella>` via inline `mod <name>;` per SEAL
> `specs/_audits/sealed/2026-05-26-w35-p2-<umbrella>-absorption.md`. Canonical
> consumer path is now `corelink_<umbrella>::*`.

Each touched runbook received the callout as a single blockquote
inserted directly after the closing frontmatter `---` (above any
pre-existing `<!-- forensics-backlink -->` block or first `#`
heading), plus a frontmatter `version: 1.0.0 → 1.1.0` minor bump
and `updated: 2026-05-27`.

Absorbed-crate → umbrella mapping (per W35 P2 absorption SEAL chain):

| Absorbed crate | Umbrella | Canonical mod path |
|---|---|---|
| `corelink-lighthouse-tracker` | `corelink-telemetry` | `corelink_telemetry::lighthouse::*` |
| `corelink-synthetic-pager` | `corelink-telemetry` | `corelink_telemetry::synthetic_pager::*` |
| `corelink-abuse` | `corelink-billing` | `corelink_billing::abuse::*` |
| `corelink-survey` | `corelink-ops` | `corelink_ops::survey::*` |
| `corelink-privacy-consent-ledger` | `corelink-privacy` | `corelink_privacy::consent::*` |
| `corelink-privacy-residency-enforcement` | `corelink-privacy` | `corelink_privacy::residency::*` |
| `corelink-privacy-sub-processor-emit` | `corelink-privacy` | `corelink_privacy::sub_processor::*` |
| `corelink-backup-verify` | `corelink-ops` | `corelink_ops::dr::backup_verify::*` |
| `corelink-d1-migrations` | `corelink-ops` | `corelink_ops::migrations::*` |
| `corelink-drata-sync` | `corelink-ops` | `corelink_ops::drata::*` |
| `corelink-byok-aws` | `corelink-byok` | `corelink_byok::aws::*` (feature-gated) |
| `corelink-byok-gcp` | `corelink-byok` | `corelink_byok::gcp::*` (feature-gated) |

## §3 Results — 14 runbooks touched (callout + frontmatter bump)

| # | File | Absorbed crate(s) | Umbrella(s) | Rationale |
|---|------|-------------------|-------------|-----------|
| 1 | `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` | `lighthouse-tracker` | `telemetry` | L44 cites `corelink-lighthouse-tracker` state-machine as canonical T1-4 trigger source |
| 2 | `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` | `lighthouse-tracker` | `telemetry` | L154 cites `corelink-lighthouse-tracker::LifecycleState::allowed_next` as canonical state-machine API; L174 cites `crates/corelink-lighthouse-tracker/src/lib.rs` as canonical crate path |
| 3 | `specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md` | `synthetic-pager` | `telemetry` | L31, L71, L122, L145 all cite `corelink-synthetic-pager` / `corelink_synthetic_pager::Region` / `corelink-synthetic-pager-cli` in present tense as canonical paths |
| 4 | `specs/_runbooks/RB-SURVEY-ABUSE.md` | `abuse`, `survey` | `billing`, `ops` | L20, L34, L41, L60, L77, L132, L172, L193 cite `corelink-survey` and `corelink-abuse` APIs (`token::decode_and_verify`, `recorder`, `sanitize_free_text`) in present tense as canonical entry points |
| 5 | `specs/_runbooks/RB-DSR-GDPR.md` | `privacy-consent-ledger`, `privacy-residency-enforcement` | `privacy` | L356, L359 cite `crates/corelink-privacy-consent-ledger/` and `crates/corelink-privacy-residency-enforcement/` as canonical crate paths in §legal-hold + §references |
| 6 | `specs/_runbooks/RB-DSR-LGPD-FULL.md` | `privacy-consent-ledger`, `privacy-residency-enforcement` | `privacy` | L164, L265, L415, L418 cite same crates including `crates/corelink-privacy-residency-enforcement/src/migration.rs` as canonical legal-hold flag location |
| 7 | `specs/_runbooks/RB-DPO-ESCALATION.md` | `privacy-residency-enforcement` | `privacy` | L113, L139 cite `crates/corelink-privacy-residency-enforcement/` config as canonical DPO veto-trigger and residency-attestation source |
| 8 | `specs/_runbooks/RB-SUBPROCESSOR-CHANGE.md` | `privacy-sub-processor-emit` | `privacy` | L119 cites consumer relationship; L191 cites `crates/corelink-privacy-sub-processor-emit/` as canonical emitter crate path |
| 9 | `specs/_runbooks/RB-CANONICAL-DRIFT.md` | `backup-verify` | `ops` | L81 §2 BACKUP-domain worked example cites `corelink-backup-verify/src/lib.rs` as canonical INV-FRESH discovery site; runbook readers parse this as present-tense procedure illustration |
| 10 | `specs/_runbooks/RB-BACKUP-VERIFICATION-FAILURE.md` | `backup-verify` | `ops` | L160 cites `crates/corelink-backup-verify` as canonical verification-trait + in-memory-fake crate path |
| 11 | `specs/_runbooks/RB-D1-MIGRATION-APPLY.md` | `d1-migrations` | `ops` | L41 cites `cargo test -p corelink-d1-migrations --test d1_migration_integration` as canonical migration-replay test command; L224 cites `crates/corelink-d1-migrations/` as canonical Rust replay harness |
| 12 | `specs/_runbooks/RB-DRATA-SYNC-FAILURE.md` | `drata-sync` | `ops` | L96 cites `crates/corelink-drata-sync/src/record.rs` as canonical `EvidenceRecord` schema source; L162 cites crate as canonical implementation |
| 13 | `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md` | `drata-sync` | `ops` | L492 cites `corelink-drata-sync upload` CLI as canonical evidence-upload invocation; L565 cites crate as canonical CLI source |
| 14 | `specs/_runbooks/RB-SYSTEM-CMK-ROTATION.md` | `byok-aws`, `byok-gcp` | `byok` | L31 cites both providers in present tense as canonical KMS-rotation surfaces; L171-L172 cite `crates/corelink-byok-aws/src/lib.rs` (R2-6) and `crates/corelink-byok-gcp/src/lib.rs` (R2-7) as canonical crate paths |

All 14 received:
1. Frontmatter `version: "1.0.0" → "1.1.0"` minor bump.
2. Frontmatter `updated: "<old>" → "2026-05-27"`.
3. Single blockquote callout inserted after frontmatter, before any
   pre-existing `<!-- forensics-backlink -->` or first `#` heading.

No body modifications beyond the inserted callout. No operational
content (steps, commands, escalation paths, decision matrices) was
touched.

## §4 Results — 4 runbooks reviewed + skipped (historical/operational identifiers, not canonical code refs)

| # | File | Match | Reason for skip |
|---|------|-------|-----------------|
| 1 | `specs/_runbooks/RB-TERRAFORM-DRIFT.md` | L163 `PagerDuty corelink-oncall` | `corelink-oncall` here is the PagerDuty schedule NAME passed to the paging system, not a Rust crate identifier. The schedule name is an operational identifier whose stability contract is independent of the code reorg. |
| 2 | `specs/_runbooks/RB-ENDURANCE-24H-DRILL.md` | L123 `#corelink-oncall` | Slack channel name (note the leading `#`). Slack channel names are user-facing identifiers, not code refs. |
| 3 | `specs/_runbooks/RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT.md` | L165 `--schedule corelink-oncall-tier-3` | PagerDuty schedule identifier passed to the `corelink oncall page` CLI flag. Same rationale as §4.1. |
| 4 | `specs/_runbooks/RB-REPLICA-FAILOVER.md` | L422 `corelink_dr_drill_*` Prometheus gauges | Prometheus metric name prefix per WI-S17-008 §19. Metric names are a stability-contract surface preserved across the W35 absorption (the absorbed crate published these metrics; the umbrella preserves them verbatim). The reader is looking up a metric, not a code path. |

These 4 were re-read end-to-end before the SKIP decision; in every
case the reference is an operational identifier (PagerDuty schedule,
Slack channel, Prometheus metric name) whose persistence is contract,
not drift. No callout would clarify anything for the runbook reader;
the umbrella mapping is irrelevant to the operational action.

## §5 Runbooks already covered by Wave B (no double-touch)

Per `specs/_audits/2026-05-27-specs-wave-b-defasado-seal.md` §2 rows 24-27, these 4 runbooks already carry the Wave-B callout + `version: "1.1.0"` + `updated: "2026-05-27"`:

- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`
- `specs/_runbooks/RB-LIGHTHOUSE-PHASE-MANAGEMENT.md`
- `specs/_runbooks/RB-ONCALL-POLICY.md`
- `specs/_runbooks/RB-TENANT-OFFBOARDING.md`

NOT touched by this audit.

## §6 Runbooks with no absorbed-crate matches (37 of 55)

The remaining 37 of the 55 runbooks under `specs/_runbooks/` contain
no `corelink-<absorbed>` or `corelink_<absorbed>` substring per the
absorbed-crate list in §3. They were not opened for inspection beyond
the grep pass.

## §7 Acceptance gates

```text
$ python3 scripts/validate_specs.py
✅ Todos validados: 449 com schema completo, 9 com YAML only (458 total).

$ grep -rEn "<<<<<<<|>>>>>>>" --include="*.md" specs/_runbooks/
(no output — no real conflict markers)

$ for absorbed in chunker canary oncall webauthn quota byok-aws abuse \
                  lighthouse-tracker synthetic-pager backup-verify \
                  d1-migrations drata-sync dr-drill privacy-consent-ledger \
                  privacy-residency-enforcement privacy-sub-processor-emit \
                  survey byok-gcp; do
  count=$(grep -rln "corelink-${absorbed}" specs/_runbooks/ | wc -l)
  echo "$absorbed: $count"
done
chunker: 0     canary: 0      oncall: 5    webauthn: 1
quota: 0       byok-aws: 1    abuse: 2     lighthouse-tracker: 3
synthetic-pager: 2  backup-verify: 2  d1-migrations: 1
drata-sync: 2  dr-drill: 0    privacy-consent-ledger: 2
privacy-residency-enforcement: 3  privacy-sub-processor-emit: 1
survey: 1      byok-gcp: 1
```

Residual hit-count interpretation: every count > 0 is a runbook whose
body still cites the absorbed crate name in present tense — AND now
carries the top-of-body callout disambiguating "use the umbrella for
new automation; this name persists for HISTORICAL log/grep
reference". The `oncall: 5` and `webauthn: 1` counts include the 4
runbooks already updated by Wave B (`ONCALL-ESCALATION-MATRIX.md`,
`RB-ONCALL-POLICY.md`, `RB-TENANT-OFFBOARDING.md`) plus the 3
operational-identifier files documented in §4 (PagerDuty schedules /
Slack channels), neither of which is drift requiring further action.

## §8 Files NOT touched

- 4 Wave-B-already-covered runbooks (§5).
- 4 operational-identifier skip-list runbooks (§4).
- 37 runbooks with zero absorbed-crate substring matches (§6).
- All operational content (steps, commands, escalation paths,
  decision matrices, severity floors, paging conventions) across
  every touched runbook.

## §9 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
