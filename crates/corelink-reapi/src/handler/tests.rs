//! Unit tests for the handler submodules. Consolidated from the
//! pre-split `#[cfg(test)] mod tests` block per Wave 33 Stream A2.1c.
//! Test set + assertions identical to the pre-split block (parity).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use corelink_hash::Digest;
use corelink_worker::Region;
use tonic::Request;
use uuid::Uuid;

use crate::pat::AuthStubError;

use super::audit_emit::deterministic_audit_id;
use super::helpers::{
    extract_bearer, parse_read_resource_name, parse_resource_name, region_str,
};

#[test]
fn parse_resource_name_canonical() {
    let name = "instance/uploads/0000-uuid/blobs/d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24/11";
    let parsed = parse_resource_name(name).unwrap();
    assert_eq!(parsed.size_bytes, 11);
    assert_eq!(
        parsed.digest.to_hex(),
        "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
    );
}

#[test]
fn parse_resource_name_rejects_short_form() {
    assert!(parse_resource_name("nope").is_err());
    assert!(parse_resource_name("uploads/uuid").is_err());
    assert!(parse_resource_name("uploads/uuid/blobs/notenoughhex/0").is_err());
}

#[test]
fn parse_read_resource_name_canonical_with_size() {
    let h = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
    let name = format!("corelink-instance/blobs/{h}/11");
    let parsed = parse_read_resource_name(&name).unwrap();
    assert_eq!(parsed.digest.to_hex(), h);
    assert_eq!(parsed.size_bytes, Some(11));
}

#[test]
fn parse_read_resource_name_canonical_bare_digest() {
    let h = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
    let name = format!("corelink-instance/blobs/{h}");
    let parsed = parse_read_resource_name(&name).unwrap();
    assert_eq!(parsed.digest.to_hex(), h);
    assert_eq!(parsed.size_bytes, None);
}

#[test]
fn parse_read_resource_name_rejects_trailing_garbage() {
    // codex round-1 P3 fix: trailing segments must be rejected.
    let h = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
    let name = format!("corelink-instance/blobs/{h}/11/extra-junk");
    assert!(parse_read_resource_name(&name).is_err());
    let name2 = format!("corelink-instance/blobs/{h}/11/x/y/z");
    assert!(parse_read_resource_name(&name2).is_err());
}

#[test]
fn parse_read_resource_name_rejects_negative_size() {
    let h = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";
    let name = format!("corelink-instance/blobs/{h}/-1");
    assert!(parse_read_resource_name(&name).is_err());
}

#[test]
fn parse_read_resource_name_rejects_malformed_hash() {
    let name = "corelink-instance/blobs/zzz/11";
    assert!(parse_read_resource_name(name).is_err());
}

#[test]
fn parse_read_resource_name_rejects_missing_blobs_segment() {
    assert!(parse_read_resource_name("corelink-instance/uploads/abc").is_err());
}

#[test]
fn deterministic_audit_id_is_stable_per_request() {
    let d = Digest::compute(b"hello");
    let t = Uuid::from_u128(1);
    let a = deterministic_audit_id("req-1", t, &d);
    let b = deterministic_audit_id("req-1", t, &d);
    assert_eq!(a, b);
    // version + variant nibbles are well-formed.
    let bs = a.as_bytes();
    assert_eq!(bs[6] & 0xF0, 0x70);
    assert_eq!(bs[8] & 0xC0, 0x80);
}

#[test]
fn extract_bearer_is_case_insensitive() {
    // codex round-2 Low fix: RFC 7235 §2.1 auth-scheme is case-insensitive.
    for variant in ["Bearer", "bearer", "BEARER", "BeArEr"] {
        let mut req = Request::new(());
        let v: tonic::metadata::MetadataValue<_> =
            format!("{variant} secret-token").parse().unwrap();
        req.metadata_mut().insert("authorization", v);
        let token = extract_bearer(&req).unwrap();
        assert_eq!(token, "secret-token");
    }
}

#[test]
fn extract_bearer_rejects_non_bearer_scheme() {
    let mut req = Request::new(());
    let v: tonic::metadata::MetadataValue<_> = "Basic dXNlcjpwYXNz".parse().unwrap();
    req.metadata_mut().insert("authorization", v);
    assert_eq!(extract_bearer(&req), Err(AuthStubError::PatInvalid));
}

#[test]
fn deterministic_audit_id_varies_per_request_or_digest_or_tenant() {
    let d1 = Digest::compute(b"a");
    let d2 = Digest::compute(b"b");
    let t1 = Uuid::from_u128(1);
    let t2 = Uuid::from_u128(2);
    assert_ne!(
        deterministic_audit_id("req-1", t1, &d1),
        deterministic_audit_id("req-1", t1, &d2)
    );
    assert_ne!(
        deterministic_audit_id("req-1", t1, &d1),
        deterministic_audit_id("req-2", t1, &d1)
    );
    // codex round-2 High fix: tenant scoping.
    assert_ne!(
        deterministic_audit_id("req-1", t1, &d1),
        deterministic_audit_id("req-1", t2, &d1)
    );
}

#[test]
fn region_str_matches_canonical_lowercase() {
    assert_eq!(region_str(Region::Wnam), "wnam");
    assert_eq!(region_str(Region::Weur), "weur");
    assert_eq!(region_str(Region::Sam), "sam");
}
