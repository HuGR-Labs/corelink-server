---
id: "ADR-S20-002-NON-EXHAUSTIVE-POLICY"
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
tags: ["adr", "charter", "non-exhaustive", "api-stability", "post-w36", "l2.1"]
references:
  - "specs/_audits/2026-05-27-charter-strict-audit-post-w36.md"
  - "specs/03_architecture/invariant_registry.md"
---

# ADR-S20-002: `#[non_exhaustive]` policy

## Status

ACTIVE 2026-05-27

## Context

Charter constraint **L2.1** (from `techlead` skill rules) requires
`#[non_exhaustive]` on public enums and structs. The post-W36
charter-strict audit (`specs/_audits/2026-05-27-charter-strict-audit-post-w36.md`
§L2.1) scanned **5 412** `pub enum` / `pub struct` declarations across
the 65 `corelink-*` crates and flagged **1 307** lines missing the
annotation:

- ~38 lines in test / proptest / fake / sim files (acceptable carve-out).
- ~280 lines in `error.rs` / `errors.rs` modules on the cross-crate
  surface.
- ~990 lines on types `pub` inside private submodules, not re-exported
  through `pub use` chains — these reach only `pub(crate)`-equivalent
  visibility and are NOT cross-crate API.

A blanket-add fix would be wrong:

1. Many of the 1 307 are not actually on the cross-crate surface
   (visibility carries from the `pub use` re-export point, not from the
   declaration alone).
2. Some enums are deliberately closed — e.g. `ConsentPurpose` is a
   spec-frozen 12-arm taxonomy (ADR-S11-006), `HttpMethod` is closed
   by HTTP/1.1 + RFC 5789, and adding `#[non_exhaustive]` to them
   forces downstream `_ =>` arms that defeat the purpose of the
   spec-closed contract.
3. Internal newtypes wrapping a single primitive (`TenantId(Uuid)`,
   `PatIdHash(String)`) cannot meaningfully grow new public fields;
   `#[non_exhaustive]` only blocks struct-literal construction with no
   compensating benefit.
4. Marker / phantom / unit structs have nothing to extend.

Charter §4 of the audit also explicitly classifies *adding*
`#[non_exhaustive]` to existing types as HIGH-RISK because it can break
downstream exhaustive matching. The right move is a documented policy
that distinguishes MUST from MAY-OMIT, not a sweep.

## Decision

### MUST have `#[non_exhaustive]`

