---
title: "Wave-30 adversarial review of wave-29 streams"
date: 2026-05-16
wave: 30
stream: review
reviewer: r-prep-wave29-adversarial-review
base_commit: 04f2dff
base_main_anchor: 365dd38
status: PASS
score: 8.70
score_formula: "10 - 1.5*P0 - 0.5*P1 - 0.15*P2 - 0.05*P3"
findings:
  P0: 0
  P1: 1
  P2: 3
  P3: 7
references:
  - specs/_audits/sealed/2026-05-16-wave29-closure.md
  - specs/_audits/sealed/2026-05-16-signup-landing-page.md
  - specs/_audits/sealed/2026-05-16-pricing-page.md
  - specs/_audits/sealed/2026-05-16-trust-center-consolidation.md
  - specs/_audits/sealed/2026-05-16-audit-chain-viz-ui.md
  - specs/_audits/sealed/2026-05-16-perf-baseline-ga-freeze.md
---

# Wave-30 adversarial review — wave-29 streams (`365dd38..04f2dff`)

## 0. TL;DR

Wave-29 lands 10 streams + 1 cross-stream JSON-union merge atop main
`365dd38` (wave-28 SEAL). The product surface gains four customer-facing
pages (signup pilot apply/welcome, pricing + calculator, trust center,
audit-chain visualization), one backend route family
(`POST /v1/signup/pilot/:token` + 3 admin routes), the
`ShadowSinkFactory` PRELUDE adoption that removes one D1 round-trip from
the audit-analytics hot path, and a frozen pre-GA perf baseline
(`perf-baseline-ga-2026-05-16`).

**Verdict: PASS at 8.70 / 10.0** (SOTA 8.5 threshold). One P1 finding
(Pro tier publicly displayed despite canonical `PRICING-WORKSHEET.md`
marking it "internal SKU, not yet launched"); three P2 findings; seven
P3s. No P0. Sanity (`cargo check -p corelink-server --tests`,
`validate_specs.py`, `validate_references.py`) green on the GA tip.

## 1. Methodology & scope

Review-only worktree at `04f2dff`. Eight focus areas adversarially
challenged against canonical sources of truth
(`marketing/sales/PRICING-WORKSHEET.md`, wave-21 INV registry,
wave-19 audit-analytics SOTs, wave-22 perf manifest, axum 0.7 matchit
0.7 routing pin). Score formula clamped to [0, 10]:

```
score = 10 − 1.5·P0 − 0.5·P1 − 0.15·P2 − 0.05·P3
      = 10 − 0       − 0.5·1  − 0.15·3   − 0.05·7
      = 10 − 0.50    − 0.45    − 0.35
      = 8.70
```

**Final score: 8.70 / 10.0 — PASS** (≥ 8.5 SOTA bar).

## 2. Per-focus-area verdicts

### 2.1 i18n `code.json` UNION across 4 locales — **PASS**

Three wave-29 streams (signup-landing, audit-chain-viz, trust-center)
each shipped a `code.json` delta across `en-US / pt-BR / de / es-419`.
The final merge state on `04f2dff` resolved the four-way union via
`python3 -m json.dump`. Verified:

- All four locale files parse as valid JSON.
- All four contain **66 keys exactly** (`wc -l = 266` each).
- Key set parity: `pt-BR / de / es-419` each report
  `missing=0, extra=0` against `en-US`.
- Translations are semantically distinct (only **1 / 66** `pt-BR`
  message is character-identical to `en-US`; `de` and `es-419` =
  **0 / 66**). The union did NOT collapse to an EN baseline.

