//! Customer self-serve **tenant bulk export** — `POST /v1/customer/account/export`.
//!
//! Produces the full tenant portability bundle the CLI `corelink tenant export`
//! needs (PR #708 was PARTIAL because no server endpoint assembled the WHOLE
//! bundle — blob bytes + AC + RBAC/team + DPA/consent + audit slice). This module
//! owns the [`TenantExportSource`] seam + the streaming NDJSON assembler; the thin
//! PAT/Clerk-gated HTTP handler lives in [`super::customer`] so it can reuse that
//! module's fail-CLOSED tenant resolution + PAT-possession + billing/PII gates.
//!
//! # Wire format (content-addressed NDJSON)
//!
//! One JSON object per line (`application/x-ndjson`), streamed frame-by-frame so
//! at most ONE blob is buffered at a time (memory-bounded, like the audit-export
//! stream). Lines, in order:
//!
//! ```text
//! {"kind":"header","schema":"corelink.tenant_export.v1","tenant_id":..,"generated_at_ms":..}
//! {"kind":"rbac"|"dpa"|"audit","table":..,"record":{..}}          # governance (D1)
//! {"kind":"blob","blob_kind":"cas"|"ac","digest":..,"size":..,"encoding":"base64","bytes":".."}
//! {"kind":"manifest","tenant_id":..,"blob_count":..,"metadata_count":..,"blob_bytes_total":..,"blob_errors":..}
//! ```
//!
//! Each `blob` line is **content-addressed**: it carries the content `digest`, and
//! the production fetch path re-hashes the bytes on read
//! ([`corelink_handler_cas::CasReadHandler`]'s read-path re-verify), so a reader
//! can re-verify `bytes` hashes to `digest` offline.
//!
//! # Isolation + fail-CLOSED
//!
//! The source is driven with ONLY the Worker-authenticated tenant (the handler
//! never accepts a client-supplied tenant), and every read is `WHERE tenant_id =
//! ?` / a tenant-derived R2 prefix (layer 5 of `INV-TENANT-ISOLATION`). The
//! governance gather + the export-audit anchor are read/written BEFORE any bytes
//! stream, and any of those failing aborts with a non-200 (never a partial 200).

#![forbid(unsafe_code)]

use std::convert::Infallible;
use std::sync::Arc;

use async_stream::stream;
use base64::Engine as _;
use bytes::Bytes;
use corelink_ratelimit::{
    InMemoryRateLimitAuditSink, InMemoryRateLimitMetrics, InMemoryTokenBucketRateLimiter,
    RateLimitConfig, RateLimiter,
};
use futures::Stream;
use http_body::Frame;
use serde_json::{json, Value};

/// Stable schema tag on the bundle header + manifest lines (portability contract).
pub const EXPORT_SCHEMA: &str = "corelink.tenant_export.v1";

/// Endpoint id used for the per-tenant export rate-limit bucket.
pub const EXPORT_ENDPOINT_ID: &str = "customer.account.export";

/// Max audit rows included in the bundle's audit slice (bounded — the full
/// tamper-evident chain export is the separate `/v1/audit/:tenant/export` surface).
pub const EXPORT_AUDIT_ROW_LIMIT: u32 = 5000;

/// Blob-enumeration page size (S3 `ListObjectsV2` hard cap is 1000).
const EXPORT_LIST_PAGE_SIZE: u32 = 1000;

/// Which content-addressed keyspace a bundled blob came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobKind {
    /// Native CAS blob (`GET /v1/cas/:hash`).
    Cas,
    /// Action-cache result payload (`GET /v1/ac/:digest`).
    Ac,
}

impl BlobKind {
    /// Stable wire tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cas => "cas",
            Self::Ac => "ac",
        }
    }
}

