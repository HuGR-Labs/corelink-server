---
id: "ADR-S30-001"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  - { role: "security_lead", name: "Gustavo Schneiter (interim until hire)" }
  - { role: "security", name: "Crypto SME — TBD (assign at next quarterly security review)" }
supersedes: null
superseded_by: null
tags: ["adr", "s30", "byok", "feature-flags", "compile-error", "ap-11", "techlead", "wave-30", "singleton", "audit"]
references:
  - "specs/_audits/2026-05-15-byok-real-provider-pattern.md"
  - "specs/_audits/2026-05-16-byok-ap11-adr-formalization.md"
  - "apps/server/src/byok_orchestrator.rs"
  - ".claude/skills/techlead/SKILL.md"
inv: ["INV-BYOK-CRYPTO-SOVEREIGNTY", "INV-KEY-OVERLAP"]
---

# ADR-S30-001 — BYOK Real Provider Mutually-Exclusive Cargo Features (singleton, compile-time)

## Status

**ACCEPTED — 2026-05-16** (wave-30 stream-8, R-prep BYOK AP-11 ADR
formalization). Promotes the §7 "design exception" that has lived inside
the wave-15 BYOK pattern audit since 2026-05-15 into a first-class
architectural decision so reviewers, `/techlead` automation, and future
contributors stop re-discovering it as a verification finding.

This ADR is **canonical** for the `cargo build --workspace
--all-features` failure mode in the CoreLink repository: that failure
is **declared, intentional, and protected by `compile_error!`** — not a
build regression.

## Context

### The 4-provider BYOK surface

CoreLink GA ships envelope-encryption support across four KMS providers
(see `specs/_audits/2026-05-15-byok-real-provider-pattern.md`):

| Provider | Adapter crate | Real-provider feature flag |
|---|---|---|
| AWS KMS | `corelink-byok-aws` | `byok-aws-real` |
| GCP Cloud KMS | `corelink-byok-gcp` | `byok-gcp-real` |
| Azure Key Vault (Premium / Managed HSM) | `corelink-byok-azure` | `byok-azure-real` |
| HashiCorp Vault | `corelink-byok-vault` | `byok-vault-real` |

The server's BYOK orchestrator (`apps/server/src/byok_orchestrator.rs`)
exposes exactly one `Arc<dyn KmsProvider>` to the rest of the binary —
threaded into envelope encrypt / decrypt, the kill-switch poller, and
erasure attestation. The choice of concrete provider is **resolved at
compile time** via cargo feature flags; the orchestrator does not
attempt runtime provider selection.

### Why runtime multi-provider was rejected

Two structural reasons make a runtime "pick a provider per request"
trait object unsafe for CoreLink:

1. **Crypto-sovereignty surface bloat (INV-BYOK-CRYPTO-SOVEREIGNTY).**
   Each real provider links a fundamentally different SDK + TLS stack +
   credential resolver: `aws-sdk-kms` (smithy/hyper), GCP's
   `cloudkms.googleapis.com` REST client (`reqwest` + `jsonwebtoken`),
   Azure Key Vault REST + Entra ID workload-identity federation, and
   HashiCorp Vault Transit (four auth modes). Linking all four into a
   single binary triples the supply-chain attack surface and the
   FIPS-validated-component matrix, and creates a runtime path where
   one customer's request could touch another provider's credential
   resolver. The four providers each have their own FIPS attestation
   (`specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`); a binary that
   links all four advertises a FIPS posture it does not actually have.

2. **Audit-chain fork (INV-KEY-OVERLAP).** Per-tenant provider mixing
   means the BLAKE3-linked audit chain
   (`corelink.byok.<provider>.audit`) would emit interleaved events
   from up to four trust roots inside the same chain. The
   `audit_status` rotation, the kill-switch poller, and the erasure
   attestation (Ed25519/JCS — see ADR-S14-007) all assume a single
   provider identity per binary. A runtime split would force a
   per-provider sub-chain plus a join discipline at audit-export time;
   the engineering cost is high and the security gain is zero (the
   tenant boundary is already enforced at the AAD layer, see
   ADR-S14-005).

