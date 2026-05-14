//! Notification sink stub. The production wiring delivers the receipt
//! via the existing transactional email pipeline (SES per S-11).

use std::sync::Mutex;

use crate::error::DpaAcceptanceError;
use crate::schema::TenantId;

/// One notification envelope captured by the sink.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationEnvelope {
    /// Recipient tenant.
    pub tenant_id: TenantId,
    /// JWT receipt enclosed in the email body / attachment.
    pub jwt_receipt: String,
    /// JWT `jti` — surfaced for test assertions / dedupe.
    pub jti: String,
}

/// Sink delivering DPA acceptance receipts (existing notification path
/// stub).
pub trait NotificationSink: Send + Sync + std::fmt::Debug {
    /// Send one notification.
    ///
    /// # Errors
    ///
    /// Returns [`DpaAcceptanceError::Notify`] on sink failure. The
    /// orchestrator currently surfaces the error but does **not** roll
    /// back the persisted record — the receipt is returned inline so
    /// the client can retry the email path out-of-band.
    fn send(&self, envelope: NotificationEnvelope) -> Result<(), DpaAcceptanceError>;
}

/// In-memory capture sink for tests.
#[derive(Debug, Default)]
pub struct InMemoryNotificationSink {
    sent: Mutex<Vec<NotificationEnvelope>>,
}

impl InMemoryNotificationSink {
    /// Build an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot all delivered envelopes.
    #[must_use]
    pub fn snapshot(&self) -> Vec<NotificationEnvelope> {
        self.sent.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Count of delivered envelopes.
    #[must_use]
    pub fn count(&self) -> usize {
        self.sent.lock().map(|g| g.len()).unwrap_or_default()
    }
}

impl NotificationSink for InMemoryNotificationSink {
    fn send(&self, envelope: NotificationEnvelope) -> Result<(), DpaAcceptanceError> {
        let mut guard = self
            .sent
            .lock()
            .map_err(|e| DpaAcceptanceError::Notify(format!("poisoned: {}", e)))?;
        guard.push(envelope);
        Ok(())
    }
}
