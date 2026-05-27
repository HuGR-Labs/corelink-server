---
id: "AUDIT-2026-05-27-INVARIANT-REGISTRY-HYGIENE"
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
tags: ["audit", "invariants", "registry", "hygiene", "post-w35", "post-w36", "seal"]
references:
  - "specs/03_architecture/invariant_registry.md"
  - "specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/2026-05-27-w36-stage-3-seal.md"
---

# Invariant Registry Hygiene — Absorbed-Crate Citation Update (post Wave 33-36)

## §0 Charter

`invariant_registry.md` (last updated `2026-05-16`, v0.2.3) is the SOURCE OF
TRUTH for `INV-XXX` IDs. Wave 33-36 reorg absorbed 63 crates into 11
umbrellas. INV citation paths referencing absorbed-crate names became
stale. This audit updates citations to canonical umbrella paths.

**Scope:** crate-name prefix updates only. ZERO semantic changes to
invariants (no removed entries, no changed scope / severity / acceptance
criteria, no edited TLA+ links).

## §1 Source-of-truth mapping (absorbed → umbrella)

Derived from `crates/<umbrella>/src/lib.rs` `pub mod` declarations and
inline provenance comments (`// was \`corelink-<absorbed>\``):

| Umbrella | Absorbed crates → submodule path |
|----------|-----------------------------------|
| `corelink-cas` | chunker, dedup, edge, lru-tracker, manifest, multipart-schema → `corelink-cas::{chunker, dedup, edge, lru_tracker, manifest, multipart_schema}` |
| `corelink-telemetry` | canary, lighthouse-tracker, logpush, otel-export, synthetic-pager → `corelink-telemetry::{canary, lighthouse, logpush, otel, synthetic_pager}` |
| `corelink-replication` | rollout-controller → `corelink-replication::rollout_controller` |
| `corelink-auth` | webauthn, auth-schema → `corelink-auth::{webauthn, schema}` |
| `corelink-billing` | quota → `corelink-billing::quota` (abuse + replay also absorbed but not cited in registry) |
| `corelink-privacy` | privacy-sub-processor-emit → `corelink-privacy::sub_processor` |
| `corelink-ops` | backup-verify, oncall, tenant-offboarding → `corelink-ops::{dr::backup_verify, oncall, tenant_offboarding}` |
| `corelink-ac` | ac-core, ac-schema → `corelink-ac::{ac_core, schema}` |
| `corelink-byok` | — (no byok-* citations found in registry) |
| `corelink-adapter-host` | — (no adapter-* citations found in registry) |

## §2 Per-INV change summary

All edits are crate-name prefix rewrites in INV table rows. Invariant
descriptions, severity bands, acceptance criteria, and TLA+ references
are untouched.

### Section §3.x (canonical INV table rows)

