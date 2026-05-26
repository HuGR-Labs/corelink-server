//! Push-side handlers: multi-step blob upload + manifest mutation.
//!
//! Push paths are the audit-fail-CLOSED hot path. Every state-mutating
//! finalize emits BEFORE the mutation lands:
//!
//! - [`upload::finalize`] emits `oci.blob.push.v1` BEFORE writing the
//!   blob to CAS (or `oci.blob.push.digest_mismatch.v1` on declared-
//!   digest mismatch BEFORE canceling the upload session).
//! - [`manifest::put`] emits `oci.manifest.push.v1` BEFORE persisting
//!   the manifest JSON, and `oci.tag.update.v1` AFTER (still BEFORE
//!   returning success to the client) when `<reference>` was a tag.

pub mod manifest;
pub mod upload;
