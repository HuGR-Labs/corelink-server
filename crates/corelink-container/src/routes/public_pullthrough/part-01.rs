impl UpstreamManifestResolver {
    /// Persist the manifest bytes + companion content-type slot under the
    /// request `reference` and, when `reference` is a tag (differs from the
    /// canonical digest), ALSO under the by-digest slot. Fail-open: any KV put
    /// failure ⇒ `false`. Kept out of the trait method so the persistence policy
    /// reads in one place.
    async fn persist_manifest(
        &self,
        tenant: &TenantId,
        repo: &str,
        reference: &str,
        digest_wire: &str,
        content_type: &str,
        body: &Bytes,
    ) -> bool {
        // The requested reference (tag or digest).
        if self
            .kv
            .put(tenant, &manifest_key(repo, reference), body.clone())
            .await
            .is_err()
        {
            return false;
        }
        if self
            .kv
            .put(
                tenant,
                &manifest_ct_key(repo, reference),
                Bytes::from(content_type.to_owned()),
            )
            .await
            .is_err()
        {
            return false;
        }
        // For a tag, also index by the resolved digest so a later
        // `GET manifests/<digest>` hits the per-tenant KV directly.
        if reference != digest_wire {
            if self
                .kv
                .put(tenant, &manifest_key(repo, digest_wire), body.clone())
                .await
                .is_err()
            {
                return false;
            }
            if self
                .kv
                .put(
                    tenant,
                    &manifest_ct_key(repo, digest_wire),
                    Bytes::from(content_type.to_owned()),
                )
                .await
                .is_err()
            {
                return false;
            }
        }
        true
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
    include!("tests-00-00.rs");
    include!("fragment-tests-00-00-01.rs");
    include!("tests-00-01.rs");
}