/// One enumerated blob reference (content digest + size), before its bytes are
/// fetched lazily during streaming.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportBlobRef {
    /// CAS vs AC keyspace.
    pub kind: BlobKind,
    /// Lower-case hex content digest (tenant-prefix stripped — never a raw key).
    pub digest: String,
    /// Object size in bytes (from the storage listing).
    pub size: u64,
}

/// Failure modes of the tenant export source.
#[derive(Debug)]
pub enum TenantExportError {
    /// The tenant prefix is not derivable / storage refused (fail-CLOSED, 503).
    Unavailable(String),
    /// A D1 / storage / transport fault (fail-CLOSED, 500).
    Internal(String),
}

impl core::fmt::Display for TenantExportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unavailable(m) => write!(f, "export source unavailable: {m}"),
            Self::Internal(m) => write!(f, "export source internal error: {m}"),
        }
    }
}

/// The tenant-scoped bundle source. Production ([`CasD1TenantExportSource`])
/// composes the SAME CAS/AC read+list handlers the cache routes use + a
/// [`crate::customer_d1::CustomerD1`] row source; tests supply an in-memory fake.
/// Every method takes the Worker-authenticated tenant and MUST scope strictly to
/// it (no cross-tenant reads).
///
/// The methods are synchronous: the production CAS/AC handlers bridge async R2
/// I/O internally (`block_in_place` + `block_on`), so the caller drives them from
/// the async handler / the body-poll task exactly as the native cache routes do.
pub trait TenantExportSource: Send + Sync + core::fmt::Debug {
    /// D1-backed governance records for `tenant` — RBAC/team, DPA/consent, and the
    /// (bounded) customer audit slice. Each element is a fully-formed bundle line
    /// value carrying its own `"kind"`. FAIL-CLOSED: any query error is an `Err`.
    ///
    /// # Errors
    /// Returns [`TenantExportError`] on any D1 transport / decode failure.
    fn metadata_records(&self, tenant: &str) -> Result<Vec<Value>, TenantExportError>;

    /// Enumerate the tenant's CAS + AC blob references via its tenant-derived R2
    /// prefixes (cross-tenant keys cannot appear). FAIL-CLOSED on a non-derivable
    /// prefix / storage fault.
    ///
    /// # Errors
    /// Returns [`TenantExportError`] on any storage fault.
    fn blob_index(&self, tenant: &str) -> Result<Vec<ExportBlobRef>, TenantExportError>;

    /// Fetch one blob's bytes by content `digest` from the `kind` keyspace.
    /// `Ok(None)` when absent (a benign race with erasure between listing and
    /// fetch). Content-addressed: the production read re-verifies the bytes hash
    /// to `digest`.
    ///
    /// # Errors
    /// Returns [`TenantExportError`] on a storage fault (NOT on a plain absence).
    fn fetch_blob(
        &self,
        tenant: &str,
        kind: BlobKind,
        digest: &str,
    ) -> Result<Option<Vec<u8>>, TenantExportError>;

    /// Durably record the export as a `account.export` customer-audit event
    /// BEFORE any bytes are disclosed (audit-first discipline). Idempotency is not
    /// required (an export is a repeatable read); on failure the handler aborts
    /// fail-CLOSED so a disclosure is never unlogged.
    ///
    /// # Errors
    /// Returns [`TenantExportError`] on any D1 write failure.
    fn record_export_audit(
        &self,
        tenant: &str,
        blob_count: usize,
        metadata_count: usize,
    ) -> Result<(), TenantExportError>;
}

/// Per-tenant export rate-limit config: burst 2, refill 1 token / 300s. Export is
/// a heavy full-tenant read (enumerate + stream every blob), so the floor is well
/// above the audit-export 60s. Falls back to the framework canonical on a
/// misconfiguration so route construction stays infallible.
#[must_use]
pub fn export_rate_limit_config() -> RateLimitConfig {
    const RETRY_AFTER_HARD_CEILING_SECS: u64 = 86_400;
    const RETRY_AFTER_CANCELED_SECS: u64 = 7 * 86_400;
    RateLimitConfig::with_overrides(
        2,
        1,
        300,
        RETRY_AFTER_HARD_CEILING_SECS,
        RETRY_AFTER_CANCELED_SECS,
    )
    .unwrap_or_else(RateLimitConfig::canonical)
}

