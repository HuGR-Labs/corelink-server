---
id: "AUDIT-2026-05-27-SPECS-WAVE-C-DUPLICATE-ANALYSIS"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "specs-cleanup", "wave-c", "duplicate-analysis", "seal"]
references:
  - "specs/_audits/2026-05-27-specs-inventory-cleanup-map.md"
---

# Wave C duplicate pair analysis SEAL (read-only)

> Read-only analysis of 2 duplicate-ID pairs flagged in
> `specs/_audits/2026-05-27-specs-inventory-cleanup-map.md` §3.3.
> NO doc modified. NO doc deleted. Human pick required before
> orchestrator dispatches mechanical merger.

---

## §1 RB-FM-SIGNUP-FAILED

### Candidate A: `specs/_runbooks/RB-FM-SIGNUP-FAILED.md` (10 090 B)

**Frontmatter snapshot:**
```yaml
id: RB-FM-SIGNUP-FAILED
type: runbook
doc_status: DRAFT
audit_status: ACTIVE
version: 0.1.0
created: 2026-05-14
updated: 2026-05-14
owner: Gustavo Schneiter
final_approver: Gustavo Schneiter
parent: WI-S19-006
tags: [runbook, p1, onboarding, signup, atomic-provisioning,
       dpa-first, stub, s19]
```

**Body summary (10 sections, 189 lines):**

- `<!-- forensics-backlink -->` to `docs/internal/FORENSICS-GUIDE.md §6`.
- Title declares S-19 ship-gate stub explicitly.
- §1 Failure modes table — 6 sub-modes (network partition, Clerk
  webhook down, D1 unavailable, Stripe outage, DPA-first race, audit
  chain break) each with exact metric trigger.
- §2 Detecção — 6 Prometheus alert names spelled out
  (`corelink_onboarding_atomicity_violation_total`,
  `corelink_onboarding_dpa_first_violation_total`,
  `corelink_onboarding_orphan_tenant_total`, etc.).
- §3 Comunicação — SEV-2/SEV-1 split tied to INV violation flag;
  mandatory Privacy Officer + Legal Counsel + DPO page for DPA
  sub-mode; Slack channel named.
- §4 Mitigação imediata — D1 SQL query for partial-state tenants;
  feature flag halt (`onboarding.signup_enabled=false` via
  config-singleton with dual-approval per CTRL-ADMIN-001); per
  sub-mode mitigation script.
- §5 Diagnóstico — R2 audit chain query, OTel span trace, integrity
  check CLI (`corelink-onboarding-funnel integrity-check --since=60m`).
- §6 Resolução — hot fix per sub-mode table + cold fix list.
- §7 Decision tree (operator) — ASCII art branching tree.
- §8 Post-incident — GDPR Art. 33 72h breach notification window
  explicit.
- §9 Stub scope (S-19 vs S-20) — explicit defer list.
- §10 References — 7 cross-refs incl. RB-GA-CUTOVER §3.6 + §3.11.

### Candidate B: `specs/05_quality/runbooks/RB-FM-SIGNUP-FAILED.md` (6 481 B)

**Frontmatter snapshot:**
```yaml
id: RB-FM-SIGNUP-FAILED
type: runbook
doc_status: DRAFT
audit_status: ACTIVE
version: 0.2.0
created: 2026-04-24
updated: 2026-04-24
owner: Gustavo Schneiter
final_approver: Gustavo Schneiter
tags: [runbook, p2, onboarding, signup, atomicity, consistency]
```

**Body summary (no numbered sections, 175 lines):**

- No forensics backlink.
- Pré-condições section — S-19 pipeline + D1 schema names (`account`,
  `tenant`, `user_account`, `membership`, `pat`, `consent_ledger`).
- Detecção — sinais primários list + concrete D1 SQL detection query
  (uses table names `tenant`, `customer_billing_profile`,
  `consent_ledger`, `pat` joined by `subject_id`) + 5 OTel metric
  names with dotted style (`corelink.onboarding.step_started_total`,
  `corelink.onboarding.orphan_tenants_total`,
  `corelink.onboarding.atomic_rollback_total`).
- Comunicação — SEV-2 default, SEV-1 if > 100 orphan tenants.
- Mitigação imediata (≤ 7 dias) — 3 steps: identify (categorize
  NO_STRIPE / NO_DPA / both missing), per-orphan remediation cases
  A/B/C (with `read_only` degrade pointing to R-S19-5 30d grace),
  audit + reconciliation step (refund clause).
