//! Drata REST client.
//!
//! - Auth: `Authorization: Bearer <DRATA_API_KEY>`.
//! - Endpoints: one POST per [`super::stream::EvidenceStream`] variant
//!   (see [`super::stream::EvidenceStream::endpoint_path`]).
//! - Idempotency: `Idempotency-Key: <record_sha256>` header per request
//!   so Drata's edge can deduplicate replays before our ledger races.
//! - Retry: synchronous, 3 retries, 250ms initial, 8s cap (5xx + 429).
//! - Audit: caller is responsible for emitting on the
//!   [`super::audit::SyncAuditSink`] BEFORE/AFTER — this client is the
//!   thin wire; the runner orchestrates the audit envelope.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::record::EvidenceRecord;
use super::retry::{RetryDecision, RetryPolicy};
use super::stream::EvidenceStream;

/// Receipt returned by Drata on a successful POST.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrataReceipt {
    /// Canonical Drata receipt id (opaque). Persisted to the ledger.
    pub receipt_id: String,
    /// HTTP status returned (200/201/202).
    pub http_status: u16,
}

/// Drata client error envelope.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum DrataClientError {
    /// Transport exhausted retries.
    #[error("drata transport exhausted after {attempts} attempts: {reason}")]
    TransportExhausted {
        /// Number of attempts made.
        attempts: u32,
        /// Last-seen reason string.
        reason: String,
    },
    /// 4xx permanent reject.
    #[error("drata permanent reject status={status}: {reason}")]
    PermanentReject {
        /// HTTP status.
        status: u16,
        /// Response body summary.
        reason: String,
    },
    /// Misconfiguration (e.g. missing API key).
    #[error("drata misconfigured: {0}")]
    Misconfigured(String),
    /// Could not encode the evidence record.
    #[error("drata encode failed: {0}")]
    EncodeFailed(String),
}

/// Canonical Drata-client trait. Production wiring uses
/// [`DrataHttpClient`]; tests use [`InMemoryDrataClient`].
pub trait DrataClient: core::fmt::Debug + Send + Sync {
    /// Push one record to Drata. The implementer is responsible for
    /// retries (R5-prep default policy).
    ///
    /// # Errors
    ///
    /// Propagates [`DrataClientError`] on transport failure, permanent
    /// reject, or encode error.
    fn push(
        &self,
        record: &EvidenceRecord,
        idempotency_key: &str,
    ) -> Result<DrataReceipt, DrataClientError>;
}

/// Production Drata client (reqwest::blocking, Bearer-token auth).
pub struct DrataHttpClient {
    base_url: String,
    api_key: String,
    retry: RetryPolicy,
    http: reqwest::blocking::Client,
    #[allow(clippy::type_complexity)]
    sleeper: Box<dyn Fn(Duration) + Send + Sync>,
}

impl core::fmt::Debug for DrataHttpClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DrataHttpClient")
            .field("base_url", &self.base_url)
            .field("api_key", &super::redact::redact_api_key(&self.api_key))
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

impl DrataHttpClient {
    /// Construct from explicit base URL + API key.
    ///
    /// # Errors
    ///
    /// Returns the underlying `reqwest` error if the HTTP client
    /// builder fails (TLS init failure).
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, reqwest::Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("corelink-drata-sync/0.1")
            .build()?;
        Ok(Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            retry: RetryPolicy::r5p_default(),
            http,
            sleeper: Box::new(std::thread::sleep),
        })
    }

    /// Construct from environment (`DRATA_API_BASE_URL`,
    /// `DRATA_API_KEY`).
    ///
    /// # Errors
    ///
    /// - [`DrataClientError::Misconfigured`] when an env var is unset.
    /// - HTTP-builder failure wrapped as `Misconfigured` so the runner
    ///   can short-circuit cleanly without a fresh error variant.
    pub fn from_env() -> Result<Self, DrataClientError> {
        let base = std::env::var("DRATA_API_BASE_URL")
            .map_err(|_| DrataClientError::Misconfigured("DRATA_API_BASE_URL unset".to_string()))?;
        let key = std::env::var("DRATA_API_KEY")
            .map_err(|_| DrataClientError::Misconfigured("DRATA_API_KEY unset".to_string()))?;
        Self::new(base, key).map_err(|e| DrataClientError::Misconfigured(e.to_string()))
    }

    /// Override retry policy + sleeper. Tests pass a no-op sleeper so
    /// they don't wait full backoffs.
    ///
    /// # Errors
    ///
    /// Propagates `reqwest::Error` on HTTP-builder failure.
    pub fn with_overrides<F>(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        retry: RetryPolicy,
        sleeper: F,
    ) -> Result<Self, reqwest::Error>
    where
        F: Fn(Duration) + Send + Sync + 'static,
    {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("corelink-drata-sync/0.1")
            .build()?;
        Ok(Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            retry,
            http,
            sleeper: Box::new(sleeper),
        })
    }

    /// Redacted view of the API key (for diagnostics / audit).
    #[must_use]
    pub fn api_key_redacted(&self) -> String {
        super::redact::redact_api_key(&self.api_key)
    }
}