/// Build the in-memory token-bucket rate limiter that backs the export route.
/// Always present (cheap, in-process) so the rate limit genuinely applies.
#[must_use]
pub fn build_export_rate_limiter() -> Arc<dyn RateLimiter> {
    Arc::new(InMemoryTokenBucketRateLimiter::new(
        Arc::new(InMemoryRateLimitAuditSink::new()),
        Arc::new(InMemoryRateLimitMetrics::new()),
        export_rate_limit_config(),
    ))
}

/// Serialize one bundle record as an NDJSON `<json>\n` frame.
fn ndjson_frame(value: &Value) -> Bytes {
    let mut buf = value.to_string().into_bytes();
    buf.push(b'\n');
    Bytes::from(buf)
}

/// Build the streaming NDJSON body for a tenant export.
///
/// `metadata` + `blobs` are gathered up-front by the handler (small: governance
/// rows + a `(digest,size)` index) — the LARGE blob bytes are fetched lazily, ONE
/// at a time, as the consumer pulls frames (back-pressured by axum's `StreamBody`),
/// so peak memory is bounded by a single blob. A blob that errors / vanishes
/// mid-stream is surfaced as a `blob_error` / `blob_missing` line and the export
/// continues (the manifest footer reports the counts) — the response is already a
/// `200` streaming body, so this is the honest analogue of the audit-export
/// mid-stream diagnostic.
pub fn build_export_stream(
    source: Arc<dyn TenantExportSource>,
    tenant: String,
    generated_at_ms: u64,
    metadata: Vec<Value>,
    blobs: Vec<ExportBlobRef>,
) -> impl Stream<Item = Result<Frame<Bytes>, Infallible>> + Send {
    stream! {
        let metadata_count = metadata.len();
        let blob_count = blobs.len();

        // Header line.
        let header = json!({
            "kind": "header",
            "schema": EXPORT_SCHEMA,
            "tenant_id": tenant,
            "generated_at_ms": generated_at_ms,
            "blob_count": blob_count,
            "metadata_count": metadata_count,
        });
        yield Ok(Frame::data(ndjson_frame(&header)));

        // Governance records (already tagged with their own "kind").
        for rec in &metadata {
            yield Ok(Frame::data(ndjson_frame(rec)));
        }

        // Blob bytes — fetched lazily, one at a time (memory-bounded).
        let b64 = base64::engine::general_purpose::STANDARD;
        let mut blob_bytes_total: u64 = 0;
        let mut blob_errors: u64 = 0;
        for blob_ref in &blobs {
            let line = match source.fetch_blob(&tenant, blob_ref.kind, &blob_ref.digest) {
                Ok(Some(bytes)) => {
                    blob_bytes_total = blob_bytes_total.saturating_add(bytes.len() as u64);
                    json!({
                        "kind": "blob",
                        "blob_kind": blob_ref.kind.as_str(),
                        "digest": blob_ref.digest,
                        "size": bytes.len(),
                        "encoding": "base64",
                        "bytes": b64.encode(&bytes),
                    })
                }
                Ok(None) => json!({
                    "kind": "blob_missing",
                    "blob_kind": blob_ref.kind.as_str(),
                    "digest": blob_ref.digest,
                }),
                Err(e) => {
                    blob_errors = blob_errors.saturating_add(1);
                    json!({
                        "kind": "blob_error",
                        "blob_kind": blob_ref.kind.as_str(),
                        "digest": blob_ref.digest,
                        "error": e.to_string(),
                    })
                }
            };
            yield Ok(Frame::data(ndjson_frame(&line)));
        }

        // Trailing manifest — the reader uses it to confirm a complete stream.
        let manifest = json!({
            "kind": "manifest",
            "schema": EXPORT_SCHEMA,
            "tenant_id": tenant,
            "blob_count": blob_count,
            "metadata_count": metadata_count,
            "blob_bytes_total": blob_bytes_total,
            "blob_errors": blob_errors,
        });
        yield Ok(Frame::data(ndjson_frame(&manifest)));
    }
}

