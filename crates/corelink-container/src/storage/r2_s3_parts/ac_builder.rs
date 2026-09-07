/// Build an `R2AcHandler` from environment variables.
///
/// Returns `None` when storage credentials are not configured (dev /
/// unit-test mode). Callers fall back to `InMemoryAcHandler`.
///
/// Mirrors [`build_r2_cas_handler_from_env`] exactly; only the bucket
/// + region defaults and the handler type differ.
///
/// # Errors
///
/// Returns `Err(String)` if credentials are present but the S3 client
/// cannot be constructed (e.g. endpoint URL is malformed).
pub async fn build_r2_ac_handler_from_env(
    bucket: &str,
    ac_region: &str,
) -> Option<Result<R2AcHandler, String>> {
    let env = super::StorageEnv::from_env()?;
    // FAIL CLOSED: see `build_r2_cas_handler_from_env`. The AC key space
    // is NOT content-addressed (AC bytes are not self-verifying), so a
    // predictable-prefix collision is even more dangerous here (F1/F5):
    // a same-ms prefix collision lets one tenant poison another's
    // ActionResult. The secret TDK is mandatory on the production path.
    let Some(tdk_bytes) = load_tdk_from_env() else {
        tracing::error!(
            "R2_TDK_HEX required for production tenant prefixing but is unset/invalid; \
             refusing to mount the R2 AC handler (fail-closed, INV-TENANT-ISOLATION)"
        );
        return Some(Err(
            "R2_TDK_HEX required for production tenant prefixing".to_owned()
        ));
    };
    let client = match R2S3Client::new(&env, bucket).await {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };
    // F1 (CAA-360) fail-CLOSED: DURABLE audit trail is mandatory on the
    // production data plane — see `build_r2_cas_handler_from_env`. The AC key
    // space is not content-addressed, so a lost/forged audit row is even more
    // dangerous. Wire the D1 `audit_outbox` sink or REFUSE (route mounts the
    // fail-CLOSED handler, never a volatile in-memory fallback).
    // Kept concrete for `with_async_audit` too — see the matching comment in
    // `build_r2_cas_handler_from_env`.
    let audit_concrete = match ac_audit_sink_from_d1_concrete(D1HttpClient::new(&env)) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(
                error = %e,
                "durable AC audit sink unavailable with storage creds present; \
                 refusing to mount the R2 AC handler (fail-closed, F1)"
            );
            return Some(Err(e));
        }
    };
    let audit: Arc<dyn corelink_handler_ac::AuditSink> = audit_concrete.clone();
    // B-057, AC twin of the CAS builder above: constant-memory aggregate
    // instead of a Vec that grew for the life of the container.
    let sli = crate::sli_aggregate::shared();
    Some(Ok(R2AcHandler::new(
        client,
        ac_region,
        Some(tdk_bytes),
        audit,
        sli,
    )
    .with_async_audit(audit_concrete)))
}

/// Load the tenant derivation key from `R2_TDK_HEX` env var (64 hex chars =
/// 32 bytes). Returns `None` when unset/invalid; on the production storage
/// path a `None` here makes `build_r2_*_handler_from_env` FAIL CLOSED (the
/// handler is not constructed and the route does not mount) rather than
/// degrade to a public-prefix scheme (F1/F2).
fn load_tdk_from_env() -> Option<Zeroizing<[u8; 32]>> {
    let hex_str = std::env::var("R2_TDK_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        warn!(
            len = hex_str.len(),
            "R2_TDK_HEX has wrong length; ignoring TDK"
        );
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    if hex::decode_to_slice(hex_str, bytes.as_mut()).is_err() {
        warn!("R2_TDK_HEX is not valid hex; ignoring TDK");
        return None;
    }
    Some(bytes)
}