- Diagnóstico — root cause typical % breakdown (Stripe outage 30-40 %,
  browser drop 20-30 %, D1 race 5-10 %, DPA flow bug 10-15 %, DPA
  legal-language mid-flight race).
- Investigação — customer session recording (S-16 if available).
- Resolução — hot fix (cleanup orphans via admin API) + cold fix
  (10k iterations chaos property test, chaos test S-19, DPA
  versioning safe-stop, "Resume signup" UI for browser drop).
- Post-incident — 5-Why mandatory if > 5 orphans/month.
- Evidence — orphan detection query results (pre/post fix).
- References — INV registry §3.12, S10/S11 spec contracts, error
  taxonomy `COR_BILLING_DPA_NOT_SIGNED`, RB-FM-151 (Stripe outage),
  LGPD Art. 7 + GDPR Art. 7 consent legitimacy.

### Recommendation — RB-FM-SIGNUP-FAILED

- **Canonical: Candidate A** (`specs/_runbooks/RB-FM-SIGNUP-FAILED.md`).
- **Reason:**
  1. **Newer authoritative content** — created 2026-05-14 on the
     S-19 ship-gate train (WI-S19-006 closing WI); B is the April
     pre-ship-gate stub that R2 codex review (audit
     `2026-04-24-codex-sota-review-r2.md` §R2-17) flagged as
     under-developed.
  2. **PRR-S19 evidence pack points at A**
     (`specs/04_sprints/_sealed/S19/PRR-S19.md:117` + `:184` both
     cite `specs/_runbooks/RB-FM-SIGNUP-FAILED.md`).
  3. **External consumer points at A** — `RB-GA-CUTOVER.md:541` and
     `BCP-DR-DRILL-CADENCE.md:77` both link
     `specs/_runbooks/RB-FM-SIGNUP-FAILED.md`.
  4. **More operationally usable** — decision tree (§7), explicit
     stub scope vs S-20 defer list, forensics backlink, INV
     callouts in title, severity tied to INV violation (not orphan
     count).
  5. **Schema alignment** — declares `parent: WI-S19-006`; B has no
     parent field.
- **Unique content in loser (B) to merge into A before deletion:**
  1. **Pré-condições block** — explicit D1 schema names
     `customer_billing_profile`, `consent_ledger` (A references the
     concept but not the table names).
  2. **Concrete detection SQL** that joins
     `customer_billing_profile.tenant_id` + `consent_ledger.subject_id`
     (A's SQL targets shorter table names; B's targets the
     S-10/S-11 contract table names — likely more accurate post
     spec landing). Merger should reconcile against the actual S-19
     schema-of-record.
  3. **Root-cause % breakdown** — 30-40 % Stripe outage, 20-30 %
     browser drop, 5-10 % D1 race, 10-15 % DPA-flow bug, rare DPA
     legal-language mid-flight race (A has root cause sub-modes
     but no observed-frequency estimate).
  4. **DPA versioning safe-stop** cold-fix item — "se DPA bumped
     mid-signup, re-prompt customer com version diff" (NOT in A;
     this is a unique behavioural commitment).
  5. **"Resume signup" UI recovery flow** cold-fix — browser-drop
     detected → UI prompt to resume (NOT in A).
  6. **Orphan categorization case-set** NO_STRIPE / NO_DPA /
     BOTH_MISSING with explicit per-case remediation script (A
     groups by sub-mode trigger; B groups by post-hoc data state —
     both useful, merge orthogonally).
  7. **Refund clause** "Refund se billing inadvertent" (A has no
     refund commitment).
  8. **LGPD Art. 7 + GDPR Art. 7 consent-legitimacy refs**
     (A has GDPR Art. 33 only).
  9. **Error-taxonomy cross-ref** `COR_BILLING_DPA_NOT_SIGNED`
     (A does not name the error code).
  10. **5-Why mandatory if > 5 orphans/month** post-incident bar
      (A bars post-mortem on SEV-1 OR INV-violation OR > 5 affected
      tenants — slightly different threshold).
  11. **OTel dotted-metric names** (`corelink.onboarding.*`) — A
      uses Prometheus snake_case (`corelink_onboarding_*`). Per
      INV-OBS-CARDINALITY-BUDGET in S-19 sprint.md the
      snake_case form is the contracted convention; B's dotted
      form is stale and should NOT be merged (informational note).
