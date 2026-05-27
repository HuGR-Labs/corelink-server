//! `ClerkHealthDo` — per-tenant Durable Object actor backing the `CLERK_DO`
//! namespace binding declared in `wrangler.toml`.
//!
//! # Why a DO actor (vs a stateless function)?
//!
//! The clerk-cf health probe captures a short-lived correlation record
//! `(tenant_id, correlation_id) → HealthRecord` so a follow-up GET
//! observes the same wall-clock timestamp that the upstream POST emitted.
//! KV (eventual consistency) is unsuitable because the read-after-write
//! window can exceed the probe's RTT budget. A DO actor pins the record
//! to a single instance per `(tenant_id, correlation_id)` namespace
//! coordinate, with strong consistency by construction.
//!
//! # Routes
//!
//! All routes are scoped under `/record/{tenant_id}/{correlation_id}`:
//!
//! | Method | Path                                  | Effect                                  |
//! |--------|---------------------------------------|-----------------------------------------|
//! | GET    | `/record/{tenant}/{cid}`              | Return record or 404                    |
//! | POST   | `/record/{tenant}/{cid}`              | Upsert (audit-emit-BEFORE)              |
//! | DELETE | `/record/{tenant}/{cid}`              | Tombstone (audit-emit-BEFORE)           |
//!
//! Mutations (`POST` / `DELETE`) emit an audit event BEFORE the storage
//! mutation; the actor returns `5xx` if the audit closure errors
//! (fail-CLOSED). GETs are audit-fenced advisorily.
//!
//! # Tenant scope enforcement
//!
//! The actor's instance name MUST satisfy
//! [`corelink_cf_bindings::TenantScopedName`] (`tenant:<id>:<purpose>`).
//! When dispatched, the request URL contains `/record/{tenant}/{cid}` —
//! the actor extracts `{tenant}` from the URL and asserts it matches the
//! `tenant_id` segment carried in its own scoped name (via
//! `state.id().name()`). A mismatch is rejected with HTTP 403 — the
//! standard "actor cannot be reached for the wrong tenant" defense.
//!
//! Because the DO namespace name itself is anchored to a single tenant
//! (the `CfDurableObjectReal` wrapper enforces this on every
//! `id_from_name`), a same-tenant routing accident still has nowhere to
//! produce data for a different tenant — the actor instance simply does
//! not hold tenant-B records.
//!
//! # TTL sweep
//!
//! Each [`HealthRecord`] carries a `created_at_ms` field; the
//! [`ClerkHealthState::ttl_ms`] default of 3,600,000 (1 hour) bounds the
//! retention window. The actor sets a Workers DO alarm to fire every
//! `ttl_ms / 4` and sweeps any record older than `ttl_ms` from storage.
//!
//! # Charter
//!
//! - `#![forbid(unsafe_code)]` (crate root).
//! - No `unwrap` / `expect` / `panic` outside `#[cfg(test)]`.
//! - Audit fail-CLOSED on every mutation (POST + DELETE + sweep).
//! - `#[non_exhaustive]` on every public error / op enum.
//! - No `tokio` in the wasm32 hot path — async surface is
//!   `worker::*` native futures.
//! - The native build provides a pure-logic surface
//!   ([`ClerkHealthLogic`]) so the contract runs on host CI without the
//!   wasm32 toolchain or the `#[durable_object]` macro shim.

// The `#[durable_object]` macro generates a `pub fn new` / `pub fn fetch` /
// `pub fn alarm` trio of `#[wasm_bindgen]` glue methods that we cannot
// doc-comment from inside the proc-macro expansion. Allow the resulting
// `missing_docs` violations at module scope rather than on the struct —
// `#[allow]` placed on the struct itself does not propagate to the macro-
// expanded impl block on every rustc version.
#![allow(
    missing_docs,
    reason = "macro-generated #[wasm_bindgen] glue (new/fetch/alarm) cannot \
              carry doc comments — see #[worker::durable_object] expansion"
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use corelink_cf_bindings::DoError;

// ---------------------------------------------------------------------------
// Public types — target-agnostic logic surface
// ---------------------------------------------------------------------------

/// Default TTL for a [`HealthRecord`]: 1 hour (3,600,000 ms).
pub const DEFAULT_TTL_MS: i64 = 3_600_000;

/// Persisted health record. Stored in the DO's transactional storage as
/// `record:<correlation_id> → HealthRecord` (no inline tenant in the
/// key, because the actor instance is already tenant-anchored).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthRecord {
    /// Stable correlation identifier (UUIDv4 / ULID / opaque opaque
    /// caller-chosen blob; the actor does not parse it).
    pub correlation_id: String,
    /// Free-form note attached to the probe. Bounded at construction.
    pub note: String,
    /// Unix epoch milliseconds at the time of upsert.
    pub created_at_ms: i64,
}

