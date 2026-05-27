# Beta-feedback triage harness — Wave-23 audit

- **Date**: 2026-05-16
- **Wave**: R-prep wave-23
- **Branch**: `wt/r-prep-beta-feedback-triage`
- **Companion process doc**:
  [`docs/internal/beta-feedback-triage.md`](../../docs/internal/beta-feedback-triage.md)
- **CLI**:
  [`scripts/beta-feedback-ingest.py`](../../scripts/beta-feedback-ingest.py)
- **Test suite**:
  [`tests/beta_feedback_triage_test.py`](../../tests/beta_feedback_triage_test.py)

## Scope

CoreLink ships its **invite-only beta** in the next two waves. Until now,
pilot-tenant feedback has been collected ad-hoc in Slack and one engineer's
DM history. That model does not scale past the first three pilots, does
not produce auditable SLA evidence, and forces the on-call to reinvent the
severity wheel every time a report arrives.

This harness establishes:

1. **A canonical intake form** (`docs/internal/beta-feedback-triage.md` §1)
   that every report — Slack, email, web form, in-product widget — maps
   onto.
2. **A deterministic P0/P1/P2/P3 rubric** (§3) anchored to observable
   blast-radius / data-loss / SLO criteria.
3. **An SLA contract** (§2) with ack and resolve windows per severity,
   and a holiday / weekend handling rule.
4. **A routing matrix** (§4) that names primary + secondary owners per
   surface area.
5. **A closure protocol** (§5) requiring root-cause + fix commit +
   regression-test-or-waiver + pilot verification.
6. **An aggregation cadence** (§6) — Friday weekly digest, first-Monday
   monthly retro — that turns the backlog into a feedback loop.
7. **A CLI** (`scripts/beta-feedback-ingest.py`) that consumes NDJSON
   intake records and emits a deterministic markdown summary grouped by
   severity, with routing + wave-N assignment per record.
8. **A pytest suite** (`tests/beta_feedback_triage_test.py`) covering
   rubric edge cases, SLA computation, routing sanity, end-to-end
   ingest, and CLI exit-code semantics.

## Design choices and trade-offs

### Stdlib-only CLI

`scripts/beta-feedback-ingest.py` uses only `json`, `argparse`,
`datetime`, `dataclasses`, `pathlib`, `sys`, `io`. The Friday digest cron
job and the ad-hoc `pipx`-style invocation by the on-call must both work
without `pip install`. Adding `pydantic` or `jsonschema` would tie the
harness to the broader Python build matrix; the validation we need
(required fields + enum membership + RFC3339 timestamp) is straightforward
in stdlib.

### Reporter-supplied severity is a **floor**, never a ceiling

§3 rubric never **downgrades** what the pilot reported. If the pilot says
P0 we keep P0 until evidence justifies a downgrade — which only happens
through manual triage, not the automated classifier. The classifier may
ratchet **up** (e.g. a full pilot outage marked P2 by the pilot
becomes P0 automatically). This bias is intentional: false-negatives on
beta severity are more expensive than false-positives during a 3–5
pilot window.

### Cross-tenant short-circuit

Any record whose `title` or `actual` field contains `cross-tenant`,
`rls bypass`, `data leak`, or `plaintext leak` short-circuits to P0
regardless of the reporter-supplied severity or impact. The rubric never
hides a tenancy isolation signal in the P2/P3 bucket. The keyword list is
explicit in `classify()` and is covered by two tests
(`test_cross_tenant_short_circuits_to_p0`,
`test_rls_bypass_in_actual_short_circuits_to_p0`).

### Wave assignment is mechanical, not discretionary

§4 specifies P0 = current wave, P1 = wave+1, P2 = wave+2, P3 = wave+3.
The dispatcher honors an explicit `wave_target` field if the triager
overrode it (escape hatch for the rare case where a P3 needs to ride
along with a P1 fix in the same area). Otherwise the assignment is
deterministic and trivially testable.

### Determinism

`emit_markdown` sorts each severity bucket by `(received_at, id)`.
Two runs over the same input produce byte-identical output, so the
weekly digest can be committed under `specs/_audits/` without spurious
diffs from non-stable iteration order. The test
`test_ingest_is_deterministic` enforces this.

### Pilot-tenant detection is a heuristic, with an opt-out

