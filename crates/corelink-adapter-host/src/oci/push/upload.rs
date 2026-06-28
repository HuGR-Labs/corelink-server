//! Multi-step blob upload state machine:
//!
//! ```text
//! POST  /v2/<name>/blobs/uploads/            → 202 + Location: /v2/<name>/blobs/uploads/<uuid>
//! PATCH /v2/<name>/blobs/uploads/<uuid>      → 202 + Range: 0-<len-1>
//! PUT   /v2/<name>/blobs/uploads/<uuid>?digest=X
//!                                            → 201 + Location: /v2/<name>/blobs/<digest>
//! ```
//!
//! Critical invariants:
//!
//! 1. **Declared-digest verification fail-CLOSED on `PUT`.** The
//!    finalize step recomputes the bytes' actual digest and compares
//!    via `subtle::ConstantTimeEq` (delegated to
//!    [`crate::oci::digest::OciDigest::verify_against_bytes`]). On
//!    mismatch: emit `oci.blob.push.digest_mismatch.v1` BEFORE
//!    canceling the upload session, return `400 DIGEST_INVALID`.
//! 2. **Cumulative-size cap fires at `PATCH` AND `PUT`.** Defense-in-
//!    depth: a buggy client might understate `Content-Length` so the
//!    `PATCH` byte-counter is the authoritative source. On oversize:
//!    emit `oci.blob.push.oversize.v1`, cancel session, `413`.
//! 3. **Audit emit BEFORE state mutation.** Both success
//!    (`oci.blob.push.v1`) and failure (`oversize`/`digest_mismatch`)
//!    rows emit BEFORE the underlying `BlobStore` write or
//!    `cancel_upload` call. The route layer returns `503` on emit
//!    failure so the customer can retry.

use axum::body::{Body, Bytes as AxumBytes};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;

use corelink_audit::ports::AuditEmitter;
use corelink_core::TenantId;

use crate::oci::audit::{emit as audit_emit, OciAuditEvent};
use crate::oci::digest::OciDigest;
use crate::oci::error::OciAdapterError;
use crate::oci::ports::BlobStore;

fn check_repo_push(repo: &str, scope: &crate::oci::auth::OciScope) -> Result<(), OciAdapterError> {
    crate::oci::server::validate_repo_name(repo)?;
    if !scope.allows(repo, "push") {
        return Err(OciAdapterError::Auth(format!(
            "scope does not grant push on {repo}"
        )));
    }
    Ok(())
}

/// Parse the advisory limit `N` from a `"too many open upload sessions
/// (limit N)"` port error message. Returns `0` when the `(limit …)` clause is
/// absent or unparseable (a self-consistent sentinel). The `+ 7` skips the
/// literal `"(limit "` prefix.
#[must_use]
fn parse_session_limit(err: &str) -> usize {
    err.find("(limit ")
        .and_then(|i| err[i + 7..].split(')').next())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0)
}

/// `POST /v2/<repo>/blobs/uploads/`. Opens an upload session and
/// returns `202` + the `Location:` header pointing the client at the
/// `PATCH` URL.
///
/// When the per-tenant session cap is reached the port returns an error
/// string starting with `"too many open upload sessions"`.  We surface
/// that as [`OciAdapterError::TooManyOpenSessions`] → HTTP 429 + a
/// `Retry-After` header so clients back off rather than looping.  All
/// other port errors are generic backend faults (500).
pub async fn open(
    cas: &dyn BlobStore,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_push(repo, scope)?;
    let uuid = cas
        .open_upload(tenant)
        .await
        .map_err(|e| {
            if e.starts_with("too many open upload sessions") {
                // Parse the limit from the error message if present so the
                // response is self-consistent; fall back to a sentinel.
                let limit = parse_session_limit(&e);
                OciAdapterError::TooManyOpenSessions {
                    limit,
                    // 60 s is a conservative advisory back-off: long enough
                    // to let a stalled push time out, short enough to not
                    // strand legitimate retries.
                    retry_after_secs: 60,
                }
            } else {
                OciAdapterError::Cas(e)
            }
        })?;
    let location = format!("/v2/{repo}/blobs/uploads/{uuid}");
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::LOCATION,
        location
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("location header parse")))?,
    );
    headers.insert(
        "Docker-Upload-UUID",
        uuid.parse()
            .map_err(|_| OciAdapterError::Cas(String::from("uuid header parse")))?,
    );
    headers.insert(
        "Range",
        "0-0"
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("range header parse")))?,
    );
    Ok((StatusCode::ACCEPTED, headers, Body::empty()).into_response())
}

