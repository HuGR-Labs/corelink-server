---
id: "COMPARE-SCCACHE-S3-SEAL-2026-05-27"
type: "audit"
doc_status: "SEALED"
audit_status: "CLOSED"
version: "1.0.0"
created: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["comparison", "sccache", "wave32", "phase4", "content", "wp-3.2"]
references:
  - "specs/_audits/2026-05-27-15-agent-dispatch-matrix.md"
  - "apps/docs/src/pages/compare/vs-sccache-s3.mdx"
---

# SEAL audit — WP-3.2: CoreLink vs sccache + S3 comparison page

## Summary

WP-3.2 delivered the comparison page **CoreLink vs sccache + S3** per
the 15-agent dispatch matrix §2 contract. Subject: sccache (Mozilla's
compiler cache, Rust/C++/Swift) paired with an S3-compatible object
store as the shared remote cache backend.

## DoD checklist

| # | Criterion | Result |
|---|---|---|
| 1 | Word count 1500–2500 (rendered text, no code blocks) | ✅ 2491 prose words |
| 2 | `pnpm build` in `apps/docs/` exits 0 | DEFERRED — build not run in agent env (no pnpm context); content is valid MDX, frontmatter YAML is well-formed |
| 3 | 4 locales present (EN + pt-BR + es-419 + de stubs) | ✅ All 4 present |
| 4 | Page links from comparison index (or footer compare-list) | N/A — no compare index exists; compare pages are standalone; Resources section in each page cross-links via `/docs/pricing/comparison` per the existing pattern |
| 5 | All external URLs return HTTP 200 | ✅ 4/4 verified (see below) |
| 6 | SEAL audit doc committed | ✅ This file |
| 7 | Single commit on worktree | ✅ (committed below) |

## Files delivered

| File | Role |
|---|---|
| `apps/docs/src/pages/compare/vs-sccache-s3.mdx` | Primary EN comparison page, 2491 prose words |
| `apps/docs/i18n/pt-BR/docusaurus-plugin-content-pages/compare/vs-sccache-s3.mdx` | pt-BR stub with canonical EN link |
| `apps/docs/i18n/es-419/docusaurus-plugin-content-pages/compare/vs-sccache-s3.mdx` | es-419 stub with canonical EN link |
| `apps/docs/i18n/de/docusaurus-plugin-content-pages/compare/vs-sccache-s3.mdx` | de stub with canonical EN link |
| `specs/_audits/2026-05-27-compare-sccache-s3-seal.md` | This SEAL document |

## Structure verification

Page sections delivered (per WP-3.1 contract structure, with sccache substitutions):

1. TL;DR — 3 bullets: pick sccache, pick CoreLink, either is fine
2. At a glance — capability comparison table (14 rows)
3. Where sccache wins — 5 wins: native interception, free software,
   wide compiler support, cloud-agnostic storage backend, active
   Mozilla/Rust community
4. Where CoreLink wins — 6 wins: zero ops overhead, REAPI v2,
   polyglot (npm/pip/brew/OCI), BYOK 4-KMS, RFC 6962 audit log,
   TLA+-verified multi-tenant isolation
5. Cost comparison — 3 scale points: 10 GB, 500 GB, 5 TB with
   sccache + S3 (AWS Standard) vs CoreLink Free/Pro/Enterprise
6. When to use which — 3-question decision tree with end nodes
7. Migration path — 7-step parallel-run pattern (shadow mode,
   cut-over, polyglot expand, BYOK)
8. Sources — bibliography with 4 verified external URLs

## External URL verification

Checked with `curl -sI --max-time 10 -L` on 2026-05-27:

| URL | HTTP Status |
|---|---|
| https://github.com/mozilla/sccache | 200 ✅ |
| https://github.com/mozilla/sccache/blob/main/docs/S3.md | 200 ✅ |
| https://aws.amazon.com/s3/pricing/ | 200 ✅ |
| https://aws.amazon.com/ec2/pricing/on-demand/ | 200 ✅ |

## Prose word count

```
Methodology: strip frontmatter (---...---), strip fenced code blocks
(```...```), strip inline code (`...`), strip table separator rows,
count whitespace-delimited tokens.

Result: 2491 words (target: 1500–2500). PASS.
Raw wc -w (including code blocks, frontmatter): 2673 words.
```

## Honesty bar

- sccache wins acknowledged clearly: free, well-known, native
  compiler interception (rustc / GCC / Clang / MSVC / NVCC / Swift),
  cloud-agnostic backend, active Mozilla community.
- All CoreLink claims grounded in charter features: BYOK (4 KMS),
  REAPI v2, polyglot adapters, RFC 6962 audit chain, TLA+-verified
  isolation. No inflated claims made.
- Cost comparison uses public AWS S3 pricing (fetched 2026-05-27)
  with explicit note to verify before quoting. sccache + S3 raw
  dollar cost advantage at small scale (10 GB) acknowledged honestly.

## Residual risks / notes

- `pnpm build` not run in agent environment. The MDX content and
  frontmatter are well-formed; the file follows the same structure
  as the 3 existing comparison pages that do build successfully.
  Orchestrator should run `cd apps/docs && pnpm build` as part of
  the merge acceptance gate.
- i18n stubs use the same "Coming soon — see English version"
  pattern. No machine-translated content was generated to avoid
  introducing unreviewed translations.
- No compare index page exists in the codebase (verified). Consistent
  with WP-3.1 treatment. The `/docs/pricing/comparison` link in the
  Resources section points to the existing capability matrix page.
