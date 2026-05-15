//! Production Durable Object adapter.
//!
//! CoreLink uses Durable Objects for two canonical purposes:
//!
//! 1. **Rollout controller** (S-13) — single-writer FSM for rollout
//!    state transitions. Each rollout is a uniquely-named DO instance.
//! 2. **CoreLinkServer lifecycle** (root `wrangler.toml`) — bridges the
//!    CF Worker control plane to the Rust gRPC Container.
//!
//! Both call sites construct stubs via the same pattern:
//!
//! ```ignore
//! let ns = env.durable_object("ROLLOUT_DO")?;
//! let id = ns.id_from_name("tenant-42")?;
//! let stub = id.get_stub()?;
//! let response = stub.fetch_with_str("https://do/state").await?;
//! ```
//!
//! [`CfDurableObjectAdapter`] wraps the `ObjectNamespace` so callers
//! can pass it as a single value across boundaries without re-querying
//! `env.durable_object(...)` per request — important because each
//! `env.durable_object` call touches the CF binding registry.

// `worker::durable` is a private module in workers-rs 0.8; the public
// re-export at crate root (`pub use crate::durable::*` in
// `worker/src/lib.rs`) lifts `ObjectNamespace`, `Stub`, etc. into
// `worker::*`. We import from the crate root to stay on the public API.
use worker::{ObjectNamespace, Result as WorkerResult, Stub};

/// Production Durable Object namespace adapter.
///
/// Construct via [`CfDurableObjectAdapter::new`] from an
/// `ObjectNamespace` obtained via `env.durable_object("ROLLOUT_DO")`.
#[derive(Debug)]
pub struct CfDurableObjectAdapter {
    namespace: ObjectNamespace,
}

impl CfDurableObjectAdapter {
    /// Wrap a `worker::durable::ObjectNamespace` binding.
    #[must_use]
    pub fn new(namespace: ObjectNamespace) -> Self {
        Self { namespace }
    }

    /// Borrow the underlying `ObjectNamespace`.
    #[must_use]
    pub fn inner(&self) -> &ObjectNamespace {
        &self.namespace
    }

    /// Resolve a named DO instance to its [`Stub`] for fetch-style
    /// invocation.
    ///
    /// `name` is the canonical instance identifier (e.g. a tenant_id
    /// or rollout_id). CF hashes the name into a 256-bit ObjectId and
    /// routes requests to the single instance that owns that id.
    ///
    /// # Errors
    ///
    /// Returns `worker::Error` if the name is empty or the namespace
    /// binding is misconfigured.
    pub fn stub_by_name(&self, name: &str) -> WorkerResult<Stub> {
        let id = self.namespace.id_from_name(name)?;
        id.get_stub()
    }

    /// Resolve a DO instance using a hex-encoded ObjectId (e.g. one
    /// previously serialized to a D1 row).
    ///
    /// # Errors
    ///
    /// Returns `worker::Error` if `hex_id` is malformed (not 64 hex
    /// chars) or the namespace binding is misconfigured.
    pub fn stub_by_hex_id(&self, hex_id: &str) -> WorkerResult<Stub> {
        let id = self.namespace.id_from_string(hex_id)?;
        id.get_stub()
    }
}
