//! Canonical digest and ByteStream resource-name validation.

use sha2::{Digest as _, Sha256};
use tonic::{Code, Status};

/// Compute the canonical REAPI SHA-256 digest string for a byte sequence.
#[must_use]
pub fn sha256_digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Validate a canonical lowercase SHA-256 digest and non-negative size.
pub fn validate_digest(hash: &str, size_bytes: i64) -> Result<(), Status> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || size_bytes < 0
    {
        return Err(Status::new(Code::InvalidArgument, "invalid REAPI digest"));
    }
    Ok(())
}

/// Validate an REAPI instance against the PAT-derived tenant namespace.
pub fn validate_instance_name(instance_name: &str, tenant_id: &str) -> Result<(), Status> {
    if instance_name.is_empty()
        || instance_name.starts_with('_')
        || tenant_id.starts_with('_')
        || crate::auth_tenant::is_reserved_sentinel(instance_name)
        || crate::auth_tenant::is_reserved_sentinel(tenant_id)
        || instance_name != tenant_id
    {
        return Err(Status::new(
            Code::PermissionDenied,
            "REAPI instance is outside the authenticated tenant",
        ));
    }
    Ok(())
}

/// Fully validated REAPI ByteStream blob resource name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedBlobResourceName {
    instance_name: String,
    hash: String,
    size_bytes: i64,
    upload_id: Option<String>,
}

impl ValidatedBlobResourceName {
    /// Tenant-matched instance name.
    #[must_use]
    pub fn instance_name(&self) -> &str {
        &self.instance_name
    }

    /// Canonical lowercase SHA-256 digest.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Declared non-negative blob length.
    #[must_use]
    pub const fn size_bytes(&self) -> i64 {
        self.size_bytes
    }

    /// Upload UUID for a ByteStream write resource, or `None` for a read.
    #[must_use]
    pub fn upload_id(&self) -> Option<&str> {
        self.upload_id.as_deref()
    }
}

/// Parse and validate a REAPI ByteStream blob resource name for an already
/// authenticated tenant. Accepted forms are `<instance>/blobs/<sha256>/<size>`
/// and `<instance>/uploads/<uuid>/blobs/<sha256>/<size>`.
pub fn validate_blob_resource_name(
    resource_name: &str,
    tenant_id: &str,
) -> Result<ValidatedBlobResourceName, Status> {
    let parts: Vec<&str> = resource_name.split('/').collect();
    let (instance, upload_id, hash, size) = match parts.as_slice() {
        [instance, "blobs", hash, size] => (*instance, None, *hash, *size),
        [instance, "uploads", upload_id, "blobs", hash, size] => {
            let parsed = uuid::Uuid::parse_str(upload_id)
                .map_err(|_| Status::new(Code::InvalidArgument, "invalid REAPI resource name"))?;
            if parsed.to_string() != *upload_id {
                return Err(Status::new(
                    Code::InvalidArgument,
                    "invalid REAPI resource name",
                ));
            }
            (*instance, Some(*upload_id), *hash, *size)
        }
        _ => {
            return Err(Status::new(
                Code::InvalidArgument,
                "invalid REAPI resource name",
            ))
        }
    };
    validate_instance_name(instance, tenant_id)?;
    let size_bytes = size
        .parse::<i64>()
        .map_err(|_| Status::new(Code::InvalidArgument, "invalid REAPI resource name"))?;
    validate_digest(hash, size_bytes)?;
    Ok(ValidatedBlobResourceName {
        instance_name: instance.to_owned(),
        hash: hash.to_owned(),
        size_bytes,
        upload_id: upload_id.map(str::to_owned),
    })
}
