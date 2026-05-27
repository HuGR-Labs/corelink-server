---
id: "AUDIT-2026-05-27-SPECS-WAVE-C-MERGER-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "specs-cleanup", "wave-c", "duplicate-merger", "seal"]
references:
  - "specs/_audits/2026-05-27-specs-wave-c-duplicate-analysis.md"
  - "specs/_audits/2026-05-27-specs-inventory-cleanup-map.md"
---

# Wave C duplicate-merger SEAL

> Mechanical execution of the 2 duplicate-pair mergers approved by
> user on 2026-05-27 per
> `specs/_audits/2026-05-27-specs-wave-c-duplicate-analysis.md`
> (commit `a2062424` in main).

---

## §1 Scope

User-confirmed canonicals (decision row from analysis §3 table):

| Pair | Canonical (kept) | Loser (deleted) |
|---|---|---|
| RB-FM-SIGNUP-FAILED | `specs/_runbooks/RB-FM-SIGNUP-FAILED.md` (Candidate A) | `specs/05_quality/runbooks/RB-FM-SIGNUP-FAILED.md` (Candidate B) |
| RB-BYOK-REVOKE | `specs/05_quality/runbooks/RB-BYOK-REVOKE.md` (Candidate B) | `specs/05_runbooks/RB-BYOK-REVOKE.md` (Candidate A) |

---

## §2 RB-FM-SIGNUP-FAILED — merger details

### §2.1 Canonical version bump

`specs/_runbooks/RB-FM-SIGNUP-FAILED.md`:
- `version: "0.1.0"` → `version: "0.2.0"`
- `updated: "2026-05-14"` → `updated: "2026-05-27"`

### §2.2 Unique content merged from loser → canonical

Per analysis §1 "Unique content in loser (B) to merge into A" — 11 items
enumerated. Item 11 (OTel dotted-metric names) was flagged informational
"should NOT be merged" by the analysis (Prometheus snake_case is the
INV-OBS-CARDINALITY-BUDGET contract); not merged.

| # | Item | Inserted at |
|---|---|---|
| 1 | Pré-condições block with D1 schema names (`account`, `tenant`, `user_account`, `membership`, `pat`, `consent_ledger`, `customer_billing_profile`) | new §0 before §1 |
| 2 | Concrete detection SQL joining `customer_billing_profile.tenant_id` + `consent_ledger.subject_id` | §4.1 "Alternate query" block |
| 3 | Root-cause % breakdown table (Stripe outage 30-40 %, browser drop 20-30 %, DPA flow bug 10-15 %, D1 race 5-10 %, DPA legal-language mid-flight race) | §5 "Observed root-cause frequency" table |
| 4 | DPA versioning safe-stop cold-fix | §6 Cold fix bullet list |
| 5 | "Resume signup" UI recovery flow cold-fix | §6 Cold fix bullet list |
| 6 | Orphan categorization case-set NO_STRIPE / NO_DPA / BOTH_MISSING | §4.1 categorize bullet block |
| 7 | Refund clause ("Refund se billing inadvertent") | §4.5 new step |
| 8 | LGPD Art. 7 + GDPR Art. 7 consent-legitimacy refs | §10 References |
| 9 | Error-taxonomy cross-ref `COR_BILLING_DPA_NOT_SIGNED` | §10 References |
| 10 | 5-Why mandatory if > 5 orphans/month | §8 Post-incident bullet list (cumulative trigger, complementary to existing per-incident bar) |
| 11 | OTel dotted-metric names (`corelink.onboarding.*`) | **NOT MERGED** — analysis flagged as stale (Prometheus snake_case is INV-OBS-CARDINALITY-BUDGET contract) |

Bonus: added `specs/05_quality/runbooks/RB-FM-151-stripe-outage.md`
cross-ref in §10 (the loser referenced `RB-FM-151` informally — promoted
to explicit path on merge).

---

## §3 RB-BYOK-REVOKE — merger details

### §3.1 Canonical version bump

`specs/05_quality/runbooks/RB-BYOK-REVOKE.md`:
- `version: "1.0.0"` → `version: "1.1.0"`
- `updated: "2026-05-14"` → `updated: "2026-05-27"`
- Added frontmatter `wi: "WI-S14-006"` (loser had this field; canonical did not)

### §3.2 Unique content merged from loser → canonical

Per analysis §2 "Unique content in loser (A) to merge into B" — 6 items
enumerated. All 6 merged.

