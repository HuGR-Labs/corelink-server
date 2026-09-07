    }
}

// ─── Live pipeline seam ──────────────────────────────────────────────────────────

/// The live data-operation driver. Production wires [`LivePipeline`] over the
/// EXISTING Wave-1 engine (`super::super::access::*` + the erasure worker); tests
/// supply a fake.
pub trait DsrPipeline: Send + Sync + core::fmt::Debug {
    /// Art.15 — gather the subject's data (machine-readable JSON).
    ///
    /// # Errors
    /// Any gather/D1 fault (fail-CLOSED: never a partial export).
    fn access(&self, tenant_id: &str, dsr_id: &str, now_ms: u64) -> Result<Value, String>;

    /// Art.20 — gather + best-effort durable export; returns the bundle and the
    /// persisted export handle (`None` when not persisted).
    ///
    /// # Errors
    /// Any gather/D1 fault (fail-CLOSED).
    fn portability(
        &self,
        tenant_id: &str,
        dsr_id: &str,
        now_ms: u64,
    ) -> Result<(Value, Option<String>), String>;

    /// Art.16 — correct the allowlisted contact email (hashed by the live
    /// pipeline). `Ok(Ok(()))` applied; `Ok(Err(msg))` is a fail-CLOSED 4xx
    /// refusal (immutable/non-editable/invalid); `Err` is an engine fault.
    ///
    /// # Errors
    /// Any engine/D1 fault.
    fn rectification(
        &self,
        tenant_id: &str,
        dsr_id: &str,
        email: &str,
        now_ms: u64,
    ) -> Result<Result<(), String>, String>;

    /// Art.17 — drive the live erasure worker for the whole subject/tenant.
    /// `Ok(())` on accept OR no-account (idempotent).
    ///
    /// # Errors
    /// Any engine/D1 fault (or the erasure path being unconfigured).
    fn erasure(&self, tenant_id: &str) -> Result<(), String>;
}

/// Production pipeline over the live Wave-1 engine.
#[non_exhaustive]
pub struct LivePipeline {
    d1: Arc<D1HttpClient>,
    r2_audit: Option<Arc<R2S3Client>>,
    /// Reuses the account-deletion requester (writes the `dsr_requested` anchor
    /// then drives the in-process erasure worker). `None` ⇒ erasure returns a
    /// fail-CLOSED error (never a silent ack).
    erasure: Option<Arc<dyn AccountDeletionRequester>>,
}

impl core::fmt::Debug for LivePipeline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LivePipeline")
            .field("d1", &"[D1HttpClient]")
            .field("r2_audit", &self.r2_audit.as_ref().map(|_| "[R2S3Client]"))
            .field("erasure", &self.erasure.is_some())
            .finish()
    }
}

impl DsrPipeline for LivePipeline {
    fn access(&self, tenant_id: &str, dsr_id: &str, now_ms: u64) -> Result<Value, String> {
        let export = super::super::access::run_access(&self.d1, dsr_id, tenant_id, now_ms)?;
        serde_json::to_value(export).map_err(|e| format!("export serialize: {e}"))
    }

    fn portability(
        &self,
        tenant_id: &str,
        dsr_id: &str,
        now_ms: u64,
    ) -> Result<(Value, Option<String>), String> {
        let (export, receipt) = super::super::access::run_portability(
            &self.d1,
            self.r2_audit.as_ref(),
            dsr_id,
            tenant_id,
            now_ms,
        )?;
        let handle = if receipt.persisted {
            receipt.r2_key
        } else {
            None
        };
        let value = serde_json::to_value(export).map_err(|e| format!("export serialize: {e}"))?;
        Ok((value, handle))
    }

    fn rectification(
        &self,
        tenant_id: &str,
        dsr_id: &str,
        email: &str,
        now_ms: u64,
    ) -> Result<Result<(), String>, String> {
        // The only live-rectifiable subject field is the account contact email
        // (`tenant.email_hash`), corrected + hashed by the live pipeline.
        match super::super::access::run_rectification(
            &self.d1,
            dsr_id,
            tenant_id,
            "tenant",
            "email_hash",
            email,
            now_ms,
        )? {
            Ok(_result) => Ok(Ok(())),
            Err(reject) => Ok(Err(reject.message().to_owned())),
        }
    }

    fn erasure(&self, tenant_id: &str) -> Result<(), String> {
        let requester = self
            .erasure
            .as_ref()
            .ok_or_else(|| "erasure pipeline not configured".to_owned())?;
        match requester.request_erasure(tenant_id) {
            // No provisioned account (already erased / never provisioned) — the
            // Art.17 obligation is satisfied by construction (idempotent).
            Ok(()) | Err(AccountDeletionError::NotFound) => Ok(()),
            Err(AccountDeletionError::Internal(e)) => Err(e),
        }
    }
}

// ─── Route state ─────────────────────────────────────────────────────────────────

/// Shared state for the `/v1/privacy/dsr/*` router.
#[non_exhaustive]
#[derive(Clone)]
pub struct PrivacyDsrRouteState {
    /// Live data-op driver. `None` (dev/CI, storage unconfigured) ⇒ the data
    /// rights fail CLOSED (503) — never a silent ack of an unhonored right.
    pub pipeline: Option<Arc<dyn DsrPipeline>>,
    /// Durable, tenant-scoped ticket store (D1 in prod, in-memory in dev/CI).
    pub tickets: Arc<dyn DsrTicketStore>,
    /// Native PAT possession backstop (mirrors `customer::pat_gate`). Skipped for
    /// Clerk callers; `None` in dev/CI.
    pub pat_gate: Option<Arc<crate::native_pat_gate::NativePatGate>>,
    /// HS256 receipt-signing key (proof-of-submission).
    pub receipt_key: Arc<Vec<u8>>,
}