### What the wave-18 incident exposed

On 2026-05-14, the L0+L2 batch verifier (Sonnet) reported `cargo build
--workspace --all-features: pass` for four wave-18 branches even
though the `--all-features` build had been structurally broken since
commit `818c055` due to the BYOK orchestrator's `compile_error!`
guard. The verifier collapsed the feature topology into a single
boolean cell. Four merges shipped with verification debt before the
retro caught it.

The fix in `/techlead` v2.1.0 (commit `ba3ef2d`) was procedural: L0.6
discovery + L1.3a 4-row sub-matrix + AP-11 anti-pattern entry. That
hardens the **verifier**. This ADR closes the loop on the **design**
side: making the `compile_error!` a ratified architectural decision
rather than an audit-document footnote (`specs/_audits/2026-05-15
-byok-real-provider-pattern.md §7.2`), so reviewers, fresh-onboarding
engineers, and future Sonnet verifiers find a first-class ADR when
they grep for "byok mutually exclusive".

## Decision

The CoreLink BYOK orchestrator (`apps/server/src/byok_orchestrator.rs`)
**enforces mutually-exclusive cargo feature flags via
`compile_error!`** for the four real-provider features. The
authoritative rules are:

### D1. Exactly one real-provider feature per binary

For every CoreLink server build, at most one of the following cargo
features may be active:

```
byok-aws-real | byok-gcp-real | byok-azure-real | byok-vault-real
```

Enabling zero of them yields the default `InMemoryFake` provider
(non-production; dev / unit-test / CI smoke path only —
`ActiveProvider::InMemoryFake`). Enabling **two or more** is a HARD
**compile error**.

### D2. Pairwise `compile_error!` for diagnostic precision

The mutually-exclusive constraint is implemented as `C(4,2) = 6`
pairwise `compile_error!` macros (one per provider pair) so the
diagnostic names the exact two flags in conflict, e.g.:

```
error: BYOK orchestrator: features `byok-aws-real` AND `byok-gcp-real`
       are mutually exclusive — only one BYOK real provider may be
       enabled at a time (the orchestrator is a singleton trait
       object). See specs/_audits/2026-05-15-byok-real-provider-pattern.md §7.
```

A single "more than one set" guard would be cheaper to write but
worse to triage during multi-feature CI experiments.

### D3. `cargo build --workspace --all-features` fails by design

The standard workspace-wide build-with-all-features command **WILL
fail** when run against `corelink-server`. This is the documented,
ratified behavior — not a regression. CI does **not** invoke
`--all-features` for `corelink-server`; it runs a per-provider
matrix (see D4). Any future contributor or verifier surprised by this
failure should be redirected to **this ADR**.

### D4. CI runs per-provider matrix instead

The CI lane that exercises real-provider wiring runs five build
configurations against `corelink-server`:

```
cargo build -p corelink-server                                     # InMemoryFake default
cargo build -p corelink-server --features byok-aws-real            # AWS KMS singleton
cargo build -p corelink-server --features byok-gcp-real            # GCP Cloud KMS singleton
cargo build -p corelink-server --features byok-azure-real          # Azure Key Vault singleton
cargo build -p corelink-server --features byok-vault-real          # HashiCorp Vault singleton
```

Plus the orchestrator integration test
(`apps/server/tests/byok_orchestrator.rs`) under each of the five
configurations. This is the canonical replacement for the
`--all-features` macro pattern; it gives strictly more signal because
each row exercises a distinct provider's credential / SDK / FIPS
endpoint resolution path.

### D5. `/techlead` AP-11 ratification cite

This ADR is the authoritative reference for the `/techlead` skill's
L1.3a `✗ (design)` exception row and AP-11 anti-pattern entry. Any
`--all-features` failure inside a Sonnet verifier's output that lacks
the AP-11 exception block with this ADR cited under `audit ref` is
itself a charter violation — the verifier must re-emit the L1.3a
sub-matrix with the populated exception block.

