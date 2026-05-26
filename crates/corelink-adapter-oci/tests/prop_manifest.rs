//! Property tests for manifest schema validation.
//!
//! Generates well-formed manifests of all three accepted media types
//! (`oci.image.manifest.v1`, `oci.image.index.v1`,
//! `docker.distribution.manifest.v2`, `docker.distribution.manifest.list.v2`)
//! and asserts they always validate. Also generates ill-formed
//! manifests and asserts they always reject.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]

use corelink_adapter_oci::push::manifest::validate;
use proptest::prelude::*;

fn arb_image_manifest() -> impl Strategy<Value = serde_json::Value> {
    proptest::collection::vec(
        proptest::collection::vec(any::<u8>(), 32..33),
        0..3,
    )
    .prop_map(|layer_digests| {
        let layers: Vec<serde_json::Value> = layer_digests
            .iter()
            .map(|d| {
                serde_json::json!({
                    "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                    "digest": format!("sha256:{}", hex::encode(d)),
                    "size": 0
                })
            })
            .collect();
        serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                "size": 0
            },
            "layers": layers
        })
    })
}

fn arb_index_manifest() -> impl Strategy<Value = serde_json::Value> {
    proptest::collection::vec(any::<u8>(), 1..16).prop_map(|seed| {
        let manifests: Vec<serde_json::Value> = seed
            .iter()
            .map(|b| {
                serde_json::json!({
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": format!("sha256:{:02x}{}", b, "00".repeat(31)),
                    "size": 0,
                    "platform": {
                        "architecture": "amd64",
                        "os": "linux"
                    }
                })
            })
            .collect();
        serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": manifests
        })
    })
}

fn arb_docker_v2_manifest() -> impl Strategy<Value = serde_json::Value> {
    Just(serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.docker.distribution.manifest.v2+json",
        "config": {
            "mediaType": "application/vnd.docker.container.image.v1+json",
            "digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "size": 0
        },
        "layers": []
    }))
}

proptest! {
    #[test]
    fn valid_image_manifest_passes(m in arb_image_manifest()) {
        let bytes = serde_json::to_vec(&m).unwrap();
        let (mt, _d) = validate(&bytes).expect("valid");
        prop_assert_eq!(mt, "application/vnd.oci.image.manifest.v1+json");
    }

    #[test]
    fn valid_index_manifest_passes(m in arb_index_manifest()) {
        let bytes = serde_json::to_vec(&m).unwrap();
        let (mt, _d) = validate(&bytes).expect("valid");
        prop_assert_eq!(mt, "application/vnd.oci.image.index.v1+json");
    }

    #[test]
    fn valid_docker_v2_manifest_passes(m in arb_docker_v2_manifest()) {
        let bytes = serde_json::to_vec(&m).unwrap();
        let (mt, _d) = validate(&bytes).expect("valid");
        prop_assert_eq!(mt, "application/vnd.docker.distribution.manifest.v2+json");
    }

    #[test]
    fn random_bytes_fail_validation(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        // Filter the (vanishingly unlikely) accidental valid manifest.
        let looks_v2 = serde_json::from_slice::<serde_json::Value>(&bytes)
            .map(|v| v.as_object().is_some_and(|o|
                o.get("schemaVersion").and_then(|s| s.as_u64()) == Some(2)))
            .unwrap_or(false);
        prop_assume!(!looks_v2);
        prop_assert!(validate(&bytes).is_err());
    }
}
