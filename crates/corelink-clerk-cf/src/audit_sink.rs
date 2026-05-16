//! Production audit sink for the four real CF binding wrappers.
//!
//! Each binding wrapper (`CfR2BucketReal`, `CfD1DatabaseReal`,
//! `CfKvNamespaceReal`, `CfDurableObjectReal`) accepts an `AuditFn`
//! closure invoked before every operation (read AND mutation — fail-CLOSED
//! on mutation, advisory on read). This module owns the production
//! wiring: a single [`AuditSink`] is constructed at request entry and the
//! four binding adapters share it.
//!
//! # Sink targets
//!
//! On `wasm32-unknown-unknown` (the production CF Worker target) audit
//! events are emitted as one canonical NDJSON line per call via
//! `worker::console_log!`. CF's logging pipeline (Logpush) ingests these
//! into the Workers Analytics / centralised audit store.
//!
//! On native (host-side integration tests) the sink uses an in-memory
//! `Vec<AuditEvent>` so tests can assert on emission. The
//! [`AuditSink::recorder`] constructor returns the test-mode sink with an
//! attached `Arc<Mutex<Vec<AuditEvent>>>` for inspection.
//!
//! # Charter
//!
//! - `#![forbid(unsafe_code)]` inherited from the crate root.
//! - No `unwrap` / `expect` / `panic` outside `#[cfg(test)]`.
//! - Audit fail-CLOSED is preserved: on mutation ops, if the audit
//!   recorder fails (e.g. lock poisoning), the binding wrapper sees an
//!   `Err` and the operation is NOT performed. The wasm32 emission path
//!   does not return an error (console_log is infallible), so the
//!   fail-CLOSED gate only fires in test mode on a poisoned mutex.
//! - Tenant identifier comparison upstream uses `subtle::ConstantTimeEq`;
//!   this module only emits the (already-validated) tenant prefix string
//!   into the event payload.
//! - CTRL-PRIV-001: the audit event payload includes the operation label
//!   and the scoped key / SQL preview / DO name — never raw blob bytes,
//!   JWT secrets, or D1 row payloads.

use std::sync::Arc;

use corelink_cf_bindings::{
    D1AuditFn, D1Error, D1Op, DoAuditFn, DoError, DoOp, KvAuditFn, KvError, KvOp, R2AuditFn,
    R2Error, R2Op,
};

/// A single audit event captured before a binding operation.
///
/// Production marshals events into a one-line NDJSON record via
/// `console_log!`; tests pull them from the recorder vec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEvent {
    /// Binding surface (`r2` / `d1` / `kv` / `do`).
    pub surface: &'static str,
    /// Per-binding operation label (e.g. `"get"`, `"put"`, `"first"`).
    pub op: &'static str,
    /// Tenant prefix as a non-secret label (already validated upstream).
    pub tenant: String,
    /// Operation-specific subject: scoped key / SQL preview / DO name.
    pub subject: String,
}

impl AuditEvent {
    /// Serialise to a one-line NDJSON string suitable for `console_log!`.
    ///
    /// Manual serialisation (vs. `serde_json::to_string`) keeps this
    /// allocation cheap and lets us guarantee the field order — the
    /// downstream ingest pipeline parses positionally for hot-path
    /// filtering before fanning out into the analytics schema.
    #[must_use]
    pub fn to_ndjson(&self) -> String {
        format!(
            "{{\"surface\":\"{}\",\"op\":\"{}\",\"tenant\":\"{}\",\"subject\":\"{}\"}}",
            escape(self.surface),
            escape(self.op),
            escape(&self.tenant),
            escape(&self.subject),
        )
    }
}