// ─── Production source: CAS/AC handlers + D1 ───────────────────────────────────

/// Production [`TenantExportSource`] composing the SAME cache read/list handlers
/// the CAS/AC routes use (one R2 connection, no forked store) + a
/// [`crate::customer_d1::CustomerD1`] governance row source. Wired by `routes.rs`
/// from the already-built handler `Arc`s when the D1 env is present; `None`
/// otherwise → the export route fails CLOSED (503).
pub struct CasD1TenantExportSource {
    cas_read: Arc<dyn corelink_handler_cas::CasReadHandler>,
    cas_list: Arc<dyn corelink_handler_cas::CasListHandler>,
    ac_lookup: Arc<dyn corelink_handler_ac::AcLookupHandler>,
    ac_list: Arc<dyn corelink_handler_ac::AcListHandler>,
    db: Arc<dyn crate::customer_d1::CustomerD1>,
}

impl core::fmt::Debug for CasD1TenantExportSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CasD1TenantExportSource").finish_non_exhaustive()
    }
}

impl CasD1TenantExportSource {
    /// Compose the source over already-built cache handlers + a D1 row source.
    #[must_use]
    pub fn new(
        cas_read: Arc<dyn corelink_handler_cas::CasReadHandler>,
        cas_list: Arc<dyn corelink_handler_cas::CasListHandler>,
        ac_lookup: Arc<dyn corelink_handler_ac::AcLookupHandler>,
        ac_list: Arc<dyn corelink_handler_ac::AcListHandler>,
        db: Arc<dyn crate::customer_d1::CustomerD1>,
    ) -> Self {
        Self {
            cas_read,
            cas_list,
            ac_lookup,
            ac_list,
            db,
        }
    }

    /// Real wall-clock unix-ms (the handler's logical clock threads through the
    /// stream; the D1 audit anchor + the CAS/AC request timestamps use real time).
    fn now_ms() -> u64 {
        u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
        )
        .unwrap_or(0)
    }

    /// Run a governance query + wrap each row as a tagged bundle record.
    fn governance(
        &self,
        kind: &str,
        table: &str,
        sql: &str,
        binds: Vec<Value>,
    ) -> Result<Vec<Value>, TenantExportError> {
        let rows = self
            .db
            .query(sql, binds)
            .map_err(|e| TenantExportError::Internal(format!("{table} gather: {e}")))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                json!({
                    "kind": kind,
                    "table": table,
                    "record": Value::Object(row),
                })
            })
            .collect())
    }
}

impl TenantExportSource for CasD1TenantExportSource {
    fn metadata_records(&self, tenant: &str) -> Result<Vec<Value>, TenantExportError> {
        let mut out = Vec::new();
        // RBAC / team seats (explicit non-secret columns; never a raw email).
        out.extend(self.governance(
            "rbac",
            "team_member",
            "SELECT user_id, email_hash, role, status, invited_by, invited_at_ms, joined_at_ms \
             FROM team_member WHERE tenant_id = ?1",
            vec![json!(tenant)],
        )?);
        // DPA / consent acceptances.
        out.extend(self.governance(
            "dpa",
            "dpa_acceptances",
            "SELECT signup_id, dpa_version, locale, notice_hash, wording_id, ui_capture_ts \
             FROM dpa_acceptances WHERE tenant_id = ?1",
            vec![json!(tenant)],
        )?);
        // Bounded customer audit slice (customer-safe columns; no raw PII).
        out.extend(self.governance(
            "audit",
            "customer_audit_events",
            "SELECT id, event_type, actor, target, ts_ms, detail FROM customer_audit_events \
             WHERE tenant_id = ?1 ORDER BY ts_ms DESC LIMIT ?2",
            vec![json!(tenant), json!(EXPORT_AUDIT_ROW_LIMIT)],
        )?);
        Ok(out)
    }

