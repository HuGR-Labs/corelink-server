//! gRPC plumbing helpers + REAPI resource-name parsers extracted from the
//! monolithic `handler.rs` per Wave 33 Stream A2.1c file-size discipline.
//!
//! Every helper is module-private to the `handler` parent (`pub(super)`
//! for items consumed across submodule boundaries; `pub` only for the
//! REAPI resource-name parsers because `cargo-fuzz` smoke-tests
//! exercise them through `corelink_reapi::handler::parse_read_resource_name`).

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_replication::region_resolver::Region;
use tonic::{Code, Request, Status};
use uuid::Uuid;

use crate::error_map::{
    miss_mapping, ReadErrorMapping, COR_AUTH_PAT_INVALID, COR_AUTH_SCOPE_INSUFFICIENT,
    COR_CAS_BAD_RESOURCE_NAME, GRPC_PERMISSION_DENIED, GRPC_UNAUTHENTICATED,
};
use crate::pat::AuthStubError;

// ---------------------------------------------------------------------------
// gRPC plumbing helpers
// ---------------------------------------------------------------------------

pub(super) fn extract_bearer<T>(req: &Request<T>) -> Result<String, AuthStubError> {
    let raw = req
        .metadata()
        .get("authorization")
        .ok_or(AuthStubError::PatInvalid)?;
    let s = raw.to_str().map_err(|_| AuthStubError::PatInvalid)?.trim();
    // RFC 7235 §2.1 — auth-scheme is case-insensitive. Match `Bearer ` with a
    // lowercase prefix probe so any variant (`BEARER`, `BeArEr`, …) parses.
    let space_pos = s.find(' ').ok_or(AuthStubError::PatInvalid)?;
    // Safe-by-construction slicing: `space_pos` is a byte index returned by
    // `str::find` and therefore a valid char boundary.
    #[allow(
        clippy::indexing_slicing,
        reason = "byte indices come from str::find; always valid char boundaries"
    )]
    let scheme = &s[..space_pos];
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(AuthStubError::PatInvalid);
    }
    #[allow(
        clippy::indexing_slicing,
        reason = "byte indices come from str::find; always valid char boundaries"
    )]
    let token = s[space_pos + 1..].trim_start();
    if token.is_empty() {
        return Err(AuthStubError::PatInvalid);
    }
    Ok(token.to_owned())
}

pub(super) fn extract_request_id<T>(req: &Request<T>) -> String {
    req.metadata()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::now_v7().to_string())
}

pub(super) fn pat_error_to_status(e: &AuthStubError) -> Status {
    let (code, taxonomy, msg) = match e {
        AuthStubError::PatInvalid => (
            Code::Unauthenticated,
            COR_AUTH_PAT_INVALID,
            "PAT invalid or expired",
        ),
        AuthStubError::ScopeInsufficient { .. } => (
            Code::PermissionDenied,
            COR_AUTH_SCOPE_INSUFFICIENT,
            "PAT scope does not allow this operation",
        ),
    };
    let _grpc_int = match code {
        Code::Unauthenticated => GRPC_UNAUTHENTICATED,
        Code::PermissionDenied => GRPC_PERMISSION_DENIED,
        _ => 0,
    };
    make_status(code, taxonomy, msg, 0)
}

pub(super) fn make_status(
    code: Code,
    taxonomy: &str,
    message: &str,
    contextual_size: u64,
) -> Status {
    let mut st = Status::new(code, message.to_owned());
    if let Ok(v) = tonic::metadata::MetadataValue::try_from(taxonomy) {
        st.metadata_mut().insert("x-corelink-error-code", v);
    }
    if contextual_size > 0 {
        if let Ok(v) = tonic::metadata::MetadataValue::try_from(contextual_size.to_string()) {
            st.metadata_mut().insert("x-corelink-context-size", v);
        }
    }
    st
}

pub(super) fn grpc_code_from_i32(c: i32) -> Code {
    match c {
        3 => Code::InvalidArgument,
        5 => Code::NotFound,
        7 => Code::PermissionDenied,
        8 => Code::ResourceExhausted,
        10 => Code::Aborted,
        11 => Code::OutOfRange,
        13 => Code::Internal,
        14 => Code::Unavailable,
        16 => Code::Unauthenticated,
        _ => Code::Unknown,
    }
}

pub(super) const fn region_str(r: Region) -> &'static str {
    // Only the 3 S-01 regions are canonical (WI-S01-003 §6.1.4): wnam,
    // weur, sam. ENAM lands in S-14 region expansion; this fn evolves at
    // that point.
    match r {
        Region::Wnam => "wnam",
        Region::Weur => "weur",
        Region::Sam => "sam",
    }
}

