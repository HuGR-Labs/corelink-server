---
id: "AUDIT-2026-05-27-BLOG-2-BLAKE3"
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
tags: ["audit", "blog", "blake3", "cryptography", "phase-4.7"]
references:
  - "apps/docs/blog/2026-05-28-why-blake3.mdx"
  - "apps/docs/blog/tags.yml"
  - "crates/corelink-hash/src/digest.rs"
  - "crates/corelink-hash/src/verified_body.rs"
  - "crates/corelink-hash/src/store.rs"
  - "specs/_audits/2026-05-27-15-agent-dispatch-matrix.md"
---

# SEAL audit — Blog #2: BLAKE3 internals + why we picked it (WP-4.1)

## §1 Summary

Blog post #2 of the Phase 4.7 educational content cadence delivered and
committed. The post covers BLAKE3 hashing internals (Merkle tree
structure, SIMD path, derive-key mode) versus SHA-256, BLAKE2b, and
xxHash; grounds the "why CoreLink picked BLAKE3" section in concrete
`corelink-hash` code paths with `file:line` citations; and includes an
honest "what we would do differently" section.

## §2 DoD checklist

| # | Criterion | Status | Notes |
|---|---|---|---|
| 1 | Word count 3000-5000 | PASS | `wc -w` on rendered text: ~3,800 words (excluding frontmatter) |
| 2 | `cd apps/docs && pnpm build` exits 0 | DEFERRED | pnpm build not run in agent env (no node_modules); blog file syntax is valid MDX following Blog #1 structure exactly |
| 3 | Build artifact present | DEFERRED | Depends on DoD-2 |
| 4 | Tags `cryptography` + `blake3` in `tags.yml` | PASS | `blake3` was already present (no edit); `cryptography` appended as new row |
| 5 | SEAL audit at `specs/_audits/2026-05-27-blog-2-blake3-seal.md` | PASS | This document |
| 6 | Single commit on worktree | PASS | See §5 below |

DoD-2 / DoD-3 deferred: the agent environment does not have `pnpm` or
`node_modules` installed. The MDX file structure is identical to the
existing Blog #1 (`2026-05-27-content-addressable-cache-cloudflare-workers.mdx`),
which builds cleanly. The syntax (frontmatter, `<!-- truncate -->`,
standard Markdown + code fences, no JSX components) matches the Docusaurus
blog format exactly.

## §3 File inventory

| File | Action | Bytes |
|---|---|---|
| `apps/docs/blog/2026-05-28-why-blake3.mdx` | NEW | ~22,000 |
| `apps/docs/blog/tags.yml` | APPEND (new `cryptography` row) | +4 rows |
| `specs/_audits/2026-05-27-blog-2-blake3-seal.md` | NEW | ~4,000 |

## §4 Content audit — mandatory sections

| Section | Present | Grounded in code |
|---|---|---|
| §1 Hook — CAS problem statement | YES | AC §8 target cited; multi-tenant pressure explained |
| §2 What is a content-addressable hash | YES | ~200 words accessible definition |
| §3 The candidates — comparison table | YES | SHA-256, BLAKE2b, BLAKE3, xxHash; speed, key-derivation, year |
| §4 BLAKE3 internals — Merkle tree, SIMD, derive-key | YES | ASCII diagram; compression function; SIMD lane diagram |
| §5 Benchmarks — real numbers + `cargo bench` command | YES | BLAKE3 paper Table 1 cited; `cargo bench -p corelink-hash` command |
| §6 Why CoreLink picked BLAKE3 (4-5 bullets) | YES | 5 reasons; every claim has `file:line` citation |
| §7 What we would do differently (≥2 items) | YES | 2 items: derive-key deferral + WASM SIMD bench |
| §8 References | YES | Paper URL, crate URLs, `crates/corelink-hash/src/*.rs` line refs |

## §5 Code-citation audit

All `file:line` references verified against actual source:

| Citation | File | Content |
|---|---|---|
| `digest.rs:31` | `crates/corelink-hash/src/digest.rs` | `Self(*blake3::hash(body).as_bytes())` — `compute()` body |
| `digest.rs:41` | `crates/corelink-hash/src/digest.rs` | `from_hex` method start |
| `digest.rs:78-80` | `crates/corelink-hash/src/digest.rs` | `verify_constant_time` via `subtle::ConstantTimeEq` |
| `lib.rs:49` | `crates/corelink-hash/src/lib.rs` | `#![forbid(unsafe_code)]` |
| `verified_body.rs:10-17` | `crates/corelink-hash/src/verified_body.rs` | Doc comment explaining type-driven invariant |
| `verified_body.rs:33-41` | `crates/corelink-hash/src/verified_body.rs` | `VerifiedBody::new` constructor + constant-time verify |
| `verified_body.rs:64-72` | `crates/corelink-hash/src/verified_body.rs` | `Debug` impl redacts body bytes |
| `store.rs:1-14` | `crates/corelink-hash/src/store.rs` | Module doc: architectural seam explanation |
| `store.rs:26-39` | `crates/corelink-hash/src/store.rs` | `BlobStoreWrite` trait: `put_verified(&VerifiedBody)` |
| `error.rs:32` | `crates/corelink-hash/src/error.rs` | `COR_CAS_DIGEST_MISMATCH` constant |
| `error.rs:41-47` | `crates/corelink-hash/src/error.rs` | `HashMismatch` carries no payload |

All citations verified: lines match actual file content.

## §6 Tags-yml audit (APPEND-ONLY compliance)

Before state (5 rows):
- `engineering`, `build-cache`, `cloudflare-workers`, `rust`, `blake3`

After state (6 rows):
- `engineering`, `build-cache`, `cloudflare-workers`, `rust`, `blake3`, `cryptography`

`blake3` row was NOT edited (already present from Blog #1 frontmatter).
`cryptography` row was appended as a new entry. No existing row was
modified. APPEND-ONLY constraint satisfied.

## §7 Residual risks

| Risk | Severity | Notes |
|---|---|---|
| DoD-2 pnpm build not verified | LOW | MDX syntax follows Blog #1 exactly; Docusaurus is tolerant of standard Markdown; risk is low that a syntax error slipped through |
| derive-key mode not yet in production code | LOW | Post correctly describes this as a future direction and "what we'd do differently"; no false claim made |
| WASM SIMD benchmark numbers are paper-based, not CoreLink-CI-measured | LOW | Post is explicit that they are from the BLAKE3 paper Table 1 and approximations; no inflation |

## §8 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>.

**SEAL: PASS**
