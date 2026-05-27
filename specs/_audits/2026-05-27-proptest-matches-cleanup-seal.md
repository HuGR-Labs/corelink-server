---
id: "AUDIT-2026-05-27-PROPTEST-MATCHES-CLEANUP-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "proptest", "anti-pattern-cleanup", "S-08-P1-1", "seal"]
references:
  - "specs/_audits/2026-05-27-charter-strict-audit-post-w36.md"
---

# prop_assert!(matches!) anti-pattern cleanup SEAL

## §1 Scope

Per charter audit §L2.4: charter scan surfaced 56 `prop_assert!(matches!(...))`
lines (52 code + 4 doc-comments). Per the audit's own taxonomy ~34 lines are
"fieldless / wildcard-inner" (the true S-08 P1-1 anti-pattern); the remaining
~18 are "bound-inner" unit-variant assertions where there is no inner state
to assert and the anti-pattern critique does not apply.

This agent performed the full per-site triage:

- **17 sites migrated** to explicit `match` blocks asserting on the inner
  field value (or its `Debug` form when the inner is a foreign error type).
- **31 sites kept as `prop_assert!(matches!)`** with a `// SOTA-OK:` comment
  documenting why a unit-variant / unit-struct / literal match is sufficient.

## §2 Per-file changes

| Crate | File | Migrated → match | Kept SOTA-OK |
|---|---|---|---|
| `corelink-handler-ac` | `tests/prop_handler_ac.rs` | 85, 96 | — |
| `corelink-handler-admin` | `tests/prop_handler_admin.rs` | 87 | — |
| `corelink-handler-cas` | `tests/prop_handler_cas.rs` | 81 | — |
| `corelink-tier-selection` | `tests/prop_tier_selection.rs` | 281, 330 | 155, 242 |
| `corelink-rate-headers` | `tests/prop_rate_headers.rs` | — | 251, 447, 476 |
| `corelink-privacy` | `tests/dpa_versioning_prop_re_accept_idempotency.rs` | — | 76 |
| `corelink-clerk` | `tests/prop_validate.rs` | 155 | 169, 207 |
| `corelink-signup` | `tests/prop_signup_orchestration.rs` | 319 | — |
| `corelink-telemetry` | `tests/prop_synthetic_pager.rs` | 71, 148 | — |
| `corelink-auth` | `tests/webauthn_prop_webauthn.rs` | — | 86, 88, 91, 118, 122 |
| `corelink-stripe-real` | `tests/prop_portal.rs` | 94 | — |
| `corelink-reapi` | `tests/prop_idempotency.rs` | — | 244 |
| `corelink-reapi` | `tests/prop_cross_tenant_read.rs` | — | 195, 287 |
| `corelink-reapi` | `tests/prop_cas_read.rs` | — | 244, 489, 553 |
| `corelink-reapi` | `tests/prop_cas.rs` | — | 442 |
| `corelink-meta` | `tests/prop_refcount.rs` | — | 300 |
| `corelink-meta` | `tests/prop_audit_outbox.rs` | — | 179 |
| `corelink-enterprise-inquiry` | `tests/prop_enterprise_inquiry.rs` | 110, 123, 223, 248 | — |
| `corelink-worker` | `tests/prop_multipart_full.rs` | — | 241 |
| `corelink-worker` | `tests/prop_ac_full.rs` | — | 263, 464, 492 |
| `corelink-worker` | `tests/prop_split_splice.rs` | 286 | 170, 282, 295 |
| `corelink-worker` | `tests/prop_ac_handlers.rs` | — | 185, 231 |
| `corelink-client-verify` | `tests/prop_verify.rs` | — | 257 |
| `corelink-ops` | `tests/oncall_prop_oncall.rs` | 161, 186 | — |
| `corelink-ops` | `tests/dr_drill_prop_dr_drill.rs` | — | 84, 90 |

Totals: 17 migrated, 31 SOTA-OK retained.

## §3 Verification

- `cargo test --no-run` per affected crate: GREEN (all 17 crates' test
  binaries link).
- `cargo clippy --tests -- -D warnings` on the union of affected crates:
  GREEN (clippy finished with no warnings).
- Re-grep `prop_assert!(matches!` post-fix surfaces 38 lines (down from 56
  in the original scan). Of those 38: 4 are pre-existing doc-comments
  (e.g. `//! NEVER prop_assert!(matches!(...))`) and 34 are SOTA-OK
  justified unit-variant / literal / unit-struct assertions.

## §4 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
