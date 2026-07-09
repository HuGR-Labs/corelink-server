---
id: "ADR-0033"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "audit", "cloudevents", "evt-047", "chain-integrity", "rfc8785", "jcs", "pii-redaction", "s03", "s09", "high-risk"]
---

# ADR-0033 — Audit event taxonomy EVT-047 + CloudEvents 1.0 envelope + per-event JCS hash chain alignment

## Status

FROZEN (S-03 WI-S03-007 ratified — SEALED 2026-05-01).

## Context

`auth_model.md §3.13.7 + observability_model.md §3 + privacy_model.md §3.5
+ security_model.md CTRL-AUDIT-001/002` mandate that every authentication
operation (S-03) emit an audit event consumed by the S-09 audit chain
processor. Five load-bearing invariants from `invariant_registry.md`
constrain the surface:

- **INV-AUDIT-NO-RAW-PII** (CRITICAL) — zero raw PII (email, raw
  `principal_id`, raw `pat_id`) in the chain. LGPD Art. 18 + GDPR
  Art. 30 + SOC 2 Type II all require redaction-at-source.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL) — the audit
  outbox INSERT is part of the same D1 transaction as the handler's
  main mutation. A handler that succeeds without persisting an audit
  row is a forensic gap.
- **INV-AUDIT-CHAIN-HASH-DETERMINISTIC** (HIGH) — `content_hash` is
  byte-deterministic across producers, platforms, and serde versions.
  A hash drift breaks every link in the chain downstream.
- **INV-AUDIT-EVENT-TYPE-EXHAUSTIVE** (HIGH) — every `AuthEventType`
  variant has a matching `AuthEventData` payload; the chain consumer
  dispatches on `type` and assumes payload shape.
- **INV-AUDIT-RETENTION-HINT-ACCURATE** (HIGH) — every event carries
  a per-tenant retention hint (`Solo30d / Team90d / Business1y /
  Enterprise7y`) so the retention worker (S-11) honors per-tenant
  promises without re-querying Neon for every row.

The canonical questions answered by this ADR:

1. **Event envelope shape** — custom CoreLink format vs CloudEvents 1.0.
2. **Emission ordering** — direct/sync emit vs outbox pattern.
3. **PII surface** — runtime redact filter vs type-system enforcement
   via hash newtypes.
4. **Chain hash compute** — where (producer / chain processor) and
   how (serde_json vs canonical JSON vs RFC 8785 JCS).
5. **SEV-1 fan-out** — single outbox path vs dual outbox + direct SIEM
   webhook.
6. **Per-tenant retention** — global retention policy vs per-event
   retention hint.

## Decision

### 1. CloudEvents 1.0 envelope (W3C-compatible)

Every audit event uses the canonical CloudEvents 1.0 envelope shape:

| CE 1.0 field | Source |
|---|---|
| `specversion` | constant `"1.0"` |
| `id` | UUIDv7 (per-event; producer generates at emit-time) |
| `source` | URI `corelink://<region>/<emitter>` |
| `type` | enum tag string (`auth.token.validated`, …) |
| `time` | Unix milliseconds (renders to RFC 3339 at chain query time) |
| `datacontenttype` | constant `"application/json"` |
| `subject` | tenant_id canonical text form (UUIDv7 hyphenated lowercase) |

CoreLink extensions land alongside the mandatory fields per CE 1.0 §3:
`tenant_id`, `principal_id_hash`, `region`, `request_id`,
`retention_hint`, `data`. **`prev_hash` is intentionally absent**;
chain linkage is computed by the S-09 chain processor (see §4 below).

**Why CloudEvents 1.0**: industry-standard W3C-compatible envelope;
SIEM (Datadog / Splunk / CrowdStrike) parses natively; mature schema
validators; deterministic field ordering supported via JCS
canonicalization. Custom format would require building parsers in
every consumer + losing W3C compatibility.

### 2. Outbox pattern (atomic with handler) — INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER

Production emitter wiring writes the audit row to `audit_outbox`
(D1) in the **same `db.batch([...])` transaction** as the handler's
main mutation. Either both rows commit or both roll back; a handler
that returns 200 without persisting an audit row is structurally
impossible. The drain worker (S-09 chain processor) consumes
`audit_outbox` rows post-COMMIT and seals them into the audit chain.