impl HealthRecord {
    /// Construct a new record. Returns [`HealthDoError::ValidationFailed`]
    /// on shape violation (empty fields, oversize note, NUL bytes).
    ///
    /// # Errors
    ///
    /// - `correlation_id` empty / longer than 256 bytes / contains NUL.
    /// - `note` longer than 1024 bytes / contains NUL.
    pub fn new(
        correlation_id: impl Into<String>,
        note: impl Into<String>,
        created_at_ms: i64,
    ) -> Result<Self, HealthDoError> {
        let cid = correlation_id.into();
        let note = note.into();
        if cid.is_empty() {
            return Err(HealthDoError::ValidationFailed(
                "correlation_id empty".to_owned(),
            ));
        }
        if cid.len() > 256 {
            return Err(HealthDoError::ValidationFailed(format!(
                "correlation_id too long ({} > 256)",
                cid.len()
            )));
        }
        if cid.contains('\0') {
            return Err(HealthDoError::ValidationFailed(
                "correlation_id contains NUL".to_owned(),
            ));
        }
        if note.len() > 1024 {
            return Err(HealthDoError::ValidationFailed(format!(
                "note too long ({} > 1024)",
                note.len()
            )));
        }
        if note.contains('\0') {
            return Err(HealthDoError::ValidationFailed(
                "note contains NUL".to_owned(),
            ));
        }
        Ok(Self {
            correlation_id: cid,
            note,
            created_at_ms,
        })
    }
}

/// Error surface for the health-DO actor. `#[non_exhaustive]` so adding
/// new variants is forward-compatible.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum HealthDoError {
    /// Request path / method validation failed.
    #[error("validation_failed: {0}")]
    ValidationFailed(String),
    /// Tenant scope assertion failed (URL tenant != actor tenant).
    #[error("tenant_scope: {0}")]
    TenantScope(String),
    /// Audit hook denied the operation (fail-CLOSED).
    #[error("audit: {0}")]
    AuditDenied(String),
    /// Record not found (used to return HTTP 404).
    #[error("not_found")]
    NotFound,
    /// Underlying DO binding error (storage / namespace).
    #[error("backend: {0}")]
    Backend(String),
}

impl HealthDoError {
    /// Map an error to the appropriate HTTP status code for the actor's
    /// `fetch()` response.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            Self::ValidationFailed(_) => 400,
            Self::TenantScope(_) => 403,
            Self::AuditDenied(_) => 403,
            Self::NotFound => 404,
            Self::Backend(_) => 500,
        }
    }
}

impl From<DoError> for HealthDoError {
    fn from(value: DoError) -> Self {
        Self::Backend(value.to_string())
    }
}

/// Canonical operation labels for the actor's audit hook. Stable strings —
/// upstream code MAY pattern-match on the `Display` output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HealthDoOp {
    /// GET `/record/{tenant}/{cid}` — record lookup.
    Get,
    /// POST `/record/{tenant}/{cid}` — upsert (mutation).
    Upsert,
    /// DELETE `/record/{tenant}/{cid}` — tombstone (mutation).
    Tombstone,
    /// Alarm-driven TTL sweep (mutation).
    Sweep,
}

impl HealthDoOp {
    /// Static label used in errors and audit emission.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "health_get",
            Self::Upsert => "health_upsert",
            Self::Tombstone => "health_tombstone",
            Self::Sweep => "health_sweep",
        }
    }

    /// Whether this op is a mutation (audit fail-CLOSED).
    #[must_use]
    pub const fn is_mutation(&self) -> bool {
        matches!(self, Self::Upsert | Self::Tombstone | Self::Sweep)
    }
}

impl std::fmt::Display for HealthDoOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Audit hook for the actor. Called BEFORE every mutation; if it returns
/// `Err`, the mutation is NOT performed (fail-CLOSED). Reads call it
/// advisorily.
///
/// The `subject` argument is `tenant:<id>:<correlation_id>` so the audit
/// trail records the exact `(tenant, correlation_id)` coordinate without
/// further parsing.
pub type AuditFn = Arc<
    dyn Fn(HealthDoOp, &str) -> Result<(), HealthDoError> + Send + Sync + 'static,
>;

/// Default audit hook — emits to `worker::console_log!` on wasm32, no-op
/// on native. Production wires a recorder via [`ClerkHealthLogic::with_audit`].
fn default_audit() -> AuditFn {
    Arc::new(|_op, _subject| Ok(()))
}

// ---------------------------------------------------------------------------
// Parsed route
// ---------------------------------------------------------------------------

/// HTTP method discriminant the actor accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HealthMethod {
    /// GET — record lookup.
    Get,
    /// POST — upsert.
    Post,
    /// DELETE — tombstone.
    Delete,
}

impl HealthMethod {
    /// Parse a method string. Case-sensitive on the canonical HTTP forms.
    pub fn parse(raw: &str) -> Result<Self, HealthDoError> {
        match raw {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "DELETE" => Ok(Self::Delete),
            other => Err(HealthDoError::ValidationFailed(format!(
                "unsupported method '{other}'"
            ))),
        }
    }
}

/// Parsed `(method, tenant_id, correlation_id)` triple from an inbound
/// actor request URL of the shape `/record/{tenant}/{cid}`.
#[derive(Clone, Debug)]
pub struct ParsedRoute {
    /// HTTP method.
    pub method: HealthMethod,
    /// Tenant id segment from the URL (NOT yet validated against the
    /// actor's anchored tenant — caller MUST invoke
    /// [`ParsedRoute::assert_tenant`]).
    pub tenant_id: String,
    /// Correlation id segment from the URL.
    pub correlation_id: String,
}

