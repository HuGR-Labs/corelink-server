//! libFuzzer harness for `derive_prefix`. Asserts that no `(tdk_bytes,
//! uuid_bytes)` triplet causes a panic, and that the output respects the
//! 16-char URL-safe-base64 invariant. Sustained 1h nightly run per
//! WI-S01-001 §10.8 / §15 / AC-5.

#![no_main]

use corelink_tenant_path::{derive_prefix, TenantDerivationKey, TENANT_PREFIX_LEN};
use libfuzzer_sys::fuzz_target;
use uuid::Uuid;
use zeroize::Zeroizing;

fuzz_target!(|data: &[u8]| {
    if data.len() < 48 {
        return;
    }
    let mut tdk_bytes = [0u8; 32];
    tdk_bytes.copy_from_slice(&data[..32]);
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(&data[32..48]);

    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(tdk_bytes));
    let tid = Uuid::from_bytes(uuid_bytes);
    let prefix = derive_prefix(&tdk, tid);

    // Exercise as_str() and Display so the only allowed lib panic surface
    // (`from_utf8().expect`) is hit on every fuzz input.
    let s = prefix.as_str();
    let _displayed = format!("{prefix}");
    let _hash = std::collections::hash_map::DefaultHasher::new();
    let _debugged = format!("{prefix:?}");

    // Invariant: 16 ASCII bytes from URL-safe base64 alphabet.
    assert_eq!(s.len(), TENANT_PREFIX_LEN);
    for &b in s.as_bytes() {
        let alpha = b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
        assert!(alpha, "non-URL-safe-b64 byte produced: 0x{b:02x}");
    }
});