**Why outbox over direct emit**:

- Direct emit on the hot path = +20-50ms latency tax.
- Direct emit failure = either silent gap (audit lost) or retry
  storm (handler blocked while audit retries).
- Outbox pattern (reuse from WI-S01-005) is atomic + at-least-once +
  natural deduplication via `(request_id, event_type)` UNIQUE
  constraint.

The `Emitter` trait surface lets the S-03 wiring layer
([`OutboxEmitter`]) compose with a `DirectSiemEmitter` for SEV-1
fan-out (§5 below).

### 3. PII redaction via type-system hash newtypes — INV-AUDIT-NO-RAW-PII

Every PII-bearing field in `AuthEventData` is typed as a hash newtype
(`PrincipalIdHash`, `PatIdHash`, `EmailHash`,
`WebAuthnCredentialIdHash`) whose only constructor is the one-way
SHA-256-prefix-16-hex derivation. **There is no public `From<String>`
or `new(&str)` for any of them.** A refactor that tries to add a raw
`String` field for principal id is a compile error in every existing
handler.

Belt-and-suspenders:

- The `redact_pat!(plaintext) -> &str` macro returns the canonical
  `[REDACTED-PAT]` placeholder, so accidental
  `format!("{}", pat_plaintext)` becomes a clippy / CI-lint signal.
- A separate `tools/audit_pii_lint/` binary CI gate (follow-up Lote)
  greps audit emit code paths for raw `format!` / `to_string` calls
  on PAT / email / principal types.
- `corelink.audit.redact_violations_total` counter alerts SEV-2 on
  any non-zero increment.

**Why hash newtypes over runtime filter**:

- Runtime filter is reactive; type system is preventive.
- Runtime filter requires perfect coverage of every emit call site;
  a missed call = silent leak.
- Hash newtypes leverage the compiler — every audit emit path is
  type-checked against the redact contract for free.

The 16-hex-char (64-bit) prefix is sufficient for cross-event
correlation (the birthday bound at CoreLink scale is ~4 B distinct
principals per 50% collision probability, orders of magnitude
beyond plausible workload). Privileged-role reverse lookup (e.g.
incident response: "who is `principal_id_hash = 70e76bb54ae2d36d`")
is handled by a separate S-09 pipeline that has access to the
global principal index; that lookup is itself audited, closing the
forensic loop without exposing raw PII in the chain.

### 4. Per-event content hash via RFC 8785 JCS — INV-AUDIT-CHAIN-HASH-DETERMINISTIC

The producer canonicalizes the event via [RFC 8785 JSON
Canonicalization Scheme][jcs] (`serde_jcs = "0.2"`) and SHA-256 the
resulting UTF-8 byte stream:

```text
content_hash = SHA-256( JCS-canonicalize( AuthEvent ) )
```

The chain processor (S-09) reads the **persisted JCS-canonical bytes
off the outbox row** and computes the new chain hash by
concatenating the two hex strings:

```text
chain_hash_n = SHA-256( prev_chain_hash || content_hash_n )
```

**The chain processor never re-canonicalizes the event** —
re-running JCS at chain time would risk a `serde_jcs` version drift
between producer and consumer corrupting every link in the chain.
The genesis link (`chain_hash_0`) is the canonical 64-zero hex
string, matching the genesis-block pattern from
[`audit_immutability.tla`].

**Why RFC 8785 JCS over `serde_json` or `canonical_json`**:

- `serde_json` does NOT guarantee key ordering for `HashMap`/
  `BTreeMap` field types; deterministic only for struct field order
  at compile-time. Adding a single map field would silently break
  the chain.
- `canonical_json` (the older crate) is deprecated and has known
  issues with floating-point + Unicode normalization.
- `serde_jcs` (`0.2.x`) is the canonical RFC 8785 implementation:
  audited, deterministic key ordering, Unicode NFC native,
  IEEE 754 floating-point canonical form.