impl ParsedRoute {
    /// Parse `(method, path)` into a route. Path must be exactly
    /// `/record/{tenant}/{cid}` (no extra trailing slashes, no query
    /// string — the actor's `fetch()` strips both before calling).
    ///
    /// # Errors
    ///
    /// Returns [`HealthDoError::ValidationFailed`] on any shape violation.
    pub fn parse(method: &str, path: &str) -> Result<Self, HealthDoError> {
        let method = HealthMethod::parse(method)?;
        let Some(tail) = path.strip_prefix("/record/") else {
            return Err(HealthDoError::ValidationFailed(format!(
                "path must start with /record/, got '{path}'"
            )));
        };
        let mut segments = tail.splitn(2, '/');
        let tenant_id = segments
            .next()
            .ok_or_else(|| {
                HealthDoError::ValidationFailed(format!("missing tenant in '{path}'"))
            })?
            .to_owned();
        let correlation_id = segments
            .next()
            .ok_or_else(|| {
                HealthDoError::ValidationFailed(format!(
                    "missing correlation_id in '{path}'"
                ))
            })?
            .to_owned();

        if tenant_id.is_empty() {
            return Err(HealthDoError::ValidationFailed(
                "tenant segment empty".to_owned(),
            ));
        }
        if correlation_id.is_empty() {
            return Err(HealthDoError::ValidationFailed(
                "correlation_id segment empty".to_owned(),
            ));
        }
        if correlation_id.contains('/') {
            return Err(HealthDoError::ValidationFailed(format!(
                "correlation_id contains '/': '{correlation_id}'"
            )));
        }
        if tenant_id.contains('\0') || correlation_id.contains('\0') {
            return Err(HealthDoError::ValidationFailed(
                "segment contains NUL byte".to_owned(),
            ));
        }
        Ok(Self {
            method,
            tenant_id,
            correlation_id,
        })
    }

    /// Assert the parsed tenant matches the actor's anchored tenant.
    ///
    /// Constant-time byte compare via `subtle::ConstantTimeEq` — same
    /// rationale as [`corelink_cf_bindings::TenantScopedName`].
    pub fn assert_tenant(&self, expected: &str) -> Result<(), HealthDoError> {
        use subtle::ConstantTimeEq;
        let a = self.tenant_id.as_bytes();
        let b = expected.as_bytes();
        if a.len() != b.len() {
            return Err(HealthDoError::TenantScope(format!(
                "tenant mismatch (URL='{}', actor='{}')",
                self.tenant_id, expected
            )));
        }
        if !bool::from(a.ct_eq(b)) {
            return Err(HealthDoError::TenantScope(format!(
                "tenant mismatch (URL='{}', actor='{}')",
                self.tenant_id, expected
            )));
        }
        Ok(())
    }

    /// Audit-subject string in the canonical `tenant:<id>:<cid>` shape.
    #[must_use]
    pub fn audit_subject(&self) -> String {
        format!("tenant:{}:{}", self.tenant_id, self.correlation_id)
    }
}

// ---------------------------------------------------------------------------
// Pure logic surface — works on both targets
// ---------------------------------------------------------------------------

/// In-memory state of a [`ClerkHealthLogic`] instance. On wasm32 this is
/// the in-actor cache mirroring the durable `state.storage` keyspace
/// `record:<cid>`. On native it's the canonical authoritative copy used
/// by integration tests.
#[derive(Debug, Default)]
pub struct ClerkHealthState {
    records: BTreeMap<String, HealthRecord>,
    /// TTL applied by [`ClerkHealthLogic::sweep`]. Records with
    /// `created_at_ms + ttl_ms < now_ms` are removed.
    ttl_ms: i64,
}

impl ClerkHealthState {
    /// Construct with the default 1 h TTL.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: BTreeMap::new(),
            ttl_ms: DEFAULT_TTL_MS,
        }
    }

    /// Override the TTL (used by tests + alarm tuning).
    #[must_use]
    pub fn with_ttl_ms(mut self, ttl_ms: i64) -> Self {
        self.ttl_ms = ttl_ms;
        self
    }

    /// Borrow the TTL.
    #[must_use]
    pub fn ttl_ms(&self) -> i64 {
        self.ttl_ms
    }

    /// Borrow the records map (test surface; production callers use the
    /// actor's `fetch()` routes).
    #[must_use]
    pub fn records(&self) -> &BTreeMap<String, HealthRecord> {
        &self.records
    }
}

/// Pure-logic actor implementation. Owns the in-memory state + audit
/// hook + the anchored tenant id. Holds no `worker::*` types so it
/// compiles and runs on both targets; the wasm32 `ClerkHealthDo`
/// shell wraps this type plus a `worker::durable::State` to persist
/// across requests.
pub struct ClerkHealthLogic {
    tenant_id: String,
    state: ClerkHealthState,
    audit: AuditFn,
}

