//! The Rust half of the two-sided parity contract with the edge-native
//! `findMissingBlobs` path.
//!
//! `worker/tests/vectors/tenant_prefix_vectors.json` is asserted by BOTH this
//! test and `worker/tests/edge_find_missing.test.ts`. The container writes CAS
//! objects at keys derived by [`derive_prefix`]; the Worker must probe the very
//! same keys, and a one-sided test would happily let the edge derive a key the
//! container never wrote — reporting every blob missing, which a cache client
//! reads as "upload everything".
//!
//! See `docs/design/2026-08-25-adr-edge-native-find-missing.md`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use uuid::Uuid;
use zeroize::Zeroizing;

/// Path to the shared vector file, relative to this crate's manifest dir.
const VECTORS: &str = "../../worker/tests/vectors/tenant_prefix_vectors.json";

fn hex_to_32(hex: &str) -> [u8; 32] {
    assert_eq!(hex.len(), 64, "TDK must be 32 bytes / 64 hex chars");
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("vector TDK must be hex");
    }
    out
}

/// Mirror of `R2S3Client::blob_key` (`crates/corelink-container/src/storage/r2_s3.rs`).
/// Duplicated deliberately: this crate does not depend on the container, and the
/// point of the vector file is that the FORMAT is pinned in one place both
/// languages read.
fn blob_key(region: &str, prefix: &str, digest: &str, sha256: bool) -> String {
    if sha256 {
        format!("{region}/{prefix}/bazel/sha256/{digest}")
    } else {
        format!("{region}/{prefix}/{digest}")
    }
}

#[test]
fn shared_vectors_match_the_rust_derivation() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(VECTORS);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let doc: serde_json::Value = serde_json::from_str(&raw).expect("vector file must be JSON");

    let tdk_hex = doc["tdk_hex"].as_str().expect("tdk_hex");
    let key = TenantDerivationKey::from_bytes(Zeroizing::new(hex_to_32(tdk_hex)));

    let cases = doc["cases"].as_array().expect("cases");
    assert!(
        !cases.is_empty(),
        "an empty vector file would pass vacuously"
    );

    for case in cases {
        let tenant = case["tenant"].as_str().expect("tenant");
        let expected_prefix = case["prefix"].as_str().expect("prefix");
        let region = case["region"].as_str().expect("region");

        let uuid = Uuid::parse_str(tenant).expect("vector tenant must be a canonical UUID");
        let got = derive_prefix(&key, uuid);
        assert_eq!(
            got.as_str(),
            expected_prefix,
            "prefix drift for tenant {tenant}: the edge would probe keys the container never wrote"
        );

        assert_eq!(
            blob_key(
                region,
                expected_prefix,
                case["blake3_digest"].as_str().expect("blake3_digest"),
                false
            ),
            case["blake3_key"].as_str().expect("blake3_key"),
            "native BLAKE3 key format drift for tenant {tenant}"
        );
        assert_eq!(
            blob_key(region, expected_prefix, case["sha256_digest"].as_str().expect("sha256_digest"), true),
            case["sha256_key"].as_str().expect("sha256_key"),
            "REAPI sha256 key format drift for tenant {tenant} — findMissingBlobs digests are sha256, \
             so this is the format the batch route actually probes"
        );
    }
}
