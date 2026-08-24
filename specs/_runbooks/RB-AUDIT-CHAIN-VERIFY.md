---
id: "RB-AUDIT-CHAIN-VERIFY"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-24"
updated: "2026-08-24"
sprint: "R-PREP-WAVE-19"
parent_wi: "WI-S09-008"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-AUDIT-EXPORT-INTEGRITY"
  - "security_model"
tags: ["runbook", "audit", "chain", "integrity", "sev-0", "soc2", "cc7.2"]
---

# RB-AUDIT-CHAIN-VERIFY — the sealed audit chain failed verification

## Why this file exists

`audit-chain-daily-verify.yml` has dispatched its SEV-0 PagerDuty page with
`custom_details.runbook = "specs/_runbooks/RB-AUDIT-CHAIN-VERIFY.md"` since it
was written. That file did not exist. Anyone paged at 03:00 followed a dead
link. This is that runbook.

## What the page means

The daily verifier walked the last seven days of NDJSON chunks in the archive
bucket and one of four things happened for a given day. The `failure_kind`
field on the page says which, and they are NOT equally serious:

| `failure_kind` | What actually happened | Severity in practice |
|---|---|---|
| `chain_break` | A chunk's bytes do not hash to the chain the chunk itself declares | **Real.** Treat as tampering until proven otherwise |
| `list_failure` | The R2 list call failed (auth, 5xx, cursor) | Infrastructure — the chain is unexamined, not broken |
| `get_failure` | A key listed but would not download | Infrastructure, same caveat |
| `json_parse` | A chunk did not parse as the expected line format | Usually a producer/format mismatch, not tampering |

The last three mean **the chain was not verified**, which is not the same as
**the chain is bad**. Do not escalate to legal on a `list_failure`. Do not
close a `chain_break` without doing the work below.

## Immediate triage — a `chain_break`

1. Read the marker on the page: `AUDIT_CHAIN_BREAK_DETECTED::<date>::<chunk-key>`.
   The chunk key is the exact object to examine.
2. Pull that object down and verify it by hand, from a worktree on
   `origin/main` — never from the repo root, which is parked on a quarantined
   branch and does not carry the current verifier:

   ```bash
   aws s3api get-object --endpoint-url "https://$ACC.r2.cloudflarestorage.com" \
     --bucket corelink-audit-weur --key "<chunk-key>" --region auto /tmp/chunk.ndjson
   cargo run -p corelink-audit-chain --bin verifier --release -- /tmp/chunk.ndjson
   ```

   `AUDIT_CHAIN_VERIFY_OK` means the object verifies now. That is a signal in
   itself — see "when it verifies on retry" below.
3. Compare the archived chunk against D1, which holds the same sealed rows.
   Every archived line carries `row_id` and `canonical_jcs`; the D1 row of that
   id must carry byte-identical `canonical_jcs` and the same `chain_hash`. A
   divergence localises the tampering to one side.

   ```sql
   SELECT id, sequence_number, prev_hash, chain_hash, canonical_jcs
   FROM audit_outbox WHERE id = '<row_id>';
   ```
4. Establish the blast radius. The chain is partitioned per
   `(tenant_id, region)` and each partition numbers its own sequence from 0, so
   a break bounds to ONE partition. Verify the partition's neighbouring chunks
   before and after the failing sequence number to find where the chain last
   agreed with itself.

## What a confirmed break means

The archive exists to be tamper-EVIDENT, not tamper-proof. A confirmed
`chain_break` means either the D1 seal was altered after the fact or the R2
object was altered after write. Both are security incidents, not bugs:

- The bucket carries a 7-year Object Lock rule, so an overwrite of an existing
  object should be impossible. If a chunk changed anyway, the retention
  configuration itself is suspect and must be re-read from the API, not from
  documentation.
- The archiver writes create-if-absent and treats a byte-different existing
  object as a hard failure rather than overwriting it. A break therefore cannot
  have been produced by an ordinary re-run.

Escalate to the Security Lead. Preserve the object and the D1 row as evidence
before doing anything that could rewrite either.

## When it verifies on retry

A chunk that fails in CI and verifies by hand means the failure was in the
READ path, not the data — a truncated download is the usual cause. Confirm by
comparing the object's size and ETag against what CI downloaded. Re-run the
workflow for that day. Do NOT silence the alert: a read path that truncates
silently is its own defect and belongs in `BACKLOG.md`.

## What this page is NOT

Absence is a different failure and a different page. If sealed rows are simply
not reaching the bucket at all, that is `class=archive-absent`, see
[RB-AUDIT-ARCHIVE-ABSENT](RB-AUDIT-ARCHIVE-ABSENT.md). A day with no chunks is
a clean no-op for THIS workflow — it verifies what it finds and cannot notice
what was never written.

## Related

- `.github/workflows/audit-chain-daily-verify.yml` — the verifier lane
- `crates/corelink-audit-chain/src/sealed_archive.rs` — the line format and the
  chunk-level verification this runbook's commands invoke
- [RB-AUDIT-EXPORT-INTEGRITY](RB-AUDIT-EXPORT-INTEGRITY.md) — the parent control
