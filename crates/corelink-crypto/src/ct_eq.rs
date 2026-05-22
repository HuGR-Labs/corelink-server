//! Constant-time equality primitives — wave-33 canonical surface.
//!
//! Surfaces the upstream `subtle::ConstantTimeEq` trait + the `Choice`
//! marker under the canonical `corelink_crypto::ct_eq::*` namespace.
//! Stage 1 streams MAY import from here so that the timing-sensitive
//! comparison primitive has a single, audit-trackable import path —
//! particularly important for charter constraint "`subtle::ConstantTimeEq`
//! preserved on every sensitive comparison currently using it".
//!
//! No new abstractions are introduced.

pub use ::subtle::{Choice, ConstantTimeEq};
