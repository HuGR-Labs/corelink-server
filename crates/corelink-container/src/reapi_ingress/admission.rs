//! Shared fail-closed quota and concurrency admission.

use std::sync::Arc;

use super::{AdmissionFailure, AdmissionLease, IngressAdmission, REAPI_INGRESS_CONCURRENCY_LIMIT};
use async_trait::async_trait;

/// Quota and bounded concurrency admission wired by the route factory.
#[derive(Debug)]
pub struct QuotaConcurrencyAdmission {
    quota: crate::routes::QuotaGate,
    permits: Arc<tokio::sync::Semaphore>,
}

impl QuotaConcurrencyAdmission {
    /// Build one process-local concurrency limit around the existing D1 quota.
    #[must_use]
    pub fn new(quota: crate::routes::QuotaGate) -> Self {
        Self {
            quota,
            permits: Arc::new(tokio::sync::Semaphore::new(REAPI_INGRESS_CONCURRENCY_LIMIT)),
        }
    }
}

#[async_trait]
impl IngressAdmission for QuotaConcurrencyAdmission {
    async fn admit(&self, tenant_id: &str) -> Result<AdmissionLease, AdmissionFailure> {
        let permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|error| match error {
                tokio::sync::TryAcquireError::NoPermits => AdmissionFailure::Exhausted,
                tokio::sync::TryAcquireError::Closed => AdmissionFailure::Unavailable,
            })?;
        if let Some(response) = self.quota.check(tenant_id).await {
            drop(permit);
            return Err(match response.status() {
                axum::http::StatusCode::PAYMENT_REQUIRED
                | axum::http::StatusCode::TOO_MANY_REQUESTS => AdmissionFailure::Exhausted,
                _ => AdmissionFailure::Unavailable,
            });
        }
        Ok(AdmissionLease::new(permit))
    }
}