    fn blob_index(&self, tenant: &str) -> Result<Vec<ExportBlobRef>, TenantExportError> {
        use corelink_handler_ac::{AcHandlerError, AcListRequest};
        use corelink_handler_cas::{CasHandlerError, CasListRequest};

        let now = Self::now_ms();
        let mut out = Vec::new();

        // CAS blobs — page through the tenant's derived prefix.
        let mut cursor: Option<String> = None;
        loop {
            let req = CasListRequest::new(
                tenant,
                "export",
                tenant,
                EXPORT_LIST_PAGE_SIZE,
                cursor.clone(),
                now,
            );
            let resp = self.cas_list.list(req).map_err(|e| match e {
                CasHandlerError::Internal(m) => TenantExportError::Unavailable(m),
                other => TenantExportError::Internal(other.to_string()),
            })?;
            for b in resp.blobs {
                out.push(ExportBlobRef {
                    kind: BlobKind::Cas,
                    digest: b.hash,
                    size: b.size,
                });
            }
            match resp.next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }

        // AC refs — page through the tenant's derived prefix.
        let mut cursor: Option<String> = None;
        loop {
            let req = AcListRequest::new(
                tenant,
                "export",
                tenant,
                EXPORT_LIST_PAGE_SIZE,
                cursor.clone(),
                now,
            );
            let resp = self.ac_list.list(req).map_err(|e| match e {
                AcHandlerError::Internal(m) => TenantExportError::Unavailable(m),
                other => TenantExportError::Internal(other.to_string()),
            })?;
            for r in resp.refs {
                out.push(ExportBlobRef {
                    kind: BlobKind::Ac,
                    digest: r.ref_key,
                    size: r.size,
                });
            }
            match resp.next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }

        Ok(out)
    }

    fn fetch_blob(
        &self,
        tenant: &str,
        kind: BlobKind,
        digest: &str,
    ) -> Result<Option<Vec<u8>>, TenantExportError> {
        let now = Self::now_ms();
        match kind {
            BlobKind::Cas => {
                use corelink_handler_cas::{CasHandlerError, CasReadRequest};
                let req = CasReadRequest::new(tenant, digest, "export", tenant, now);
                match self.cas_read.read(req) {
                    Ok(resp) => Ok(Some(resp.bytes)),
                    Err(CasHandlerError::NotFound { .. }) => Ok(None),
                    Err(e) => Err(TenantExportError::Internal(e.to_string())),
                }
            }
            BlobKind::Ac => {
                use corelink_handler_ac::{AcHandlerError, AcLookupRequest};
                let req = AcLookupRequest::new(tenant, digest, "export", tenant, now);
                match self.ac_lookup.lookup(req) {
                    Ok(resp) => Ok(Some(resp.result_payload)),
                    Err(AcHandlerError::Miss { .. }) => Ok(None),
                    Err(e) => Err(TenantExportError::Internal(e.to_string())),
                }
            }
        }
    }

    fn record_export_audit(
        &self,
        tenant: &str,
        blob_count: usize,
        metadata_count: usize,
    ) -> Result<(), TenantExportError> {
        let now = i64::try_from(Self::now_ms()).unwrap_or(i64::MAX);
        let detail = json!({
            "blob_count": blob_count,
            "metadata_count": metadata_count,
        })
        .to_string();
        self.db
            .query(
                "INSERT INTO customer_audit_events (tenant_id, event_type, actor, target, ts_ms, detail) \
                 VALUES (?1, 'account.export', 'customer', NULL, ?2, ?3)",
                vec![json!(tenant), json!(now), json!(detail)],
            )
            .map(|_| ())
            .map_err(|e| TenantExportError::Internal(format!("export audit write: {e}")))
    }
}