/// Minimal JSON string escape for the four fields. Only handles the
/// subset of bytes that can appear in a tenant prefix / op label /
/// scoped key (no raw user content reaches this path — CTRL-PRIV-001).
fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Backing store for an audit sink — production fans into Logpush,
/// tests fan into an in-memory vec.
#[derive(Clone)]
enum SinkBackend {
    /// Production: emit one NDJSON line per event via `console_log!`.
    /// The unit variant carries no state — the emission is stateless.
    ConsoleNdjson,
    /// Native test recorder. Wrapped in `Arc<std::sync::Mutex<...>>` so
    /// the closure can be `Fn + Send + Sync` (matching the binding
    /// `AuditFn` bounds) without giving up shared mutation.
    Recorder(Arc<std::sync::Mutex<Vec<AuditEvent>>>),
}

impl std::fmt::Debug for SinkBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConsoleNdjson => f.write_str("ConsoleNdjson"),
            Self::Recorder(_) => f.write_str("Recorder"),
        }
    }
}

/// Fan-out audit sink for the four CF binding real wrappers.
///
/// Build once per request, adapt to each binding's `AuditFn` via
/// [`AuditSink::r2`], [`AuditSink::d1`], [`AuditSink::kv`], [`AuditSink::do_`].
#[derive(Clone, Debug)]
pub struct AuditSink {
    backend: SinkBackend,
    tenant: String,
}

impl AuditSink {
    /// Production sink: emit NDJSON via `console_log!` (Logpush ingest).
    ///
    /// `tenant_label` is the validated tenant prefix (R2) / tenant id
    /// (D1) / tenant prefix (KV / DO). The sink only stores it for
    /// emission; identity comparison is done upstream by the binding
    /// wrappers using `subtle::ConstantTimeEq`.
    #[must_use]
    pub fn console_ndjson(tenant_label: impl Into<String>) -> Self {
        Self {
            backend: SinkBackend::ConsoleNdjson,
            tenant: tenant_label.into(),
        }
    }

    /// Native test sink: capture events into an in-memory vec.
    ///
    /// Returns the sink and a handle to the underlying recording buffer
    /// so tests can assert on the captured events.
    #[must_use]
    pub fn recorder(
        tenant_label: impl Into<String>,
    ) -> (Self, Arc<std::sync::Mutex<Vec<AuditEvent>>>) {
        let buf = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Self {
            backend: SinkBackend::Recorder(Arc::clone(&buf)),
            tenant: tenant_label.into(),
        };
        (sink, buf)
    }