The determinism property is asserted by 10 000-iter property tests
in `crates/corelink-audit/tests/prop_audit.rs::prop_content_hash_deterministic`
plus RFC 8785 Annex A.3 smoke vector.

### 5. SEV-1 dual fan-out — outbox + direct SIEM webhook

Six `AuthEventType` variants are SEV-1 (`AuthEventType::is_sev1()`):

- `auth.anomaly.token_replay_detected`
- `auth.webauthn.sign_count_regression`
- `auth.webauthn.origin_attack_attempt`
- `auth.pat.scope_escalated`
- `auth.admin_op.mass_revoke`
- `auth.anomaly.cross_region_burst`

For these events, the `MultiplexEmitter` (S-09 wiring) writes to
`audit_outbox` AND fans out to a direct SIEM webhook in parallel.
The 60-s outbox-drain lag is unacceptable for incident response on
these signals; webhook fan-out gives sub-second SIEM visibility
while the outbox path remains the durable canonical record.

**Trade-off**: extra HTTP request per SEV-1 emit. SEV-1 events are
~0.01% of total volume (anomaly detections, not steady-state); the
TCO bump is negligible vs the 60-s detection latency saved.

### 6. Per-tenant retention hint on every event

Every emitted event carries `RetentionHint` derived from
`tenant.tier` at emit-time:

| Tier | Hint | Days |
|---|---|---|
| Solo | `Solo30d` | 30 |
| Team | `Team90d` | 90 |
| Business | `Business1y` | 365 |
| Enterprise | `Enterprise7y` | 2555 |

The retention worker (S-11 forward) reads the hint off the chain
row and decides reap eligibility. **Why hint on every event vs
global policy**: Solo-tier promised 30-d retention must be honored
without a global Solo override; per-event hint lets the retention
worker honor per-tenant promises in a single SQL pass without
joining `tenant.tier`. Enterprise tier defaults to 7 y (SOC 2 Type
II + LGPD Art. 16 + GDPR Art. 30 baseline); custom durations are
deferred to a follow-up Lote.

### 7. Backward compat via `#[non_exhaustive]`

Every public enum (`AuthEventType`, `AuthEventData`, `RegionTag`,
`TokenKind`, `DenyReason`, `MembershipRole`, `RetentionHint`,
`TenantTier`, `EmitterError`, `AuditError`) is marked
`#[non_exhaustive]`. The S-09 chain processor's `match` over
`AuthEventType` is intentionally non-default so a missing arm
becomes a compile error when the consumer is updated to a new
event-type set. Producer-side, additive variants land via
non-breaking minor version bumps; renames or removals are
breaking changes that require a `1`-yr deprecation per WI §23.

## Consequences

### Positive

