//! Durable D1 `audit_outbox`-backed [`AuditSink`] for the native CAS + AC
//! data plane.
//!
//! # Why this exists (F1 / CAA-360)
//!
//! `R2CasHandler` / `R2AcHandler` emit an audit row BEFORE every data-plane
//! mutation and treat any sink error as fail-CLOSED (the route returns 503 on
//! `AuditFailed`, aborting the mutation). Until this module existed, the
//! deployed builders (`build_r2_cas_handler_from_env` /
//! `build_r2_ac_handler_from_env`) hardcoded the in-process
//! `InMemoryAuditSink`, so:
//!
//! * every CAS/AC data-plane audit event (`ReadAttempted`, `ReadDenied`,
//!   `WriteCommitted`, `CorrectnessViolation`, …) was written ONLY to RAM and
//!   lost on container restart — the "durable audit row before mutation"
//!   guarantee was unwired; and
//! * `InMemoryAuditSink::emit` only errors under a test-injected failure, so
//!   the route's fail-CLOSED `AuditFailed → 503` guard was dead code in prod.
//!
//! This sink appends each event as a PLAIN, UNCHAINED row to the D1
//! `audit_outbox` intake table (`migrations/d1/0001_blob_meta.sql`) — the
//! exact same trail the S-09 audit-drain (`routes/audit_drain.rs`) seals and
//! the DSR erasure sink (`routes/dsr/audit.rs`) already writes to. A D1
//! transport failure propagates as `Err`, which the handler treats as
//! fail-CLOSED — so the 503 guard is now reachable in production.
//!
//! ## Tamper-evidence posture (identical to the DSR sink)
//!
//! Rows land with `emitted_at = NULL` and carry NO BLAKE3 chain link. They are
//! ordinary mutable D1 rows until the S-09 drain seals them; do NOT describe
//! the live trail as "chained" / "sealed" / "tamper-evident".
//!
//! ## `region` column
//!
//! Mirrors the DSR sink exactly: the INSERT omits `region`, so the row takes
//! the table DEFAULT (`'wnam'`, migration 0023). This is the established,
//! prod-live durable-writer convention; a per-event residency-region column is
//! a cross-cutting change tracked with the DSR sink, not introduced here.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::customer_d1::ms_to_iso8601;
use crate::storage::d1_http::D1HttpClient;

/// Durable audit sink that appends CAS/AC data-plane audit rows to the D1
/// `audit_outbox` table.
///
/// A single struct implements BOTH [`corelink_handler_cas::AuditSink`] and
/// [`corelink_handler_ac::AuditSink`]; the `source` field (`"corelink/cas"` /
/// `"corelink/ac"`) — set at construction — distinguishes the CloudEvents
/// `source` in the persisted envelope.
pub struct D1AuditOutboxSink {
    d1: Arc<D1HttpClient>,
    /// CloudEvents `source` — `"corelink/cas"` or `"corelink/ac"`.
    source: &'static str,
}

