//! `corelink-slack-real` — shared Slack incoming-webhook real client (R2-4).
//!
//! This crate is the canonical production wiring for Slack notifications
//! across CoreLink. Multiple consumers — enterprise inquiry intake
//! (S-19), PagerDuty alert fallback, breach notification dispatch
//! (S-13/S-19 privacy lane), oncall handoff (S-17) and the lighthouse
//! customer tracker (S-20) — all dispatch through one
//! [`SharedSlackClient`] trait with per-channel webhook URL routing.
//!
//! # Wire surface
//!
//! - [`SharedSlackClient`] — canonical channel-aware trait.
//! - [`SlackChannel`] — six-variant `#[non_exhaustive]` routing enum.
//! - [`WebhookRegistry`] — loads `SLACK_WEBHOOK_URL_<CHANNEL>` env vars
//!   so each channel has its own webhook for routing isolation.
//! - [`SlackMessage`] — Block Kit message builder
//!   (header / fields / footer+timestamp / optional actions).
//! - [`MessageTemplate`] — structured per-channel payload enum; the
//!   template owns the schema so untrusted input is never spliced into
//!   Slack `mrkdwn` strings.
//! - [`RetryPolicy`] — three retries with exponential backoff on
//!   transient errors (5xx + 429). 4xx is fatal (give-up immediately).
//! - [`SlackAuditSink`] / [`SlackAuditEvent`] — every send emits
//!   `corelink.notification.slack.sent` or `.failed` BEFORE the call
//!   completes (audit-emit-before-mutation; INV-AUDIT-EMIT-ATOMIC).
//! - [`SlackHttpClient`] — real wiring (reqwest blocking).
//! - [`InMemorySharedSlackClient`] — in-memory fake for unit tests.
//! - [`InquirySlackAdapter`] — bridge implementing the existing
//!   `corelink_enterprise_inquiry::SlackClient` trait on top of any
//!   [`SharedSlackClient`].
//!
//! # Privacy / safety
//!
//! - The full webhook URL is NEVER logged. [`redact_webhook`] truncates
//!   to `https://hooks.slack.com/services/T0000/B0000/***`.
//! - Templates own field rendering — callers cannot pass raw mrkdwn,
//!   only structured key/value pairs that the renderer escapes.
//! - Customer PII included in any [`MessageTemplate`] variant must be
//!   pre-redacted by the caller per CTRL-PRIV-001.
//! - No Slack Bot API token surface (different protocol; requires bot
//!   scopes — explicitly out of scope per R2-4 constraints).
//!
//! # Runtime
//!
//! Production wiring uses `reqwest::blocking`. There is no `tokio`
//! dependency in `src/`. `wiremock` is dev-only.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod adapter;
pub mod audit;
pub mod channel;
pub mod client;
pub mod http;
pub mod memory;
pub mod message;
pub mod redact;
pub mod retry;
pub mod template;

pub use adapter::InquirySlackAdapter;
pub use audit::{
    InMemorySlackAuditSink, SlackAuditError, SlackAuditEvent, SlackAuditOutcome, SlackAuditSink,
};
pub use channel::{SlackChannel, WebhookRegistry, WebhookRegistryError};
pub use client::{SendOutcome, SharedSlackClient, SlackClientError};
pub use http::SlackHttpClient;
pub use memory::{InMemorySharedSlackClient, RecordedSend};
pub use message::{SlackActionButton, SlackField, SlackMessage};
pub use redact::redact_webhook;
pub use retry::{RetryDecision, RetryPolicy};
pub use template::MessageTemplate;
