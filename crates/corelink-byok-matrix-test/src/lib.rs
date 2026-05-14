//! `corelink-byok-matrix-test` — 16-combination matrix test framework
//! (4 providers × 4 ops) + property tests + adversarial regression.
//!
//! This crate exists to satisfy the workspace build requirement;
//! all tests live in the `tests/` directory.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

/// 16-combination matrix test result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixCell {
    /// Provider name (e.g. "gcp_kms").
    pub provider: &'static str,
    /// Operation (e.g. "wrap").
    pub op: &'static str,
    /// Whether the cell passed.
    pub passed: bool,
    /// Optional error message on failure.
    pub error: Option<String>,
}

/// All 4 providers in the matrix.
pub const PROVIDERS: [&str; 4] = ["aws_kms", "gcp_kms", "azure_key_vault", "hashicorp_vault"];

/// All 4 operations in the matrix.
pub const OPS: [&str; 4] = ["write", "read", "wrap", "unwrap"];

/// Total expected cells in the matrix (4 × 4 = 16).
pub const MATRIX_CELLS: usize = PROVIDERS.len() * OPS.len();
