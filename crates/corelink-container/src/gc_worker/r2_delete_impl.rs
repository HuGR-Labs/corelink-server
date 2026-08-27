//! Real R2-backed [`R2Delete`] implementation for production physical_delete phase.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use corelink_gc::physical_delete::{R2Delete, PhysicalDeleteError};
use crate::storage::r2_s3::R2S3Client;

/// R2-backed delete implementation — real production implementation.
#[derive(Clone, Debug)]
pub struct R2DeleteImpl {
    client: Arc<R2S3Client>,
}

impl R2DeleteImpl {
    #[must_use]
    pub fn new(client: Arc<R2S3Client>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl corelink_gc::physical_delete::R2Delete for R2DeleteImpl {
    async fn delete(
        &self,
        tenant_id: Uuid,
        digest: &corelink_gc::physical_delete::BlobDigest,
        region: corelink_gc::region::GcRegion,
    ) -> Result<(), corelink_gc::physical_delete::PhysicalDeleteError> {
        // Build the R2 object key: tenant/<prefix>/cas/<digest>
        let tenant_prefix = crate::storage::r2_s3::derive_prefix_for_tenant(&tenant_id);
        let key = format!("{}/{}/cas/{}", region.as_str(), tenant_prefix, digest.as_str());

        self.client
            .delete_object(&key)
            .await
            .map_err(|e| corelink_gc::physical_delete::PhysicalDeleteError::Backend(e.to_string()))?;

        Ok(())
    }
}