impl std::fmt::Debug for ClerkHealthLogic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClerkHealthLogic")
            .field("tenant_id", &self.tenant_id)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl ClerkHealthLogic {
    /// Construct a logic instance anchored to a tenant id (the tenant
    /// segment extracted from the actor's DO name). Audit defaults to
    /// the no-op hook; tests replace via [`Self::with_audit`].
    ///
    /// # Errors
    ///
    /// Returns [`HealthDoError::ValidationFailed`] if `tenant_id` is empty
    /// or contains forbidden bytes (`:`, `/`, NUL, whitespace) — these
    /// would corrupt the audit subject derivation.
    pub fn new(tenant_id: impl Into<String>) -> Result<Self, HealthDoError> {
        let tenant_id = tenant_id.into();
        if tenant_id.is_empty() {
            return Err(HealthDoError::ValidationFailed(
                "tenant_id empty".to_owned(),
            ));
        }
        for forbidden in [':', '/', '\0'] {
            if tenant_id.contains(forbidden) {
                return Err(HealthDoError::ValidationFailed(format!(
                    "tenant_id contains forbidden byte '{forbidden}'"
                )));
            }
        }
        if tenant_id.chars().any(char::is_whitespace) {
            return Err(HealthDoError::ValidationFailed(
                "tenant_id contains whitespace".to_owned(),
            ));
        }
        Ok(Self {
            tenant_id,
            state: ClerkHealthState::new(),
            audit: default_audit(),
        })
    }

    /// Replace the audit hook.
    #[must_use]
    pub fn with_audit(mut self, audit: AuditFn) -> Self {
        self.audit = audit;
        self
    }

    /// Replace the in-memory state (used to override the TTL or seed
    /// records in tests).
    #[must_use]
    pub fn with_state(mut self, state: ClerkHealthState) -> Self {
        self.state = state;
        self
    }

    /// Borrow the anchored tenant id.
    #[must_use]
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// Borrow the state map.
    #[must_use]
    pub fn state(&self) -> &ClerkHealthState {
        &self.state
    }

    /// Emit the audit event for an op. Fail-CLOSED on mutations.
    fn audit(&self, op: HealthDoOp, route: &ParsedRoute) -> Result<(), HealthDoError> {
        let subject = route.audit_subject();
        (self.audit)(op, &subject)
    }

    /// Look up a record. Returns `Err(HealthDoError::NotFound)` if absent.
    ///
    /// # Errors
    ///
    /// - [`HealthDoError::TenantScope`] if the route tenant != actor tenant.
    /// - [`HealthDoError::NotFound`] if the record is absent.
    /// - [`HealthDoError::AuditDenied`] if the advisory audit emission errors.
    pub fn get(&self, route: &ParsedRoute) -> Result<HealthRecord, HealthDoError> {
        route.assert_tenant(&self.tenant_id)?;
        // Advisory audit; on a recorder mutex poison this still aborts —
        // the audit trail is incomplete and the caller MUST fail loud.
        self.audit(HealthDoOp::Get, route)?;
        self.state
            .records
            .get(&route.correlation_id)
            .cloned()
            .ok_or(HealthDoError::NotFound)
    }

    /// Upsert a record. Audit-emit-BEFORE; fail-CLOSED on audit error.
    ///
    /// # Errors
    ///
    /// - [`HealthDoError::TenantScope`] if the route tenant != actor tenant.
    /// - [`HealthDoError::AuditDenied`] if the audit hook returns `Err`.
    /// - [`HealthDoError::ValidationFailed`] if the record fails construction.
    pub fn upsert(
        &mut self,
        route: &ParsedRoute,
        note: impl Into<String>,
        now_ms: i64,
    ) -> Result<HealthRecord, HealthDoError> {
        route.assert_tenant(&self.tenant_id)?;
        // Fail-CLOSED: audit BEFORE mutation.
        self.audit(HealthDoOp::Upsert, route)?;
        let record = HealthRecord::new(route.correlation_id.clone(), note, now_ms)?;
        self.state
            .records
            .insert(route.correlation_id.clone(), record.clone());
        Ok(record)
    }

    /// Tombstone (delete) a record. Audit-emit-BEFORE; fail-CLOSED on
    /// audit error.
    ///
    /// Returns `Ok(true)` if the key existed and was removed; `Ok(false)`
    /// if the key was already absent (idempotent delete).
    ///
    /// # Errors
    ///
    /// - [`HealthDoError::TenantScope`] if the route tenant != actor tenant.
    /// - [`HealthDoError::AuditDenied`] if the audit hook returns `Err`.
    pub fn tombstone(&mut self, route: &ParsedRoute) -> Result<bool, HealthDoError> {
        route.assert_tenant(&self.tenant_id)?;
        self.audit(HealthDoOp::Tombstone, route)?;
        Ok(self.state.records.remove(&route.correlation_id).is_some())
    }

    /// TTL sweep — remove every record whose `created_at_ms + ttl_ms <
    /// now_ms`. Audit-emit-BEFORE for every key removed (one event per
    /// key); fail-CLOSED on audit error.
    ///
    /// Returns the count of records removed.
    ///
    /// # Errors
    ///
    /// - [`HealthDoError::AuditDenied`] if the audit hook returns `Err`
    ///   for any key (the sweep aborts after the failed audit; remaining
    ///   keys are left in storage).
    pub fn sweep(&mut self, now_ms: i64) -> Result<usize, HealthDoError> {
        let ttl = self.state.ttl_ms;
        let expired_keys: Vec<String> = self
            .state
            .records
            .iter()
            .filter(|(_, r)| r.created_at_ms.saturating_add(ttl) < now_ms)
            .map(|(k, _)| k.clone())
            .collect();
        let mut removed = 0usize;
        for cid in expired_keys {
            let route = ParsedRoute {
                method: HealthMethod::Delete,
                tenant_id: self.tenant_id.clone(),
                correlation_id: cid.clone(),
            };
            // Fail-CLOSED: each sweep emission is audit-fenced.
            self.audit(HealthDoOp::Sweep, &route)?;
            if self.state.records.remove(&cid).is_some() {
                removed = removed.saturating_add(1);
            }
        }
        Ok(removed)
    }
}