**Docusaurus key lookup:** Docusaurus reads each `code.json` per
active locale at build time; key ordering inside the JSON is not
load-bearing (lookup is dict-keyed). `pnpm build` was already verified
green by the originating streams on Node 22 across 4 locales (see
each stream's audit doc §Quality gates). No regression risk from
the `json.dump` ordering.

### 2.2 Audit-chain viz recovery (`1f48ad3`) — **PASS** (with P2)

The recovery commit `1f48ad3` re-introduces
`apps/docs/src/pages/customer/audit-chain.{tsx,module.css}` (549 + 240
LOC), the four i18n deltas, and the `WI-S09-008 §14` closure note. The
audit doc `specs/_audits/sealed/2026-05-16-audit-chain-viz-ui.md` is present
and consistent (front-matter `status: SHIPPED`, base_commit `365dd38`,
references resolve).

The brief flagged that the recovery happened "BEFORE the original
agent emitted final SEAL". The resulting tree on `04f2dff`:

- `audit-chain.tsx` exists with the documented endpoints
  (`/v1/audit/analytics/event-count`, `/v1/audit/analytics/timeline`,
  `/v1/audit/export`).
- `WI-S09-008 §14` mentions the BLAKE3 chain-head anchor,
  pilot-data banner, and Clerk JWT contract — all consistent with
  wave-19 commits (`3d835cb`, `7ec5435`).

**P2-AUDITVIZ-01:** the audit doc lists the `WI-S09-008` closure note
as a deliverable, but the WI itself does not declare a wave-29 SEAL
status in the front-matter (only the §14 closure paragraph). This is
cosmetic — the validate_specs scan passes (457 / 457) — but future
operators tracing the WI status via `grep status:` will miss the
wave-29 update.

### 2.3 Pricing 5-tier vs canonical — **P1 finding**

The pricing page ships **Free / Starter / Team / Pro / Enterprise**.
This matches `marketing/sales/PRICING-WORKSHEET.md` `## Worked tier
P&L` headings (`### Free tier`, `### Starter tier`, `### Team tier`,
`### Pro tier (internal SKU, not yet launched)`, `### Enterprise
tier`). The brief asserted the canonical was "Solo/Team/Business/
Enterprise" — that 4-tier shape does **not** appear in any canonical
source under `marketing/sales/` or `specs/`. The pricing-page
header comment explicitly invokes the "wave-13 tier taxonomy".

**However:** the worksheet labels **Pro** as
*"internal SKU, not yet launched"* and the **`Sales playbook`**
section enumerates landing patterns only for Starter / Team /
Enterprise (no Pro landing column). The wave-29 pricing page
nonetheless renders Pro publicly:

```
apps/docs/src/pages/pricing.tsx:53
description="CoreLink plans: Free, Starter, Team, Pro, Enterprise. Pricing is provisional pending GA."

apps/docs/src/pages/pricing.tsx:229
<CheckOrDash included={t === "pro" || t === "enterprise"} />
```

This is a **P1 marketing-canonical drift**: the public pricing page
exposes a tier the canonical worksheet flagged as not-yet-launched,
without a complementary update to `PRICING-WORKSHEET.md ## Pro tier`
re-classifying Pro as "public, launchable at GA". Recommended fix
(wave-30): either remove Pro from the public page until Finance
re-classifies, or land a Finance/Product co-sign that promotes Pro
from internal SKU to public SKU, with a corresponding worksheet edit.
The page-level "provisional pending Finance / Legal / Product sign-off"
banner partially mitigates (does not eliminate) the drift.

### 2.4 Signup backend audit fail-CLOSED — **PASS**

`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` enforcement verified in
`apps/server/src/routes/signup.rs`:

- The `SignupAuditSink` trait `emit` returns `Result<(), &'static str>`
  with a docstring that explicitly invokes the invariant
  (lines 435-444).
- The `emit_or_503` helper (lines 506-522) wraps the emit call and
  returns `503 Service Unavailable` ("audit pipeline closed") on
  Err, mirroring the canonical `audit_export::emit_or_503` shape.
- The `InMemorySignupAuditSink::inject_failure` method (lines 480-498)
  is exercised by the test harness to drive the 503 fail-CLOSED
  regression.

**Test count:** 9 unit (`#[test]` in `signup.rs` lines 894-1051) + 8
integration (`signup_pilot.rs`) = **17**, matching the brief's claim.

### 2.5 Pilot admin web UI — `:tenant_id` route syntax — **PASS**

`apps/server/src/routes/admin_pilot.rs` lines 87-91 declare:

```
PILOTS_GRANT_TIER_ROUTE = "/v1/admin/pilots/:tenant_id/grant-tier"
PILOTS_CHECKIN_ROUTE    = "/v1/admin/pilots/:tenant_id/checkin"
```

…and the `Router::new()` block at lines 570-573 wires the three
handlers (`handle_list`, `handle_grant_tier`, `handle_checkin`). The
colon-prefix syntax is correct for axum 0.7 (workspace pin
`axum = "0.7"`, matchit 0.7). This **correctly avoids** the latent
`{name}` literal bug observed in `ac.rs` and `admin.rs` (called out
in the brief).

Tests: 9 unit (`admin_pilot.rs` body) + 12 integration
(`tests/admin_pilot.rs`) = **21**. Polling: `POLL_INTERVAL_MS =
30_000` confirmed at `apps/docs/src/pages/admin/pilots.tsx:68`,
with a second 60s interval at line 176 for the relative-timestamp
re-render (unrelated to the 30s data poll).

### 2.6 `ShadowSinkFactory` full adoption + `region_source` telemetry — **PASS** (with P2)

`apps/server/src/routes/audit_analytics.rs` defines
`REGION_SOURCE_PRELUDE = "prelude"` (line 141) and
`REGION_SOURCE_FALLBACK = "fallback"` (line 146). The
`resolve_shadow_via_prelude` function (lines 520-565) returns a
`(sink, region_source)` pair in both arms:

- Prelude arm (lines 523-535): `for_tenant_in_region` + returns
  `REGION_SOURCE_PRELUDE` — skips the D1 round-trip per the wave-29
  charter.
- Fallback arms (mismatch and `None`, lines 536-565): `for_tenant` +
  returns `REGION_SOURCE_FALLBACK`.

All emit sites (lines 674, 700, 726, 848, …) decorate the audit row
via `.with_region_source(region_source)`. The `-1 D1 round-trip`
brief claim is structurally accurate for the prelude-bound path.

**P2-SHADOWSINK-01:** the prelude-mismatch arm (lines 547-559) emits a
synthetic `REQUEST_PRELUDE_MISSING_EXIT` marker row via
`state.audit_sink.emit(...)` *without* a `.with_region_source(...)`
decoration. That row is the SEV-3 wiring observability signal called
out in the inline comment; it carries no region context. If an
operator filters analytics-query rows by `region_source IS NOT NULL`
they will miss the mismatch marker. Recommended fix (wave-30): emit
the marker with `.with_region_source("prelude_mismatch")` so dashboards
have a third canonical value.

**P3-SHADOWSINK-02:** the resolver-failure emit (lines 645-672)
constructs an `AnalyticsAuditRow` *before* the `(shadow,
region_source)` pair is bound, so the failure-path row also lacks
`region_source`. Defensible (no resolver result yet), but worth
documenting in the audit doc for D+24h dashboard authors.

### 2.7 Trust center 12 / 1 / 2 classification — **PASS**

`specs/_audits/sealed/2026-05-16-trust-center-consolidation.md` enumerates:

- **12 PUBLISH:** 8 MDX deep-dives (`/trust/overview`, `/compliance`,
  `/data-handling`, `/subprocessors`, `/incident-response`,
  `/iso27001`, `/pci-dss`, `/fedramp-info`) + 3 React pages
  (`/trust`, `/trust/sub-processor-register`,
  `/trust/incident-history`) + 1 new VDP page
  (`/security/report-security`).
- **1 DRAFT-CLOSE-WAVE-29+:** `/security/responsible-disclosure`
  (74-line draft stub; superseded by `report-security`; deletion +
  redirect recommended wave-30).
- **2 DEFER-POST-GA:** `/security/incident-history` (44-line MDX
  stub superseded by React page) + `/security/hall-of-fame`
  (Hall-of-Thanks launches GA + 30d per VDP policy).

Each classification carries an explicit defensibility rationale.
**P3-TRUST-01:** the `apps/docs/sidebars.ts` baseline test
("1 baseline failure unchanged — pre-existing Trust category counted
as 5th sidebar entry on main; not in scope") is a known-failing test
left as-is. The audit doc transparently calls this out; carrying a
known-fail test into GA is cosmetic but should be closed wave-30.

### 2.8 Perf baseline GA freeze — **PASS**

`reports/perf/baseline-ga-2026-05-16-365dd38.json` declares
`measurement_mode: "criterion --quick (laptop wall-clock budget); CI
canonical refresh per §5"` and ships **3 CRITICAL benches measured**
(`derive_prefix_v2`, `blake3`, `jcs_canonicalize`) + **8 pending
recaptures** enumerated in `benches_pending_recapture`. Sample
measurements (units: ns):

```
derive_prefix_v2  1000-distinct-tenants  mean=1,149,612
blake3            100MiB                 mean=31,760,873
blake3            1B                     mean=78.3
jcs_canonicalize  10240B-data            mean=26,728
```

`scripts/perf-regression-check.py` line 30 documents
"A baseline with `median_ns == null` is tolerated as [pending
recapture]" — so the 8 pending benches do not break the gate.
`baseline-ga-diff-vs-wave22.md` is an honest "first numeric capture"
diff (wave-22 carried all-null measurements), with the verdict
"Regression vs wave-22: none possible". This is **structurally
correct** but does mean the wave-29 freeze is **not a regression
defence** — it is a forward-fixed reference point for the GA +24h
SRE check.

**P3-PERF-01:** the audit doc front-matter omits the criterion
`--quick` wall-clock budget (operator §5 owns canonical p99 recapture).
Future operators may assume `--quick` proxies are GA-final; the
README does call this out, but the JSON manifest `measurement_mode`
field is the durable signal.

## 3. Cross-stream merge artefact (`04f2dff`)

The brief flagged the final merge as a 4-locale `code.json` UNION
resolution via `python3 -m json.dump`. The resulting state:

- 4 × 266-line files, each 66 keys, parity verified.
- Three wave-29 streams contributed deltas (signup-landing +
  audit-chain-viz + trust-center each shipped 108-144 lines per
  locale). The union preserved all three contributors with no
  collisions on key names.
- Docusaurus key lookup is dict-keyed — ordering inside `code.json`
  does not affect runtime behaviour.

## 4. Sanity gates re-run on `04f2dff`

| Gate | Result |
|---|---|
| `cargo check -p corelink-server --tests` | green (1m 43s) |
| `python3 scripts/validate_specs.py` | 448 + 9 = 457 OK |
| `python3 scripts/validate_references.py` | 0 dangling |

## 5. Finding tally

| Severity | Count | Items |
|---|---|---|
| P0 | 0 | — |
| P1 | 1 | Pro tier public exposure vs `PRICING-WORKSHEET.md` "internal SKU, not yet launched" |
| P2 | 3 | (a) WI-S09-008 wave-29 closure not reflected in front-matter `status:` field; (b) prelude-mismatch marker lacks `region_source`; (c) Pro-tier feature columns ("SCIM provisioning") shipped publicly without worksheet co-sign |
| P3 | 7 | shadow-sink resolver-failure row lacks region_source; sidebars.ts known-fail test carried into GA; perf JSON `--quick` mode caveat absent from front-matter; pricing test count 22 not re-verified end-to-end (vitest not re-run); audit-chain-viz Clerk shell-hooks degrade gracefully but no test pins behaviour; trust-center incident-history "auto-pull T+30d post-GA" promise has no scheduled WI; signup landing page a11y baseline regen deferred (per the originating audit) |

## 6. Score computation

```
score = 10 − (1.5 × 0) − (0.5 × 1) − (0.15 × 3) − (0.05 × 7)
      = 10 − 0          − 0.50     − 0.45        − 0.35
      = 8.70
```

**Final: 8.70 / 10.0 — PASS** (SOTA bar 8.5).

## 7. Recommendations for wave-30

1. **P1-PRICING-01 (immediate):** either remove Pro tier from
   `/pricing` until Finance promotes it from "internal SKU" status,
   or land a Finance + Product co-sign in `PRICING-WORKSHEET.md`
   re-classifying Pro as public-launchable at GA. Page-level
   "provisional" banner is insufficient on its own.
2. **P2-AUDITVIZ-01:** stamp `status: WAVE-29-CLOSURE` (or similar)
   into the `WI-S09-008` front-matter so the closure is grep-visible.
3. **P2-SHADOWSINK-01:** decorate the prelude-mismatch marker emit
   with `REGION_SOURCE = "prelude_mismatch"` (third canonical value).
4. **P3 sweep:** address the 7 P3s in a single wave-30 sweep
   (`wt/r-prep-wave29-tail-cleanup`).

## 8. Trace

- Worktree: `.claude/worktrees/agent-wave29-review` @
  `wt/r-prep-wave29-adversarial-review` (branch from `04f2dff`).
- No source code changed; review-only.
- Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
- Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