/// Map [`crate::read::ReadOrchestratorError`] → tonic `Status`.
pub(super) fn read_orch_error_to_status(e: &crate::read::ReadOrchestratorError) -> Status {
    let m = e.mapping();
    make_status(
        grpc_code_from_i32(m.grpc_code),
        m.taxonomy_code,
        m.message,
        0,
    )
}

/// Canonical label for a [`crate::read::MissReason`] arm, mirrored by
/// `corelink-worker::middleware::MissArm::label`. Used to encode the
/// arm into the Status metadata so the timing-padding emit hook can
/// attribute the padded latency to the arm.
pub(super) fn miss_reason_label(reason: crate::read::MissReason) -> &'static str {
    match reason {
        crate::read::MissReason::NeverExisted => "never_existed",
        crate::read::MissReason::Tombstoned => "tombstoned",
        crate::read::MissReason::R2OrphanRow => "r2_orphan_row",
    }
}

/// Map a `ReadOutcome::NotFound` into the canonical wire status. Per
/// ADR-0028 every [`crate::read::MissReason`] variant maps to the same
/// wire 404 + `COR_CAS_BLOB_NOT_FOUND` taxonomy code; the disambiguation
/// lives in the audit envelope, not in the wire response.
#[allow(
    dead_code,
    reason = "back-compat helper for handlers that surface 404 without a MissReason arm; production callers use miss_to_status_with_arm"
)]
pub(super) fn miss_to_status() -> Status {
    miss_to_status_with_arm(None)
}

/// Variant of [`miss_to_status`] that mixes the canonical `MissArm`
/// discriminator into the Status metadata so the
/// `corelink-worker::middleware::TimingPaddingService` emit hook can
/// attribute the padded latency to the arm. tonic's
/// `Status::into_http` propagates metadata into the response
/// headers, which is the only post-conversion-readable channel
/// (response extensions do not survive `Status → http::Response`).
/// The metadata key matches `corelink-worker`'s emit-side reader.
/// Codex round-5 P1 fix.
pub(super) fn miss_to_status_with_arm(arm: Option<&'static str>) -> Status {
    let m = miss_mapping();
    let mut status = make_status(
        grpc_code_from_i32(m.grpc_code),
        m.taxonomy_code,
        m.message,
        0,
    );
    if let Some(label) = arm {
        if let Ok(v) = label.parse::<tonic::metadata::AsciiMetadataValue>() {
            status.metadata_mut().insert("x-corelink-miss-arm", v);
        }
    }
    status
}

/// Apply REAPI v2 `read_offset` / `read_limit` semantics to a body.
///
/// Per `google.bytestream` §read:
/// - `read_offset` MUST be in `[0, body.len()]`. Equal to `body.len()`
///   yields an empty stream (legitimate trailing read). Greater than
///   `body.len()` is `OUT_OF_RANGE`.
/// - `read_limit == 0` ⇒ "no limit" (read to end).
/// - `read_limit > 0` ⇒ stream `min(read_limit, body.len() - offset)` bytes.
pub(super) fn slice_for_offset_limit(
    body: Bytes,
    offset: i64,
    limit: i64,
) -> Result<Bytes, Status> {
    let body_len = body.len();
    let body_len_i64 = i64::try_from(body_len).unwrap_or(i64::MAX);
    if offset > body_len_i64 {
        return Err(make_status(
            Code::OutOfRange,
            COR_CAS_BAD_RESOURCE_NAME,
            "ByteStream::Read read_offset exceeds blob size",
            body_len as u64,
        ));
    }
    let offset_usize = usize::try_from(offset).unwrap_or(usize::MAX);
    let remaining = body_len.saturating_sub(offset_usize);
    let take = if limit == 0 {
        remaining
    } else {
        usize::try_from(limit).unwrap_or(usize::MAX).min(remaining)
    };
    Ok(body.slice(offset_usize..offset_usize.saturating_add(take)))
}

// ---------------------------------------------------------------------------
// Resource-name parsers
// ---------------------------------------------------------------------------

/// Parsed REAPI ByteStream::Read resource_name. Symmetric to
/// `ParsedResource` (write-side; module-private) but for the read
/// surface — there is no `uploads/<uuid>` segment in the read form
/// per REAPI v2 §"reading from the CAS".
#[derive(Debug, PartialEq, Eq)]
pub struct ParsedReadResource {
    /// The 64-char-hex BLAKE3-256 digest extracted from the second
    /// segment after `blobs/`.
    pub digest: Digest,
    /// Optional size hint — `None` if the client omitted the size_bytes
    /// segment (REAPI v2 §read accepts the bare-digest form). The
    /// handler cross-checks against the blob_meta-recorded size when
    /// the hint is `Some(_)`, surfacing a mismatch as
    /// `INVALID_ARGUMENT` to discourage tooling drift; the
    /// blob_meta-recorded value is the canonical source of truth for
    /// stream framing.
    pub size_bytes: Option<i64>,
}