The skill's AP-11 entry is updated to cite
`specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md`
in addition to the wave-15 baseline audit.

## Consequences

### Positive

- **Single canonical reference** for the `--all-features` failure mode.
  Reviewers grep "byok mutually exclusive" / "AP-11" / "compile_error"
  and land on this ADR.
- **Compile-time guarantee** that production binaries link exactly one
  real provider — no runtime drift possible.
- **Diagnostic clarity**: 6 pairwise `compile_error!` macros name the
  exact pair in conflict.
- **FIPS-matrix integrity**: each shipped binary advertises exactly
  one provider's FIPS posture, so the
  `BYOK-FIPS-ATTESTATION-MATRIX.md` column the binary claims is the
  one a customer can actually audit.
- **Audit-chain integrity**: one `target =
  "corelink.byok.<provider>.audit"` stream per binary; no cross-
  provider interleave.

### Negative / Accepted

- `cargo build --workspace --all-features` fails — by design.
  Documented here + at `/techlead` AP-11 + in the workspace `README`
  build matrix.
- CI matrix size: 5 build configurations per
  `corelink-server`-touching commit (1 default + 4 real-provider).
  Acceptable cost (each config <2 min on the workspace cache); the
  alternative is `--all-features` red, which gives zero useful signal.
- Multi-tenant BYOK with per-tenant provider mixing is **not
  supported at the binary level**. Customers needing AWS-tenant +
  GCP-tenant in the same operator deployment must run two
  `corelink-server` instances behind a routing fronted by the
  control-plane (a tenant → provider lookup feeds the orchestrator
  selector at deploy time, not runtime). This is the documented GA
  shape; see `ROADMAP-TO-GA.md`.

### Risk register (L9 cross-reference)

- **R1 (Low):** A contributor in a one-off experiment toggles two
  features and is surprised by a compile error. Mitigation: the
  `compile_error!` message explicitly cites the audit ref, and now
  this ADR.
- **R2 (Low):** A future verifier (Sonnet or human) reports
  `--all-features: pass` falsely. Mitigation: `/techlead` v2.1.x L0.6
  + L1.3a sub-matrix + AP-11 refusal — strictly enforced.
- **R3 (Negligible):** A customer requests "one binary, multiple
  providers". Mitigation: documented as not-supported; multi-binary
  routing is the GA shape and accepted by the design partners.

## Alternatives considered

### Alt-A: Runtime provider selection via `provider_kind` config

Read `BYOK_PROVIDER=aws|gcp|azure|vault` from env at boot and
instantiate the corresponding trait object. The binary would link all
four SDKs unconditionally.

**Rejected** because:

- Surface bloat: all four SDKs + TLS stacks + credential chains
  always linked → supply-chain attack surface, FIPS-validated-component
  matrix sprawl, and binary size (~40MB extra for the inactive three).
- One customer's mis-configured `BYOK_PROVIDER` could surface
  credentials from another customer's provider in error logs (the
  credential chains are eager at SDK construction in `aws-sdk-kms`
  and `google-cloud-kms`).
- No structural benefit: deployment-time selection is identical in
  outcome to compile-time selection, only worse in attack surface.

### Alt-B: Dynamic trait object across all four providers (factory pattern)

`Box<dyn KmsProvider>` resolved from a `ProviderFactory` registry at
boot.

**Rejected** because:

- Same surface-area problem as Alt-A (all four SDKs linked).
- The orchestrator's const-time keying
  (`active_provider() -> ActiveProvider`) becomes a runtime value;
  the `/healthz` and `/readyz` probes lose their compile-time
  invariant ("this binary IS the AWS binary"), which is load-bearing
  for the customer onboarding flow.
- `Arc<dyn KmsProvider>` is already a trait object; the optimization
  argument ("we already pay vtable cost") misses the point — the
  static dispatch we lose is the **link-time** dispatch, not the
  call-time one.

### Alt-C: Feature flag with `required-features = ["one-of-the-byok-real"]`