impl core::fmt::Debug for D1AuditOutboxSink {
    // Never surface the inner client's token; the struct NAME is the durable
    // marker the builder tests assert on (vs `InMemoryAuditSink`).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("D1AuditOutboxSink")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl D1AuditOutboxSink {
    /// Construct over a shared [`D1HttpClient`] with a fixed CloudEvents
    /// `source`.
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>, source: &'static str) -> Self {
        Self { d1, source }
    }

    /// Append a single audit event to `audit_outbox`.
    ///
    /// Keys (`id`, `request_id`) are deterministic per logical event so a
    /// re-emit collides on the PK / `UNIQUE(request_id, event_type)` and is an
    /// `INSERT OR IGNORE` no-op — idempotent on a handler retry (mirrors the
    /// DSR sink). Returns `Err` on serialize / D1 transport failure; the
    /// handler treats that as fail-CLOSED.
    fn append(
        &self,
        event_type: &str,
        tenant: &str,
        digest: Option<&str>,
        principal: &str,
        at_unix_ms: u64,
    ) -> Result<(), String> {
        // Epoch ms never realistically exceeds i64::MAX; saturate defensively.
        let at_ms = i64::try_from(at_unix_ms).unwrap_or(i64::MAX);
        // Treat an empty digest (pre-write events supply no hash) as SQL NULL.
        let dig = digest.filter(|d| !d.is_empty());

        let id = format!(
            "{}:{tenant}:{event_type}:{}:{principal}:{at_ms}",
            self.source,
            dig.unwrap_or("-"),
        );
        let request_id = format!("{tenant}:{}:{principal}:{at_ms}", dig.unwrap_or("-"));

        let payload = json!({
            "specversion": "1.0",
            "type": event_type,
            "source": self.source,
            "id": id,
            "subject": tenant,
            "time": ms_to_iso8601(at_ms),
            "data": {
                "tenant_id": tenant,
                "digest": dig,
                "principal": principal,
            }
        });
        let payload_json =
            serde_json::to_string(&payload).map_err(|e| format!("audit payload serialize: {e}"))?;

        // `region` MUST be the tenant's `primary_region`, NOT the `'wnam'` column
        // default: migration 0023 installs a BEFORE-INSERT residency trigger
        // (`trg_audit_outbox_region_match_insert`) that RAISE(ABORT)s when
        // `NEW.region != tenant.primary_region`. Tenants default to `'enam'`
        // (migration 0028), so relying on the `'wnam'` default aborts the INSERT
        // for essentially every tenant → the handler fails CLOSED → 503 on every
        // CAS/AC op (prod incident 2026-07-17, surfaced when task #74 flipped this
        // sink from InMemory to durable-D1). Set `region` from a correlated
        // subquery so the row is tagged with the tenant's true residency region
        // and always satisfies the trigger; COALESCE to `'wnam'` for the
        // tenant-absent case (the trigger's `NEW.region != NULL` is UNKNOWN ⇒ no
        // abort). Mirrors the fix owed to `routes/dsr/audit.rs`.
        let sql = "INSERT OR IGNORE INTO audit_outbox \
             (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at, region) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, \
                     COALESCE((SELECT primary_region FROM tenant WHERE tenant_id = ?2), 'wnam'))";
        let digest_param = match dig {
            Some(d) => Value::String(d.to_owned()),
            None => Value::Null,
        };
        let params = vec![
            json!(id),
            json!(tenant),
            digest_param,
            json!(request_id),
            json!(event_type),
            json!(payload_json),
            json!(at_ms),
        ];
        self.write_blocking(sql, params)
    }

    /// Drive the async [`D1HttpClient::query`] to completion from the sync
    /// `AuditSink::emit` surface. MUST run on a multi-thread tokio worker (the
    /// axum handler path) — identical bridge + safety envelope as
    /// `routes/dsr/d1util::d1_query_blocking`.
    fn write_blocking(&self, sql: &str, params: Vec<Value>) -> Result<(), String> {
        let d1 = Arc::clone(&self.d1);
        let sql = sql.to_owned();
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move { d1.query(&sql, &params).await })
        })
        .map(|_| ())
    }
}

impl corelink_handler_cas::AuditSink for D1AuditOutboxSink {
    fn emit(&self, event: corelink_handler_cas::AuditEvent) -> Result<(), String> {
        self.append(
            event.kind.slug(),
            &event.tenant,
            Some(event.hash.as_str()),
            &event.principal,
            event.at_unix_ms,
        )
    }
}

impl corelink_handler_ac::AuditSink for D1AuditOutboxSink {
    fn emit(&self, event: corelink_handler_ac::AuditEvent) -> Result<(), String> {
        self.append(
            event.kind.slug(),
            &event.tenant,
            Some(event.action_digest.as_str()),
            &event.principal,
            event.at_unix_ms,
        )
    }
}

