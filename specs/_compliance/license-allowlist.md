---
id: "CTRL-SUPPLY-005-LICENSE-ALLOWLIST"
type: "compliance"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["supply-chain", "license", "sbom", "compliance", "FF-HR-005"]
references:
  - "deny.toml §[licenses]"
  - "specs/_audits/2026-05-27-sbom-license-audit-seal.md"
---

# CoreLink License Allowlist (CTRL-SUPPLY-005)

> **Canonical source of truth** for permissible open-source licenses used by
> CoreLink dependencies. Enforced by `deny.toml [licenses]` at every PR
> that touches Rust code. NPM enforcement is manual (SBOM audit + this doc).
>
> Any addition requires: (1) ADR documenting the decision, (2) update to
> `deny.toml`, (3) update to this doc, (4) security review sign-off.

## §1 Allowed licenses (Rust + NPM)

| SPDX ID | Type | Notes |
|---|---|---|
| MIT | Permissive | Standard MIT license |
| Apache-2.0 | Permissive | Apache License 2.0 |
| Apache-2.0 WITH LLVM-exception | Permissive | Apache-2.0 + LLVM patent exception (rustc/LLVM-derived crates) |
| BSD-2-Clause | Permissive | 2-clause BSD |
| BSD-3-Clause | Permissive | 3-clause BSD |
| ISC | Permissive | ISC License (functionally equivalent to MIT) |
| MPL-2.0 | Weak copyleft | Mozilla Public License 2.0 — file-level copyleft only |
| Unicode-DFS-2016 | Permissive | Unicode data files distribution license |
| Unicode-3.0 | Permissive | Unicode License 3.0 (supersedes DFS-2016 for newer crates) |
| Zlib | Permissive | zlib/libpng license |
| CC0-1.0 | Public domain | Creative Commons Zero — no rights reserved |
| 0BSD | Permissive | Zero-clause BSD — public domain equivalent |
| CDLA-Permissive-2.0 | Permissive | Community Data License Agreement (data distribution); added 2026-05-14 for webpki-roots 1.0+ per ADR-S20-RSA-MARVIN-MITIGATION §3.2 |
| UNLICENSED | Proprietary | CoreLink workspace crates (closed-source); exempted via `[licenses.private]` in deny.toml |

## §2 Phase 1 dep additions (post-Wave 35 GA, 2026-05-27)

These dependencies were added during Phase 1 launch preparation. All licenses
fall within the existing allowlist — no new SPDX IDs required.

| Package | Version | License | Context |
|---|---|---|---|
| `@sentry/nextjs` | 8.55.2 | MIT | Sentry error tracking — admin-ui |
| `@sentry/cloudflare` | 8.55.2 | MIT | Sentry error tracking — CF Workers |
| `@sentry/node` | 8.55.2 | MIT | Sentry error tracking — server |
| `@clerk/nextjs` | 6.39.3 | MIT | Clerk authentication — admin-ui |
| `@posthog/node` | (workspace dep) | MIT | PostHog analytics (removed from sub-processors per 2026-05-27-sub-processors-finalization.md) |
| `resend` | (workspace dep) | MIT | Resend email — newsletter signup |
| `betterstack-js` | (workspace dep) | MIT | BetterStack status pill |

### Sentry FSSA Note (R21)

Sentry SDK packages (`@sentry/*`) are licensed under **MIT** (SDK core) and
**BSD-3-Clause** (CLI tooling). Both are on the allowlist. Sentry's
"Functional Software Support Agreement" (FSSA) is a **service agreement**
between Sentry Inc. and users of their hosted SaaS — it does not restrict
open-source redistribution rights of the MIT-licensed SDK. SBOM consumers
should note:

- The MIT SDK license grants full redistribution, modification, and sublicensing.
- The FSSA applies only to use of `sentry.io` hosted service; not to the SDK
  artifacts included in this SBOM.
- All `@sentry/*` packages at version 8.x carry SPDX: `MIT`.
- Legacy `@sentry/core@6.19.7` and related 6.x packages carry `BSD-3-Clause`.

No FSSA review is required for SBOM purposes. The SDK is an open-source
dependency; the hosted service agreement is a separate contract.

## §3 Explicitly denied licenses (org-wide)

| License | Reason |
|---|---|
| GPL-2.0, GPL-3.0 | Copyleft — incompatible with CoreLink hosted service + closed worker code |
| AGPL-3.0 | Network-copyleft — would require source disclosure of CF Worker code |
| SSPL-1.0 | SSPL requires open-sourcing all service infrastructure |
| Commons-Clause | Restricts commercial use — incompatible with SaaS model |
| BUSL-1.0 | Business Source License — non-compete restrictions |

## §4 Under review / edge cases

| SPDX ID | Status | Notes |
|---|---|---|
| MIT-0 | Acceptable (not yet in deny.toml) | Zero-attribution MIT variant. `constant_time_eq@0.4.2`, `dunce@1.0.5` use `CC0-1.0 OR MIT-0 OR Apache-2.0`. Since expressions include CC0-1.0 or Apache-2.0 (both allowed), these crates are permissible. Follow-up: add MIT-0 to deny.toml allowlist. |
| Unlicense | Acceptable (not yet in deny.toml) | Public domain dedication. `aho-corasick@1.1.4`, `memchr@2.8.0` use `Unlicense OR MIT`. Since expressions include MIT (allowed), these crates are permissible. Follow-up: add Unlicense to deny.toml allowlist. |
| BSL-1.0 | REVIEW REQUIRED | Boost Software License 1.0. `ryu@1.0.23`, `ryu-js@0.2.2` use `Apache-2.0 OR BSL-1.0`. BSL-1.0 is a permissive license with minimal requirements. However it is not yet in the allowlist. Since the expression includes Apache-2.0 (allowed), `cargo-deny` should be able to satisfy either clause. ADR recommended before next SBOM refresh. |
| Apache-2.0 WITH LLVM-exception | Allowed (in deny.toml) | Parser false-positive in audit script: `target-lexicon@0.13.5` and `rustix@1.1.4` carry this. Already in `deny.toml allow[]`. |

## §5 Audit history

| Date | Change | Auditor |
|---|---|---|
| 2026-05-27 | v1.2.0 — Initial formal allowlist doc; Phase 1 dep additions catalogued; Sentry FSSA R21 paragraph added; 4 edge cases identified (MIT-0, Unlicense, BSL-1.0, LLVM-exception false positive) | Claude Sonnet 4.6 (WP-7.2) |
| 2026-05-14 | CDLA-Permissive-2.0 added (R1-9 emergency dep update, webpki-roots) | see ADR-S20-RSA-MARVIN-MITIGATION |

## §6 Follow-up actions required

1. **ADR for MIT-0** — add MIT-0 to `deny.toml [licenses] allow[]`. Low risk, equivalent to public domain.
2. **ADR for Unlicense** — add Unlicense to `deny.toml [licenses] allow[]`. Public domain; already licensed dual with MIT.
3. **ADR for BSL-1.0 review** — `ryu@1.0.23` / `ryu-js@0.2.2`. Either (a) confirm Apache-2.0 clause satisfies per `deny.toml` expression handling, or (b) add BSL-1.0 to allowlist with justification.
4. **NPM transitive SBOM** — current NPM SBOMs capture direct deps only (50 admin-ui, 25 docs). Future refresh should expand to full transitive closure via `pnpm install` + cdxgen with node_modules present.