- **Cross-refs pointing to loser (B):**
  - `specs/_audits/sealed/2026-04-24-codex-sota-review-r2.md:88`
    (calling it out as a stub needing expansion) — **this ref is
    historically accurate and should remain** pointing at the old
    path snapshot since this is a sealed audit.
  - `specs/_audits/sealed/2026-04-24-codex-sota-review-r2.md:384`
    (same — sealed audit, do not rewrite).
- **Broken refs flagged for orchestrator (separate from this pair)** —
  S-19 work item + sprint references to
  `specs/05_runbooks/RB-FM-SIGNUP-FAILED.md` (a THIRD path, not the
  loser path). That directory has no `RB-FM-SIGNUP-FAILED.md`:
  - `specs/04_sprints/_sealed/S19/sprint.md:57`, `:120`
  - `specs/04_sprints/_sealed/S19/work_items/WI-S19-006-...md:71`,
    `:202`, `:285`, `:460`
  - `specs/04_sprints/_sealed/S19/work_items/WI-S19-001-...md:123`
  These should be rewritten to point at A
  (`specs/_runbooks/RB-FM-SIGNUP-FAILED.md`) as part of the
  mechanical-merger pass — the S-19 sprint set is `_sealed/` so
  may require a SEAL-amend explanation.

---

## §2 RB-BYOK-REVOKE

### Candidate A: `specs/05_runbooks/RB-BYOK-REVOKE.md` (6 680 B)

**Frontmatter snapshot:**
```yaml
id: RB-BYOK-REVOKE
type: runbook
wi: WI-S14-006
doc_status: ACTIVE
audit_status: ACTIVE
version: 1.0.0
created: 2026-05-14
updated: 2026-05-14
owner: SRE Lead
final_approver: Gustavo Schneiter
tags: [runbook, byok, kill-switch, cmk-revocation, s14,
       inv-byok-crypto-sovereignty]
```

**Body summary (10 sections, 224 lines):**

- §1 Summary — kill switch ≤ 5 min p99 global; SLA detection ≤ 60s
  + total ≤ 300s; INV-BYOK-CRYPTO-SOVEREIGNTY callout
  (NO operator override / advisory mode, NOT waivable per spec
  contract §19).
- §2 Trigger Conditions — 4-row trigger table including
  `corelink_byok_kill_switch_sla_violation_total > 0` → SEV-1.
- §3 Detection — wrangler D1 raw commands for audit_outbox lookup,
  tenants table status query, Prometheus SLA metric reference.
- §4 Investigation — KMS-status probe table (AWS / GCP / Azure /
  Vault each with one canonical CLI), DEK cache eviction extraction
  via `json_extract(payload, '$.evicted_dek_count')`,
  `customer_alerts` delivery check.
- §5 Customer Communication — 2 templates (revocation +
  recovery), Mustache-style `{{provider}}` interpolation.
- §6 Recovery — auto-detect on next `RevocationDetector` cycle;
  D1 status flip; `corelink.byok.cmk_restored` audit event;
  DEK cache auto-repopulates on next read.
- §7 SLA Violation Response (SEV-1) — 5-step escalation, CRITICAL
  post-mortem ≤ 24h, INV-BYOK-CRYPTO-SOVEREIGNTY review, customer
  breach notification if data accessible post-revoke.
- §8 Escalation Matrix — 5 rows incl. operator-override-in-code
  → immediate security incident.
- §9 Drift Assessment Checklist — quarterly cadence, 6 boxes.
- §10 Change Log — single 1.0.0 row.

### Candidate B: `specs/05_quality/runbooks/RB-BYOK-REVOKE.md` (14 584 B)

**Frontmatter snapshot:**
```yaml
id: RB-BYOK-REVOKE
type: runbook
doc_status: ACTIVE
audit_status: ACTIVE
version: 1.0.0
created: 2026-04-24
updated: 2026-05-14
owner: Gustavo Schneiter
final_approver: Gustavo Schneiter
supersedes: "0.1.0"
tags: [runbook, p1, byok, enterprise, kill-switch,
       production-grade, s14-ship-gate]
```

**Body summary (11 numbered sections + References, 391 lines):**