// ---------------------------------------------------------------------------
// JSON request / response bodies
// ---------------------------------------------------------------------------

/// Body for POST `/record/{tenant}/{cid}`. The actor reads the request
/// body as JSON of this shape.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpsertRequest {
    /// Free-form note attached to the probe (≤ 1024 bytes, no NUL).
    pub note: String,
}

/// Response body for `GET /record/{tenant}/{cid}` and the POST mutation
/// echo. Mirrors [`HealthRecord`] one-to-one for now; lives as its own
/// type so the wire format can drift independently of the storage shape.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordResponse {
    /// Correlation identifier.
    pub correlation_id: String,
    /// Note attached at upsert time.
    pub note: String,
    /// Unix epoch ms at upsert.
    pub created_at_ms: i64,
}

impl From<HealthRecord> for RecordResponse {
    fn from(value: HealthRecord) -> Self {
        Self {
            correlation_id: value.correlation_id,
            note: value.note,
            created_at_ms: value.created_at_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// wasm32 actor: #[durable_object] shell
// ---------------------------------------------------------------------------
//
// The wasm32 path wires `ClerkHealthLogic` into a `#[durable_object]`
// actor whose `fetch()` parses the route, applies the logic operation,
// and persists state via `state.storage`. The `alarm()` handler invokes
// `sweep()` and re-arms the alarm.
//
// Storage layout: each `HealthRecord` is persisted under `record:<cid>`
// (JSON via worker::Storage::put). The tenant id is NOT part of the key
// (the actor instance is already tenant-anchored).
//
// On every `fetch()` call we hydrate `ClerkHealthLogic.state` from the
// durable storage map (one-shot read) so the audit + validation contract
// runs against the same record set the request will mutate.

/// Storage-key prefix for persisted records inside the DO's
/// transactional storage. The actor instance is tenant-anchored so
/// the tenant id is NOT part of the key.
#[cfg(target_arch = "wasm32")]
const RECORD_KEY_PREFIX: &str = "record:";

/// Alarm cadence — fires every `DEFAULT_TTL_MS / 4` so a record is
/// guaranteed to be swept within `TTL + 1/4 TTL`.
#[cfg(target_arch = "wasm32")]
const ALARM_INTERVAL_MS: i64 = DEFAULT_TTL_MS / 4;

/// Audit-emitter wired into the actor's hot path. On wasm32 the
/// emission fans into `worker::console_log!` (Logpush ingest).
#[cfg(target_arch = "wasm32")]
fn console_audit() -> AuditFn {
    Arc::new(|op: HealthDoOp, subject: &str| -> Result<(), HealthDoError> {
        worker::console_log!(
            "{{\"surface\":\"clerk_health_do\",\"op\":\"{}\",\"subject\":\"{}\"}}",
            op.as_str(),
            subject,
        );
        Ok(())
    })
}

/// `ClerkHealthDo` — production Durable Object actor backing the
/// `CLERK_DO` namespace binding. See the module docs for the route
/// surface and TTL sweep semantics.
#[cfg(target_arch = "wasm32")]
#[worker::durable_object]
#[allow(
    missing_debug_implementations,
    missing_docs,
    reason = "the #[durable_object] macro emits #[wasm_bindgen] glue methods \
              (new/fetch/alarm) that do not carry doc comments, and the \
              generated struct cannot derive Debug because `worker::State` / \
              `worker::Env` are not `Debug`"
)]
pub struct ClerkHealthDo {
    state: worker::State,
    /// Worker `Env` retained on the actor in case future routes need to
    /// hand-off to other bindings (e.g. emit an audit event into D1).
    /// Currently unused at the route level.
    #[allow(dead_code)]
    env: worker::Env,
}

#[cfg(target_arch = "wasm32")]
impl ClerkHealthDo {
    /// Build a `ClerkHealthLogic` for the current request, hydrating
    /// the in-memory state from durable storage.
    async fn load_logic(&self) -> Result<ClerkHealthLogic, HealthDoError> {
        let tenant = self.actor_tenant_id().await?;
        let mut logic = ClerkHealthLogic::new(tenant)?.with_audit(console_audit());
        let mut hydrated_state = ClerkHealthState::new();
        let map = self
            .state
            .storage()
            .list()
            .await
            .map_err(|e| HealthDoError::Backend(format!("storage.list: {e}")))?;
        let entries = js_sys::Array::from(&map.entries().into());
        for entry in entries.iter() {
            let pair = js_sys::Array::from(&entry);
            if let (Some(k), Some(v)) = (pair.get(0).as_string(), pair.get(1).as_string())
            {
                if let Some(cid) = k.strip_prefix(RECORD_KEY_PREFIX) {
                    if let Ok(rec) = serde_json::from_str::<HealthRecord>(&v) {
                        // Manual insert: bypass the audit hook
                        // (hydration is not a mutation event).
                        hydrated_state = ClerkHealthLogic::seed_state(
                            hydrated_state,
                            cid.to_owned(),
                            rec,
                        );
                    }
                }
            }
        }
        logic = logic.with_state(hydrated_state);
        Ok(logic)
    }

