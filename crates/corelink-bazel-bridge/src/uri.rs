//! REAPI v2 URI pattern parser.
//!
//! # URI patterns
//!
//! REAPI v2 cache REST endpoints share the instance (tenant) prefix:
//!
//! ```text
//! GET  /{instance}/blobs/{hash}/{size}
//! POST /{instance}/uploads/{uuid}/blobs/{hash}/{size}
//! GET  /{instance}/blobs/ac/{hash}/{size}
//! PUT  /{instance}/blobs/ac/{hash}/{size}
//! POST /{instance}/findMissingBlobs
//! ```
//!
//! The `instance` path segment maps 1:1 to a CoreLink tenant. This module
//! extracts both the operation kind and the tenant/digest from an incoming
//! URI path string.
//!
//! # Design
//!
//! The parser operates on the path-only portion of the URI (no scheme/host).
//! The caller is responsible for stripping a mount prefix (e.g.
//! `/bazel/v2`) before passing the path to [`parse_reapi_path`].

use crate::digest::Digest;
use crate::error::BazelBridgeError;

/// The operation kind extracted from a REAPI v2 cache URI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoapiOperation {
    /// `GET /{instance}/blobs/{hash}/{size}` — CAS read.
    CasRead {
        /// Tenant / instance name.
        instance: String,
        /// Validated REAPI Digest.
        digest: Digest,
    },

    /// `POST /{instance}/uploads/{uuid}/blobs/{hash}/{size}` — CAS write.
    /// Bazel uses a resumable upload URI; this bridge accepts single-shot
    /// writes only and ignores the `{uuid}` correlation ID.
    CasWrite {
        /// Tenant / instance name.
        instance: String,
        /// The resumable-upload UUID (kept for correlation; not validated
        /// beyond being non-empty).
        upload_id: String,
        /// Validated REAPI Digest.
        digest: Digest,
    },

    /// `GET /{instance}/blobs/ac/{hash}/{size}` — AC lookup.
    AcRead {
        /// Tenant / instance name.
        instance: String,
        /// Validated REAPI Digest of the action.
        digest: Digest,
    },

    /// `PUT /{instance}/blobs/ac/{hash}/{size}` — AC update.
    AcWrite {
        /// Tenant / instance name.
        instance: String,
        /// Validated REAPI Digest of the action.
        digest: Digest,
    },

    /// `POST /{instance}/findMissingBlobs` — batch find-missing.
    FindMissingBlobs {
        /// Tenant / instance name.
        instance: String,
    },
}

