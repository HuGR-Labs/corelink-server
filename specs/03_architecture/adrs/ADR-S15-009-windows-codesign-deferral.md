---
id: "ADR-S15-009"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
wi: "WI-S15-006"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags:
  - "adr"
  - "s15"
  - "ship-gate"
  - "windows"
  - "authenticode"
  - "codesign"
  - "deferral"
  - "ctrl-cas-002"
  - "lote-10-15-codex-p1"
supersedes: null
superseded_by: null
---

# ADR-S15-009 — Windows Authenticode Cert: No Unsigned Fallback; Deferral Path Only

## Status

**ACTIVE** — Ratified 2026-05-14, WI-S15-006.

---

## Context

S-15 ship gate (WI-S15-006) requires **3 OSes signed** release binaries:
macOS notarized + Linux GPG + Windows Authenticode. The Windows Authenticode
path requires an **EV (Extended Validation) code-signing certificate** from
DigiCert / Sectigo / GlobalSign with a 1–2 week typical lead time and
$300–500/year amortised cost.

A prior draft of WI-S15-006 (cycle 12.S15.0 v1.0.0) admitted an
"unsigned-with-warning fallback" for Windows if cert acquisition slipped
past the sprint window. **Lote 10.15 codex P1 review rejected this fallback**
on two grounds:

1. **CAP-CLI-002 promise weakening**: "Cross-OS distribution signed (3 OSes)"
   is a hard sprint contract DoD §6 line. Allowing unsigned Windows with a
   waiver dilutes the marketing claim and the customer trust signal.
2. **Customer harm**: Windows users see SmartScreen warnings on first launch
   of unsigned executables. The reputational + onboarding friction cost is
   non-trivial.

We need a decision that preserves the "3 OSes signed" invariant without
blocking the entire sprint on Windows cert procurement.

---

## Decision

### D1 — No unsigned Windows release shipment under any circumstance.

The `.github/workflows/sign-windows.yml` workflow is gated by the
`windows-cert-present` job. If `WINDOWS_CODE_SIGNING_CERT` is absent, the
signing step is **skipped silently**; no unsigned `corelink-windows-x86_64.zip`
is published as a Release asset.

### D2 — macOS + Linux ship at D+15 SEAL even if Windows cert slips.

`release-cli.yml` produces unsigned binaries for all 5 targets. The notarize +
GPG workflows operate on macOS / Linux assets independently; they do not gate
on Windows. Sprint SEAL ceremony D+15 therefore lands on a verified two-OS
signed release plus a documented Windows deferral.

### D3 — Windows ships in the subsequent sprint, contingent on cert ready.

When the EV cert is delivered + the secret populated, a manual
`workflow_dispatch` against the previously-released tag produces the signed
Windows binary and uploads it as an additional release asset (the `.zip` is
mutable via `gh release upload --clobber`). End users on Windows therefore
get a delayed-but-signed binary; they are not asked to run unsigned code.

### D4 — Sprint promise updated.

Per Lote 10.15 codex P1, the sprint contract S-15 CAP-CLI-002 expands to:

> "macOS + Linux signed at D+15 SEAL; Windows signed within +1 sprint
> contingent on cert acquisition."

WI-S15-006 §6.6 completeness criterion is satisfied by either:

- Windows Authenticode-signed at D+15 SEAL (cert acquired on time), **or**
- Windows release deferred per this ADR + tracked as an explicit follow-up
  in the next sprint's backlog with `expires_at = sprint_end + 30d`.

---

## Consequences

### Positive

- "3 OSes signed" remains an honest claim: when the third OS ships, it ships
  signed. We do not silently downgrade the trust posture.
- No SmartScreen warnings for end users. Reputational risk averted.
- Sprint timeline decoupled from EV cert vendor SLA variance.
- No new ADR class (this is a deferral ADR; it does not introduce a new
  invariant, control, or capability).

### Negative / Trade-offs

- Windows users wait 1–N weeks longer for their binary on the first
  release. This is a one-time cost; subsequent releases sign in the same
  pipeline run as macOS / Linux.
- Sprint SEAL evidence pack must explicitly mark Windows as "deferred"
  rather than "complete". PRR-S15 §3 records this state.
- The deferral path is **not** an indefinite waiver: if Windows is still
  unsigned at +1 sprint, a post-mortem is opened (WI-S15-006 §24).

### Invariants Preserved

- **CAP-CLI-002**: "Cross-OS distribution signed" — preserved by ensuring no
  unsigned ship; the promise becomes "macOS + Linux at D+15, Windows at
  D+15 or D+15 + 1 sprint".
- **CTRL-CAS-002**: client-verify default-on is orthogonal to OS signing
  (covered by FFI in WI-S15-004); not affected.
- **CTRL-CRED-001**: PAT redaction is not affected by signing flow.

---

## References

| Reference | Type | Purpose |
|---|---|---|
| WI-S15-006 §6.4 | WI | Windows Authenticode deliverable |
| `_spec_contract.md` §4 (CAP-CLI-002) | Spec contract | "3 OSes signed" capability |
| `_spec_contract.md` §6 (DoD) | Spec contract | DoD line: "CLI distribuído + signed em 3 OSes" |
| Lote 10.15 codex P1 | Review | Removes prior "unsigned-with-warning" fallback |
| `.github/workflows/sign-windows.yml` | Workflow | Implements gate (D1) |
| ADR-S14-008 | ADR (template) | Frontmatter pattern for sprint ADRs |
| Microsoft Authenticode | Standard | <https://learn.microsoft.com/en-us/windows-hardware/drivers/install/authenticode> |

---

*ADR-S15-009 · Version 1.0.0 · 2026-05-14 · WI-S15-006.*