    /// Resolve the tenant id segment from the actor's DO name. The
    /// caller (`CfDurableObjectReal`) constructs names of the shape
    /// `tenant:<id>:<purpose>` — we strip both ceremony segments.
    async fn actor_tenant_id(&self) -> Result<String, HealthDoError> {
        let name = self.state.id().name().ok_or_else(|| {
            HealthDoError::Backend(
                "actor name unavailable (idFromName required)".to_owned(),
            )
        })?;
        // Expect `tenant:<id>:<purpose>` — split on `:` and take the
        // middle segment.
        let parts: Vec<&str> = name.splitn(3, ':').collect();
        if parts.len() < 2 || parts.first().copied() != Some("tenant") {
            return Err(HealthDoError::TenantScope(format!(
                "actor name '{name}' is not tenant-scoped"
            )));
        }
        let tenant = parts.get(1).copied().unwrap_or("").to_owned();
        if tenant.is_empty() {
            return Err(HealthDoError::TenantScope(format!(
                "actor name '{name}' has empty tenant segment"
            )));
        }
        Ok(tenant)
    }

    /// Persist a single record under the canonical key.
    async fn persist_record(
        &self,
        record: &HealthRecord,
    ) -> Result<(), HealthDoError> {
        let key = format!("{RECORD_KEY_PREFIX}{}", record.correlation_id);
        let json = serde_json::to_string(record)
            .map_err(|e| HealthDoError::Backend(format!("serialize: {e}")))?;
        self.state
            .storage()
            .put_raw(&key, wasm_bindgen::JsValue::from_str(&json))
            .await
            .map_err(|e| HealthDoError::Backend(format!("storage.put: {e}")))
    }

    /// Delete a single record under the canonical key.
    async fn delete_record(&self, cid: &str) -> Result<(), HealthDoError> {
        let key = format!("{RECORD_KEY_PREFIX}{cid}");
        self.state
            .storage()
            .delete(&key)
            .await
            .map(|_| ())
            .map_err(|e| HealthDoError::Backend(format!("storage.delete: {e}")))
    }

    /// Ensure the TTL sweep alarm is armed.
    async fn ensure_alarm(&self) -> Result<(), HealthDoError> {
        let already = self
            .state
            .storage()
            .get_alarm()
            .await
            .map_err(|e| HealthDoError::Backend(format!("get_alarm: {e}")))?;
        if already.is_none() {
            let now_ms = worker::js_sys::Date::now() as i64;
            let next = now_ms.saturating_add(ALARM_INTERVAL_MS);
            self.state
                .storage()
                .set_alarm(next)
                .await
                .map_err(|e| HealthDoError::Backend(format!("set_alarm: {e}")))?;
        }
        Ok(())
    }

    /// Translate a [`HealthDoError`] into a `worker::Response`.
    fn err_response(err: HealthDoError) -> worker::Result<worker::Response> {
        let status = err.status();
        // `Response::error` requires status >= 400; use `from_json` +
        // `with_status` for full coverage. `with_status` is infallible
        // (returns `Response` directly, not `Result`) so wrap it back into
        // the `worker::Result` shape via `map`.
        worker::Response::from_json(&serde_json::json!({"error": err.to_string()}))
            .map(|r| r.with_status(status))
    }

