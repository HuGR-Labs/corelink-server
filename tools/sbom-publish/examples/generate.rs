//! Example: generate a CycloneDX 1.5+ SBOM from an existing workspace.
//!
//! # Usage
//!
//! ```sh
//! cargo run --example generate -- \
//!   --cargo-lock Cargo.lock \
//!   --cargo-toml Cargo.toml \
//!   --version 0.1.0 \
//!   --output /tmp/sbom.cdx.json
//! ```
//!
//! Requires `cargo-cyclonedx` to be installed:
//!
//! ```sh
//! cargo install cargo-cyclonedx --version "^0.5"
//! ```

#![forbid(unsafe_code)]
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::expect_used)]

use sbom_publish::publisher::{DefaultSbomPublisher, ReleaseMetadata, SbomPublisher};
use url::Url;

#[tokio::main]
async fn main() {
    // Minimal arg parsing for the example
    let cargo_lock = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Cargo.lock".to_owned());
    let cargo_toml = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "Cargo.toml".to_owned());
    let version = std::env::args()
        .nth(3)
        .unwrap_or_else(|| "0.1.0".to_owned());
    let output = std::env::args()
        .nth(4)
        .unwrap_or_else(|| "/tmp/sbom.cdx.json".to_owned());

    let tsa_url = Url::parse("https://tsa.sigstore.dev/api/v1/timestamp").expect("valid TSA URL");

    let publisher = DefaultSbomPublisher::new(
        tsa_url,
        // workspace member names — add your crates here
        &["corelink-worker", "corelink-hash", "corelink-meta"],
        // patched crates (via [patch.crates-io]) — none by default
        &[],
    );

    let meta = ReleaseMetadata {
        version: version.clone(),
        commit_sha: "0000000000000000000000000000000000000000".to_owned(),
        project_name: "corelink-server".to_owned(),
    };

    match publisher
        .generate(
            std::path::Path::new(&cargo_lock),
            std::path::Path::new(&cargo_toml),
            &meta,
        )
        .await
    {
        Ok(signed) => {
            let sbom_bytes =
                serde_json::to_vec_pretty(&signed.sbom).expect("serialisation must not fail");
            std::fs::write(&output, &sbom_bytes).expect("writing SBOM");
            println!("SBOM written to {output}");
            println!("  components  : {}", signed.component_count);
            println!("  NTIA OK     : {}", signed.ntia_compliant);
            println!("  TSR present : {}", signed.tsr_token.is_some());
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