impl DrataClient for DrataHttpClient {
    fn push(
        &self,
        record: &EvidenceRecord,
        idempotency_key: &str,
    ) -> Result<DrataReceipt, DrataClientError> {
        let url = format!("{}{}", self.base_url, record.stream.endpoint_path());
        let body = record
            .to_canonical_json()
            .map_err(|e| DrataClientError::EncodeFailed(e.to_string()))?;

        let mut attempt: u32 = 0;
        loop {
            let send = self
                .http
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Idempotency-Key", idempotency_key)
                .header("Content-Type", "application/json")
                .body(body.clone())
                .send();

            let status_code: Option<u16> = match &send {
                Ok(resp) => Some(resp.status().as_u16()),
                Err(_) => None,
            };
            let decision = self.retry.decide(status_code, attempt);
            match decision {
                RetryDecision::Success => {
                    let status = status_code.unwrap_or(0);
                    let resp = match send {
                        Ok(r) => r,
                        Err(e) => {
                            return Err(DrataClientError::TransportExhausted {
                                attempts: attempt + 1,
                                reason: e.to_string(),
                            });
                        }
                    };
                    // Parse body for receipt_id; if Drata returns plain
                    // text (e.g. `ok`) we fall back to the idempotency
                    // key so the ledger still has a non-empty receipt.
                    let body_text = resp.text().unwrap_or_default();
                    let receipt_id = serde_json::from_str::<serde_json::Value>(&body_text)
                        .ok()
                        .and_then(|v| {
                            v.get("receipt_id")
                                .and_then(|r| r.as_str().map(str::to_string))
                        })
                        .unwrap_or_else(|| format!("idem-{idempotency_key}"));
                    return Ok(DrataReceipt {
                        receipt_id,
                        http_status: status,
                    });
                }
                RetryDecision::Retry { delay } => {
                    (self.sleeper)(delay);
                    attempt = attempt.saturating_add(1);
                    continue;
                }
                RetryDecision::GiveUp => {
                    let attempts = attempt + 1;
                    let reason = match (status_code, send) {
                        (Some(s), _) => format!("status {s}"),
                        (None, Err(e)) => format!("transport: {e}"),
                        (None, Ok(_)) => "transport: unknown".to_string(),
                    };
                    return Err(match status_code {
                        Some(s) if s == 429 || (500..600).contains(&s) => {
                            DrataClientError::TransportExhausted { attempts, reason }
                        }
                        Some(s) => DrataClientError::PermanentReject { status: s, reason },
                        None => DrataClientError::TransportExhausted { attempts, reason },
                    });
                }
            }
        }
    }
}

/// Recorded push captured by [`InMemoryDrataClient`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedPush {
    /// Stream targeted.
    pub stream: EvidenceStream,
    /// Idempotency key used.
    pub idempotency_key: String,
    /// Record SHA-256 (== idempotency_key in production).
    pub record_sha256: String,
}

/// Behaviour the in-memory client emits per push call.
#[derive(Clone, Debug)]
enum PushBehavior {
    Receipt(DrataReceipt),
    Error(DrataClientError),
}