/// Parse a REAPI v2 path string (no leading scheme/host; leading `/` is
/// required) and return the [`RoapiOperation`] it represents.
///
/// The caller MUST indicate the HTTP method so the parser can disambiguate
/// `GET /{instance}/blobs/ac/…` (AC read) from `PUT /{instance}/blobs/ac/…`
/// (AC write) and single-blob CAS reads from `findMissingBlobs`.
///
/// # Errors
///
/// Returns [`BazelBridgeError::InvalidDigest`] if the digest segment fails
/// validation. Returns [`BazelBridgeError::Internal`] if the path does not
/// match any known REAPI v2 pattern.
pub fn parse_reapi_path(method: &str, path: &str) -> Result<RoapiOperation, BazelBridgeError> {
    // Strip optional leading `/`.
    let path = path.trim_start_matches('/');

    // Collect at most 6 segments (max needed: instance/uploads/uuid/blobs/hash/size).
    let parts: Vec<&str> = path.splitn(6, '/').collect();

    // Use explicit length + get() to satisfy clippy::indexing_slicing.
    let instance = parts
        .first()
        .copied()
        .ok_or_else(|| BazelBridgeError::Internal("REAPI path: empty".into()))?;

    if instance.is_empty() {
        return Err(BazelBridgeError::Internal(
            "REAPI path: instance segment is empty".into(),
        ));
    }

    let second = parts
        .get(1)
        .copied()
        .ok_or_else(|| BazelBridgeError::Internal(format!("REAPI path too short: {path:?}")))?;

    // /{instance}/findMissingBlobs
    if second == "findMissingBlobs" && parts.len() == 2 {
        return Ok(RoapiOperation::FindMissingBlobs {
            instance: instance.to_owned(),
        });
    }

    // /{instance}/blobs/…
    if second == "blobs" {
        let third = parts.get(2).copied().ok_or_else(|| {
            BazelBridgeError::Internal(format!("REAPI blobs path incomplete: {path:?}"))
        })?;
        let fourth = parts.get(3).copied().ok_or_else(|| {
            BazelBridgeError::Internal(format!("REAPI blobs path missing digest: {path:?}"))
        })?;

        // /{instance}/blobs/ac/{hash}/{size}
        if third == "ac" {
            let fifth = parts.get(4).copied().ok_or_else(|| {
                BazelBridgeError::Internal(format!("REAPI AC path missing size: {path:?}"))
            })?;
            let digest_str = format!("{fourth}/{fifth}");
            let digest = Digest::parse(&digest_str)?;
            return match method.to_uppercase().as_str() {
                "GET" => Ok(RoapiOperation::AcRead {
                    instance: instance.to_owned(),
                    digest,
                }),
                "PUT" => Ok(RoapiOperation::AcWrite {
                    instance: instance.to_owned(),
                    digest,
                }),
                other => Err(BazelBridgeError::Internal(format!(
                    "unsupported HTTP method {other:?} for AC path"
                ))),
            };
        }

        // /{instance}/blobs/{hash}/{size}  (third = hash, fourth = size)
        let digest_str = format!("{third}/{fourth}");
        let digest = Digest::parse(&digest_str)?;
        return Ok(RoapiOperation::CasRead {
            instance: instance.to_owned(),
            digest,
        });
    }

    // /{instance}/uploads/{uuid}/blobs/{hash}/{size}
    if second == "uploads" {
        // parts = [instance, "uploads", uuid, "blobs", hash, size]
        // (splitn(6) so parts[5] = size, all within bounds)
        let upload_id = parts.get(2).copied().ok_or_else(|| {
            BazelBridgeError::Internal(format!("REAPI uploads path: missing uuid: {path:?}"))
        })?;
        if upload_id.is_empty() {
            return Err(BazelBridgeError::Internal(
                "REAPI uploads path: uuid segment is empty".into(),
            ));
        }
        let blobs_seg = parts.get(3).copied().ok_or_else(|| {
            BazelBridgeError::Internal(format!("REAPI uploads path incomplete: {path:?}"))
        })?;
        if blobs_seg != "blobs" {
            return Err(BazelBridgeError::Internal(format!(
                "REAPI uploads path: expected 'blobs' at segment 3, got {blobs_seg:?}"
            )));
        }
        let hash_seg = parts.get(4).copied().ok_or_else(|| {
            BazelBridgeError::Internal(format!("REAPI uploads path: missing hash: {path:?}"))
        })?;
        let size_seg = parts.get(5).copied().ok_or_else(|| {
            BazelBridgeError::Internal(format!("REAPI uploads path: missing size: {path:?}"))
        })?;
        let digest = Digest::parse(&format!("{hash_seg}/{size_seg}"))?;
        return Ok(RoapiOperation::CasWrite {
            instance: instance.to_owned(),
            upload_id: upload_id.to_owned(),
            digest,
        });
    }

    Err(BazelBridgeError::Internal(format!(
        "REAPI path does not match any known pattern: {path:?}"
    )))
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

    const HASH: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    const UUID: &str = "550e8400-e29b-41d4-a716-446655440000";

    fn cas_read_path() -> String {
        format!("/myinstance/blobs/{HASH}/1024")
    }

    fn ac_read_path() -> String {
        format!("/myinstance/blobs/ac/{HASH}/2048")
    }

    fn cas_write_path() -> String {
        format!("/myinstance/uploads/{UUID}/blobs/{HASH}/512")
    }

    fn find_missing_path() -> String {
        "/myinstance/findMissingBlobs".to_owned()
    }

    #[test]
    fn parse_cas_read() {
        let op = parse_reapi_path("GET", &cas_read_path()).expect("parse");
        match op {
            RoapiOperation::CasRead { instance, digest } => {
                assert_eq!(instance, "myinstance");
                assert_eq!(digest.hash, HASH);
                assert_eq!(digest.size_bytes, 1024);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_cas_read_without_leading_slash() {
        let path = cas_read_path();
        let trimmed = path.trim_start_matches('/');
        let op = parse_reapi_path("GET", trimmed).expect("parse no leading slash");
        assert!(matches!(op, RoapiOperation::CasRead { .. }));
    }

    #[test]
    fn parse_cas_write() {
        let op = parse_reapi_path("POST", &cas_write_path()).expect("parse write");
        match op {
            RoapiOperation::CasWrite {
                instance,
                upload_id,
                digest,
            } => {
                assert_eq!(instance, "myinstance");
                assert_eq!(upload_id, UUID);
                assert_eq!(digest.hash, HASH);
                assert_eq!(digest.size_bytes, 512);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_ac_read() {
        let op = parse_reapi_path("GET", &ac_read_path()).expect("ac read");
        match op {
            RoapiOperation::AcRead { instance, digest } => {
                assert_eq!(instance, "myinstance");
                assert_eq!(digest.size_bytes, 2048);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_ac_write() {
        let op = parse_reapi_path("PUT", &ac_read_path()).expect("ac write");
        assert!(matches!(op, RoapiOperation::AcWrite { .. }));
    }

    #[test]
    fn parse_find_missing_blobs() {
        let op = parse_reapi_path("POST", &find_missing_path()).expect("find missing");
        match op {
            RoapiOperation::FindMissingBlobs { instance } => {
                assert_eq!(instance, "myinstance");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_invalid_digest_in_path_propagates_error() {
        let bad = "/myinstance/blobs/badhash/1024";
        let err = parse_reapi_path("GET", bad).expect_err("bad hash");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn parse_empty_instance_is_error() {
        // Leading double-slash produces an empty instance segment.
        let err = parse_reapi_path("GET", "//blobs/a/b").expect_err("empty instance");
        assert!(matches!(err, BazelBridgeError::Internal(_)));
    }

    #[test]
    fn parse_unknown_pattern_is_error() {
        let err = parse_reapi_path("GET", "/instance/unknownaction").expect_err("unknown pattern");
        assert!(matches!(err, BazelBridgeError::Internal(_)));
    }
}