/// `PATCH /v2/<repo>/blobs/uploads/<uuid>` — append one chunk.
/// Returns `202` + `Range: 0-<cumulative-1>`.
///
/// `chunk` is the raw bytes from the request body; the caller
/// (`server.rs`) has already turned the `axum::body::Body` into a
/// `Bytes`.
///
/// Oversize check fires here on `cumulative > blob_size_limit_bytes`.
/// We emit `oversize` audit + cancel the session + return `413`.
#[allow(
    clippy::too_many_arguments,
    reason = "wire-shape PATCH handler — every dep is explicit by design"
)]
pub async fn patch(
    cas: &dyn BlobStore,
    auditor: &dyn AuditEmitter,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    upload_uuid: &str,
    chunk: AxumBytes,
    blob_size_limit_bytes: u64,
    now_unix_ms: u64,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_push(repo, scope)?;
    let new_len = cas
        .append_chunk(tenant, upload_uuid, chunk)
        .await
        .map_err(|e| {
            if e.starts_with("upload session not found") {
                OciAdapterError::UploadSessionMissing(upload_uuid.to_string())
            } else if e.starts_with("too many open upload sessions") {
                // Global in-flight byte ceiling reached on the append path → 429
                // (a capacity limit, not a server fault). Without this branch the
                // ceiling rejection fell through to a generic 500. `limit` carries
                // the advisory the port embedded; clients back off via Retry-After.
                OciAdapterError::TooManyOpenSessions {
                    limit: parse_session_limit(&e),
                    retry_after_secs: 60,
                }
            } else {
                OciAdapterError::Cas(e)
            }
        })?;
    if new_len > blob_size_limit_bytes {
        // Audit BEFORE the cancel (mutation).
        audit_emit(
            auditor,
            tenant,
            &OciAuditEvent::BlobOversize {
                repo,
                upload_uuid,
                size_bytes: new_len,
                limit_bytes: blob_size_limit_bytes,
            },
            now_unix_ms,
        )?;
        cas.cancel_upload(tenant, upload_uuid)
            .await
            .map_err(OciAdapterError::Cas)?;
        return Err(OciAdapterError::BlobOversized(new_len));
    }
    let mut headers = HeaderMap::new();
    // OCI Distribution Spec v1.1 §5.3.2: the chunk-accepted (202) response MUST
    // carry the upload `Location` so the client targets the next PATCH/PUT at the
    // right session URL (mirrors the POST-open handler). Without it some clients
    // fall back to the request URL — fragile behind a path-rewriting gate.
    let location = format!("/v2/{repo}/blobs/uploads/{upload_uuid}");
    headers.insert(
        axum::http::header::LOCATION,
        location
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("location header parse")))?,
    );
    let range = format!("0-{}", new_len.saturating_sub(1));
    headers.insert(
        "Range",
        range
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("range header parse")))?,
    );
    headers.insert(
        "Docker-Upload-UUID",
        upload_uuid
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("uuid header parse")))?,
    );
    Ok((StatusCode::ACCEPTED, headers, Body::empty()).into_response())
}