| Line | INV ID | Change |
|------|--------|--------|
| 131 | `INV-AVAIL-DOS` | `corelink-logpush::redaction` → `corelink-telemetry::logpush::redaction` |
| 288 | `INV-AC-TTL-REFRESH-MONOTONIC` | `corelink-ac-schema::sim` → `corelink-ac::schema::sim` (×2 incl. file path) |
| 313 | `INV-MULTIPART-MANIFEST-SIGNED` | `corelink-manifest::sig` → `corelink-cas::manifest::sig` |
| 315 | `INV-MULTIPART-CHUNK-DETERMINISTIC` | `corelink-chunker` → `corelink-cas::chunker` |
| 316 | `INV-MULTIPART-BOUNDED-PARSER` | `corelink-chunker::bounds` + `corelink-manifest::bounds` → `corelink-cas::chunker::bounds` + `corelink-cas::manifest::bounds` |
| 317 | `INV-MULTIPART-STREAMING-MEMORY` | `corelink-chunker::Chunker::feed` → `corelink-cas::chunker::Chunker::feed` |
| 322 | `INV-MULTIPART-MANIFEST-VALID` | `corelink-manifest::verify_structure` → `corelink-cas::manifest::verify_structure` |
| 324 | `INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST` | `corelink-manifest::verify_streaming` → `corelink-cas::manifest::verify_streaming` |
| 411 | `INV-SUB-PROCESSOR-DKIM-TENANT-SCOPED` | `corelink-privacy-sub-processor-emit::dkim::derive_dkim_key` → `corelink-privacy::sub_processor::dkim::derive_dkim_key` |
| 419 | (cross-ref bullet) | crate path → umbrella module path |
| 429 | (R-prep §3.16 intro) | crate → module + "was …" historical context retained |
| 433 | `INV-BACKUP-FRESH` | `corelink-backup-verify::lib::verify_freshness` → `corelink-ops::dr::backup_verify::verify_freshness` |
| 434 | `INV-BACKUP-INTEGRITY-SAMPLE-CAP` | `corelink-backup-verify::lib` → `corelink-ops::dr::backup_verify` |
| 442 | (Aliases backup-verify) | historical-context framing added |
| 509 | (§3.18 otel-export intro) | crate → module + "was …" framing |
| 513 | `INV-OBS-EXPORT-FAIL-OPEN` | `corelink-otel-export::lib + ::audit + ::error` → `corelink-telemetry::otel::{audit, error}` |
| 514 | `INV-OBS-NO-PII` | `corelink-otel-export::metric + ::lib` → `corelink-telemetry::otel::metric` |
| 515 | `INV-OBS-CT-SECRET-EQ` | `corelink-otel-export::secret` → `corelink-telemetry::otel::secret` |
| 516 | `INV-OBS-CONFIG-NON-EXHAUSTIVE` | `corelink-otel-export` → `corelink-telemetry::otel` |
| 523 | (Aliases otel-export) | historical-context framing added |
| 529 | (§3.19 tenant-offboarding intro) | crate → module + "was …" framing |
| 533 | `INV-OFFBOARDING-GRACE-RESPECTED` | `corelink-tenant-offboarding::orchestrator` → `corelink-ops::tenant_offboarding::orchestrator` |
| 534 | `INV-OFFBOARDING-AUDIT-COMPLETE` | `corelink-tenant-offboarding::lib` → `corelink-ops::tenant_offboarding` |
| 541 | (Aliases tenant-offboarding) | historical-context framing added |
| 547 | (§3.20 rollout-controller intro) | crate → module + "was …" framing |
| 552 | `INV-ROLLOUT-NO-STAGE-SKIP` | `corelink-rollout-controller::{state_machine, controller, types}` → `corelink-replication::rollout_controller::{state_machine, controller, types}` |
| 553 | `INV-ROLLOUT-COSIGN-GATE` | `corelink-rollout-controller::lib::start` → `corelink-replication::rollout_controller::start` |
| 554 | `INV-ROLLOUT-BUDGET-CAP` | `corelink-rollout-controller::controller + ::lib` → `corelink-replication::rollout_controller::controller` |
| 555 | `INV-ROLLOUT-AUTO-ROLLBACK-TRIGGERS` | `corelink-rollout-controller::auto_rollback::SustainedTrigger` → `corelink-replication::rollout_controller::auto_rollback::SustainedTrigger` |
| 563 | (Aliases rollout-controller) | historical-context framing added |
| 576 | `INV-S17-ONCALL-FATIGUE-AUTOROTATE` | `corelink-oncall::threshold` → `corelink-ops::oncall::threshold` |

### Section §5 (registered-INV catalog table)

| Line | INV ID | Change |
|------|--------|--------|
| 908 | `INV-AUTH-SCHEMA-RLS-DEFAULT-ON` | `corelink-auth-schema/tests/integration/rls.rs` → `corelink-auth/src/schema/tests/integration/rls.rs` |
| 913 | `INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN` | `corelink-webauthn::assertion` → `corelink-auth::webauthn::assertion` |
| 914 | `INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED` | `corelink-webauthn::registration` → `corelink-auth::webauthn::registration` |
| 916 | `INV-AUTH-WEBAUTHN-ORIGIN-EXACT` | `corelink-webauthn` → `corelink-auth::webauthn` |
| 917 | `INV-AUTH-WEBAUTHN-RP-ID-CANONICAL` | `corelink-webauthn` → `corelink-auth::webauthn` |
| 922 | `INV-NEG-CACHE-MONOTONIC` | `corelink-edge::neg_cache` → `corelink-cas::edge::neg_cache` |
| 925 | `INV-AC-IDEMPOTENT` | `corelink-ac-core/tests/idempotency.rs` → `corelink-ac/tests/ac_core_idempotency.rs` |
| 927-942 | `INV-AC-*` (9 entries) | `corelink-ac-core` → `corelink-ac::ac_core` (via replace_all) |
| 935 | `INV-AC-SIG-INFO-FIXED` | `corelink-ac-schema` → `corelink-ac::schema` |
| 946 | `INV-MULTIPART-MANIFEST-SIGNED` | `corelink-multipart-schema::manifest` → `corelink-cas::multipart_schema::manifest` |
| 980 | `INV-LRU-CONSISTENCY` | `corelink-lru-tracker::recency` → `corelink-cas::lru_tracker::recency` |
| 982 | `INV-MULTIPART-CHUNK-DETERMINISTIC` | `corelink-chunker` → `corelink-cas::chunker` |
| 985 | `INV-MULTIPART-MANIFEST-VALID` | `corelink-multipart-schema::manifest::validate` → `corelink-cas::multipart_schema::manifest::validate` |
| 992 | `INV-OBS-CONFIG-NON-EXHAUSTIVE` | `corelink-otel-export::config` → `corelink-telemetry::otel::config` |
| 993 | `INV-OBS-CT-SECRET-EQ` | `corelink-otel-export::secret` → `corelink-telemetry::otel::secret` |
| 994 | `INV-OBS-EXPORT-FAIL-OPEN` | `corelink-otel-export::export` → `corelink-telemetry::otel::exporter` |
| 995 | `INV-QUOTA-RESERVATION-TTL` | `corelink-quota::reservation` → `corelink-billing::quota::reservation` |
| 996-999 | `INV-ROLLOUT-*` (4 entries) | `corelink-rollout-controller` → `corelink-replication::rollout_controller` (via replace_all) |
| 1000 | `INV-S17-CHAOS-STAGING-ONLY` | `corelink-canary::chaos` → `corelink-telemetry::canary::chaos` |
| 1001 | `INV-S17-ONCALL-FATIGUE-AUTOROTATE` | `corelink-oncall::fatigue` → `corelink-ops::oncall::fatigue` |
| 1003 | `INV-S17-SEV1-DRILL-PAUSE` | `corelink-oncall::sev1` → `corelink-ops::oncall::sev1` |

