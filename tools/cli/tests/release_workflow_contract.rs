//! Structural regression tests for the public CLI release root.
//!
//! These tests deliberately inspect the workflow text.  The critical contract
//! spans GitHub Actions, the installer, and three independent signing lanes;
//! a Rust-only unit test cannot observe it.  Keep the mutation controls here so
//! a superficially valid workflow cannot silently repoint, skip tool validation,
//! or stop publishing the files downstream signers request.

#[path = "release_workflow_contract/assertions.rs"]
mod assertions;
#[path = "release_workflow_contract/cases.rs"]
mod cases;

use assertions::*;
