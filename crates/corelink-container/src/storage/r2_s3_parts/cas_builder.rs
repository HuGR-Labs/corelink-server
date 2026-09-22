/// Build an `R2CasHandler` from environment variables.
///
/// Returns `None` when storage credentials are not configured (dev /
/// unit-test mode). Callers fall back to `InMemoryCasHandler`.
///
/// # Errors
///
/// Returns `Err(String)` if credentials are present but the S3 client
/// cannot be constructed (e.g. endpoint URL is malformed).
pub async fn build_r2_cas_handler_from_env(
    bucket: &str,
    cas_region: &str,
) -> Option<Result<R2CasHandler, String>> {
    if let Err(e) = validate_cas_bucket_for_region(bucket, cas_region) {
        tracing::error!(bucket = %bucket, region = %cas_region, error = %e,
            "refusing CAS handler with non-residency bucket (fail-closed)");
        return Some(Err(e));
    }
    let env = super::StorageEnv::from_env()?;
    // FAIL CLOSED: storage creds are present, so this is the production
    // data plane — the secret TDK is MANDATORY (F1/F2). Without it the
    // tenant prefix would degrade to a public, predictable scheme and
    // enable same-millisecond cross-tenant blob co-residence. Refuse to
    // construct the handler (the route will not mount) and emit a loud,
    // structured error rather than serving in the silently-degraded
    // public-prefix mode.
    let Some(tdk_bytes) = load_tdk_from_env() else {
        tracing::error!(
            "R2_TDK_HEX required for production tenant prefixing but is unset/invalid; \
             refusing to mount the R2 CAS handler (fail-closed, INV-TENANT-ISOLATION)"
        );
        return Some(Err(
            "R2_TDK_HEX required for production tenant prefixing".to_owned()
        ));
    };
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    // F1 (CAA-360) fail-CLOSED: storage creds ARE present, so this is the
    // production data plane. The audit trail MUST be DURABLE — a volatile
    // `InMemoryAuditSink` here loses every CAS audit event on restart AND makes
    // the route's `AuditFailed → 503` guard dead code (in-memory emit only
    // errors under a test-injected failure). Wire the D1 `audit_outbox` sink
    // (the same trail the S-09 drain seals); if it cannot be constructed,
    // REFUSE to build the handler — the route mounts the fail-CLOSED 503
    // handler, never a silent in-memory fallback (mirrors the `R2_TDK_HEX`
    // refusal above).
    //
    // The CONCRETE `Arc<D1AuditOutboxSink>` is kept (not just the
    // type-erased `Arc<dyn AuditSink>`) for the explicit batch-exists audit
    // seam below. Single-object reads and lists use the sync trait object so
    // durable audit success is serialized before storage dispatch.
    // B071: the live CAS writer has an explicit D1 fence dependency. Keep a
    // dedicated client for it so a future audit-sink refactor cannot silently
    // drop the writer-side lease wiring.
    let cas_write_fence = match D1HttpClient::new(&env) {
        Ok(d1) => Arc::new(crate::storage::cas_write_fence::D1CasWriteFence::new(
            Arc::new(d1),
        )),
        Err(e) => {
            tracing::error!(
                error = %e,
                "CAS write D1 fence unavailable; refusing to mount the R2 CAS handler"
            );
            return Some(Err(e));
        }
    };
    let audit_concrete = match cas_audit_sink_from_d1_concrete(D1HttpClient::new(&env)) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(
                error = %e,
                "durable CAS audit sink unavailable with storage creds present; \
                 refusing to mount the R2 CAS handler (fail-closed, F1)"
            );
            return Some(Err(e));
        }
    };
    let audit: Arc<dyn AuditSink> = audit_concrete.clone();
    // B-057: constant-memory aggregate, NOT the capture-everything test
    // observer. The previous `InMemorySliObserver` here retained every
    // observation in a `Vec` for the life of the container process, with
    // no production reader of `snapshot()`/`count()` anywhere.
    let sli = crate::sli_aggregate::shared();
    let handler = R2CasHandler::new(client, cas_region, Some(tdk_bytes), audit, sli)
        .with_async_audit(audit_concrete)
        .with_cas_write_fence(cas_write_fence);

    // Router assembly attaches the one DataPlaneByok instance, including its
    // operation gate. Do not create a handler-local gate here: reservation and
    // storage must consume the same request-scoped authority.
    Some(Ok(handler))
}