/// [`crate::routes::signup::SignupAuditSink`] over the SAME durable
/// `audit_outbox` table + bridge as the CAS/AC impls above — this is a NEW
/// trait impl on the EXISTING durable sink, not a new audit mechanism.
///
/// `SignupAuditRow` doesn't carry a `digest` (pre-auth pilot signups have
/// no blob hash); the token id/prefix rides in the `digest` column slot
/// instead (same "nullable, non-blob events" contract the column doc
/// already states) and the full row (`tenant_id`, `token_id_or_prefix`,
/// `exit_status`, `payload`) is preserved verbatim in the CloudEvents
/// `data` envelope, unlike the CAS/AC `append()` helper which only carries
/// `(tenant, digest, principal)`. Pre-auth events (rate-limit / token
/// reject, before a tenant_id is allocated) use the nil UUID sentinel —
/// mirrors `routes::signup::PRE_AUTH_TENANT`.
impl crate::routes::signup::SignupAuditSink for D1AuditOutboxSink {
    fn emit(&self, row: crate::routes::signup::SignupAuditRow) -> Result<(), &'static str> {
        let at_ms = i64::try_from(row.emitted_at_ms).unwrap_or(i64::MAX);
        let tenant = row.tenant_id.unwrap_or_else(uuid::Uuid::nil).to_string();
        let token = row.token_id_or_prefix.clone().unwrap_or_default();
        let id = format!(
            "{}:{tenant}:{}:{}:{at_ms}",
            self.source,
            row.event_type,
            if token.is_empty() { "-" } else { &token },
        );
        let request_id = format!(
            "{tenant}:{}:{}:{at_ms}",
            row.event_type,
            if token.is_empty() { "-" } else { &token },
        );
        let payload = json!({
            "specversion": "1.0",
            "type": row.event_type,
            "source": self.source,
            "id": id,
            "subject": tenant,
            "time": ms_to_iso8601(at_ms),
            "data": {
                "tenant_id": row.tenant_id.map(|u| u.to_string()),
                "token_id_or_prefix": row.token_id_or_prefix,
                "exit_status": row.exit_status,
                "payload": row.payload,
            }
        });
        let payload_json = serde_json::to_string(&payload)
            .map_err(|_| "signup audit sink: payload serialize failed")?;
        let digest_param = if token.is_empty() {
            Value::Null
        } else {
            json!(token)
        };
        // Same region-safety COALESCE as `append()`: a pilot signup's
        // tenant_id typically does NOT exist in `tenant` yet (the operator's
        // `grant-pilot-tier.sh` provisions it later), so the correlated
        // subquery returns NULL and the trigger's `!= NULL` is UNKNOWN ⇒ no
        // abort; COALESCE pins the row to `'wnam'` in that case.
        let sql = "INSERT OR IGNORE INTO audit_outbox \
             (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at, region) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, \
                     COALESCE((SELECT primary_region FROM tenant WHERE tenant_id = ?2), 'wnam'))";
        let params = vec![
            json!(id),
            json!(tenant),
            digest_param,
            json!(request_id),
            json!(row.event_type),
            json!(payload_json),
            json!(at_ms),
        ];
        self.write_blocking(sql, params)
            .map_err(|_| "signup audit sink: D1 write failed")
    }
}

/// Yield a DURABLE CAS audit sink from a D1-client construction result, or
/// REFUSE (fail-CLOSED) when the D1 client could not be built.
///
/// The CAS builder passes `D1HttpClient::new(&env)` straight in; the `Result`
/// seam makes the refusal deterministically testable (a durable sink can't
/// force `reqwest` to fail).
///
/// # Errors
///
/// Returns `Err(String)` when `d1` is `Err` — the builder maps that to
/// `Some(Err(..))` and the route mounts the fail-CLOSED 503 handler instead of
/// a silent in-memory fallback.
pub(crate) fn cas_audit_sink_from_d1(
    d1: Result<D1HttpClient, String>,
) -> Result<Arc<dyn corelink_handler_cas::AuditSink>, String> {
    Ok(Arc::new(D1AuditOutboxSink::new(
        durable_client(d1)?,
        "corelink/cas",
    )))
}

