        }
    }
}

/// `POST /v1/customer/account/export` — self-serve tenant bulk export (SEAM).
///
/// Streams the full portability bundle (content-addressed NDJSON): the tenant's
/// CAS + AC blob BYTES + the D1 governance records (RBAC/team, DPA/consent, audit
/// slice). Unblocks the CLI `corelink tenant export` (PR #708) and serves GDPR
/// Art.20 data portability for the whole tenant.
///
/// Auth: owner/admin only. Accepts EITHER a dashboard Clerk session OR a
/// write-capable (`cas:rw`) PAT — the bundle exposes the tenant's team PII, DPA
/// records, audit log AND every blob, so a read-only (`cas:r`) cache token is NOT
/// sufficient (gated on `requires_cache_write`, exactly like the billing/keys/team
/// surfaces). The native PAT-possession backstop runs first (a leaked
/// `PAT_SIGNING_KEY` cannot forge a bearer for a victim tenant). Rate-limited
/// per-tenant. Audited: a durable `account.export` row is written BEFORE any bytes
/// are disclosed. Fail-CLOSED: unwired source → 503; a gather/audit fault → 5xx —
/// never a partial 200.
async fn handle_account_export(
    State(state): State<CustomerRouteState>,
    headers: HeaderMap,
) -> axum::response::Response {
    use crate::routes::customer_export::TenantExportError;

    // 1. Fail-CLOSED tenant resolution (401 on missing/sentinel).
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    // 2. Native PAT possession backstop (forged / wrong-tenant PAT → 401/503).
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    // 3. Owner/admin + PII gate: the bundle carries team PII / DPA / audit / all
    //    blobs — a read-only cache token must not export it (F-018 sibling).
    if let Some(resp) = billing_pii_gate_reject(&headers) {
        return resp;
    }
    // 4. Require the export source (fail-CLOSED 503 when unwired — dev/CI).
    let Some(source) = state.export.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "tenant export not configured",
        )
            .into_response();
    };
    // 5. Per-tenant rate limit (heavy full-tenant read). The tenant may be a
    //    non-UUID fixture id, so bucket on a deterministic v5 UUID derived from it.
    let export_now_ms = crate::wall_clock::default_wall_clock().now_ms();
    let bucket_tenant = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, t.as_bytes());
    let bucket_key = corelink_ratelimit::BucketKey::per_tenant_per_endpoint(
        bucket_tenant,
        crate::routes::customer_export::EXPORT_ENDPOINT_ID,
    );
    match state
        .export_rate_limiter
        .try_acquire(bucket_tenant, bucket_key, 1, export_now_ms)
    {
        Ok(outcome) => match outcome.decision {
            corelink_ratelimit::RateLimitDecision::Allow { .. } => {}
            corelink_ratelimit::RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                let body = format!("rate-limited; retry after {retry_after_secs}s");
                let mut resp = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                if let Ok(val) = format!("{retry_after_secs}").parse() {
                    resp.headers_mut()
                        .insert(axum::http::header::RETRY_AFTER, val);
                }
                return resp;
            }
            // The decision enum is `#[non_exhaustive]`; any future non-Allow arm
            // is treated as a deny so the route never streams under an unknown
            // decision shape.
            _ => return (StatusCode::TOO_MANY_REQUESTS, "rate-limited").into_response(),
        },
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "rate-limit pipeline failed",
            )
                .into_response();
        }
    }
    // 6. Gather governance + the blob index UP FRONT (fail-CLOSED — never a
    //    partial-looking 200). Small: D1 rows + a (digest,size) index.
    let metadata = match source.metadata_records(&t) {
        Ok(m) => m,
        Err(TenantExportError::Unavailable(e)) => {
            tracing::warn!(error = %e, tenant = %t, "tenant export: source unavailable");
            return (StatusCode::SERVICE_UNAVAILABLE, "export source unavailable").into_response();
        }
        Err(TenantExportError::Internal(e)) => {
            tracing::error!(error = %e, tenant = %t, "tenant export: metadata gather failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "export gather failed").into_response();
        }
    };
    let blobs = match source.blob_index(&t) {
        Ok(b) => b,
        Err(TenantExportError::Unavailable(e)) => {
            tracing::warn!(error = %e, tenant = %t, "tenant export: blob index unavailable");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "export storage unavailable",
            )
                .into_response();
        }
        Err(TenantExportError::Internal(e)) => {
            tracing::error!(error = %e, tenant = %t, "tenant export: blob index failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "export enumerate failed").into_response();
        }
    };
    // 7. Audit BEFORE disclosure (durable `account.export` row). A fault here
    //    aborts fail-CLOSED so a disclosure is never unlogged.
    if let Err(e) = source.record_export_audit(&t, blobs.len(), metadata.len()) {
        tracing::error!(error = %e, tenant = %t, "tenant export: audit write failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, "export audit failed").into_response();
    }
    tracing::info!(
        event = "customer.account.export",
        tenant = %t,
        blob_count = blobs.len(),
        metadata_count = metadata.len(),
        "tenant export streaming"
    );
    // 8. Stream the content-addressed NDJSON bundle. Blob bytes are fetched
    //    lazily one at a time (memory-bounded).
    let body_stream = crate::routes::customer_export::build_export_stream(
        source,
        t,
        export_now_ms,
        metadata,
        blobs,
    );
    let body = axum::body::Body::new(http_body_util::StreamBody::new(body_stream));
    let mut resp = (StatusCode::OK, body).into_response();
    if let Ok(val) = axum::http::HeaderValue::from_str("application/x-ndjson") {
        resp.headers_mut()
            .insert(axum::http::header::CONTENT_TYPE, val);
    }
    if let (Ok(name), Ok(val)) = (
        axum::http::HeaderName::from_bytes(b"x-corelink-export-schema"),
        axum::http::HeaderValue::from_str(crate::routes::customer_export::EXPORT_SCHEMA),
    ) {
        resp.headers_mut().insert(name, val);
    }
    resp
}

// ─── Account-deletion (DSR erasure) collaborators (C-ACCTDEL) ──────────────────

/// Failure modes of a self-serve account-deletion request.
#[non_exhaustive]
#[derive(Debug)]
pub enum AccountDeletionError {
    /// No account/tenant row exists to erase (already deleted / never
    /// provisioned) — the route maps this to an idempotent 202 no-op.
    NotFound,
    /// A D1 / transport / configuration fault — the route fails CLOSED (500) so
    /// the obligation is retried, never silently dropped.
    Internal(String),
}

/// Self-serve account-erasure requester (route collaborator). The production
/// impl is [`D1AccountDeletionRequester`]; tests supply a mock. Wired into
/// [`CustomerRouteState::account_deletion`] by `routes.rs` (the erasure sink is a
/// cross-module collaborator built alongside the DSR worker), exactly as
/// `pat_gate` is wired there.
pub trait AccountDeletionRequester: Send + Sync + core::fmt::Debug {
    /// Durably anchor + enqueue a GDPR erasure for `tenant_id`. Idempotent
    /// (deterministic `dsr_id` + `INSERT OR IGNORE`). Uses a real wall clock for
    /// the SLA-anchor `queued_at_ms` (the route's logical clock is 0).
    ///
    /// # Errors
    ///
    /// [`AccountDeletionError::NotFound`] when no tenant row exists;
    /// [`AccountDeletionError::Internal`] on any D1 / transport / config fault.
    fn request_erasure(&self, tenant_id: &str) -> Result<(), AccountDeletionError>;
}

/// Transport seam for the built `dsr.queued.v1` message. The genuinely
/// cross-service piece (the container has no CF Queue producer binding): the
/// production sink is wired in `routes.rs` over the in-process DSR erasure worker
/// (or a queue producer). Kept behind a trait so [`D1AccountDeletionRequester`]
/// owns the message construction + the `dsr_requested` anchor (the C-ACCTDEL
/// D1-observable effects) hermetically, with the transport injected.
pub trait DsrErasureSink: Send + Sync + core::fmt::Debug {
    /// Enqueue an already-anchored `dsr.queued.v1` erasure message.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any transport failure (the requester maps it to
    /// [`AccountDeletionError::Internal`] → 500 fail-CLOSED).
    fn enqueue(&self, message: &Value) -> Result<(), String>;
}

/// `dev.hugr.corelink.dsr.queued.v1` schema id (FROZEN — mirrors
/// `apps/signup-worker/src/webhooks/clerk.ts buildErasureQueueMessage`).
const DSR_QUEUED_SCHEMA: &str = "dev.hugr.corelink.dsr.queued.v1";
