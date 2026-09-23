# Current `main` drift readback — 2026-09-23 (`cd74a094`)

## Pins and method

- Immutable campaign source snapshot: `50a5ab3a0e3e90bff56beb18fade52f50e3ff005`.
- Latest fetched remote tip: `cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6`, subject
  `docs(okf): repair ADR source anchors and citations (#2221)`.
- Previous observed tip: `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`.
- Readback used Git object comparisons (`git diff --name-status`, focused diffs,
  and manifest path counts) against both the immutable snapshot and previous
  observation. It did not execute Rust, CI, production, or GitHub writes.

## Population and campaign-framework drift

- The comparison finds 107 tracked `Cargo.toml` paths at both the baseline and
  current tip. No `Cargo.toml` or `Cargo.lock` changed; this readback does not
  change the prepared 105 eligible package identities.
- No ownership framework, ownership artifact, or frozen
  `docs/internal/okf-wiki/` file changed on `main` relative to the baseline.
- Three OKF knowledge ADR pages changed: ADR-0070 repairs a malformed
  provenance field and refreshes source-blob provenance; ADR-0101 adds exact
  decision/consequence citations and source metadata; ADR-0103 adds a bounded
  decision citation and source metadata. These are knowledge-page provenance
  and citation repairs; they do not change the frozen OKF profile. No current
  ownership artifact cites these ADRs directly.
- Other non-package changes include workflow/verifier/test updates, a secrets
  checklist update, capacity/operator documentation, and the `BACKLOG.md`
  `last-verified` metadata date. The backlog ownership-match content itself has
  no delta in the focused diff.

## Package-source and contract delta

Since the campaign snapshot, `corelink-server` source changes include the
previously recorded extraction of runtime helpers into
`crates/corelink-container/src/main_runtime.rs` and DSR classification into
`crates/corelink-container/src/routes/dsr/adapter_d1/classification.rs`. The
current tip additionally changes
`crates/corelink-container/src/routes/billing_ingest/tests_durable_classification.rs`:
the loopback D1 fixture can drop one INSERT acknowledgement after committing
the mutation, and `durable_classification_matrix` verifies that the first call
is uncertain, a retry is classified `Deduped`, and only one durable winner
remains. This adds explicit lost-ack recovery test coverage; it does not change
the production implementation in this delta.

These changes are not present at `cd74a094`; a later commit adds them. See the
successor readback
[`SERVER-MAIN-READBACK-20260923-38F43.md`](SERVER-MAIN-READBACK-20260923-38F43.md)
for the verified test, workflow, and operator-contract delta. No execution is
implied by either readback.

## Correction to the prior readback

The earlier B9B3 note said the server tree was unchanged from `5e4339c` through
`b9b3ee8c`. That statement was too broad about the whole tree but not about the
durable-classification test: its blob at `5e4339c` and `cd74a094` is identical.
The test/workflow/operator-contract delta belongs to successor commit
`38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018`, not this readback's tip. The
runtime-helper and DSR-classification source facts remain present. This
correction supersedes the earlier mistaken attribution and does not itself
approve any artifact.

## Decision

Keep `50a5ab3a` as the immutable campaign snapshot and record `cd74a094` as the
latest observed `main`. Identity/population preparation remains stable. The
server package is materially stale for test coverage and must be re-read at the
current tip; the OKF profile and ownership framework themselves have no main
delta. Registry regeneration and freeze decisions are intentionally outside
this evidence record.
