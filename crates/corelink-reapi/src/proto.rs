//! Generated proto modules (gated on `host-server` feature).
//!
//! tonic-build emits one Rust module per protobuf package; the
//! cross-package paths it generates use `super::super::…` chains relative
//! to where each module is included. To make those resolve correctly, we
//! create the package modules at the **top level** of this `proto` module
//! so the chains have enough levels to climb.
//!
//! Lints disabled wholesale: tonic-generated code does not honor our
//! workspace lints.

#![allow(
    missing_docs,
    missing_debug_implementations,
    clippy::indexing_slicing,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::dbg_macro,
    clippy::todo,
    clippy::unimplemented,
    clippy::pedantic,
    clippy::nursery,
    clippy::doc_lazy_continuation,
    clippy::doc_overindented_list_items,
    clippy::doc_markdown,
    clippy::module_name_repetitions,
    clippy::derive_partial_eq_without_eq,
    clippy::large_enum_variant,
    clippy::missing_errors_doc,
    clippy::result_large_err,
    clippy::needless_pass_by_value,
    reason = "tonic-generated code is opaque to our lints"
)]

pub mod build {
    pub mod bazel {
        pub mod semver {
            tonic::include_proto!("build.bazel.semver");
        }
        pub mod remote {
            pub mod execution {
                pub mod v2 {
                    tonic::include_proto!("build.bazel.remote.execution.v2");
                }
            }
        }
    }
}

pub mod google {
    pub mod rpc {
        tonic::include_proto!("google.rpc");
    }
    pub mod bytestream {
        tonic::include_proto!("google.bytestream");
    }
}

/// Stable convenience aliases for the most-used package modules.
pub use build::bazel::remote::execution::v2 as reapi;
pub use build::bazel::semver;
pub use google::bytestream;
pub use google::rpc as google_rpc;
