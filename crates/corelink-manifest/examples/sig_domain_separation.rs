//! Example: HKDF info-string domain separation across the three
//! canonical sig surfaces — `b"ac-sig"` (WI-S04-004), `b"manifest-sig"`
//! (this crate), `b"meta-manifest-sig"` (reserved for WI-S05-006).
//!
//! Demonstrates that a manifest sig produced for the manifest domain
//! cannot pass as a valid AC sig (cross-domain replay defense).

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "example: prints + unwraps acceptable for runnable demonstration"
)]

use corelink_manifest::HKDF_INFO_MANIFEST_SIG;
use uuid::Uuid;

fn main() {
    println!("HKDF info strings:");
    println!(
        "  ac-sig:           {:?} ({} bytes)",
        std::str::from_utf8(corelink_ac::sig::HKDF_INFO_AC_SIG).unwrap_or("<binary>"),
        corelink_ac::sig::HKDF_INFO_AC_SIG.len()
    );
    println!(
        "  manifest-sig:     {:?} ({} bytes)",
        std::str::from_utf8(HKDF_INFO_MANIFEST_SIG).unwrap_or("<binary>"),
        HKDF_INFO_MANIFEST_SIG.len()
    );
    println!(
        "  meta-manifest-sig (reserved): {:?} ({} bytes)",
        std::str::from_utf8(corelink_manifest::HKDF_INFO_META_MANIFEST_SIG_RESERVED)
            .unwrap_or("<binary>"),
        corelink_manifest::HKDF_INFO_META_MANIFEST_SIG_RESERVED.len()
    );

    // Same TDK + same canonical bytes + same sig_key_id → DIFFERENT
    // sigs across the two domains.
    let tenant = Uuid::nil();
    let tdk = corelink_ac::sig::derive_default_mock_tdk(tenant, 1);
    let canonical_bytes = [0xCD; 102];

    let manifest_sig = corelink_manifest::compute_signature(&tdk, 1, &canonical_bytes)
        .expect("compute_signature OK");
    let ac_sig = corelink_ac::sig::compute_signature(&tdk, 1, &canonical_bytes)
        .expect("compute_signature OK");

    assert_ne!(manifest_sig, ac_sig);
    println!(
        "manifest sig: {} (truncated)",
        hex::encode(&manifest_sig[..8])
    );
    println!("ac sig:       {} (truncated)", hex::encode(&ac_sig[..8]));
    println!("→ domain separation holds (sigs differ for same input)");
}
