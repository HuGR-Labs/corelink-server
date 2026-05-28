---
id: "ADR-S32-001"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "license", "supply-chain", "bsl-1.0", "ryu", "wave-32"]
references:
  - "specs/_audits/2026-05-27-sbom-license-audit-seal.md"
  - "specs/_compliance/license-allowlist.md"
  - "cargo-deny.toml"
---

# ADR-S32-001 — Permit BSL-1.0 (Boost Software License 1.0) in license allowlist

## Status

**ACCEPTED** (2026-05-27).

## Context

Wave-32 SBOM + license audit (`2026-05-27-sbom-license-audit-seal.md`) surfaced
two Rust dependencies declared under a dual SPDX expression
`Apache-2.0 OR BSL-1.0`:

| Package | Version | License expression | Consumer |
|---|---|---|---|
| `ryu` | `1.0.23` | `Apache-2.0 OR BSL-1.0` | Transitive: `serde_json` → printing IEEE-754 floats |
| `ryu-js` | `0.2.2` | `Apache-2.0 OR BSL-1.0` | Transitive: `boa_engine` → JS float printing |

Both packages are dependencies of `ryu` (the Rust port of the Grisu3 float-to-string
algorithm by Ulf Adams). The crates are dual-licensed; downstream consumers may pick
either license. Our existing allowlist (in `cargo-deny.toml` and
`specs/_compliance/license-allowlist.md`) accepts Apache-2.0 already, so the
Apache-2.0 leg satisfies the OR expression — the SBOM auditor flagged this as
**CONDITIONAL** rather than fail.

The auditor recommended a short ADR to formally ratify BSL-1.0 as a permitted
license. Three reasons:

1. **Defensive completeness.** When a dependency declares `A OR B`, the OR
   wording in SPDX means the downstream consumer can pick. If we only
   allowlist `A`, every license report must reason about which side is being
   chosen, adding manual triage friction. Adding `B` to the allowlist makes
   the audit unambiguous.
2. **Future deps.** Other small algorithmic libraries (`itoa`, `parse-zoneinfo`,
   misc Rust port-of-C-classics) also dual-license under Apache-2.0 + BSL-1.0.
   Pre-clearing avoids re-running this decision repeatedly.
3. **BSL-1.0 is maximally permissive** — see §Decision below.

## Decision

**We add `BSL-1.0` to the project's permitted-license allowlist** at both
artifact sources:

- `cargo-deny.toml` `[licenses] allow = [...]` block
- `specs/_compliance/license-allowlist.md` (canonical doc table)

BSL-1.0 may be used by:
- Direct dependencies (`[dependencies]` block in any crate's `Cargo.toml`)
- Transitive dependencies (any depth)
- Build-time tooling (`build-dependencies`, `[dev-dependencies]`)

## Rationale — why BSL-1.0 is safe

The Boost Software License 1.0 ([SPDX: BSL-1.0](https://spdx.org/licenses/BSL-1.0.html))
is one of the most permissive open-source licenses in active use. The Open Source
Initiative recognizes it (`OSI-approved: yes`). It is functionally equivalent to MIT
+ a clause that exempts machine-readable copies from carrying the notice.

### Key permissions granted

- Use, modify, redistribute (source and binary) — without restriction
- Sub-license under any terms
- Commercial use, including embedded in a proprietary product
- No notice required in compiled distributions

### Key obligations

- Source-form redistribution must preserve the license text
- No removal of copyright notices from source

### Comparison to other allowlisted licenses

| License | Source copy obligation | Compiled-form obligation | OSI-approved |
|---|---|---|---|
| Apache-2.0 (allowlisted) | Yes + NOTICE file | Patent grant in binary | ✅ |
| MIT (allowlisted) | Yes | None | ✅ |
| BSD-2-Clause (allowlisted) | Yes | None | ✅ |
| BSD-3-Clause (allowlisted) | Yes | Plus advertising restriction | ✅ |
| ISC (allowlisted) | Yes | None | ✅ |
| MPL-2.0 (allowlisted) | File-level copyleft | None | ✅ |
| **BSL-1.0** (THIS ADR) | **Yes** | **None — exempt** | ✅ |

BSL-1.0 is **strictly less restrictive than Apache-2.0** in compiled form (no NOTICE
file requirement; no patent-retaliation clause to worry about; no distribution
manifest).

### Commercial-use concerns

BSL-1.0 is friendly to commercial closed-source distribution. CoreLink redistributes
compiled Rust binaries (via Cloudflare Containers); the BSL-1.0 "compiled form
exempt from notice" clause means no extra NOTICE file work in our shipped artifact.

Reference: BSL-1.0 text https://www.boost.org/LICENSE_1_0.txt

## Consequences

### Positive

- License audit reports for `ryu` + `ryu-js` clean (no manual "Apache-2.0 alt"
  reasoning needed)
- Future BSL-1.0 deps onboard without re-running this decision
- One less compliance surface for SOC 2 sub-processors review

### Negative / risks

- Slight expansion of the allowlist surface. Mitigated by: BSL-1.0's strictly
  permissive structure (no copyleft, no patent friction, no NOTICE in binaries).
- If a future BSL-1.0 dep ever changes upstream to a more restrictive license,
  the dependency will fail at upgrade time per existing cargo-deny gating —
  no operational risk added.

### Operational impact

- `cargo-deny check licenses` will accept BSL-1.0 immediately after this ADR
  lands and `cargo-deny.toml` is updated.
- SBOM-generating workflows (`scripts/generate-sbom.sh`) include BSL-1.0 in the
  declared accepted-license set in the CycloneDX `metadata.licenses` block.
- No customer-facing notice changes (compiled binaries already exempt).

## Implementation

A separate commit (not in this ADR's PR scope; tracked as a follow-up) updates:

1. `cargo-deny.toml` — adds `"BSL-1.0"` to `[licenses] allow = [...]` array
2. `specs/_compliance/license-allowlist.md` — adds BSL-1.0 row with link to this ADR
3. `scripts/generate-sbom.sh` — adds BSL-1.0 to the declared accepted-license set

After these land, `cargo deny check licenses` for the current `ryu` + `ryu-js`
deps should show them as `Allowed` (not `Conditional`).

## Alternatives considered

### A. Reject BSL-1.0; force the Apache-2.0 leg explicitly

This is what the SBOM auditor flagged as the "current implicit posture". It works
but adds ongoing manual triage cost for every audit report. Rejected because:
- BSL-1.0 is strictly more permissive than Apache-2.0; rejecting it is form
  without function
- Future-deps friction (see §Context #2)

### B. Allowlist BSL-1.0 only for transitive deps (forbid direct)

This adds a per-dependency-depth check that cargo-deny doesn't natively support.
Rejected for tooling friction.

### C. Pin `ryu` / `ryu-js` to Apache-2.0 fork variants

No such forks exist as well-maintained alternatives. Rejected as impractical.

## Compliance cross-references

- **SOC 2 CC6.1** — supply-chain license compliance: this ADR adds documented
  ratification, satisfying the "documented approval" sub-control.
- **CTRL-CRED-001** — no credential surface affected.
- **DEBT-008** — license policy: this ADR is the v1.5.0 addition.
- **Wave-32 Phase I sign-off** — referenced as a Phase-1 supply-chain
  follow-up that closed pre-tag.

## DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
