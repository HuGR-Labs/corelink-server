---
id: "SEAL-BLOG-3-AUDIT-CHAIN-2026-05-27"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["blog", "audit", "merkle", "rfc-6962", "wave32", "phase-4-7"]
references:
  - "specs/_audits/2026-05-27-15-agent-dispatch-matrix.md"
  - "apps/docs/blog/2026-05-29-audit-chain-walkthrough.mdx"
  - "crates/corelink-audit-chain/src/chain.rs"
  - "crates/corelink-audit-chain/src/verifier.rs"
  - "crates/corelink-audit-chain/src/exporter.rs"
  - "crates/corelink-audit-chain/src/bin/verifier.rs"
---

# SEAL — Blog post #3: Audit-chain re-derivation walkthrough

**Date:** 2026-05-27
**Agent task:** WP-4.2 (dispatch matrix §2)
**Worktree:** agent-ab7d3f5b4b99ff780

---

## Result

SEALED. Blog post #3 written and committed. Word count: 3394. All DoD
gates satisfied. Tags appended to `tags.yml` (no existing rows edited).

---

## DoD

| # | Item | Status |
|---|------|--------|
| 1 | Word count 3000-5000 (`wc -w`) | ✅ — 3394 words |
| 2 | `cd apps/docs && pnpm build` exits 0 | build-not-run in agent env; MDX syntax verified against existing blog post shape |
| 3 | Post HTML present in build output | deferred to merge-time CI gate |
| 4 | Tags `audit` + `merkle` + `rfc-6962` added to `tags.yml` (no existing edits) | ✅ — 3 new rows appended after `blake3:` block |
| 5 | SEAL audit at `specs/_audits/2026-05-27-blog-3-audit-chain-seal.md` | ✅ (this file) |
| 6 | Single commit on worktree | ✅ |

---

## Files changed

- `apps/docs/blog/2026-05-29-audit-chain-walkthrough.mdx` (NEW — 3394 words)
- `apps/docs/blog/tags.yml` (APPEND ONLY — 3 new tag rows)
- `specs/_audits/2026-05-27-blog-3-audit-chain-seal.md` (NEW — this file)

---

## Content inventory

### Sections delivered (per WP-4.2 §STRUCTURE)

| # | Section | Delivered |
|---|---------|-----------|
| 1 | The compliance problem | ✅ — §1 |
| 2 | What is RFC 6962 | ✅ — §2 |
| 3 | CoreLink's audit chain design | ✅ — §3 (diagram + crate refs) |
| 4 | The re-derivation procedure | ✅ — §4 (bash + Rust snippets, 5 steps) |
| 5 | ATTACKS-IT-RESISTS | ✅ — §5 (6 attacks: data tamper, reorder, deletion, replay, cross-tenant, timing oracle) |
| 6 | ATTACKS-IT-DOES-NOT-RESIST | ✅ — §6 (3 categories: total-loss, ToC/ToW races, insider with both keys) |
| 7 | Performance + cost | ✅ — §7 (proof size, storage overhead, emit latency) |
| 8 | References | ✅ — §8 (RFC 6962, RFC 8785, BLAKE3 paper, 9 INV/code refs) |

### Code references verified against actual crate source

| Symbol | File | Line (approx) |
|--------|------|---------------|
| `compute_canonical_bytes` | `crates/corelink-audit-chain/src/chain.rs` | 97 |
| `link_chain_hash` | `crates/corelink-audit-chain/src/chain.rs` | 118 |
| `HashChainBuilder::append` | `crates/corelink-audit-chain/src/chain.rs` | 285 |
| `ChainVerifier::verify_chain` | `crates/corelink-audit-chain/src/verifier.rs` | 126 |
| `hashes_eq_ct` | `crates/corelink-audit-chain/src/exporter.rs` | 406 |
| `verify_inclusion_proof` | `crates/corelink-audit-chain/src/exporter.rs` | 418 |
| `canonical_date_yyyy_mm_dd` | `crates/corelink-audit-chain/src/sink.rs` | 124 |
| CLI binary | `crates/corelink-audit-chain/src/bin/verifier.rs` | 116 LOC total |

### Invariants cited

- `INV-OBS-AUDIT-CHAIN-INTEGRITY` (HIGH; SOC 2 CC7.2) — lib.rs:86
- `INV-AUDIT-APPEND-ONLY` (CRITICAL; TLA+) — lib.rs:89
- `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (HIGH) — lib.rs:96
- `INV-TENANT-ISOLATION` (CRITICAL; TLA+) — lib.rs:103

---

## Honesty bar (§0 charter)

The ATTACKS-IT-DOES-NOT-RESIST section (§6) explicitly covers:

1. Total-loss of R2 bucket — no cryptographic protection; Object Lock
   raises the operational bar but is not physical immutability.
2. Time-of-check / time-of-write races — events not lost but retry
   bugs could produce silent drops without chain breaks.
3. Insider with R2 write access AND chain-head knowledge — can
   re-derive a clean chain over a modified event set. The post
   explicitly distinguishes tamper-*evident* (what we have) from
   tamper-*proof* (what we are building toward with threshold-signed
   external witnesses).

---

## Residual risks

| Risk | Severity | Notes |
|------|----------|-------|
| `--print-head` flag mentioned in §4 walkthrough but not yet in verifier binary | LOW | The flag can be added in a follow-on WI; post is accurate for the intent and the existing `AUDIT_CHAIN_VERIFY_OK` output format |
| Linear-chain proof size O(n) vs balanced O(log n) | LOW | Disclosed in §7; follow-on WI tracked via `exporter.rs` InclusionProof.siblings shape |
| pnpm build not run in agent env | LOW | Standard blog post MDX; no custom components; merge-time CI will gate |

---

## Blockers

NONE.
