//! R2/S3 storage adapter facade.
//!
//! The implementation is kept in dedicated, bounded source modules so the
//! CAS and AC paths remain reviewable. They are included into one private
//! module intentionally: this preserves private helper visibility and the
//! public `R2S3Client`/handler API without changing `storage.rs` or downstream
//! wiring.

mod implementation {
    include!("r2_s3_parts/client.rs");
    include!("r2_s3_parts/cas_core.rs");
    include!("r2_s3_parts/cas_ops.rs");
    include!("r2_s3_parts/ac_core.rs");
    include!("r2_s3_parts/ac_ops.rs");

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

        include!("r2_s3_parts/tests_1.rs");
        include!("r2_s3_parts/tests_2.rs");
        include!("r2_s3_parts/tests_3.rs");
    }
}

pub use implementation::{
    build_r2_ac_handler_from_env, build_r2_cas_handler_from_env, CappedGet, R2AcHandler,
    R2CasHandler, R2S3Client,
};
pub(crate) use implementation::{
    public_namespace_prefix, validate_cas_bucket_for_region, verify_content_hash,
};

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "these tests validate the checked-in source layout"
)]
mod structure_tests {
    use std::fs;
    use std::path::PathBuf;

    const PARTS: &[&str] = &[
        "client.rs",
        "cas_core.rs",
        "cas_ops.rs",
        "ac_core.rs",
        "ac_ops.rs",
        "tests_1.rs",
        "tests_2.rs",
        "tests_3.rs",
    ];

    fn parts_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/storage/r2_s3_parts")
    }

    #[test]
    fn every_extracted_part_is_bounded_and_present() {
        let dir = parts_dir();
        assert_eq!(PARTS.len(), 8, "part population must not silently shrink");
        for name in PARTS {
            let path = dir.join(name);
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("missing r2_s3 part {path:?}: {e}"));
            assert!(text.lines().count() <= 1000, "{name} exceeds 1000 lines");
            assert!(!text.trim().is_empty(), "{name} is empty");
        }
    }

    #[test]
    fn central_facade_loads_the_complete_symbol_map() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let facade = fs::read_to_string(root.join("src/storage/r2_s3.rs"))
            .expect("r2_s3 facade must be readable");
        for name in [
            "client.rs",
            "cas_core.rs",
            "cas_ops.rs",
            "ac_core.rs",
            "ac_ops.rs",
            "tests_1.rs",
            "tests_2.rs",
            "tests_3.rs",
        ] {
            assert_eq!(
                facade
                    .matches(&format!("include!(\"r2_s3_parts/{name}\")"))
                    .count(),
                1,
                "part {name} must be included exactly once"
            );
        }
        let ownership = [
            ("client.rs", &["R2S3Client", "CappedGet"][..]),
            (
                "cas_core.rs",
                &[
                    "R2CasHandler",
                    "public_namespace_prefix",
                    "verify_content_hash",
                ][..],
            ),
            (
                "ac_core.rs",
                &["R2AcHandler", "validate_cas_bucket_for_region"][..],
            ),
            ("ac_core.rs", &["build_r2_cas_handler_from_env"][..]),
            ("ac_ops.rs", &["build_r2_ac_handler_from_env"][..]),
        ];
        for (part, symbols) in ownership {
            let text = fs::read_to_string(parts_dir().join(part))
                .unwrap_or_else(|e| panic!("missing symbol-map part {part}: {e}"));
            for symbol in symbols {
                assert!(text.contains(symbol), "symbol {symbol} moved out of {part}");
                assert!(facade.contains(symbol), "facade symbol map lost {symbol}");
            }
        }
    }
}