/// Parse a REAPI ByteStream::Read resource_name. Canonical forms (per
/// REAPI v2 + remote_cache_product_profile.md §7.2.1):
///
/// - `<instance_name>/blobs/<digest_hash>/<size_bytes>`
/// - `blobs/<digest_hash>/<size_bytes>` (instance empty)
/// - `<instance_name>/blobs/<digest_hash>` (size omitted; legacy
///   bare-digest form accepted by REAPI v2 conformance suite).
///
/// `<digest_hash>` MUST be 64 lowercase hex chars (BLAKE3-256). The
/// optional `<size_bytes>` segment must parse as a non-negative `i64`.
/// Trailing segments after `<size_bytes>` are rejected — `blobs/<h>/<s>`
/// is the canonical tail; anything beyond is a wire-protocol violation.
///
/// Exposed `pub` so `cargo-fuzz` smoke tests can exercise the parser
/// against arbitrary bytes without rebuilding the whole gRPC stack.
///
/// # Errors
///
/// Returns a `&'static str` diagnostic on malformed input. Never
/// panics; the parser is total over `str` input.
pub fn parse_read_resource_name(name: &str) -> Result<ParsedReadResource, &'static str> {
    let parts: Vec<&str> = name.split('/').collect();
    let blobs_pos = parts
        .iter()
        .position(|p| *p == "blobs")
        .ok_or("missing 'blobs' segment in resource_name")?;
    let after = parts
        .get(blobs_pos..)
        .ok_or("malformed resource_name: missing tail")?;
    // After splitting on `/`, the slice is `["blobs", hash, size?, …]`.
    // Codex round-1 P3: the prior implementation used `[_, hash, size_str, ..]`
    // which silently accepted trailing garbage (`blobs/<h>/<s>/etc/junk`).
    // Tighten to exact-match arms so the parser is total + canonical.
    let (hash, size_bytes) = match after {
        [_, hash] => (*hash, None),
        [_, hash, size_str] => {
            let size: i64 = size_str
                .parse()
                .map_err(|_| "resource_name: size_bytes is not a non-negative integer")?;
            if size < 0 {
                return Err("resource_name: size_bytes is negative");
            }
            (*hash, Some(size))
        }
        _ => {
            return Err(
                "malformed resource_name: expected exactly blobs/<hash>[/<size>]; trailing segments are not permitted",
            );
        }
    };
    let digest = Digest::from_hex(hash).map_err(|_| "resource_name: digest hex malformed")?;
    Ok(ParsedReadResource { digest, size_bytes })
}

/// Parsed REAPI ByteStream::Write resource_name. Module-private — write
/// callers consume via `parse_resource_name` only.
pub(super) struct ParsedResource {
    pub(super) digest: Digest,
    pub(super) size_bytes: i64,
}

/// Parse a REAPI ByteStream resource_name. Canonical forms:
///
/// - `<instance>/uploads/<uuid>/blobs/<digest_hash>/<size_bytes>`
/// - `uploads/<uuid>/blobs/<digest_hash>/<size_bytes>` (instance empty)
///
/// We accept either. The `<digest_hash>` MUST be 64 lowercase hex
/// characters (BLAKE3-256 / SHA-256). `<size_bytes>` must parse as a
/// non-negative `i64`.
pub(super) fn parse_resource_name(name: &str) -> Result<ParsedResource, &'static str> {
    let parts: Vec<&str> = name.split('/').collect();
    // Walk to find `uploads`; everything from there must be
    // `uploads/<uuid>/blobs/<digest>/<size>`.
    let uploads_pos = parts
        .iter()
        .position(|p| *p == "uploads")
        .ok_or("missing 'uploads' segment in resource_name")?;
    let after = parts
        .get(uploads_pos..)
        .ok_or("malformed resource_name: missing tail")?;
    let &[_, _uuid, blobs, hash, size_bytes_str, ..] = after else {
        return Err("malformed resource_name: expected uploads/<uuid>/blobs/<hash>/<size>");
    };
    if blobs != "blobs" {
        return Err("malformed resource_name: expected 'blobs' segment after upload uuid");
    }
    let digest = Digest::from_hex(hash).map_err(|_| "resource_name: digest hex malformed")?;
    let size_bytes: i64 = size_bytes_str
        .parse()
        .map_err(|_| "resource_name: size_bytes is not a non-negative integer")?;
    if size_bytes < 0 {
        return Err("resource_name: size_bytes is negative");
    }
    Ok(ParsedResource { digest, size_bytes })
}
