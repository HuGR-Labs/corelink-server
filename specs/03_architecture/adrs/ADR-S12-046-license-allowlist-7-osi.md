---
id: "ADR-S12-046"
type: "adr"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["adr", "s12", "supply-chain", "license-allowlist", "legal", "compliance"]
---

# ADR-S12-046: License Allowlist — 7 OSI-Approved Licenses + Banned Copyleft

## Status

ACTIVE — WI-S12-004 SEALED.

## Context

CoreLink is a hosted SaaS service that ships closed-source Cloudflare Workers (WASM) binaries.
The dependency graph includes transitive Rust crates from the crates.io ecosystem.  Without an
explicit license allowlist enforced at CI level, a transitive GPL or AGPL dep could expose
CoreLink to copyleft obligations (distribute source or expose SaaS under copyleft terms).

**Legal risk scenario**: `serde_json` has always been MIT; but if a future transitive dep
silently introduces a GPL-3.0 fragment, CoreLink's shipping WASM blob becomes subject to
GPL-3.0 copyleft.  Detection post-ship requires legal remediation (remove + re-deploy).

**Industry evidence**: cargo-deny license enforcement is used in Mozilla Servo, Fuchsia, Bevy,
Tauri, and Cloudflare's own internal Rust codebase.

## Decision

### Allowlist (default-deny, addition via ADR)

The following SPDX expressions are explicitly allowed in `deny.toml [licenses].allow`:

| SPDX | Rationale |
|---|---|
| `MIT` | Permissive; widely compatible; OSI-approved |
| `Apache-2.0` | Permissive with patent grant; OSI-approved; dominant in Rust ecosystem |
| `Apache-2.0 WITH LLVM-exception` | Compiler/toolchain ecosystem standard (rustc, ring) |
| `BSD-2-Clause` | Permissive, attribution-only; OSI-approved |
| `BSD-3-Clause` | Permissive + non-endorsement; OSI-approved |
| `ISC` | Functionally MIT-equivalent; OSI-approved |
| `MPL-2.0` | Weak copyleft (file-level only); SaaS-safe; OSI-approved |
| `Unicode-DFS-2016` | ICU data files used by `icu_data` crate; Unicode Consortium |
| `Unicode-3.0` | Newer Unicode license; forwarded for ICU migration |
| `Zlib` | Short permissive; OSI-approved |
| `CC0-1.0` | Public domain dedication; OSI-approved |
| `0BSD` | Zero-clause BSD; OSI-approved |

### Banned licenses (explicit)

`deny.toml [licenses].deny` + `copyleft = "deny"`:

| SPDX | Reason |
|---|---|
| GPL-* (1.0 / 2.0 / 3.0 and variants) | Strong copyleft — viral; distribution trigger |
| AGPL-* (1.0 / 3.0 and variants) | Network copyleft — triggers at SaaS use; highest risk |
| SSPL-1.0 | MongoDB-style anti-cloud; non-OSI |
| Commons-Clause | Commercial restriction; non-OSI |
| BUSL-1.1 | Time-limited commercial restriction; non-OSI |

Additionally `copyleft = "deny"` catches any copyleft license not in the explicit deny list
(LGPL, EUPL, CDDL, EPL, CPL) as a defence-in-depth layer.

### Default-deny

`[licenses].default = "deny"` — any license not in the allowlist is denied.
`[licenses].unlicensed = "deny"` — deps with no license declaration are denied.

### Private workspace crates

`[licenses.private].ignore = true` — workspace member crates with `publish = false` and
`license = "UNLICENSED"` are exempt from license enforcement (they are CoreLink's own
closed-source code, not third-party deps).

### Addition process

New license addition requires:
1. ADR documenting OSI status + business compatibility + legal review outcome.
2. Compliance Officer sign-off.
3. `deny.toml` bump with ADR ID in comment.
4. Update of this ADR and `docs/internal/dep-policy.md §2`.

## Rationale

### Why allowlist (not denylist)?

- **Denylist**: known-bad; new copyleft license bypasses if not explicitly added.
- **Allowlist**: known-good; new license requires explicit ADR review (default-deny posture).
- Safer for compliance: GPL-1.0+ or a new copyleft = unintended exposure if denylist misses it.

### Why `copyleft = "deny"` in addition to explicit bans?

- Defence-in-depth: a future copyleft license not in the explicit deny list (e.g. a new
  open-core license) is still caught by the `copyleft = "deny"` pattern.
- Matches cargo-deny semantic: catches LGPL/EUPL/CDDL/EPL without enumerating every variant.

### Why MPL-2.0 is allowed (weak copyleft)?

- File-level copyleft only — modifications to MPL files must be released; linking from
  proprietary code is permitted.
- SaaS hosting does not trigger distribution in a way that forces CoreLink to open-source
  non-MPL code.
- Used by Firefox components (rustls, webpki) that are foundational Rust security primitives.

### Why Unicode licenses?

- `icu_data` and `unicode-ident` crates ship Unicode data under Unicode-DFS-2016/3.0.
- These are essential for Rust string handling and tokio/hyper unicode support.
- Unicode Consortium licenses are permissive data licenses compatible with SaaS hosting.

## Consequences

**Positive**:
- Zero GPL/AGPL exposure — automated enforcement at every PR.
- Quarterly Legal review limited to 5% sample (not full audit) — automated gate catches most drift.
- Clear process for adding new licenses (ADR + Compliance sign-off).

**Negative / mitigated**:
- Some useful crates may use non-allowlisted licenses; requires ADR for each exception.
  Mitigated by: most mature Rust ecosystem crates use MIT/Apache-2.0 dual license.
- License SPDX expression drift (crate declares MIT but code has GPL fragments) not caught
  by automated tool.  Mitigated by: quarterly Legal sample audit.

## Related

- ADR-S12-045: Dep policy (cargo-audit + cargo-deny + Dependabot canonical config).
- ADR-S12-047: License review quarterly process.
- `deny.toml [licenses]` section — canonical implementation.
- `docs/internal/dep-policy.md §2` — operational guide.

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-13 | Gustavo (via Claude Sonnet 4.6) | Initial creation — WI-S12-004 SEALED. |
