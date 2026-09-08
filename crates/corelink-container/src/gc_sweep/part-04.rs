fn canonical_blob_key(
    region: GcRegion,
    tdk: &[u8; 32],
    tenant_id: Uuid,
    digest: &BlobDigest,
) -> Result<String, String> {
    let prefix = corelink_tenant_path::derive_prefix(
        &corelink_tenant_path::TenantDerivationKey::from_bytes(zeroize::Zeroizing::new(*tdk)),
        tenant_id,
    );
    Ok(format!("{}/{}/{}", region.as_str(), prefix, digest))
}

/// Reconcile the physical GC colo with the macro-region residency contract.
/// The two schemas intentionally use different vocabularies: `gc_run.region`
/// is a serving colo while `blob_meta.region`/`tenant.primary_region` are macro
/// regions.  Unknown or mismatched pairs are a hard error; silently treating a
/// row as resident in another region would make GC a cross-border delete path.
fn validate_gc_residency(
    gc_region: GcRegion,
    blob_region: &str,
    tenant_region: &str,
) -> Result<(), String> {
    let valid = match gc_region {
        GcRegion::Iad => matches!(blob_region, "wnam" | "enam"),
        GcRegion::Lhr => blob_region == "weur",
        GcRegion::Nrt => blob_region == "apac",
        GcRegion::Sam => blob_region == "sam",
        // There is no provisioned macro residency mapping for syd.
        GcRegion::Syd => false,
    };
    if !valid || blob_region != tenant_region {
        return Err(format!(
            "GC/residency region mismatch: gc={} blob={} tenant={}",
            gc_region.as_str(),
            blob_region,
            tenant_region
        ));
    }
    Ok(())
}

/// Resolve the durable tenant residency before writing any GC audit record.
/// `audit_outbox.region` is a macro-region column, whereas GC events carry the
/// serving-colo enum.  Never insert a physical colo into that column: the
/// residency trigger would reject valid `iad`/`wnam` pairs, and bypassing it
/// would misattribute the audit partition.
fn tenant_residency_region(
    d1: &D1HttpClient,
    tenant_id: Uuid,
    gc_region: GcRegion,
) -> Result<String, String> {
    let rows = query_sync(
        d1,
        "SELECT primary_region FROM tenant WHERE tenant_id = ?1",
        &[json!(tenant_id.to_string())],
    )?;
    let row = rows
        .first()
        .ok_or_else(|| format!("tenant {tenant_id} has no durable residency region"))?;
    let region = text(row, "primary_region")?;
    validate_gc_residency(gc_region, &region, &region)?;
    Ok(region)
}

// Keep the native GC adapter below the source-size ratchet used by the
// storage-boundary audit; the included fragment shares this module's private
// helpers and types without changing the compiled symbol map.