/// In-memory Drata fake. Tests configure scripted responses; the fake
/// records every push so test code can assert routing + payload.
#[derive(Clone, Debug)]
pub struct InMemoryDrataClient {
    pushes: Arc<Mutex<Vec<RecordedPush>>>,
    scripted: Arc<Mutex<Vec<PushBehavior>>>,
    default_receipt_prefix: String,
}

impl Default for InMemoryDrataClient {
    fn default() -> Self {
        Self {
            pushes: Arc::new(Mutex::new(Vec::new())),
            scripted: Arc::new(Mutex::new(Vec::new())),
            default_receipt_prefix: "rcp".to_string(),
        }
    }
}

impl InMemoryDrataClient {
    /// Empty client with the default receipt prefix `rcp`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a scripted success receipt. Drained in FIFO order.
    pub fn enqueue_receipt(&self, receipt: DrataReceipt) {
        if let Ok(mut g) = self.scripted.lock() {
            g.push(PushBehavior::Receipt(receipt));
        }
    }

    /// Push a scripted error. Drained in FIFO order.
    pub fn enqueue_error(&self, err: DrataClientError) {
        if let Ok(mut g) = self.scripted.lock() {
            g.push(PushBehavior::Error(err));
        }
    }

    /// All pushes observed (in order).
    #[must_use]
    pub fn pushes(&self) -> Vec<RecordedPush> {
        match self.pushes.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Convenience — count of pushes.
    #[must_use]
    pub fn push_count(&self) -> usize {
        self.pushes().len()
    }
}

impl DrataClient for InMemoryDrataClient {
    fn push(
        &self,
        record: &EvidenceRecord,
        idempotency_key: &str,
    ) -> Result<DrataReceipt, DrataClientError> {
        if let Ok(mut g) = self.pushes.lock() {
            g.push(RecordedPush {
                stream: record.stream,
                idempotency_key: idempotency_key.to_string(),
                record_sha256: idempotency_key.to_string(),
            });
        }
        let behavior = {
            let mut g = self
                .scripted
                .lock()
                .map_err(|e| DrataClientError::Misconfigured(format!("mutex poisoned: {e}")))?;
            if g.is_empty() {
                PushBehavior::Receipt(DrataReceipt {
                    receipt_id: format!("{}-{}", self.default_receipt_prefix, idempotency_key),
                    http_status: 200,
                })
            } else {
                g.remove(0)
            }
        };
        match behavior {
            PushBehavior::Receipt(r) => Ok(r),
            PushBehavior::Error(e) => Err(e),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::super::record::record_sha256;
    use super::*;

    fn rec() -> EvidenceRecord {
        EvidenceRecord::new(EvidenceStream::AuditLogs, "src#1", 10, [("k", "v")])
    }

    #[test]
    fn fake_default_returns_synthetic_receipt() {
        let c = InMemoryDrataClient::new();
        let r = rec();
        let key = record_sha256(&r).unwrap();
        let receipt = c.push(&r, &key).unwrap();
        assert_eq!(receipt.http_status, 200);
        assert!(receipt.receipt_id.starts_with("rcp-"));
        assert_eq!(c.push_count(), 1);
    }

    #[test]
    fn fake_scripted_receipt_drains_fifo() {
        let c = InMemoryDrataClient::new();
        c.enqueue_receipt(DrataReceipt {
            receipt_id: "one".into(),
            http_status: 201,
        });
        c.enqueue_receipt(DrataReceipt {
            receipt_id: "two".into(),
            http_status: 202,
        });
        let r = rec();
        let key = record_sha256(&r).unwrap();
        assert_eq!(c.push(&r, &key).unwrap().receipt_id, "one");
        assert_eq!(c.push(&r, &key).unwrap().receipt_id, "two");
    }

    #[test]
    fn fake_scripted_error_propagates() {
        let c = InMemoryDrataClient::new();
        c.enqueue_error(DrataClientError::PermanentReject {
            status: 403,
            reason: "forbidden".into(),
        });
        let r = rec();
        let key = record_sha256(&r).unwrap();
        let err = c.push(&r, &key).unwrap_err();
        match err {
            DrataClientError::PermanentReject { status, .. } => assert_eq!(status, 403),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
