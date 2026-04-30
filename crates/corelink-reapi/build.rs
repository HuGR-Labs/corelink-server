//! Build script for `corelink-reapi`.
//!
//! Compiles the vendored REAPI v2.12.0 minimal subset + ByteStream proto
//! (see `proto/` tree) into Rust with `tonic-build`. The build is gated on
//! the `host-server` cargo feature so a wasm32 build (which excludes tonic +
//! tokio) skips proto codegen entirely.
//!
//! Why a build.rs vs a generated checked-in module:
//! - `tonic-build` re-runs codegen on every checkout, guaranteeing the Rust
//!   stubs stay in lock-step with the proto contract under any hand-edit.
//! - Proto changes are reviewable as proto diffs (the canonical surface).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("CARGO_FEATURE_HOST_SERVER").is_none() {
        return Ok(());
    }

    let proto_root = "proto";
    let protos = [
        "proto/build/bazel/semver/semver.proto",
        "proto/google/rpc/status.proto",
        "proto/google/bytestream/bytestream.proto",
        "proto/build/bazel/remote/execution/v2/remote_execution.proto",
    ];

    for p in &protos {
        println!("cargo:rerun-if-changed={p}");
    }

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&protos, &[proto_root])?;

    Ok(())
}
