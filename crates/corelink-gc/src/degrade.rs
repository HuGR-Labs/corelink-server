//! Degrade-mode `gc-pause` config-singleton + probe trait.
//!
//! WI-S06-001 §6.1.5 + PAT-DEGRADE-001 alignment freezes the
//! degrade-mode contract:
//!
//! - The mode lives in a Cloudflare Durable Object config-singleton
//!   (per-environment global). Operator-driven enable/disable via the
//!   S-13 admin plane.
//! - Workers MUST probe at every batch boundary (≤ 100 ms next-tick
//!   propagation gate per WI §6.1.5 / chaos test #4 / property test
//!   `prop_degrade_mode_propagation`).
//! - On `GcPause`, a `Running` worker aborts with terminal status =
//!   [`crate::run::GcStatus::Aborted`].
//! - On `GcReadOnly`, a `Running` worker continues mark + sweep but
//!   skips physical-delete (forward — wired in WI-S06-004 P0
//!   wiring).
//! - Probe failure → fail-closed: caller MUST treat the unprobeable
//!   degrade state as `GcPause` (silently proceeding could continue
//!   mark/sweep during an operator-declared incident).

use std::sync::Mutex;

use thiserror::Error;

/// Canonical degrade kinds. WI §1 enumerates the 3 variants;
/// `#[non_exhaustive]` so future S-13 admin-plane work can add
/// per-control-knob variants additively.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DegradeKind {
    /// GC normal operation.
    Off,
    /// GC suspended; cron triggers reject; running workers abort
    /// within ≤ 100 ms (next batch boundary). PAT-DEGRADE-001.
    GcPause,
    /// Mark + sweep continue; physical-delete suspended (forward —
    /// WI-S06-004 honours).
    GcReadOnly,
}

impl DegradeKind {
    /// Lower-snake-case mnemonic (e.g. for metric label
    /// `corelink.gc.degrade_mode_active{kind="gc-pause"}`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::GcPause => "gc-pause",
            Self::GcReadOnly => "gc-read-only",
        }
    }

    /// Whether this kind requires a Running worker to abort
    /// immediately at the next batch boundary. Only `GcPause` does;
    /// `GcReadOnly` lets mark + sweep continue (skips physical-delete
    /// only).
    #[must_use]
    pub const fn requires_abort(self) -> bool {
        matches!(self, Self::GcPause)
    }

    /// Whether this kind blocks new cron-tick admission. WI §6.1.5.
    #[must_use]
    pub const fn blocks_new_runs(self) -> bool {
        matches!(self, Self::GcPause | Self::GcReadOnly)
    }
}

/// Snapshot of the degrade-mode config-singleton. The DO read returns
/// this shape; the trait surface is sync because the config-singleton
/// read is a single KV / DO storage GET.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DegradeMode {
    /// Current degrade kind.
    pub kind: DegradeKind,
    /// PAT id hex of the operator who enabled the mode (`None` when
    /// `kind == Off`). Aligned with `corelink-audit::PatIdHash`
    /// redaction discipline — never the raw secret.
    pub enabled_by_pat_id: Option<String>,
    /// Wall-clock instant the mode was enabled (`0` when
    /// `kind == Off`).
    pub enabled_at_ms: u64,
    /// Operator-supplied incident reason (free-form; bounded length
    /// at the S-13 admin plane). Empty when `kind == Off`.
    pub reason: String,
}

impl DegradeMode {
    /// Canonical "off" snapshot.
    #[must_use]
    pub fn off() -> Self {
        Self {
            kind: DegradeKind::Off,
            enabled_by_pat_id: None,
            enabled_at_ms: 0,
            reason: String::new(),
        }
    }
}

/// Errors surfaced by [`DegradeProbe::probe`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DegradeProbeError {
    /// Config-singleton DO storage read failure.
    #[error("degrade probe backend error: {0}")]
    Backend(String),
}

/// Trait every config-singleton DO satisfies. WI §6.1.5 — production
/// wiring composes this against a Cloudflare DO; the
/// `InMemoryDegradeProbe` fake satisfies the same surface for unit
/// + property tests.
pub trait DegradeProbe: Send + Sync + core::fmt::Debug {
    /// Snapshot the current degrade mode.
    ///
    /// # Errors
    ///
    /// Backend-class via [`DegradeProbeError::Backend`]. Caller MUST
    /// fail-closed on error (treat unprobeable state as `GcPause`)
    /// per WI §6.1.5.
    fn probe(&self) -> Result<DegradeMode, DegradeProbeError>;
}