| # | Item | Inserted at |
|---|---|---|
| 1 | Raw `wrangler d1 execute` commands (audit_outbox lookup, tenants byok_status query, recovery verify) | new Appendix A (fallback when `corelink-admin` unavailable) |
| 2 | `json_extract(payload, '$.evicted_dek_count')` + `$.kill_switch_duration_ms` SQL pattern | §3.4 "Underlying SQL pattern" block |
| 3 | `customer_alerts` table delivery query | new §3.5 "Verify customer alert delivery" |
| 4 | `{{provider}}` Mustache placeholder convention note | §5 intro callout (acknowledges both `<PLACEHOLDER>` + `{{var}}` styles) |
| 5 | Frontmatter `wi: WI-S14-006` field | frontmatter |
| 6 | §10 Change Log section | new top-level "Change Log" section at end |

---

## §4 Cross-ref updates

### §4.1 Broken S-19 refs (bonus fix per analysis §1 "Broken refs flagged for orchestrator")

6 references pointed at the nonexistent path
`specs/05_runbooks/RB-FM-SIGNUP-FAILED.md` (a third path that never
existed). All rewritten to canonical `specs/_runbooks/RB-FM-SIGNUP-FAILED.md`:

| File | Lines (pre-rewrite) |
|---|---|
| `specs/04_sprints/_sealed/S19/sprint.md` | :57, :120 |
| `specs/04_sprints/_sealed/S19/work_items/WI-S19-006-...md` | :71, :202, :285, :460 |
| `specs/04_sprints/_sealed/S19/work_items/WI-S19-001-...md` | :123 |

(Total: 7 ref-site rewrites across 3 files — analysis quoted 6
references; actual count was 7 after `grep -n` enumeration.)

The S-19 directory is named `_sealed/` but the affected docs carry
`doc_status: DRAFT` (sprint.md, WI-S19-001) or `doc_status: SEALED`
(WI-S19-006). Per mandate §2 Step 2 "Fix them" — mechanical rewrite to
canonical path applied; this audit doc is the historical record of the
pre-rewrite state.

### §4.2 BYOK-REVOKE non-sealed refs

Per analysis §2 "Cross-refs pointing to loser (A)" — 4 non-sealed
ref-sites rewritten from `specs/05_runbooks/RB-BYOK-REVOKE.md` →
`specs/05_quality/runbooks/RB-BYOK-REVOKE.md`:

| File | Line (pre-rewrite) |
|---|---|
| `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` | :180 |
| `specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md` | :138 |
| `specs/04_sprints/S14/sprint.md` | :125 |
| `specs/04_sprints/S14/work_items/WI-S14-006-cmk-revocation-kill-switch-5min-chaos-drill.md` | :279, :588 |

(Total: 5 ref-sites across 4 files.)

### §4.3 Sealed audit refs NOT rewritten (per analysis directive)

These keep their loser-path references as historical snapshots:

- `specs/_audits/sealed/2026-04-24-codex-sota-review-r2.md:88`, `:384` —
  references loser RB-FM-SIGNUP-FAILED (B) as the stub being critiqued.
- `specs/_audits/sealed/2026-05-14-rb-byok-revoke-dry-run.md:17`,
  `:53`, `:58` — references loser RB-BYOK-REVOKE (A) as the dry-run
  target at execution time.
- `specs/_audits/2026-05-27-specs-wave-c-duplicate-analysis.md` (the
  analysis itself) — references both losers in its body as analysis
  subjects.
- `specs/_audits/2026-05-27-specs-inventory-cleanup-map.md` — same
  rationale (inventory snapshot at time of writing).

---

## §5 Loser deletions

```bash
git rm specs/05_quality/runbooks/RB-FM-SIGNUP-FAILED.md
git rm specs/05_runbooks/RB-BYOK-REVOKE.md
```

NO redirect stubs created (per mandate §2 Step 3); this SEAL audit +
the analysis audit serve as the historical record.

---

## §6 Acceptance

| Check | Result |
|---|---|
| `python3 scripts/validate_specs.py` | `✅ Todos validados: 447 com schema completo, 9 com YAML only (456 total).` (was 458 → 456 after 2 deletions) |
| `python3 scripts/validate_references.py` | `✅ Nenhuma dangling reference detectada.` |
| `grep -rln <loser-path-A>` | only `_audits/sealed/2026-04-24-codex-sota-review-r2.md` (historical snapshot — keep per analysis) + this SEAL + analysis audit |
| `grep -rln <loser-path-B>` | only `_audits/sealed/2026-05-14-rb-byok-revoke-dry-run.md` (historical snapshot — keep) + inventory map + analysis audit |
| `grep -rEn "<<<<<<<\|>>>>>>>" --include="*.md" specs/` | clean (only string-mentions inside other audit docs documenting the absence of conflict markers) |
| `grep -rln "05_runbooks/RB-FM-SIGNUP-FAILED" specs/` | only this audit + analysis audit (no broken refs remain) |

---

## §7 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