    /// Inner dispatch: parse the request, invoke the matching logic
    /// op, persist, return the response.
    async fn dispatch(
        &self,
        req: worker::Request,
    ) -> Result<worker::Response, HealthDoError> {
        let method = req.method().to_string();
        let url = req
            .url()
            .map_err(|e| HealthDoError::Backend(format!("request.url(): {e}")))?;
        let path = url.path().to_owned();

        let route = ParsedRoute::parse(&method, &path)?;
        let mut logic = self.load_logic().await?;

        match route.method {
            HealthMethod::Get => {
                let record = logic.get(&route)?;
                worker::Response::from_json(&RecordResponse::from(record))
                    .map_err(|e| HealthDoError::Backend(format!("response: {e}")))
            }
            HealthMethod::Post => {
                let mut req = req;
                let body: UpsertRequest = req.json().await.map_err(|e| {
                    HealthDoError::ValidationFailed(format!(
                        "request body not UpsertRequest JSON: {e}"
                    ))
                })?;
                let now_ms = worker::js_sys::Date::now() as i64;
                let record = logic.upsert(&route, body.note, now_ms)?;
                self.persist_record(&record).await?;
                self.ensure_alarm().await?;
                worker::Response::from_json(&RecordResponse::from(record))
                    .map_err(|e| HealthDoError::Backend(format!("response: {e}")))
            }
            HealthMethod::Delete => {
                let existed = logic.tombstone(&route)?;
                if existed {
                    self.delete_record(&route.correlation_id).await?;
                }
                worker::Response::from_json(&serde_json::json!({
                    "deleted": existed,
                }))
                .map_err(|e| HealthDoError::Backend(format!("response: {e}")))
            }
        }
    }

    /// Inner alarm handler: sweep + re-arm.
    async fn alarm_inner(&self) -> Result<usize, HealthDoError> {
        let mut logic = self.load_logic().await?;
        let now_ms = worker::js_sys::Date::now() as i64;
        let removed = logic.sweep(now_ms)?;
        // Mirror sweep result onto persistent storage.
        let surviving: std::collections::BTreeSet<String> =
            logic.state().records().keys().cloned().collect();
        // Delete any storage key not surviving.
        let map = self
            .state
            .storage()
            .list()
            .await
            .map_err(|e| HealthDoError::Backend(format!("storage.list: {e}")))?;
        let entries = js_sys::Array::from(&map.entries().into());
        for entry in entries.iter() {
            let pair = js_sys::Array::from(&entry);
            if let Some(k) = pair.get(0).as_string() {
                if let Some(cid) = k.strip_prefix(RECORD_KEY_PREFIX) {
                    if !surviving.contains(cid) {
                        let _ = self.state.storage().delete(&k).await;
                    }
                }
            }
        }
        // Re-arm.
        let next = now_ms.saturating_add(ALARM_INTERVAL_MS);
        let _ = self.state.storage().set_alarm(next).await;
        Ok(removed)
    }
}

#[cfg(target_arch = "wasm32")]
impl worker::DurableObject for ClerkHealthDo {
    fn new(state: worker::State, env: worker::Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&self, req: worker::Request) -> worker::Result<worker::Response> {
        match self.dispatch(req).await {
            Ok(resp) => Ok(resp),
            Err(err) => Self::err_response(err),
        }
    }

