//! `corelink-customer-alerts` — multi-channel customer alert delivery.
//!
//! Implements [`CustomerAlerter`] for production use, dispatching to:
//!
//! - **Dashboard**: D1 row INSERT into `customer_alerts` + WebSocket fanout.
//! - **Email**: SendGrid / SES (configurable per tenant).
//! - **In-app notification**: D1 row + WebSocket push.
//! - **Slack webhook**: optional; customer-configured per tenant.
//!
//! At least one channel must succeed; `Ok` is returned if any succeeds.
//! Failed channels are logged at WARN. If all channels fail, `Err` is
//! returned.
//!
//! Used by `corelink-byok-revocation` kill switch (WI-S14-006).
//!
//! # Example
//!
//! ```rust
//! use corelink_ops::alerts::{MultiChannelAlerter, AlerterConfig};
//! use corelink_byok_core::KmsKeyId;
//! use corelink_byok_revocation::alerter::RevocationAlertPayload;
//! use corelink_byok_revocation::CustomerAlerter;
//!
//! # tokio_test::block_on(async {
//! let config = AlerterConfig::default();
//! let alerter = MultiChannelAlerter::new(config);
//!
//! let payload = RevocationAlertPayload {
//!     provider: "aws".to_string(),
//!     kms_key_id: KmsKeyId {
//!         provider: corelink_byok_core::KmsProviderKind::AwsKms,
//!         key_arn_or_id: "k1".to_string(),
//!         region: "us-east-1".to_string(),
//!     },
//!     tenant_id_hashed: "h1".to_string(),
//!     detected_at_ms: 0,
//!     kill_switch_duration_ms: 0,
//!     recovery_instructions: "Re-enable CMK".to_string(),
//! };
//!
//! // In CI/test mode, channels are stubbed to succeed silently.
//! let result = alerter.alert(payload).await;
//! assert!(result.is_ok());
//! # });
//! ```

#![forbid(unsafe_code)]

pub mod alerter;
pub mod channel;
pub mod config;

pub use alerter::MultiChannelAlerter;
pub use config::AlerterConfig;