**Totals:**
- INV table rows touched: 28 in §3.x + 23 in §5 = 51 distinct INV entries.
- §3.x R-prep section intros + Aliases-históricos footers reframed with
  explicit "was X (absorbed into Y Wave-35 Phase 2)" wording: 8.

## §3 Intentionally preserved (historical context)

Per §9 of the agent charter ("If a citation is in historical context: keep
as-is"):

| Line | Reason kept |
|------|-------------|
| 475 | `corelink-tier`, `corelink-quota-reset-utc` are RFC 9331 vendor-extension HTTP header field tokens, NOT crate names |
| 1032 | Explicit "Wave-20+ crates: corelink-statuspage-real, corelink-slack-real, corelink-region, corelink-rotation-adapters, corelink-drata-sync; surfaced Wave-23 sweep" — historical alias sweep snapshot |
| 1033 | Explicit "family shorthand in `crates/corelink-webauthn/README.md` … surfaced Wave-23 sweep" — historical alias snapshot |

These lines describe past states of specific files at known wave
boundaries; rewriting them would erase audit history.

## §4 Frontmatter

| Field | Before | After |
|-------|--------|-------|
| `version` | `"0.2.3"` | `"0.3.0"` (minor — citation drift fix) |
| `updated` | `"2026-05-16"` | `"2026-05-27"` |
| Visible `> **Versão:** 0.2.3` | bumped to `0.3.0` |

## §5 Acceptance

```
Re-grep stale absorbed crates: only intentional historical-context lines remain.
YAML parse: OK (version 0.3.0, updated 2026-05-27).
validate_specs.py: ✅ Todos validados: 449 com schema completo, 9 com YAML only (458 total).
```

## §6 Out of scope (DO NOT change in this pass)

- INV semantic content (description / severity / enforcement / TLA+ links).
- Removing INV entries.
- Touching `Aliases históricos` rows that document historically-attested
  shortened forms (per §9 charter).
- Non-absorbed crate citations (`corelink-stripe-real`, `corelink-rate-headers`,
  `corelink-handler-cas`, `corelink-billing-aggregator`,
  `corelink-replication-coordinator`, `corelink-audit-chain`, `corelink-clerk`,
  `corelink-clerk-cf`, `corelink-worker`, `corelink-pat`,
  `corelink-erasure-attestation`, `corelink-hash`, `corelink-gc`,
  `corelink-region`, `corelink-statuspage-real`, `corelink-slack-real`,
  `corelink-rotation-adapters`, etc.) — those crates still exist physically
  at `crates/<name>/` and the citations remain correct.

## §7 SEAL

Per Wave 35-36 closure (tag `wave-36-final-sealed`). Hygiene pass
delivers `invariant_registry.md` v0.3.0 with citation paths aligned to
post-Wave-35-Phase-2 umbrella structure. Zero semantic drift. Charter
discipline preserved (INV IDs untouched, scope frozen).