A type MUST carry `#[non_exhaustive]` if it is **cross-crate
`pub`-reachable** (re-exported via `pub use` from a crate root or
listed in any crate's public surface) and falls into one of:

1. **Error enums** — any `pub enum` implementing `std::error::Error`
   (typically via `#[derive(thiserror::Error)]`). New error variants
   appear naturally as new failure modes are added; consumers must use
   `_ =>` for unknown variants so library evolution stays additive.
   *Examples (current code):*
   - `corelink_audit::AuditError` (`crates/corelink-audit/src/error.rs:13`)
   - `corelink_tier_selection::TierError`
     (`crates/corelink-tier-selection/src/error.rs:12`)
   - `corelink_audit::AuditEmitError`
     (`crates/corelink-audit/src/ports.rs:141`)

2. **Public wire / serde-derived types** — any `pub struct` /
   `pub enum` carrying `#[derive(Serialize, Deserialize)]` that is
   either (a) sent across a process boundary (HTTP, RPC, queue,
   replication, audit-log envelope) or (b) embedded in a published
   schema (CloudEvents, SBOM, AC entry).
   *Rationale:* schema evolution must remain additive; consumers
   deserializing into structs with public fields must tolerate new
   fields.

3. **Public config structs in workspace-API crates** — config-shaped
   `pub struct` types in `corelink-core`, `corelink-traits-*`, and
   umbrella / facade crates whose stability contract spans crates.
   *Reference:* `INV-OBS-CONFIG-NON-EXHAUSTIVE`
   (`specs/03_architecture/invariant_registry.md:516`) already mandates
   this for `corelink-telemetry::otel`; this ADR generalises that
   precedent.

4. **ADR-published public surface** — any type explicitly cited as
   public API by an ADR (e.g. a trait's associated error type, a
   public DTO named in a sequence diagram). The ADR's stability
   promise is binding.

5. **State-machine variant enums** — enums whose variants represent
   states of a documented machine (audit-replay state, multipart
   finalize outcome, rollout phase, etc.) where transitions are
   expected to evolve as new edge cases are documented.
   *Example:* `MultipartFinalizeOutcome`
   (`crates/corelink-cas/src/multipart_schema/sim.rs:390`).

### MAY OMIT (default-OK without)

A type MAY omit `#[non_exhaustive]` — and L2.1 does NOT flag it — when
**any** of the following applies:

6. **Internal newtypes wrapping a single primitive**
   (`TenantId(Uuid)`, `Digest(...)`, `IdempotencyToken(String)`,
   `SessionId(Uuid)`, `PatIdHash(String)`, `ContentHash(String)`,
   `ChainHash(String)` …). The inner field is the only state; adding a
   second field would be a semantic redesign, not additive growth.
   *Examples (current code):*
   - `corelink_audit::TenantId` (`crates/corelink-audit/src/lib.rs:160`)
   - `corelink_audit::SessionId`
     (`crates/corelink-audit/src/events.rs:165`)
   - `corelink_audit::PatIdHash`
     (`crates/corelink-audit/src/redact.rs:81`)

7. **Marker / phantom / zero-sized types** — unit structs or
   `PhantomData` carriers. There is no field surface to evolve.

8. **Builder structs that explicitly document private-fields policy**
   — `*Builder` structs whose construction goes only through `::new()`
   plus chained `with_*` setters, and where the type's doc-comment
   states "fields are not part of the public API". Adding fields then
   reaches private state only.
   *Example:* `ClerkConfigBuilder`
   (`crates/corelink-clerk/src/config.rs:124`).

9. **Test-only types** — anything gated by `#[cfg(test)]`, declared
   inside `tests/`, `proptest/`, `sim/`, `fake_*` modules, or behind a
   `test-utils` cargo feature. These are not part of any stability
   contract.

10. **Closed-set enums where adding a variant IS the API change** —
    enums whose variant set is fixed by an external standard, a frozen
    ADR, or a regulatory taxonomy. Adding a variant requires a
    super-seding ADR / minor-version bump anyway; `#[non_exhaustive]`
    would force perpetual `_ =>` arms that silently swallow new
    spec-defined variants.
    *Examples:*
    - `ConsentPurpose` (12-arm taxonomy frozen by ADR-S11-006;
      `crates/corelink-privacy/src/consent/schema.rs:100`).
    - `HttpMethod` (closed by HTTP/1.1 + RFC 5789;
      `crates/corelink-privacy/src/dpa/versioning/middleware.rs:26`).

### Decision criterion (ambiguous cases)

When a type does not cleanly fit a MUST or MAY-OMIT class, apply this
single test:

> "Would adding a field (struct) or variant (enum) to this type break
> a downstream caller's exhaustive match or struct-literal
> construction?"
>
> - **YES** → add `#[non_exhaustive]`.
> - **NO** → omit.

If the answer is "we don't know yet", **prefer adding
`#[non_exhaustive]`** on greenfield types: the cost of a `_ =>` arm
downstream is much smaller than a SemVer break later.

### Scope clarifications

- **Visibility is computed at re-export, not declaration.** L2.1
  applies to the cross-crate API surface — a `pub struct` inside a
  module that is never `pub use`-exported is not in scope.
- **`pub(crate)` is out of scope.** L2.1 never applies to
  crate-private types.
- **`pub(super)` / `pub(in path)` is out of scope** unless the parent
  module is itself cross-crate-reachable.

## Consequences

- The charter-audit §L2.1 rule is refined: enforcement runs on the
  **MUST** categories only; **MAY-OMIT** types pass without flagging.
  Future audits use this MUST / MAY-OMIT split.
- **NO mass migration.** The 1 307 flagged lines are NOT retroactively
  changed. If a MAY-OMIT type later needs to extend its surface, the
  author adds `#[non_exhaustive]` in the same PR as the extension
  (which is the SemVer-correct moment anyway).
- **Greenfield types** (added Wave 37+) follow the MUST / MAY-OMIT
  rules at authoring time. The default for any "potentially
  extensible" public type is `#[non_exhaustive]`.
- `invariant_registry.md` references to L2.1 / non_exhaustive are
  updated to point at this ADR for the precise scope.
- `INV-OBS-CONFIG-NON-EXHAUSTIVE` and `INV-AUDIT-EVENT-TYPE-EXHAUSTIVE`
  are unchanged — both fall under MUST category 3 / 4 respectively and
  this ADR is consistent with them.

## Alternatives considered

- **Blanket-add `#[non_exhaustive]` to all 1 307 sites.** Rejected:
  HIGH-RISK (downstream-break exposure), forces `_ =>` arms on
  spec-closed enums (`ConsentPurpose`, `HttpMethod`), and adds zero
  benefit to newtype wrappers.
- **Drop L2.1 from the charter.** Rejected: extensibility of error
  enums and public DTOs is a real SemVer concern; the rule is correct
  in spirit, only the universal phrasing was wrong.
- **Per-crate opt-in via crate-level lint.** Rejected: too coarse —
  most crates need both MUST and MAY-OMIT types in the same module.

## Examples (current code)

| Class | Type | Location | Decision |
|---|---|---|---|
| MUST-1 (error enum) | `AuditError` | `crates/corelink-audit/src/error.rs:13` | Has `#[non_exhaustive]` (compliant) |
| MUST-1 (error enum) | `TierError` | `crates/corelink-tier-selection/src/error.rs:12` | Has `#[non_exhaustive]` (compliant) |
| MUST-3 (config) | `corelink-telemetry::otel` config | per `INV-OBS-CONFIG-NON-EXHAUSTIVE` | Already enforced |
| MUST-5 (state) | `MultipartFinalizeOutcome` | `crates/corelink-cas/src/multipart_schema/sim.rs:390` | Should carry annotation if cross-crate-exported |
| MAY-OMIT-6 (newtype) | `TenantId(Uuid)` | `crates/corelink-audit/src/lib.rs:160` | Omit OK |
| MAY-OMIT-6 (newtype) | `SessionId(Uuid)` | `crates/corelink-audit/src/events.rs:165` | Omit OK |
| MAY-OMIT-6 (newtype) | `PatIdHash(String)` | `crates/corelink-audit/src/redact.rs:81` | Omit OK |
| MAY-OMIT-8 (builder) | `ClerkConfigBuilder` | `crates/corelink-clerk/src/config.rs:124` | Omit OK (private fields by convention) |
| MAY-OMIT-10 (closed set) | `ConsentPurpose` | `crates/corelink-privacy/src/consent/schema.rs:100` | Omit REQUIRED (per ADR-S11-006) |
| MAY-OMIT-10 (closed set) | `HttpMethod` | `crates/corelink-privacy/src/dpa/versioning/middleware.rs:26` | Omit OK |

## Closure

Resolves **Escalation 1** of
`specs/_audits/2026-05-27-charter-strict-audit-post-w36.md` §L2.1
(1 307 flagged lines). No code changes. Future charter audits MUST
apply the MUST / MAY-OMIT split defined in §Decision.
