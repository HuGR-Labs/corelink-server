//! Example: 6-digit OTP recovery flow (Lote 10.3-tris P0-R5-002a).
//!
//! Magic-link recovery is **not representable** in this crate's
//! [`RecoveryChannel`] enum.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "example binary"
)]

use std::time::Duration;

use corelink_webauthn::{
    InMemoryRecoveryOtpStore, RecoveryChannel, RecoveryOtpStore, RecoveryOtpVerifyOutcome,
    RecoveryRateLimit, UserAccountId,
};

fn main() {
    let store = InMemoryRecoveryOtpStore::new(RecoveryRateLimit::canonical());
    let user = UserAccountId::new_v7();
    let now_ms: u64 = 1_700_000_000_000;

    let minted = corelink_webauthn::recovery::mint_otp(
        user,
        now_ms,
        Duration::from_secs(600),
        RecoveryChannel::ClerkSsoEmail,
        5,
    )
    .unwrap();
    let plaintext = minted.plaintext.into_string();
    println!("OTP issued (6 digits, 10-min TTL).");
    store.put(minted.record).unwrap();

    let outcome = store
        .verify_and_consume(user, &plaintext, now_ms + 1_000)
        .unwrap();
    match outcome {
        RecoveryOtpVerifyOutcome::Consumed { otp_id } => {
            println!("Recovery succeeded; otp_id={otp_id:?}.");
        }
        RecoveryOtpVerifyOutcome::Mismatch { attempts_remaining } => {
            println!("Recovery rejected (attempts remaining: {attempts_remaining})");
        }
    }
}