impl core::fmt::Debug for PrivacyDsrRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PrivacyDsrRouteState")
            .field("pipeline", &self.pipeline.is_some())
            .field("tickets", &self.tickets)
            .field("pat_gate", &self.pat_gate.is_some())
            .field("receipt_key", &"<redacted>")
            .finish()
    }
}

/// Build production state from env. The router is ALWAYS mountable (so the
/// dashboard surface exists), but the data rights fail CLOSED (503) until the D1
/// storage env is configured. Mirrors `customer::build_handlers_from_env`.
#[must_use]
pub fn build_state_from_env() -> PrivacyDsrRouteState {
    let (pipeline, tickets): (Option<Arc<dyn DsrPipeline>>, Arc<dyn DsrTicketStore>) =
        match build_live() {
            Some((pipe, store)) => (Some(pipe), store),
            None => {
                tracing::warn!(
                    "StorageEnv unset/invalid; /v1/privacy/dsr/* data rights fail CLOSED \
                     (503) — ticket store is in-memory (dev/CI)"
                );
                (None, Arc::new(InMemoryDsrTicketStore::new()))
            }
        };
    PrivacyDsrRouteState {
        pipeline,
        tickets,
        // Wired by `routes.rs` from `native_pat_gate_from_env()` (mirrors the
        // customer plane); `None` here keeps the factory env-pure.
        pat_gate: None,
        receipt_key: Arc::new(receipt_key_from_env()),
    }
}

/// Build the live pipeline + D1 ticket store when storage is configured.
fn build_live() -> Option<(Arc<dyn DsrPipeline>, Arc<dyn DsrTicketStore>)> {
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = Arc::new(D1HttpClient::new(&storage_env).ok()?);
    // R2 audit bucket for the portability signed export (best-effort; `None`
    // unless the operator asserted a single attestation region).
    let r2_audit = super::super::build_audit_r2_client();
    // Reuse the account-deletion requester (anchor + in-process erasure worker).
    let erasure = crate::routes::customer::account_deletion_from_env();
    let pipeline: Arc<dyn DsrPipeline> = Arc::new(LivePipeline {
        d1: Arc::clone(&d1),
        r2_audit,
        erasure,
    });
    let tickets: Arc<dyn DsrTicketStore> = Arc::new(D1DsrTicketStore::new(d1));
    Some((pipeline, tickets))
}

/// Resolve the HS256 receipt-signing key: the dedicated
/// `DSR_RECEIPT_SIGNING_KEY`, else the shared erase/internal-auth key (so prod
/// receipts are genuine without a new secret), else a dev placeholder.
fn receipt_key_from_env() -> Vec<u8> {
    if let Ok(k) = std::env::var("DSR_RECEIPT_SIGNING_KEY") {
        if k.len() >= 32 {
            return k.into_bytes();
        }
    }
    if let Some(k) = crate::routes::admin::erase_auth_key_from_env() {
        if k.len() >= 32 {
            return k.as_bytes().to_vec();
        }
    }
    tracing::warn!(
        "no DSR_RECEIPT_SIGNING_KEY / erase-auth key (>=32 chars); DSR receipts \
         signed with a dev placeholder (non-prod)"
    );
    b"corelink-dev-dsr-receipt-placeholder-key".to_vec()
}

// ─── Router ──────────────────────────────────────────────────────────────────────

/// Mount `/v1/privacy/dsr/*` (the six rights + status + list + verify-mfa).
pub fn router(state: PrivacyDsrRouteState) -> Router {
    Router::new()
        .route("/v1/privacy/dsr/access", post(handle_access))
        .route("/v1/privacy/dsr/portability", post(handle_portability))
        .route("/v1/privacy/dsr/rectification", post(handle_rectification))
        .route("/v1/privacy/dsr/erasure", post(handle_erasure))
        .route("/v1/privacy/dsr/restriction", post(handle_restriction))
        .route("/v1/privacy/dsr/objection", post(handle_objection))
        .route("/v1/privacy/dsr", get(handle_list))
        .route("/v1/privacy/dsr/{request_id}/status", get(handle_status))
        .route(
            "/v1/privacy/dsr/{request_id}/verify-mfa",
            post(handle_verify_mfa),
        )
        .with_state(state)
}

// ─── Request body shapes (dsr-client.ts contract) ──────────────────────────────

/// `POST /v1/privacy/dsr/{action}` body (the `action` field is in the URL and
/// stripped by the client, so it is NOT in the body).
#[derive(Debug, Default, Deserialize)]
struct DsrSubmitBody {
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    rectification: Option<RectificationBody>,
}

/// Rectification target fields (only the contact email is live-rectifiable).
#[derive(Debug, Default, Deserialize)]
struct RectificationBody {
    #[serde(default)]
    email: Option<String>,
}

/// `POST /v1/privacy/dsr/{request_id}/verify-mfa` body (optional step-up token;
/// the load-bearing gate is the Worker-trusted freshness header).
#[derive(Debug, Default, Deserialize)]
struct VerifyMfaBody {
    #[serde(default)]
    #[allow(dead_code)]
    token: Option<String>,
}

/// `GET /v1/privacy/dsr` query params.
#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    #[serde(default)]
    limit: Option<usize>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────────
