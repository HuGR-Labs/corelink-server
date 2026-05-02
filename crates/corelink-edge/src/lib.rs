//! `corelink-edge` — CoreLink edge per-IP enforcement + CIDR blocklist
//! (WI-S08-002).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of camada 2 (per-IP edge) of the 4-layer rate-limit
//! bulkhead PAT-RATE-LIMIT-001: the canonical CIDR matcher, the durable
//! source-of-truth migration, and the in-memory orchestrator that
//! exercises every load-bearing invariant the production CF Ruleset
//! Engine + CF List replica wiring relies on. Property tests pinned at
//! 10 k iter against the orchestrator cover INV-AVAIL-ISOLATION (HIGH;
//! per-IP edge layer drops adversarial floods at zero cost), the
//! canonical longest-prefix-match semantic, and the audit fail-closed
//! envelope per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.
//!
//! Specifically, the crate ships:
//!
//! 1. The canonical SQL artifact `migrations/d1/0011_edge_blocklist.sql`
//!    embedded via [`MIGRATION_0011_EDGE_BLOCKLIST`] so production code
//!    can pass the DDL to `wrangler d1 migrations apply` without
//!    re-reading from disk. Schema mirrors the in-memory blocklist
//!    1:1; durable reload across worker restarts; soft-delete watermark
//!    for the audit trail.
//! 2. The [`cidr`] module ships [`Cidr`] (canonical CIDR with
//!    `[Cidr::parse]` + `[Cidr::contains]` + canonical text rendering;
//!    hand-rolled bit-level matcher; wasm32-clean) + [`CidrFamily`]
//!    `#[non_exhaustive]` 2-canonical {V4, V6} + [`IpAddr`] +
//!    [`longest_match`] (canonical longest-prefix-match) +
//!    [`overlaps`] (admin add overlap detection per WI §6.1.7).
//! 3. The [`audit`] module ships [`EdgeEventType`] (`#[non_exhaustive]`
//!    5-event taxonomy: `corelink.edge.{allowed, denied_blocklist,
//!    denied_abuse, blocklist_added, blocklist_removed}`) +
//!    [`EdgeAuditRecord`] + [`EdgeAuditSink`] +
//!    [`InMemoryEdgeAuditSink`] capture sink + [`FailingEdgeAuditSink`]
//!    adversarial fixture (fail-closed envelope per
//!    `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//! 4. The [`metrics`] module ships [`EdgeMetricsObserver`] +
//!    [`InMemoryEdgeMetrics`] + [`FailingEdgeMetrics`] (5 canonical
//!    metrics: `corelink.edge.{decision_total,
//!    cidr_blocklist_size, cidr_blocklist_added_total,
//!    cidr_blocklist_removed_total, cf_api_error_total}`).
//! 5. The [`error`] module ships the canonical [`EdgeError`]
//!    `#[non_exhaustive]` taxonomy (audit / metrics / CIDR parse / CIDR
//!    overlap / capacity / backend arms).
//! 6. The [`config`] module ships [`EdgeConfig`] + canonical defaults
//!    pinned to spec contract §5 R-S08-2 (CF List 10000 hard ceiling
//!    per WI §6.1.6; 8000 alert threshold).
//! 7. The [`policy`] module ships the [`CidrBlocklist`] trait +
//!    [`InMemoryCidrBlocklist`] (per-tenant prefix index +
//!    longest-prefix-match scan) and the [`EdgePolicy`] trait +
//!    [`InMemoryEdgePolicy`] orchestrator wired to [`CidrBlocklist`],
//!    [`EdgeAuditSink`], [`EdgeMetricsObserver`], [`EdgeClock`].
//!    [`EdgeDecision`] `#[non_exhaustive]` 3-arm enum (Allow,
//!    `DenyBlocklisted`, `DenyAbuse`) and [`MatchedPrefix`] complete
//!    the public surface.
//!
//! # Why edge is `trait + fake` here, real CF Ruleset / List in WI-S08-006
//!
//! S-08 lands without the live Cloudflare Ruleset Engine + CF List
//! bindings wired into CI (no remote + Cloudflare Workers + CF API
//! staging are HARD inflection points per
//! `corelink_autonomous_execution_charter.md`). The fake covers the
//! algorithmic invariants that a production binding bug would expose:
//! longest-prefix-match correctness; per-tenant isolation; audit
//! fail-closed envelope; idempotent add/remove; F-001 closure. The
//! live CF Ruleset + CF List + reconcile DO conformance tests run
//! alongside WI-S08-006 (PRR ship gate) once miniflare / wrangler-dev
//! integration tests land.
//!
//! # Cripto-driven invariants enforced
//!
//! - **INV-AVAIL-ISOLATION** (HIGH; spec_contract §8 +
//!   invariant_registry §3.8): edge enforcement is pre-auth (no
//!   `tenant_id` at the IP boundary); cross-tenant linkability
//!   architecturally impossible at THIS layer. Pinned by
//!   `prop_tenant_isolation`.
//! - **Longest-prefix-match canonical**: the most-specific entry wins
//!   when multiple admin entries cover the same address. Pinned by
//!   `prop_cidr_longest_prefix_match`.
//! - **Address-family isolation**: IPv4 entries never match IPv6
//!   addresses and vice versa. Pinned by `prop_ipv6_supported`.
//! - **Idempotent add / round-trip remove**: repeated `add_prefix`
//!   calls are no-op; `add → remove → no match`. Pinned by
//!   `prop_blocklist_idempotent_add` + `prop_blocklist_remove_round_trip`.
//! - **Decision determinism**: same `(tenant, ip)` → same
//!   [`EdgeDecision`] across calls. Pinned by `prop_decision_deterministic`.
//! - **Default-allow on empty blocklist**: an empty blocklist admits
//!   every IP. Pinned by `prop_no_match_default_allow`.
//! - **Audit fail-closed** (Lote 10.6bis pattern): every admin add /
//!   remove emits an audit BEFORE state mutation; emit failure aborts
//!   the operation and leaves the blocklist unchanged. Pinned by
//!   `prop_audit_emit_per_decision_arm`.
//!
//! # Forbidden surface
//!
//! - **No `unsafe`** anywhere in the crate.
//! - **No `unwrap` / `expect` / `panic` / direct `[i]` indexing** in
//!   library code (all crate-strict clippy lints are `deny`).
//! - **No `tokio`** in `src/` (wasm32-clean lib code).
//! - **No process-global `static LazyLock<Mutex<>>`** — F-001 closure
//!   preserved via per-instance `Arc<Mutex<HashMap<TenantId,
//!   BlocklistTrie>>>`.
//!
//! # Composition with WI-S08-001 per-IP token bucket
//!
//! The 4-layer bulkhead PAT-RATE-LIMIT-001 composes the THIS crate
//! (camada 2 — per-IP edge CIDR blocklist) with `corelink-ratelimit`
//! (camada 1 — per-tenant token bucket; camada 1 also serves the
//! per-IP token bucket via `BucketKey::per_ip`). Production wiring at
//! WI-S08-006 PRR ship gate composes the two in this canonical order:
//!
//! 1. **Edge blocklist consulted FIRST** (zero-cost drop of adversarial
//!    IPs at the CF Ruleset layer; pre-Worker enforcement).
//! 2. **Per-IP token bucket SECOND** (after the blocklist admits the
//!    IP, the per-IP rate-limiter enforces tier 2 protections —
//!    NAT-aware allowance per Lote 10.7bis P0-6).
//!
//! This order keeps the adversarial-flood path off the Worker CPU
//! budget and off the per-IP DO state contention path. The CF Ruleset
//! Engine + CF List replica + reconcile DO production wiring lands at
//! WI-S08-006 (PRR ship gate).