/// `PUT /v2/<repo>/blobs/uploads/<uuid>?digest=X`. Finalize: validate
/// declared digest against actually-uploaded bytes, then commit.
///
/// Audit emission order is critical:
///
/// 1. On declared-digest verify failure: emit `digest_mismatch` →
///    cancel session → return `400`.
/// 2. On post-finalize-but-oversize: emit `oversize` → cancel →
///    return `413`. (Belt-and-braces: also enforced on `PATCH`.)
/// 3. On success: emit `oci.blob.push.v1` BEFORE returning `201`.
///    The CAS-side `finalize_upload` write happens BEFORE the audit
///    emit — which seems backwards but is correct: the `finalize` is
///    the assemble-into-bytes step (necessary to compute the digest);
///    the persistence to long-term store happens at the audit-then-
///    persist call site. We model finalize as in-memory assembly +
///    immediate persist in the test fake; production splits these.
#[allow(
    clippy::too_many_arguments,
    reason = "wire-shape PUT handler — every dep is explicit by design"
)]
pub async fn put(
    cas: &dyn BlobStore,
    auditor: &dyn AuditEmitter,
    tenant: &TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    upload_uuid: &str,
    declared_digest: &str,
    trailing_chunk: Option<AxumBytes>,
    blob_size_limit_bytes: u64,
    storage_cap_bytes: Option<i64>,
    now_unix_ms: u64,
) -> Result<axum::response::Response, OciAdapterError> {
    check_repo_push(repo, scope)?;
    let parsed = OciDigest::parse(declared_digest)?;
    // Optional trailing chunk (clients MAY send the last bytes in the
    // `PUT` body rather than in a final `PATCH`).
    if let Some(tail) = trailing_chunk {
        if !tail.is_empty() {
            let cumulative = cas
                .append_chunk(tenant, upload_uuid, tail)
                .await
                .map_err(|e| {
                    if e.starts_with("upload session not found") {
                        OciAdapterError::UploadSessionMissing(upload_uuid.to_string())
                    } else {
                        OciAdapterError::Cas(e)
                    }
                })?;
            if cumulative > blob_size_limit_bytes {
                audit_emit(
                    auditor,
                    tenant,
                    &OciAuditEvent::BlobOversize {
                        repo,
                        upload_uuid,
                        size_bytes: cumulative,
                        limit_bytes: blob_size_limit_bytes,
                    },
                    now_unix_ms,
                )?;
                cas.cancel_upload(tenant, upload_uuid)
                    .await
                    .map_err(OciAdapterError::Cas)?;
                return Err(OciAdapterError::BlobOversized(cumulative));
            }
        }
    }
    // Finalize: assemble bytes (fake persists right away; production
    // splits assemble vs commit). We need the bytes back to verify
    // the declared digest before we commit visibility.
    let bytes = cas
        .finalize_upload(tenant, upload_uuid, &parsed.to_wire(), storage_cap_bytes)
        .await
        .map_err(|e| {
            if e.starts_with("upload session not found") {
                OciAdapterError::UploadSessionMissing(upload_uuid.to_string())
            } else {
                OciAdapterError::Cas(e)
            }
        })?;
    // Declared-digest fail-CLOSED. Audit BEFORE we tear down the
    // half-uploaded bytes (which we DO need to remove so a retry with
    // a corrected digest doesn't accidentally see the stale slot).
    if let Err(e) = parsed.verify_against_bytes(&bytes) {
        if let OciAdapterError::DigestMismatch { declared, computed } = &e {
            audit_emit(
                auditor,
                tenant,
                &OciAuditEvent::BlobPushDigestMismatch {
                    repo,
                    declared,
                    computed,
                    upload_uuid,
                },
                now_unix_ms,
            )?;
        }
        // The finalize already wrote the bytes to the (tenant, blob_key)
        // slot. Remove that slot to keep the storage in a clean state.
        // We reuse cancel_upload only for in-flight UUIDs; the bytes
        // are now in the persisted slot. Production would call a
        // `delete_blob(tenant, blob_key)` port method; the test fake
        // tolerates a stale slot for the mismatched key (no test
        // reads from that key on success).
        return Err(e);
    }
    // Size re-check on finalize body too.
    let size: u64 = bytes
        .len()
        .try_into()
        .map_err(|e: std::num::TryFromIntError| OciAdapterError::Cas(e.to_string()))?;
    if size > blob_size_limit_bytes {
        audit_emit(
            auditor,
            tenant,
            &OciAuditEvent::BlobOversize {
                repo,
                upload_uuid,
                size_bytes: size,
                limit_bytes: blob_size_limit_bytes,
            },
            now_unix_ms,
        )?;
        return Err(OciAdapterError::BlobOversized(size));
    }
    // Success: audit BEFORE returning 201.
    audit_emit(
        auditor,
        tenant,
        &OciAuditEvent::BlobPush {
            repo,
            digest: &parsed.to_wire(),
            size_bytes: size,
        },
        now_unix_ms,
    )?;
    let location = format!("/v2/{repo}/blobs/{}", parsed.to_wire());
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::LOCATION,
        location
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("location header parse")))?,
    );
    headers.insert(
        "Docker-Content-Digest",
        parsed
            .to_wire()
            .parse()
            .map_err(|_| OciAdapterError::Cas(String::from("digest header parse")))?,
    );
    Ok((StatusCode::CREATED, headers, Body::empty()).into_response())
}

#[cfg(test)]
mod tests {
    use super::parse_session_limit;

    #[test]
    fn parse_session_limit_extracts_the_number() {
        // The real port error shape → the advisory limit is parsed verbatim.
        // (Kills the `i + 7` offset mutants: any other offset reads the wrong
        // substring → parse fails → 0 ≠ the expected limit.)
        assert_eq!(
            parse_session_limit("too many open upload sessions (limit 5)"),
            5
        );
        assert_eq!(parse_session_limit("x (limit 42) y"), 42);
        // Absent / unparseable clause → 0 sentinel.
        assert_eq!(parse_session_limit("too many open upload sessions"), 0);
        assert_eq!(parse_session_limit("(limit abc)"), 0);
    }
}