- Trigger banner — quad-cloud (AWS KMS / GCP KMS / Azure KV /
  HashiCorp Vault) explicit; kill-switch SLA ≤ 6 min p99 (60s
  detection + 5 min DEK cache TTL); intentional customer kill
  switch (NOT internal incident); SEV-2 baseline → SEV-1 if
  customer asserts accidental; CTRL-KEY-011 cross-ref.
- §1 Alert Signatures — 5 Prometheus alerts table + KMS API
  error codes by provider table + audit event names.
- §2 Severity Triage — 4-row table with explicit triage paths
  (intentional rotation / offboarding vs accidental vs potential
  compromise vs SLA breach); PagerDuty policy `byok-kill-switch`;
  Slack `#incidents-corelink`.
- §3 Detection Verification — 4 sub-steps with
  `corelink-admin byok status / kill-switch-timing` CLI calls
  (uses admin-CLI abstraction, NOT raw wrangler).
- §4 Immediate Containment — 5-step auto-flow narrative + 3
  parallel operator verification commands (`dek-cache status`,
  `byok status`, `in-flight-ops`).
- §5 Communication Templates — 3 templates (Slack internal +
  intentional revoke customer email + accidental revoke URGENT
  email with emergency phone-number placeholder).
- §6 Intentional Revoke — Full Procedures — Scenario A (CMK
  rotation, 6 steps incl. `byok rewrap-deks` with dual-approval
  CTRL-KEY-014 + `byok enable-tenant` with CTRL-KEY-014 +
  CTRL-ADMIN-002), Scenario B (Customer offboarding erasure, 6
  steps incl. DSR initiate-erasure type=BYOK_OFFBOARD, 12-backend
  cascade per privacy_model.md §6.2, Ed25519 erasure attestation
  INV-ERASURE-ATTESTATION-SIGNED, 7y archive NIST SP 800-88 Rev.1).
- §7 Accidental Revoke — Recovery Procedures — 6 steps incl.
  `byok force-check` if auto-detect doesn't fire within 2 min,
  end-to-end smoke test, audit-chain verification mandatory,
  accidental-recovery email template.
- §8 Forensics & Post-Incident — 4 sub-sections: evidence
  collection (CloudTrail / GCP Cloud Audit Logs / Azure KV
  audit / Vault audit by provider), SLA verification table (4
  metrics), post-incident report 7-item structure, regulatory
  notification check (GDPR Art. 33 72h, DPA §4 customer
  notification clause).
- §9 Prevention & Verification Cadence — 5-row cadence table
  incl. weekly chaos drill via
  `.github/workflows/byok_kill_switch_drill_weekly.yml` (with
  exact filename) + semi-annual full RB dry-run.
- §10 Controls Cross-Reference — 8-row table mapping
  CTRL-KEY-010..014 + CTRL-PRIV-031 + INV-BYOK-CRYPTO-SOVEREIGNTY
  + INV-ERASURE-ATTESTATION-SIGNED to evidence pointers.
- §11 Dry-Run Record — 2026-05-14 dry-run summary (executor +
  participants + duration + result PASS + p99 2s validated +
  drift findings 0 + full report ref).
- References — 9 cross-refs incl.
  `WI-S14-006` and `WI-S14-009` work items,
  `key_management.md §3.2.1`, `invariant_registry.md §3.12`,
  `scripts/byok_kill_switch_drill.sh`, NIST SP 800-57 Pt 1 Rev 5
  §5.3, NIST SP 800-88 Rev.1.

### Recommendation — RB-BYOK-REVOKE

- **Canonical: Candidate B**
  (`specs/05_quality/runbooks/RB-BYOK-REVOKE.md`).
- **Reason:**
  1. **2.2× larger and substantively more complete** — 391 vs 224
     lines, §1-§11 vs §1-§10, dry-run record §11, controls
     cross-reference §10 (CTRL-KEY-010..014 mapped). Frontmatter
     declares `supersedes: "0.1.0"` — explicit production-grade
     successor.
  2. **PRR-S14 evidence pack pin** — `PRR-S14.md:115` (S-14 Engineer
     line-item evidence), `:148` and `:212` all reference
     `specs/05_quality/runbooks/RB-BYOK-REVOKE.md v1.0.0` as the
     production-grade artefact.
  3. **TT-01 tabletop + dry-run audits land on B**:
     - `specs/_compliance/ir-scenarios/TT-01-data-breach.md:210`
       cites B for semestral dry-run cadence.
     - `specs/_audits/sealed/2026-05-14-s17-tabletop-byok-revoke.md:133`
       says "Primary runbook walked: B".
  4. **PRR-S14 line :115 narrates §1-11 production-grade content
     explicitly** — the §1-11 surface only exists in B. A has 10
     sections.
  5. **Quad-cloud explicit in B title** (AWS / GCP / Azure / Vault)
     and provider-specific evidence collection §8.1; A is
     provider-agnostic in §4.1 only.