- **Forensic completeness**: outbox atomicity + at-least-once delivery
  guarantee every authenticated handler success has a corresponding
  audit row (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
- **Compile-time PII safety**: hash newtypes prevent raw PII from
  ever appearing in canonical bytes; CI lint is belt-and-suspenders
  rather than first line of defense (INV-AUDIT-NO-RAW-PII).
- **Deterministic chain**: RFC 8785 JCS + persisted bytes-on-row
  pattern prevents producer/consumer version drift from corrupting
  the chain (INV-AUDIT-CHAIN-HASH-DETERMINISTIC).
- **SIEM-friendly**: CloudEvents 1.0 envelope is parsed natively by
  every major SIEM; cross-event correlation via `request_id` is
  trivial in standard query languages.
- **Tenant differentiation**: per-event retention hint honors
  per-tenant tier promises without re-querying Neon for every row.

### Negative

- **33 event types is a maintenance surface**: each new variant
  requires touching `AuthEventType` + `AuthEventData` + canonical
  vector test + taxonomy doc. We mitigate via the `canonical()`
  iterator + exhaustive proptest that fails on mismatch.
- **`serde_jcs = "0.2"` is a load-bearing dep**: any future major
  bump must preserve byte-for-byte canonicalization. Mitigation:
  pin `=0.2.x` minor + 10 000-iter property test catches drift on
  upgrade attempt.
- **SEV-1 dual fan-out couples to a SIEM webhook target**: webhook
  outage triggers SEV-2 (`corelink.audit.outbox_lag_seconds`); we
  do not block emit on webhook failure (the outbox is canonical).
- **Hash-prefix collision risk**: 64-bit prefix has birthday bound
  ~4 B; sufficient for plausible scale but not a security boundary.
  Reverse-lookup happens in privileged-role queries, not on the
  chain.
- **Production emitter shim is deferred**: this Lote ships the
  trait surface + `InMemoryEmitter` test sink only. The
  `OutboxEmitter` (D1 batch INSERT) + `DirectSiemEmitter` (webhook)
  + `MultiplexEmitter` (compose) wire in S-03 / S-09 follow-up
  Lotes per charter trait-abstraction-defer pattern (matches
  `corelink-clerk` / `corelink-pat` / `corelink-webauthn`).

### Mitigations

- 10 000-iter property tests (`prop_audit.rs`) cover content_hash
  determinism, no-raw-PII-in-canonical-bytes, chain link
  determinism, retention hint correctness, exhaustive event-type
  round-trip.
- Canonical-vector test (`canonical_vectors.rs`) pins all 33
  event-type strings, the SEV-1 set, the CloudEvents envelope shape
  for `auth.token.validated`, and the 3-event chain extension.
- Redaction integration tests (`redaction.rs`) assert hash newtypes
  reject empty input, do not leak raw PII through serialization,
  and the `redact_pat!` macro returns the canonical placeholder.
- Workspace clippy with crate-strict lints
  (`forbid(unsafe_code)` + `deny(unwrap/expect/panic/indexing/…)`)
  prevents accidental panics in production paths.

## Cross-references

- **WI**: `specs/04_sprints/_sealed/S03/work_items/WI-S03-007-audit-events-evt047-chain.md`
- **Implementation**: `crates/corelink-audit/`
- **Forward consumer**: `WI-S09-004` (audit chain processor; reads
  persisted JCS bytes; computes `chain_hash_n`).
- **Forward consumer**: `WI-S11-XXX` (retention worker; honors
  per-event `retention_hint`).
- **Cross-WI emit hooks**: `WI-S03-001` (Clerk) / `WI-S03-002` (PAT)
  / `WI-S03-003` (middleware) / `WI-S03-004` (revocation) /
  `WI-S03-005` (Neon schema) / `WI-S03-006` (WebAuthn).
- **Sibling ADR**: `ADR-0030` (revocation propagation; emits
  `auth.token.revoked` / `auth.session.revoked`).
- **Sibling ADR**: `ADR-0031` (Neon schema with email_hash; the
  `EmailHash` derivation here is the audit-chain pseudonym, distinct
  from the salted HKDF email_hash on `user_account`).
- **TLA+**: `specs/tla/audit_immutability.tla` (genesis-block
  pattern + chain integrity model).

## References

- [RFC 8785 — JSON Canonicalization Scheme (JCS)][jcs]
- [CloudEvents 1.0 Specification][cloudevents]
- [LGPD Art. 18 (right to erasure)][lgpd-18] / [Art. 38 (registro de operações)][lgpd-38]
- [GDPR Art. 30 (records of processing activities)][gdpr-30]
- [SOC 2 Type II audit trail completeness criteria][soc2]

[jcs]: https://www.rfc-editor.org/rfc/rfc8785
[cloudevents]: https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md
[audit_immutability.tla]: https://github.com/HumanGuardrail/corelink-server/blob/main/specs/tla/audit_immutability.tla
[lgpd-18]: https://www.gov.br/anpd/pt-br/canais_atendimento/agente-de-tratamento/lgpd
[lgpd-38]: https://www.gov.br/anpd/pt-br/canais_atendimento/agente-de-tratamento/lgpd
[gdpr-30]: https://gdpr-info.eu/art-30-gdpr/
[soc2]: https://www.aicpa-cima.com/topic/audit-assurance/audit-and-assurance-greater-than-soc-2

## Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7) | Criação ADR-0033 (S-03 WI-S03-007 SEAL); CloudEvents 1.0 envelope + 33 event types + RFC 8785 JCS hash chain + hash-newtype PII redaction + outbox atomicity + dual fan-out + per-tenant retention hint. |
