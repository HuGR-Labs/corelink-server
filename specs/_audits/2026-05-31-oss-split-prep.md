---
id: "AUDIT-2026-05-31-OSS-SPLIT-PREP"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-31"
updated: "2026-05-31"
owner: "Gustavo Schneiter"
tags: ["audit", "oss", "licensing", "launch", "DEBT-002", "pre-launch"]
references:
  - "docs/OSS_STRATEGY.md"
  - "docs/internal/OSS-VS-CLOSED-MATRIX.md"
  - "docs/POSITIONING.md"
  - "specs/_audits/2026-05-27-launch-decision-log.md (D-22 open-source the cache core)"
  - "scripts/check-oss-license-tags.sh"
  - ".github/workflows/license-policy.yml"
---

# OSS Split Prep — Pre-Launch Gap Analysis & Remediation

## Purpose

Research-only prep for the decided OSS split (`OSS_STRATEGY.md` v1.0.0,
`OSS-VS-CLOSED-MATRIX.md` v1.0.0). The strategy was NOT re-litigated. This
audit verified the *codebase against the decision* and remediated the
publish blockers for the pre-launch crate batch. Branch:
`oss/license-tags-prep-2026-05-31`.

## Findings (all verified in tree, not inferred)

### F-1 — DEBT-002 license tags regressed (HIGH)

`[workspace.package] license = "UNLICENSED"` (correct — ~85 crates are
proprietary). All crates inherit it via `license.workspace = true`. The
`OSS-VS-CLOSED-MATRIX` (2026-05-15) closed DEBT-002 claiming "13 crates
tagged `MIT OR Apache-2.0`". As of this audit: **0 crates carried the
literal tag** — the Wave 33-36 reorg (149→87 crates) standardized every
member to workspace inheritance and silently wiped the OSS tags. DEBT-002
was marked CLOSED with evidence that no longer held.

### F-2 — OSS→closed coupling = `cargo publish` blockers (HIGH)

| OSS crate (per matrix) | Closed dep | Nature | Verdict |
|---|---|---|---|
| `corelink-rate-headers` | `corelink-ratelimit` | **dead dep** — zero real `use`, only doc-comment + arch-prose mentions | Removed; crate compiles + docs clean without it |
| `corelink-audit` | `corelink-audit-chain`, `corelink-analytics` | `pub use ...::*` — **re-exports entire closed crates** | Reclassified CLOSED; matrix's "schema-only split" was never implemented |
| `corelink-cli` | `corelink-analytics`, `corelink-audit-chain`, `corelink-runbook-tracker` | binary, real deps | Distribute as compiled binary (brew tap + GH releases), NOT a crates.io lib |

### F-3 — Matrix name drift post-reorg (MEDIUM)

- `tenant-path` → renamed `corelink-tenant-path` (dir unchanged).
- `corelink-ac-schema`, `corelink-auth-schema`, `corelink-multipart-schema`
  → no longer exist (absorbed in the reorg). The matrix's 13-crate OSS list
  is stale.

### F-4 — Publish metadata gaps (LOW)

`repository` field absent on all OSS crates; README absent on
`corelink-rate-headers` (+ openapi/py/go/wasm/audit, out of this batch's
scope).

### F-5 — CI blind spot (MEDIUM)

`license-policy.yml` audits *dependency* licenses (cargo-license + deny.toml)
but never checked that *our own* OSS crates carry the right license — which
is exactly why F-1 went unnoticed.

## Remediation applied (this branch, local only — no publish, no deploy)

| # | Action | Crates | Verified by |
|---|---|---|---|
| P0 | Literal `license = "MIT OR Apache-2.0"` + `publish = true` + `repository` (override workspace UNLICENSED/publish=false) | hash, client-verify, tenant-path, rate-headers | `scripts/check-oss-license-tags.sh` PASS |
| P1 | Removed dead `corelink-ratelimit` dep | rate-headers | `cargo build` + `cargo doc -D warnings` clean |
| P2 | Wrote crates.io README | rate-headers | present |
| P3 | Build + clippy `-D warnings` + `cargo doc` + `cargo package --no-verify` | all 4 | green (rust 1.91.1); hash 16 files, tenant-path 13 files packaged |
| P5 | `scripts/check-oss-license-tags.sh` + wired into `license-policy.yml` | — | gate PASS; closes F-5 |

The pre-launch crates.io batch is now publishable in this order:
`corelink-hash` → `corelink-client-verify` (dep: hash) ; `corelink-tenant-path`
and `corelink-rate-headers` independent. (Real `cargo publish` is a separate
launch-day decision — not done here.)

## Open follow-ups (NOT done — need founder call)

1. **DEBT-002 reopened** as the F-1 regression record. No live debt register
   exists post-seal; this audit is the durable record.
2. **`corelink-audit` schema extraction** — if the CloudEvents schema must be
   OSS, split a real schema-only crate from the re-export facade. Deferred WI;
   does not block launch (reclassified closed for now).
3. **At-launch / post-launch OSS crates** (`corelink-openapi`, `-wasm`, `-py`,
   `-go`, `-cli`) — tag + README per the `OSS_STRATEGY.md` roadmap phases when
   their phase arrives. Add each to `check-oss-license-tags.sh` on tagging.
4. **Matrix v1.1** — reconcile the stale 13-crate list with the post-reorg
   reality (see the Reconciliation section appended to the matrix).