/// AC counterpart of [`cas_audit_sink_from_d1`].
///
/// # Errors
///
/// Returns `Err(String)` when `d1` is `Err` (fail-CLOSED — see
/// [`cas_audit_sink_from_d1`]).
pub(crate) fn ac_audit_sink_from_d1(
    d1: Result<D1HttpClient, String>,
) -> Result<Arc<dyn corelink_handler_ac::AuditSink>, String> {
    Ok(Arc::new(D1AuditOutboxSink::new(
        durable_client(d1)?,
        "corelink/ac",
    )))
}

/// Pilot-signup counterpart of [`cas_audit_sink_from_d1`] — same
/// durable `audit_outbox` seam, wired for
/// `routes::signup::build_state_from_env()`.
///
/// # Errors
///
/// Returns `Err(String)` when `d1` is `Err` (fail-CLOSED — see
/// [`cas_audit_sink_from_d1`]).
pub(crate) fn signup_audit_sink_from_d1(
    d1: Result<D1HttpClient, String>,
) -> Result<Arc<dyn crate::routes::signup::SignupAuditSink>, String> {
    Ok(Arc::new(D1AuditOutboxSink::new(
        durable_client(d1)?,
        "corelink/signup",
    )))
}

/// Wrap the D1-client result into a shared handle, or fail CLOSED with a loud
/// message.
fn durable_client(d1: Result<D1HttpClient, String>) -> Result<Arc<D1HttpClient>, String> {
    d1.map(Arc::new)
        .map_err(|e| format!("durable audit sink unavailable: D1 client build failed: {e}"))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::storage::StorageEnv;

    /// A stub, fully-populated `StorageEnv` — built by struct literal (fields
    /// are `pub(crate)`) so no PROCESS ENV is mutated (this crate forbids the
    /// parallel-test `set_var` race — see `routes/residency.rs`).
    fn stub_env() -> StorageEnv {
        StorageEnv {
            r2_endpoint: "https://acct.r2.cloudflarestorage.com".to_owned(),
            r2_access_key_id: "ak".to_owned(),
            r2_secret_access_key: "sk".to_owned(),
            cloudflare_account_id: "acct123".to_owned(),
            cf_api_token: "tok".to_owned(),
            d1_database_id: "db456".to_owned(),
        }
    }

    #[test]
    fn deployed_cas_builder_yields_durable_non_inmemory_sink() {
        // This is EXACTLY the construction `build_r2_cas_handler_from_env`
        // performs (`cas_audit_sink_from_d1(D1HttpClient::new(&env))`), so it
        // proves the deployed builder wires a DURABLE sink when D1 env present.
        let sink =
            cas_audit_sink_from_d1(D1HttpClient::new(&stub_env())).expect("durable sink builds");
        let dbg = format!("{sink:?}");
        assert!(
            dbg.contains("D1AuditOutboxSink"),
            "must be the durable D1 sink, got: {dbg}"
        );
        assert!(
            !dbg.contains("InMemory"),
            "must NOT be the volatile in-memory sink, got: {dbg}"
        );
    }

    #[test]
    fn deployed_ac_builder_yields_durable_non_inmemory_sink() {
        let sink =
            ac_audit_sink_from_d1(D1HttpClient::new(&stub_env())).expect("durable sink builds");
        let dbg = format!("{sink:?}");
        assert!(dbg.contains("D1AuditOutboxSink"), "got: {dbg}");
        assert!(!dbg.contains("InMemory"), "got: {dbg}");
    }

    #[test]
    fn cas_sink_fails_closed_when_d1_client_unavailable() {
        // Storage creds present but the durable sink cannot be constructed →
        // REFUSE (Err), never a silent in-memory fallback. The builder maps
        // this to Some(Err(..)) → the route mounts the fail-CLOSED 503 handler.
        let err = cas_audit_sink_from_d1(Err("reqwest tls unavailable".to_owned()))
            .expect_err("must fail closed");
        assert!(
            err.contains("durable audit sink unavailable"),
            "loud fail-closed message, got: {err}"
        );
    }

    #[test]
    fn ac_sink_fails_closed_when_d1_client_unavailable() {
        let err = ac_audit_sink_from_d1(Err("reqwest tls unavailable".to_owned()))
            .expect_err("must fail closed");
        assert!(err.contains("durable audit sink unavailable"), "got: {err}");
    }
}