#![forbid(unsafe_code)]

/// Embedded canonical migration SQL (D1 Cloudflare SQLite) for
/// `edge_blocklist` (WI-S08-002; per-IP edge CIDR blocklist durable
/// source-of-truth; the CF Ruleset Engine + CF List replica are pushed
/// from this row set per the reconcile DO at WI-S08-006).
///
/// The exact bytes ship to production via `scripts/migrate_d1.sh` /
/// `wrangler d1 migrations apply`. The simulator does not parse this
/// string; the algorithmic invariants are re-implemented directly so
/// test failures are easy to triage.
pub const MIGRATION_0011_EDGE_BLOCKLIST: &str =
    include_str!("../../../migrations/d1/0011_edge_blocklist.sql");

pub mod audit;
pub mod cidr;
pub mod config;
pub mod error;
pub mod metrics;
pub mod policy;

pub use audit::{
    canonical_audit_event_strings, EdgeAuditRecord, EdgeAuditSink,
    EdgeAuditSinkError, EdgeEventType, FailingEdgeAuditSink,
    InMemoryEdgeAuditSink,
};
pub use cidr::{
    longest_match, overlaps, Cidr, CidrFamily, CidrParseError, IpAddr,
};
pub use config::{
    EdgeConfig, EdgeDefaultAction, DEFAULT_ALERT_THRESHOLD_SIZE,
    DEFAULT_MAX_BLOCKLIST_SIZE,
};
pub use error::EdgeError;
pub use metrics::{
    canonical_metric_names, EdgeMetricKind, EdgeMetricsObserver,
    EdgeMetricsObserverError, EdgeResultLabel, FailingEdgeMetrics,
    InMemoryEdgeMetrics,
};
pub use policy::{
    CidrBlocklist, EdgeClock, EdgeDecision, EdgePolicy,
    InMemoryCidrBlocklist, InMemoryEdgePolicy, MatchedPrefix,
    SystemEdgeClock,
};

/// Returns the canonical schema version recorded by the latest
/// migration in the D1 `edge` domain.
///
/// Version is sequential within the D1 domain (`blob_meta` = 1,
/// `ac_meta` = 2, …, `ratelimit_buckets` = 10,
/// **`edge_blocklist` = 11**).
#[must_use]
pub const fn edge_schema_version() -> u32 {
    11
}