The §3 P0 floor for `tenant_impact == full_outage` applies only when
the affected tenant is a real pilot. We use the prefix `t_pilot_` on
`tenant_id` as the heuristic, with an opt-out by including
`non-pilot-tenant` in `notes`. Once the pilot roster is encoded in a
proper registry, this heuristic should be replaced with a registry
lookup (tracked under the "Followups" section below).

## First-week post-pilot-launch process

The harness applies as soon as the first invite-only pilot accepts
their invite. The **first calendar week** of pilot operations runs the
following daily cycle, ratcheted down to weekly cadence from week 2:

### Day 0 (pilot ship)

- Open `#beta-pilot-<tenant>` Slack channel; pin a link to the intake
  form + `docs/internal/beta-feedback-triage.md`.
- Run `python3 scripts/beta-feedback-ingest.py --schema > /tmp/schema.json`
  and post a one-line "here's the schema if you have a webhook" follow-up
  for the pilot's tech contact.
- Confirm on-call rotation in `docs/internal/oncall-24-7-readiness.md`
  covers the next 7 days inclusive.

### Days 1–7 (daily)

- **09:00 UTC** — triager pulls the previous 24 h of pilot feedback
  (Slack export + email forwards + form submissions) into a single
  NDJSON file `var/pilot-feedback/YYYY-MM-DD.ndjson`.
- **09:15 UTC** — run
  `python3 scripts/beta-feedback-ingest.py --in
  var/pilot-feedback/YYYY-MM-DD.ndjson --current-wave 23
  --out var/pilot-feedback/YYYY-MM-DD-summary.md`.
- Triager walks each accepted row, posts ack in the pilot channel
  within the §2 SLA, opens / updates the corresponding wave-N backlog
  entry using the routing matrix label.
- Triager investigates rejections — every rejected line is either an
  intake-form bug (fix the schema) or a real validation issue
  (fix the source); rejections do not silently disappear.

### Day 7 (Friday review)

- Run the `--summary --week` aggregation (rolling 7-day window).
- Hold a 30-minute review covering: SLA breaches by name, surfaces with
  the most reports, any P0/P1 still open, pilot-by-pilot sentiment.
- Output: `specs/_audits/YYYY-MM-DD-beta-feedback-friday-review.md`.

### Week 2+ cadence

- Daily morning triage continues, but the all-hands review compresses
  to the Friday meeting only.
- The first-Monday monthly retrospective takes over the trend-line
  analysis.

## Quality gates run for this PR

- `python3 -m pytest tests/beta_feedback_triage_test.py -v` —
  41 tests pass (12 classification, 7 SLA, 6 routing, 5 wave-assignment,
  4 validation, 7 ingest end-to-end).
- `python3 scripts/beta-feedback-ingest.py --schema` smoke check — schema
  prints as valid JSON.
- `python3 scripts/beta-feedback-ingest.py --in <sample> --current-wave 23`
  end-to-end smoke check — markdown summary matches the documented
  routing + wave assignment.
- `python3 scripts/validate_specs.py` — green (this audit doc carries
  no front-matter since it lives under `specs/_audits/` which is
  exempt).
- `python3 scripts/validate_references.py` — green (this harness adds
  no IDs that the validator tracks).

## Followups

| ID                  | Description                                                                                            | Wave    |
| ------------------- | ------------------------------------------------------------------------------------------------------ | ------- |
| BFT-FU-001          | Replace the `t_pilot_` prefix heuristic with a lookup against a proper pilot-tenant registry.          | wave-25 |
| BFT-FU-002          | Implement the Friday digest cron job (`scripts/beta-feedback-weekly-digest.py`) wrapping ingest.       | wave-24 |
| BFT-FU-003          | Wire up the in-product feedback widget on the dashboard to POST records to a Worker that drops NDJSON. | wave-25 |
| BFT-FU-004          | Extend the rubric with a `data_class` field (PII / non-PII) so privacy-incident routing is automatic.  | wave-26 |
| BFT-FU-005          | Add `--since/--until` time-window flags to `beta-feedback-ingest.py` for retro slicing.                | wave-24 |

## Cross-links

- `docs/internal/beta-feedback-triage.md`
- `scripts/beta-feedback-ingest.py`
- `tests/beta_feedback_triage_test.py`
- `docs/internal/oncall-24-7-readiness.md`
- `docs/internal/PERFORMANCE-PLAYBOOK.md`
- `docs/internal/auth-event-taxonomy.md`
