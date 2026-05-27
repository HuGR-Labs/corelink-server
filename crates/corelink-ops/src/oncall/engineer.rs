//! Engineer identifier + struct.

/// Opaque engineer identifier (tenant-agnostic per CTRL-PRIV-001).
/// The id wraps a `String` slug (e.g. `eng-001`) the PagerDuty
/// schedule binds via `user_reference`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EngineerId(String);

impl EngineerId {
    /// Construct a new engineer id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Canonical slug accessor.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for EngineerId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Engineer record — id + display name (display name remains
/// PII-equivalent and is only used in oncall handoff template /
/// manager 1:1 manual reviews; tenant-agnostic per CTRL-PRIV-001).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Engineer {
    /// Canonical id.
    pub id: EngineerId,
    /// Display name (handoff template scope only).
    pub display_name: String,
}

impl Engineer {
    /// Construct a new engineer record.
    #[must_use]
    pub fn new(id: EngineerId, display_name: impl Into<String>) -> Self {
        Self {
            id,
            display_name: display_name.into(),
        }
    }
}