/// Validate the physical CAS bucket/region pairing before any handler is
/// constructed. A key prefix is not residency: the R2 bucket is the physical
/// location boundary. Unknown or legacy regional pairings fail closed instead
/// of silently writing non-IAD data into `corelink-cas-prod`.
pub(crate) fn validate_cas_bucket_for_region(bucket: &str, region: &str) -> Result<(), String> {
    let expected = match region {
        "iad" => "corelink-cas-prod",
        "lhr" => "corelink-cas-eu",
        "nrt" => "corelink-cas-apac",
        other => {
            return Err(format!(
                "no physically residency-bound CAS bucket is provisioned for region '{other}'"
            ));
        }
    };
    if bucket != expected {
        return Err(format!(
            "CAS bucket '{bucket}' is not the physical residency bucket for region '{region}' (expected '{expected}')"
        ));
    }
    Ok(())
}

/// A sync `AcLookupHandler` + `AcUpdateHandler` backed by [`R2S3Client`].
///
/// Mirrors `R2CasHandler` exactly — bridges async S3 I/O to the sync
/// AC handler trait surface via `tokio::runtime::Handle::current().block_on(...)`.
/// Wired through `routes::ac::build_handlers` when storage credentials
/// are configured; otherwise the route falls back to `InMemoryAcHandler`.
///
/// # Key scheme
///
/// AC entries reuse the canonical
/// `<region>/<tenant_prefix_16>/<action_digest>` key layout from CAS,
/// only the bucket differs (`R2_AC_BUCKET` / default `corelink-ac-iad`).
/// Per-tenant prefix isolation (layer 5 of `INV-TENANT-ISOLATION`)
/// applies identically.
#[non_exhaustive]
pub struct R2AcHandler {
    client: R2S3Client,
    /// The R2 region string (e.g. `"iad"`) used as key prefix.
    ac_region: String,
    /// Tenant derivation key — see [`R2CasHandler`].
    tdk: Option<TenantDerivationKey>,
    audit: Arc<dyn corelink_handler_ac::AuditSink>,
    /// Concrete durable sink retained for builder compatibility; AC reads and
    /// lists are always serial and do not use this seam.
    audit_async: Option<Arc<crate::storage::d1_audit_sink::D1AuditOutboxSink>>,
    sli: Arc<dyn corelink_handler_ac::SliObserver>,
    /// BYOK Wave 3b (GATED-INERT): per-tenant BYOK config cache. `None` on the
    /// non-BYOK build / tests → the plaintext path runs unchanged. When `Some`
    /// AND a tenant is `active`, the AC update/lookup path encrypts the
    /// `result_payload` at rest under the `"ac"` surface (closes audit H1).
    /// Mirrors [`R2CasHandler::byok_config_cache`].
    byok_config_cache: Option<Arc<ByokConfigCache>>,
    /// BYOK Wave 3b: the Tcs resolver (CMK-unwrap → convergence secret). `None`
    /// → plaintext path. Both this and `byok_config_cache` must be `Some` for
    /// AC encryption to engage. Mirrors [`R2CasHandler::tcs_resolver`].
    tcs_resolver: Option<Arc<TcsResolver>>,
    /// BYOK Wave 3c: the Mode-B (random-DEK) encryptor + `byok_envelope` store
    /// for the AC surface. Mirrors [`R2CasHandler::byok_mode_b`].
    byok_mode_b: Option<Arc<ModeBEncryptor>>,
    /// Tenant-wide transition exclusion plus authoritative generation catalog.
    byok_runtime_gate: Option<Arc<dyn ByokRuntimeGate>>,
}