- **Unique content in loser (A) to merge into B before deletion:**
  1. **Raw `wrangler d1 execute` commands** in §3.1/§3.2/§4.2/§4.3
     and §6.2 (B uses `corelink-admin` CLI abstraction; A's raw
     wrangler is useful as fallback when admin-CLI is unavailable
     or for staging diagnostics — merge as appendix / fallback
     block).
  2. **`json_extract(payload, '$.evicted_dek_count')` + `$.kill_switch_duration_ms`
     SQL pattern** — extracts SLA timing directly from audit_outbox
     payload (B's §3.4 has `byok kill-switch-timing` CLI but does
     not document the underlying SQL).
  3. **`customer_alerts` table query for delivery confirmation
     (§4.3)** — B does not have an explicit alert-delivery
     verification step.
  4. **`{{provider}}` mustache interpolation in customer email
     template** (B uses literal `<PROVIDER>` placeholder; mustache
     style suggests an existing template-rendering pipeline —
     merger should reconcile against the actual notification
     templating engine to keep the working convention).
  5. **Frontmatter `wi: WI-S14-006`** field — explicit work-item
     linkage. B has no `wi:` field (uses `tags: [s14-ship-gate]`
     + references-only). Schema-of-record check: if `wi:` is a
     supported field, B should adopt it.
  6. **§10 Change Log row format** — A includes an explicit Change
     Log; B has only the `supersedes: "0.1.0"` frontmatter
     declaration. Merger should add a change-log section to B for
     forward versioning.
- **Cross-refs pointing to loser (A):**
  - `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md:178`
  - `specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:138`
  - `specs/04_sprints/S14/sprint.md:125`
  - `specs/04_sprints/S14/work_items/WI-S14-006-cmk-revocation-kill-switch-5min-chaos-drill.md:279`,
    `:588`
  - `specs/_audits/sealed/2026-05-14-rb-byok-revoke-dry-run.md:17`,
    `:53`, `:58` (sealed audit — do NOT rewrite)
  Mechanical rewrite step: rewrite all non-sealed refs (4 lines) to
  point at `specs/05_quality/runbooks/RB-BYOK-REVOKE.md`. Sealed
  audit (`2026-05-14-rb-byok-revoke-dry-run.md`) keeps A path
  reference as a historical snapshot — same handling pattern as
  §1 codex-sota-review-r2.

---

## §3 Next action (human required)

For each pair: confirm canonical pick; then orchestrator dispatches:

1. **Mechanical merger of unique content from loser → canonical**
   per the unique-content list above.
2. **Cross-ref rewrite** for all non-sealed references (paths
   enumerated above).
3. **Delete-after-redirect of loser** (only after merge + rewrite
   PR lands green).
4. **Bonus broken-ref fix** for SIGNUP-FAILED: 6 S-19 sealed-doc
   references to nonexistent `specs/05_runbooks/RB-FM-SIGNUP-FAILED.md`
   path must be rewritten to canonical
   `specs/_runbooks/RB-FM-SIGNUP-FAILED.md`. Since S-19 is sealed,
   this requires SEAL-amend explanation.

Picks summary (decision row, fill in approval):

| Pair | Recommended canonical | Loser to merge-then-delete | Approved? |
|---|---|---|---|
| RB-FM-SIGNUP-FAILED | `specs/_runbooks/RB-FM-SIGNUP-FAILED.md` (A) | `specs/05_quality/runbooks/RB-FM-SIGNUP-FAILED.md` (B) | __ |
| RB-BYOK-REVOKE | `specs/05_quality/runbooks/RB-BYOK-REVOKE.md` (B) | `specs/05_runbooks/RB-BYOK-REVOKE.md` (A) | __ |

---

## §4 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