    /// Emit a single audit event through the configured backend.
    ///
    /// # Errors
    ///
    /// On the production console path this is infallible. On the
    /// recorder path a poisoned mutex returns `Err(())` so the caller
    /// (the binding `AuditFn` closure) can translate to the correct
    /// per-binding error variant and trigger the fail-CLOSED gate.
    fn record(&self, surface: &'static str, op: &'static str, subject: &str) -> Result<(), ()> {
        let event = AuditEvent {
            surface,
            op,
            tenant: self.tenant.clone(),
            subject: subject.to_owned(),
        };
        match &self.backend {
            SinkBackend::ConsoleNdjson => {
                #[cfg(target_arch = "wasm32")]
                {
                    worker::console_log!("{}", event.to_ndjson());
                }
                // On native the ConsoleNdjson variant is a no-op — the
                // host-side path of the binding wrappers does not exercise
                // real `worker::*` ops anyway. Native callers that need
                // visibility use `recorder()`.
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = event;
                }
                Ok(())
            }
            SinkBackend::Recorder(buf) => match buf.lock() {
                Ok(mut guard) => {
                    guard.push(event);
                    Ok(())
                }
                Err(_) => Err(()),
            },
        }
    }

    /// Emit a synthetic [`AuditEvent`] directly through the configured
    /// backend, bypassing the per-binding `AuditFn` adapter layer.
    ///
    /// Used by the wave-26 CF Worker request-prelude prefetch wire
    /// (`prod_wiring::prefetch_request_prelude`) to surface a
    /// `tenant_region_unresolved` row when the
    /// `TenantRegionResolver::resolve_region` chain fails. The per-binding
    /// `D1Op` / `R2Op` / `KvOp` / `DoOp` enums intentionally do not
    /// include the `tenant_region_unresolved` op label (their variants
    /// gate the hot-path wrapper ops, not orchestration-level audit
    /// events); the synthetic path lets the prefetch wire emit through
    /// the same NDJSON canonical sink without expanding those enums.
    ///
    /// Infallible by design — the recorder backend's mutex-poison case
    /// degrades to a silent drop (the prefetch wire is already on the
    /// fail-CLOSED path; an additional log line failing is not actionable
    /// and must not double-fault the 503 response).
    pub fn emit_synthetic(&self, event: AuditEvent) {
        match &self.backend {
            SinkBackend::ConsoleNdjson => {
                #[cfg(target_arch = "wasm32")]
                {
                    worker::console_log!("{}", event.to_ndjson());
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = event;
                }
            }
            SinkBackend::Recorder(buf) => {
                if let Ok(mut guard) = buf.lock() {
                    guard.push(event);
                }
            }
        }
    }

    /// Build an [`R2AuditFn`] adapter routing R2 events to this sink.
    #[must_use]
    pub fn r2(&self) -> R2AuditFn {
        let sink = self.clone();
        Arc::new(move |op: R2Op, key: &str| -> Result<(), R2Error> {
            sink.record("r2", op.as_str(), key)
                .map_err(|()| R2Error::Backend("audit sink recorder poisoned".to_owned()))
        })
    }

    /// Build a [`D1AuditFn`] adapter routing D1 events to this sink.
    #[must_use]
    pub fn d1(&self) -> D1AuditFn {
        let sink = self.clone();
        Arc::new(move |op: D1Op, sql: &str| -> Result<(), D1Error> {
            sink.record("d1", op.as_str(), sql)
                .map_err(|()| D1Error::Backend("audit sink recorder poisoned".to_owned()))
        })
    }

    /// Build a [`KvAuditFn`] adapter routing KV events to this sink.
    #[must_use]
    pub fn kv(&self) -> KvAuditFn {
        let sink = self.clone();
        Arc::new(move |op: KvOp, key: &str| -> Result<(), KvError> {
            sink.record("kv", op.as_str(), key)
                .map_err(|()| KvError::Backend("audit sink recorder poisoned".to_owned()))
        })
    }

    /// Build a [`DoAuditFn`] adapter routing DO events to this sink.
    #[must_use]
    pub fn do_(&self) -> DoAuditFn {
        let sink = self.clone();
        Arc::new(move |op: DoOp, name: &str| -> Result<(), DoError> {
            sink.record("do", op.as_str(), name)
                .map_err(|()| DoError::Backend("audit sink recorder poisoned".to_owned()))
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics on assertion failure are the canonical signal"
)]
mod tests {
    use super::*;

    #[test]
    fn ndjson_roundtrip_basic() {
        let event = AuditEvent {
            surface: "r2",
            op: "get",
            tenant: "tnt0123456789abcd".to_owned(),
            subject: "tnt0123456789abcd/foo".to_owned(),
        };
        let line = event.to_ndjson();
        assert!(line.starts_with("{\"surface\":\"r2\""));
        assert!(line.contains("\"op\":\"get\""));
        assert!(line.contains("\"tenant\":\"tnt0123456789abcd\""));
        assert!(line.ends_with("\"}"));
    }

    #[test]
    fn ndjson_escapes_special_chars() {
        let event = AuditEvent {
            surface: "kv",
            op: "put",
            tenant: "tnt".to_owned(),
            subject: "k\"\\\n".to_owned(),
        };
        let line = event.to_ndjson();
        assert!(line.contains("\\\""));
        assert!(line.contains("\\\\"));
        assert!(line.contains("\\n"));
    }

    #[test]
    fn recorder_captures_events() {
        let (sink, buf) = AuditSink::recorder("tnt0123456789abcd");
        sink.record("r2", "get", "tnt0123456789abcd/foo")
            .expect("recorder ok");
        sink.record("kv", "put", "tnt0123456789abcd:bar")
            .expect("recorder ok");
        let captured = buf.lock().expect("lock");
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].surface, "r2");
        assert_eq!(captured[1].op, "put");
    }
}