Cargo does not natively support "exactly one of" on the
`required-features` line; we could approximate via a single
`compile_error!` that lists all 6 conflicting pairs.

**Rejected** because:

- The diagnostic loses the pair-naming property (D2). A contributor
  who sets `byok-aws-real + byok-gcp-real` should see a message
  naming AWS + GCP, not a generic "more than one provider".
- The 6 pairwise macros are 6 short blocks of code; the cost is
  negligible.

### Alt-D: Move the `compile_error!` into a build script (`build.rs`)

**Rejected** because:

- `build.rs` runs in `cargo check` / IDE flows AFTER feature
  resolution but BEFORE source compilation; the error message would
  surface, but build-script errors are noisier and harder to filter
  in CI logs than direct `compile_error!` diagnostics.
- `compile_error!` in `src/lib.rs` (or `src/byok_orchestrator.rs`) is
  the idiomatic Rust pattern (`rust-lang/rust` issue #54489 ratifies
  it for feature-conflict signalling); deviating from idiom buys
  nothing.

## Implementation

The decision is already implemented (since wave-15 commit `818c055`,
documented at `specs/_audits/2026-05-15-byok-real-provider-pattern.md
§7.2`). The implementation lives in
`apps/server/src/byok_orchestrator.rs`:

- Lines 76–117: six pairwise `compile_error!` macros (4 choose 2 = 6
  pairs).
- Lines 119–183: `ActiveProvider` enum + `active_provider()` const fn
  (compile-time discriminator).
- Lines 208–272: `make_provider()` + `build_active()` dispatch using
  `cfg(feature = "byok-*-real")` arms.

This ADR adds **no new code**; it formalises the existing design and
adds:

1. `/techlead` skill v2.1.0 → v2.1.1: AP-11 entry cites this ADR;
   L1.3a sub-matrix `✗ (design)` exception cites this ADR as
   `audit ref`.
2. `scripts/byok-feature-validate.sh`: a thin verifier that
   re-asserts the mutually-exclusive constraint is in place
   (regression guard if a future contributor "fixes" the
   `--all-features` failure by removing the macros).
3. Wave-15 baseline audit §7 updated to point at this ADR as the
   ratification.
4. Audit doc `specs/_audits/2026-05-16-byok-ap11-adr-formalization.md`
   records the formalization event itself.

## Validation gates

- `cargo build -p corelink-server --features byok-aws-real` → green.
- `cargo build -p corelink-server --features byok-gcp-real` → green.
- `cargo build -p corelink-server --features byok-azure-real` → green.
- `cargo build -p corelink-server --features byok-vault-real` → green.
- `cargo build -p corelink-server --features byok-aws-real,byok-gcp-real`
  → **must** fail with the AWS+GCP `compile_error!` diagnostic
  (regression test).
- `scripts/byok-feature-validate.sh` → exit 0.
- `python3 scripts/validate_specs.py` → no new failures.
- `python3 scripts/validate_references.py` → no new dangling refs.

## References

- `specs/_audits/2026-05-15-byok-real-provider-pattern.md` §7
  (wave-15 baseline; this ADR's parent).
- `specs/_audits/2026-05-16-byok-ap11-adr-formalization.md`
  (formalization event audit; this ADR's child).
- `specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md`
  (`KmsProvider` trait contract).
- `specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md`
  (4-provider semantics).
- `specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md`
  (kill-switch contract on the singleton).
- `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`
  (per-provider FIPS posture).
- `.claude/skills/techlead/SKILL.md` v2.1.1 §AP-11 (this ADR is the
  `audit ref` for the L1.3a `✗ (design)` exception).
- `apps/server/src/byok_orchestrator.rs` (implementation).

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-16 | Gustavo (via Claude Opus 4.7, wave-30 stream-8) | Initial ratification. Promotes wave-15 audit §7 design exception to first-class ADR; cited by `/techlead` v2.1.1 AP-11 entry; backed by `scripts/byok-feature-validate.sh` regression guard. |