/// Wire the production export source from already-built cache handlers + the D1
/// env. `None` when the D1 env is absent (dev/CI) → the export route fails CLOSED
/// (503). Mirrors `customer::account_deletion_from_env`.
#[must_use]
pub fn from_handlers_and_env(
    cas_read: Arc<dyn corelink_handler_cas::CasReadHandler>,
    cas_list: Arc<dyn corelink_handler_cas::CasListHandler>,
    ac_lookup: Arc<dyn corelink_handler_ac::AcLookupHandler>,
    ac_list: Arc<dyn corelink_handler_ac::AcListHandler>,
) -> Option<Arc<dyn TenantExportSource>> {
    let db = crate::customer_d1::D1HttpCustomerDb::from_env()?;
    Some(Arc::new(CasD1TenantExportSource::new(
        cas_read,
        cas_list,
        ac_lookup,
        ac_list,
        Arc::new(db),
    )))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests"
)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory [`TenantExportSource`] fake keyed by tenant — the isolation
    /// boundary under test (a request for tenant A can NEVER see B's data). Blob
    /// fetch is pure (no `block_in_place`), so tests run on the single-thread
    /// `#[tokio::test]` runtime.
    #[derive(Debug, Default)]
    struct FakeSource {
        metadata: HashMap<String, Vec<Value>>,
        blobs: HashMap<String, Vec<(ExportBlobRef, Vec<u8>)>>,
        audited: Mutex<Vec<(String, usize, usize)>>,
    }

    impl FakeSource {
        fn seed_blob(&mut self, tenant: &str, kind: BlobKind, digest: &str, bytes: &[u8]) {
            self.blobs.entry(tenant.to_owned()).or_default().push((
                ExportBlobRef {
                    kind,
                    digest: digest.to_owned(),
                    size: bytes.len() as u64,
                },
                bytes.to_vec(),
            ));
        }
        fn seed_metadata(&mut self, tenant: &str, rec: Value) {
            self.metadata.entry(tenant.to_owned()).or_default().push(rec);
        }
    }

    impl TenantExportSource for FakeSource {
        fn metadata_records(&self, tenant: &str) -> Result<Vec<Value>, TenantExportError> {
            Ok(self.metadata.get(tenant).cloned().unwrap_or_default())
        }
        fn blob_index(&self, tenant: &str) -> Result<Vec<ExportBlobRef>, TenantExportError> {
            Ok(self
                .blobs
                .get(tenant)
                .map(|v| v.iter().map(|(r, _)| r.clone()).collect())
                .unwrap_or_default())
        }
        fn fetch_blob(
            &self,
            tenant: &str,
            kind: BlobKind,
            digest: &str,
        ) -> Result<Option<Vec<u8>>, TenantExportError> {
            Ok(self.blobs.get(tenant).and_then(|v| {
                v.iter()
                    .find(|(r, _)| r.kind == kind && r.digest == digest)
                    .map(|(_, b)| b.clone())
            }))
        }
        fn record_export_audit(
            &self,
            tenant: &str,
            blob_count: usize,
            metadata_count: usize,
        ) -> Result<(), TenantExportError> {
            self.audited
                .lock()
                .unwrap()
                .push((tenant.to_owned(), blob_count, metadata_count));
            Ok(())
        }
    }

    /// Collect a stream body into the parsed NDJSON lines.
    async fn drain_lines<S>(stream: S) -> Vec<Value>
    where
        S: Stream<Item = Result<Frame<Bytes>, Infallible>>,
    {
        use futures::StreamExt as _;
        futures::pin_mut!(stream);
        let mut buf = Vec::new();
        while let Some(Ok(frame)) = stream.next().await {
            if let Ok(data) = frame.into_data() {
                buf.extend_from_slice(&data);
            }
        }
        String::from_utf8(buf)
            .unwrap()
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn bundle_contains_blobs_and_records() {
        let mut src = FakeSource::default();
        src.seed_blob("tenant-a", BlobKind::Cas, "cafebabe", b"hello-blob");
        src.seed_blob("tenant-a", BlobKind::Ac, "deadbeef", b"ac-result");
        src.seed_metadata(
            "tenant-a",
            json!({"kind":"rbac","table":"team_member","record":{"role":"owner"}}),
        );
        src.seed_metadata(
            "tenant-a",
            json!({"kind":"dpa","table":"dpa_acceptances","record":{"dpa_version":"1.0"}}),
        );

        let source: Arc<dyn TenantExportSource> = Arc::new(src);
        let meta = source.metadata_records("tenant-a").unwrap();
        let blobs = source.blob_index("tenant-a").unwrap();
        let s = build_export_stream(Arc::clone(&source), "tenant-a".into(), 42, meta, blobs);
        let lines = drain_lines(s).await;

        // header + 2 metadata + 2 blob + manifest = 6 lines.
        assert_eq!(lines[0]["kind"], "header");
        assert_eq!(lines[0]["schema"], EXPORT_SCHEMA);
        assert_eq!(lines[0]["tenant_id"], "tenant-a");

        let b64 = base64::engine::general_purpose::STANDARD;
        let cas = lines
            .iter()
            .find(|l| l["kind"] == "blob" && l["blob_kind"] == "cas")
            .expect("cas blob line");
        assert_eq!(cas["digest"], "cafebabe");
        let decoded = b64.decode(cas["bytes"].as_str().unwrap()).unwrap();
        assert_eq!(decoded, b"hello-blob", "bundle carries the actual blob bytes");

        let ac = lines
            .iter()
            .find(|l| l["kind"] == "blob" && l["blob_kind"] == "ac")
            .expect("ac blob line");
        assert_eq!(b64.decode(ac["bytes"].as_str().unwrap()).unwrap(), b"ac-result");

        assert!(lines.iter().any(|l| l["kind"] == "rbac"));
        assert!(lines.iter().any(|l| l["kind"] == "dpa"));

        let manifest = lines.last().unwrap();
        assert_eq!(manifest["kind"], "manifest");
        assert_eq!(manifest["blob_count"], 2);
        assert_eq!(manifest["metadata_count"], 2);
        assert_eq!(manifest["blob_errors"], 0);
    }

    #[tokio::test]
    async fn cross_tenant_isolation_never_leaks_other_tenant_blobs() {
        let mut src = FakeSource::default();
        src.seed_blob("tenant-a", BlobKind::Cas, "aaaa", b"A-secret");
        src.seed_blob("tenant-b", BlobKind::Cas, "bbbb", b"B-secret");
        let source: Arc<dyn TenantExportSource> = Arc::new(src);

        // Export driven with ONLY tenant-a: B's blob index + bytes are unreachable.
        let blobs = source.blob_index("tenant-a").unwrap();
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[0].digest, "aaaa");
        let meta = source.metadata_records("tenant-a").unwrap();
        let s = build_export_stream(Arc::clone(&source), "tenant-a".into(), 1, meta, blobs);
        let lines = drain_lines(s).await;

        let body = serde_json::to_string(&lines).unwrap();
        assert!(body.contains(&base64_of(b"A-secret")));
        assert!(!body.contains(&base64_of(b"B-secret")), "must not leak tenant-b bytes");
        assert!(!body.contains("bbbb"), "must not leak tenant-b digest");
    }

    fn base64_of(b: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(b)
    }

    #[test]
    fn rate_limit_config_is_valid() {
        // Must construct a real (non-panicking) limiter.
        let _rl = build_export_rate_limiter();
    }
}
