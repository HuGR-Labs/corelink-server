//! Core cross-cutting newtypes shared across every CoreLink context
//! crate. Apex of the dependency graph (depends on nothing
//! `corelink-*`).

pub mod digest;
pub mod region;
pub mod secret;
pub mod tenant;

pub use digest::{Digest, DIGEST_LEN};
pub use region::Region;
pub use secret::SecretWrap;
pub use tenant::TenantId;
