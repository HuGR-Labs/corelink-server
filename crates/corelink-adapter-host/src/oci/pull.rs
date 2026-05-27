//! Pull-side handlers (`GET`/`HEAD /v2/<name>/blobs/<digest>` and
//! `GET`/`HEAD /v2/<name>/manifests/<reference>`).
//!
//! Pull paths are NON-mutating, so they do NOT emit audit rows on the
//! success path (the OCI Distribution Spec v1.1 §pull does not require
//! audit emission for reads; high-volume pulls would otherwise saturate
//! the audit chain). Failed-auth pulls DO emit `oci.auth.denied.v1`
//! through the dispatch layer.

pub mod blob;
pub mod manifest;