    async fn alarm(&self) -> worker::Result<worker::Response> {
        match self.alarm_inner().await {
            Ok(removed) => worker::Response::from_json(&serde_json::json!({
                "swept": removed,
            })),
            Err(err) => Self::err_response(err),
        }
    }
}

// ---------------------------------------------------------------------------
// Native test helpers — seed without firing audit
// ---------------------------------------------------------------------------

impl ClerkHealthLogic {
    /// Seed a [`ClerkHealthState`] with a single record. Bypasses the
    /// audit hook — intended only for state hydration / tests.
    #[doc(hidden)]
    #[must_use]
    pub fn seed_state(
        mut state: ClerkHealthState,
        cid: String,
        record: HealthRecord,
    ) -> ClerkHealthState {
        state.records.insert(cid, record);
        state
    }
}

// ---------------------------------------------------------------------------
// Inline tests — pure logic surface (runs on both targets).
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::type_complexity,
    reason = "test code: panics on assertion failure are the canonical signal"
)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    type TestAuditBuf = Arc<Mutex<Vec<(HealthDoOp, String)>>>;

    fn recorder() -> (AuditFn, TestAuditBuf) {
        let buf: TestAuditBuf = Arc::new(Mutex::new(Vec::new()));
        let buf_clone = Arc::clone(&buf);
        let f: AuditFn = Arc::new(move |op, subject| {
            buf_clone
                .lock()
                .map_err(|_| HealthDoError::Backend("audit lock poisoned".to_owned()))?
                .push((op, subject.to_owned()));
            Ok(())
        });
        (f, buf)
    }

    #[test]
    fn parsed_route_happy_path() {
        let r =
            ParsedRoute::parse("POST", "/record/tnt0123/cid-xyz").expect("happy parse");
        assert_eq!(r.tenant_id, "tnt0123");
        assert_eq!(r.correlation_id, "cid-xyz");
        assert_eq!(r.method, HealthMethod::Post);
        assert_eq!(r.audit_subject(), "tenant:tnt0123:cid-xyz");
    }

    #[test]
    fn parsed_route_rejects_missing_segments() {
        ParsedRoute::parse("GET", "/record/onlyTenant")
            .expect_err("missing cid rejected");
        ParsedRoute::parse("GET", "/record//cid")
            .expect_err("empty tenant rejected");
        ParsedRoute::parse("GET", "/wrongprefix/a/b")
            .expect_err("non /record/ prefix rejected");
    }

    #[test]
    fn parsed_route_rejects_bad_method() {
        ParsedRoute::parse("PATCH", "/record/a/b").expect_err("PATCH rejected");
    }

    #[test]
    fn tenant_assertion_constant_time_path() {
        let r =
            ParsedRoute::parse("GET", "/record/tnt0123/cid").expect("parse");
        r.assert_tenant("tnt0123").expect("match");
        let err = r
            .assert_tenant("tntFFFF")
            .expect_err("mismatch rejected");
        assert!(err.to_string().contains("tenant_scope"));
    }

    #[test]
    fn upsert_get_delete_roundtrip() {
        let (audit, buf) = recorder();
        let mut logic = ClerkHealthLogic::new("tnt0123")
            .expect("ctor")
            .with_audit(audit);
        let route = ParsedRoute::parse("POST", "/record/tnt0123/cid1").expect("parse");
        let rec = logic.upsert(&route, "hello", 1_000_000).expect("upsert");
        assert_eq!(rec.note, "hello");
        assert_eq!(rec.created_at_ms, 1_000_000);

        let got = logic
            .get(&ParsedRoute::parse("GET", "/record/tnt0123/cid1").expect("parse"))
            .expect("get");
        assert_eq!(got, rec);

        let removed = logic
            .tombstone(&ParsedRoute::parse("DELETE", "/record/tnt0123/cid1").expect(
                "parse",
            ))
            .expect("delete");
        assert!(removed, "first delete returns true");

        let err = logic
            .get(&ParsedRoute::parse("GET", "/record/tnt0123/cid1").expect("parse"))
            .expect_err("absent after delete");
        assert!(matches!(err, HealthDoError::NotFound));

        let captured = buf.lock().expect("lock");
        // upsert + get + tombstone + get-after-delete = 4 audit events.
        // The advisory audit on GET fires BEFORE the storage lookup —
        // a 404-equivalent outcome does not suppress the trail.
        assert_eq!(captured.len(), 4, "events: {captured:?}");
        assert_eq!(captured[0].0, HealthDoOp::Upsert);
        assert_eq!(captured[1].0, HealthDoOp::Get);
        assert_eq!(captured[2].0, HealthDoOp::Tombstone);
        assert_eq!(captured[3].0, HealthDoOp::Get);
    }

    #[test]
    fn cross_tenant_upsert_rejected_close() {
        let mut logic = ClerkHealthLogic::new("tntAAAA").expect("ctor");
        let route =
            ParsedRoute::parse("POST", "/record/tntBBBB/cidX").expect("parse");
        let err = logic
            .upsert(&route, "spoof", 1_000)
            .expect_err("cross-tenant rejected");
        assert!(matches!(err, HealthDoError::TenantScope(_)));
    }

    #[test]
    fn audit_fail_closed_blocks_mutation() {
        let deny: AuditFn = Arc::new(|_op, _subj| {
            Err(HealthDoError::AuditDenied("policy".to_owned()))
        });
        let mut logic = ClerkHealthLogic::new("tnt0123")
            .expect("ctor")
            .with_audit(deny);
        let route = ParsedRoute::parse("POST", "/record/tnt0123/cid").expect("parse");
        let err = logic
            .upsert(&route, "note", 1_000)
            .expect_err("audit blocks upsert");
        assert!(matches!(err, HealthDoError::AuditDenied(_)));
        // State stayed empty — fail-CLOSED.
        assert!(logic.state().records().is_empty());
    }

    #[test]
    fn sweep_removes_expired_records() {
        let (audit, buf) = recorder();
        let state = ClerkHealthState::new().with_ttl_ms(1_000);
        let mut logic = ClerkHealthLogic::new("tnt0123")
            .expect("ctor")
            .with_audit(audit)
            .with_state(state);
        let route_a = ParsedRoute::parse("POST", "/record/tnt0123/A").expect("parse");
        let route_b = ParsedRoute::parse("POST", "/record/tnt0123/B").expect("parse");
        logic.upsert(&route_a, "old", 1_000).expect("upsert A");
        logic.upsert(&route_b, "young", 5_000).expect("upsert B");
        // now=5500 → A is older than 5500-1000=4500 → A expires; B remains.
        let removed = logic.sweep(5_500).expect("sweep");
        assert_eq!(removed, 1);
        assert!(!logic.state().records().contains_key("A"));
        assert!(logic.state().records().contains_key("B"));
        let captured = buf.lock().expect("lock");
        // upsert + upsert + sweep(A) = 3 audit events.
        assert_eq!(captured.len(), 3);
        assert_eq!(captured[2].0, HealthDoOp::Sweep);
        assert_eq!(captured[2].1, "tenant:tnt0123:A");
    }

    #[test]
    fn record_size_limits_enforced() {
        HealthRecord::new("", "note", 0).expect_err("empty cid");
        let big_cid = "a".repeat(257);
        HealthRecord::new(big_cid, "note", 0).expect_err("oversize cid");
        let big_note = "x".repeat(1025);
        HealthRecord::new("cid", big_note, 0).expect_err("oversize note");
        HealthRecord::new("c\0id", "note", 0).expect_err("NUL in cid");
        HealthRecord::new("cid", "no\0te", 0).expect_err("NUL in note");
    }
}