/// In-memory degrade-mode probe. Tests + property tests use the
/// `set` / `clear` helpers to drive the snapshot deterministically.
#[derive(Debug)]
pub struct InMemoryDegradeProbe {
    inner: Mutex<DegradeMode>,
}

impl Default for InMemoryDegradeProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryDegradeProbe {
    /// Construct a probe pinned to `Off`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(DegradeMode::off()),
        }
    }

    /// Drive the mode to `kind` with the supplied operator metadata.
    /// `kind = Off` clears the operator metadata (back to canonical
    /// off snapshot).
    pub fn set(&self, kind: DegradeKind, enabled_by_pat_id: &str, enabled_at_ms: u64, reason: &str) {
        let snap = if kind == DegradeKind::Off {
            DegradeMode::off()
        } else {
            DegradeMode {
                kind,
                enabled_by_pat_id: Some(enabled_by_pat_id.to_owned()),
                enabled_at_ms,
                reason: reason.to_owned(),
            }
        };
        match self.inner.lock() {
            Ok(mut g) => *g = snap,
            Err(p) => *p.into_inner() = snap,
        }
    }

    /// Convenience: set to `GcPause` with the canonical operator
    /// metadata.
    pub fn pause(&self, enabled_by_pat_id: &str, enabled_at_ms: u64, reason: &str) {
        self.set(DegradeKind::GcPause, enabled_by_pat_id, enabled_at_ms, reason);
    }

    /// Reset to the canonical `Off` snapshot.
    pub fn clear(&self) {
        self.set(DegradeKind::Off, "", 0, "");
    }
}

impl DegradeProbe for InMemoryDegradeProbe {
    fn probe(&self) -> Result<DegradeMode, DegradeProbeError> {
        match self.inner.lock() {
            Ok(g) => Ok(g.clone()),
            Err(p) => Ok(p.into_inner().clone()),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn off_snapshot_is_canonical() {
        let m = DegradeMode::off();
        assert_eq!(m.kind, DegradeKind::Off);
        assert_eq!(m.enabled_by_pat_id, None);
        assert_eq!(m.enabled_at_ms, 0);
        assert!(m.reason.is_empty());
    }

    #[test]
    fn requires_abort_only_gc_pause() {
        assert!(DegradeKind::GcPause.requires_abort());
        assert!(!DegradeKind::GcReadOnly.requires_abort());
        assert!(!DegradeKind::Off.requires_abort());
    }

    #[test]
    fn blocks_new_runs_excludes_off() {
        assert!(!DegradeKind::Off.blocks_new_runs());
        assert!(DegradeKind::GcPause.blocks_new_runs());
        assert!(DegradeKind::GcReadOnly.blocks_new_runs());
    }

    #[test]
    fn pause_round_trip() {
        let probe = InMemoryDegradeProbe::new();
        assert_eq!(probe.probe().unwrap().kind, DegradeKind::Off);
        probe.pause("pat_abcd", 1234, "incident_42");
        let snap = probe.probe().unwrap();
        assert_eq!(snap.kind, DegradeKind::GcPause);
        assert_eq!(snap.enabled_by_pat_id.as_deref(), Some("pat_abcd"));
        assert_eq!(snap.enabled_at_ms, 1234);
        assert_eq!(snap.reason, "incident_42");
    }

    #[test]
    fn clear_resets_to_off() {
        let probe = InMemoryDegradeProbe::new();
        probe.pause("pat_xyz", 1, "x");
        probe.clear();
        let snap = probe.probe().unwrap();
        assert_eq!(snap.kind, DegradeKind::Off);
        assert!(snap.enabled_by_pat_id.is_none());
    }

    #[test]
    fn as_str_canonical() {
        assert_eq!(DegradeKind::Off.as_str(), "off");
        assert_eq!(DegradeKind::GcPause.as_str(), "gc-pause");
        assert_eq!(DegradeKind::GcReadOnly.as_str(), "gc-read-only");
    }
}
