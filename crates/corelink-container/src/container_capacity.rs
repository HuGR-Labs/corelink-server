//! Capacity and memory-budget truth for the deployed cache container.
//!
//! The production container is the Cloudflare `basic` shape: 1 GiB of memory
//! and 0.25 vCPU.  Keep the measurement and every process-wide memory slice in
//! this module so a resize cannot leave one route family on stale capacity
//! arithmetic.  The individual route modules still own their semaphores and
//! tenant fairness rules; this module owns their shared envelope.

#![forbid(unsafe_code)]

/// Deployed Cloudflare Containers memory, measured for all production regions.
pub const CONTAINER_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;

/// Deployed Cloudflare Containers CPU in milli-vCPU (0.25 vCPU).
pub const CONTAINER_VCPU_MILLICORES: u64 = 250;

/// One memory-budget unit.  All weighted reservations use whole MiB units.
pub const MEMORY_BUDGET_UNIT_BYTES: u64 = 1024 * 1024;

/// CAS process-wide read slice. One 64 MiB object may have three live copies
/// (SDK bytes, handler `Vec`, and BYOK plaintext). The remaining read envelope
/// covers one maximum batch request body, bounded parser metadata, and its
/// streamed response, so these request allocations cannot sit outside the
/// BYOK-inclusive process cap.
pub const CAS_READ_GLOBAL_BUDGET_BYTES: u64 = 220 * MEMORY_BUDGET_UNIT_BYTES;

/// Maximum simultaneously-live copies of one CAS read. Plain reads retain the
/// SDK buffer and handler `Vec`; BYOK additionally retains plaintext beside
/// ciphertext, making three the conservative bound.
pub const CAS_READ_COPY_MULTIPLIER: u64 = 3;
/// Maximum request body admitted by the global axum body limit.
pub const CAS_WRITE_SINGLE_BODY_LIMIT_BYTES: u64 = 10 * MEMORY_BUDGET_UNIT_BYTES;
/// The global body limit also applies to the read-side NDJSON batch request.
pub const CAS_READ_BATCH_BODY_LIMIT_BYTES: u64 = 10 * MEMORY_BUDGET_UNIT_BYTES;
/// Bounded entries, strings, and request clones retained while parsing either
/// batch direction. The route-level line/hash caps keep the live metadata under
/// this reservation before any storage work begins.
pub const CAS_BATCH_PARSE_METADATA_BYTES: u64 = 4 * MEMORY_BUDGET_UNIT_BYTES;
/// Maximum object payload admitted by a CAS batch. The request also carries
/// manifest framing, but the global 10 MiB body limit bounds that overhead.
pub const CAS_WRITE_BATCH_PAYLOAD_LIMIT_BYTES: u64 = 8 * MEMORY_BUDGET_UNIT_BYTES;
/// Peak read-side batch reservation: body + parsed strings/clones + response.
pub const CAS_READ_BATCH_PEAK_BYTES: u64 = CAS_READ_BATCH_BODY_LIMIT_BYTES
    + CAS_BATCH_PARSE_METADATA_BYTES
    + CAS_WRITE_BATCH_PAYLOAD_LIMIT_BYTES;
/// Peak bytes for one single-object read, including the worst BYOK copies and
/// bounded request metadata.
pub const CAS_READ_SINGLE_PEAK_BYTES: u64 =
    CAS_READ_COPY_MULTIPLIER * (64 * MEMORY_BUDGET_UNIT_BYTES) + CAS_BATCH_PARSE_METADATA_BYTES;
/// A single PUT retains the SDK `Bytes` body while `body.to_vec()` is passed to
/// the handler, so reserve two body-sized copies before buffering.
pub const CAS_WRITE_SINGLE_PEAK_BYTES: u64 = 2 * CAS_WRITE_SINGLE_BODY_LIMIT_BYTES;
/// A batch retains the ≤10 MiB request body while each object is copied into a
/// storage request. The complete payload cap is 8 MiB, so 10 + 8 MiB is the
/// conservative pre-storage peak for one batch; parser metadata adds a bounded
/// 4 MiB reservation while the batch is being decoded.
pub const CAS_WRITE_BATCH_PEAK_BYTES: u64 = CAS_WRITE_SINGLE_BODY_LIMIT_BYTES
    + CAS_WRITE_BATCH_PAYLOAD_LIMIT_BYTES
    + CAS_BATCH_PARSE_METADATA_BYTES;

/// CAS process-wide write slice. Single and batch requests draw from the same
/// 38 MiB pool, so their weighted guards prevent their complete peaks from
/// coexisting when their sum would exceed the deployed contract.
pub const CAS_WRITE_GLOBAL_BUDGET_BYTES: u64 = 38 * MEMORY_BUDGET_UNIT_BYTES;

/// Argon2id process-wide working-set slice.  At the production m-cost of 64
/// MiB this admits one concurrent verification.
pub const ARGON2_MEMORY_BYTES: u64 = 64 * MEMORY_BUDGET_UNIT_BYTES;
/// Number of process-wide Argon2id permits derived from the memory slice.
pub const ARGON2_VERIFY_PERMITS: usize =
    (ARGON2_MEMORY_BUDGET_BYTES / ARGON2_MEMORY_BYTES) as usize;

/// Argon2id's reserved process-wide slice.
pub const ARGON2_MEMORY_BUDGET_BYTES: u64 = 64 * MEMORY_BUDGET_UNIT_BYTES;

/// Turbo artifact size and process-wide slices. PUT reserves 2x its body cap
/// because the handler's transient `to_vec()` clone can coexist with the body.
pub const TURBO_ARTIFACT_BYTES: u64 = 100 * MEMORY_BUDGET_UNIT_BYTES;
/// Turbo PUT process-wide memory slice.
pub const TURBO_PUT_MEMORY_BUDGET_BYTES: u64 = 200 * MEMORY_BUDGET_UNIT_BYTES;
/// Turbo GET process-wide memory slice.
pub const TURBO_GET_MEMORY_BUDGET_BYTES: u64 = 104 * MEMORY_BUDGET_UNIT_BYTES;
/// Number of process-wide Turbo PUT permits.
pub const TURBO_PUT_GLOBAL_PERMITS: usize =
    (TURBO_PUT_MEMORY_BUDGET_BYTES / (2 * TURBO_ARTIFACT_BYTES)) as usize;
/// Number of process-wide Turbo GET permits.
pub const TURBO_GET_GLOBAL_PERMITS: usize =
    (TURBO_GET_MEMORY_BUDGET_BYTES / TURBO_ARTIFACT_BYTES) as usize;

/// Small dedicated telemetry slice, separate from Turbo artifact traffic.
pub const TURBO_EVENTS_MEMORY_BUDGET_BYTES: u64 = 8 * MEMORY_BUDGET_UNIT_BYTES;
/// Number of process-wide Turbo events permits.
pub const TURBO_EVENTS_GLOBAL_PERMITS: usize =
    (TURBO_EVENTS_MEMORY_BUDGET_BYTES / (64 * 1024)) as usize;

/// Memory intentionally left for the allocator, runtime, protocol framing,
/// and non-buffering route work.  It is part of the checked envelope.
pub const RUNTIME_MEMORY_RESERVE_BYTES: u64 = 128 * MEMORY_BUDGET_UNIT_BYTES;

/// Fixed bit-array bytes for the bounded 2048-tenant Bloom cache.
pub const BLOOM_CACHE_BIT_ARRAY_BYTES: u64 = 256 * MEMORY_BUDGET_UNIT_BYTES;
/// Bounded tenant-map metadata allowance (tenant key, entry, and allocator
/// bookkeeping). Tenant identifiers are bounded at the authenticated edge;
/// this fixed allowance keeps metadata out of the unaccounted heap.
pub const BLOOM_CACHE_METADATA_BYTES: u64 = 2 * MEMORY_BUDGET_UNIT_BYTES;
/// Total Bloom cache slice charged to the process-wide declaration.
pub const BLOOM_CACHE_MEMORY_BUDGET_BYTES: u64 =
    BLOOM_CACHE_BIT_ARRAY_BYTES + BLOOM_CACHE_METADATA_BYTES;

/// Sum of the named process-wide working-set slices.
pub const DECLARED_MEMORY_BUDGET_BYTES: u64 = CAS_READ_GLOBAL_BUDGET_BYTES
    + CAS_WRITE_GLOBAL_BUDGET_BYTES
    + ARGON2_MEMORY_BUDGET_BYTES
    + TURBO_PUT_MEMORY_BUDGET_BYTES
    + TURBO_GET_MEMORY_BUDGET_BYTES
    + TURBO_EVENTS_MEMORY_BUDGET_BYTES
    + BLOOM_CACHE_MEMORY_BUDGET_BYTES;

/// Validate a capacity declaration without reading mutable process state.
/// Kept as a function so startup and adversarial tests exercise the same rule.
pub const fn budget_fits(
    memory_bytes: u64,
    declared_bytes: u64,
    reserve_bytes: u64,
    vcpu_millicores: u64,
) -> bool {
    memory_bytes > 0
        && vcpu_millicores > 0
        && reserve_bytes <= memory_bytes
        && declared_bytes <= memory_bytes - reserve_bytes
}

/// Runtime fail-closed check for the compiled capacity declaration.
pub fn validate_runtime_budget() -> Result<(), &'static str> {
    if CONTAINER_MEMORY_BYTES != 1024 * 1024 * 1024 {
        return Err("container memory declaration is not the deployed basic shape");
    }
    if CONTAINER_VCPU_MILLICORES != 250 {
        return Err("container CPU declaration is not the deployed basic shape");
    }
    if !budget_fits(
        CONTAINER_MEMORY_BYTES,
        DECLARED_MEMORY_BUDGET_BYTES,
        RUNTIME_MEMORY_RESERVE_BYTES,
        CONTAINER_VCPU_MILLICORES,
    ) {
        return Err("declared process-wide memory budgets exceed container capacity");
    }
    if ARGON2_VERIFY_PERMITS == 0
        || CAS_READ_GLOBAL_BUDGET_BYTES < CAS_READ_SINGLE_PEAK_BYTES + CAS_READ_BATCH_PEAK_BYTES
        || CAS_WRITE_GLOBAL_BUDGET_BYTES < CAS_WRITE_SINGLE_PEAK_BYTES
        || CAS_WRITE_GLOBAL_BUDGET_BYTES < CAS_WRITE_BATCH_PEAK_BYTES
        || BLOOM_CACHE_MEMORY_BUDGET_BYTES < BLOOM_CACHE_BIT_ARRAY_BYTES
        || TURBO_PUT_GLOBAL_PERMITS == 0
        || TURBO_GET_GLOBAL_PERMITS == 0
        || TURBO_EVENTS_GLOBAL_PERMITS == 0
    {
        return Err("a process-wide memory pool has no usable permits");
    }
    Ok(())
}

const _: () = assert!(CONTAINER_MEMORY_BYTES == 1024 * 1024 * 1024);
const _: () = assert!(CONTAINER_VCPU_MILLICORES == 250);
const _: () = assert!(CAS_READ_GLOBAL_BUDGET_BYTES % MEMORY_BUDGET_UNIT_BYTES == 0);
const _: () = assert!(CAS_WRITE_GLOBAL_BUDGET_BYTES % MEMORY_BUDGET_UNIT_BYTES == 0);
const _: () =
    assert!(CAS_READ_GLOBAL_BUDGET_BYTES >= CAS_READ_SINGLE_PEAK_BYTES + CAS_READ_BATCH_PEAK_BYTES);
const _: () = assert!(CAS_WRITE_GLOBAL_BUDGET_BYTES >= CAS_WRITE_SINGLE_PEAK_BYTES);
const _: () = assert!(CAS_WRITE_GLOBAL_BUDGET_BYTES >= CAS_WRITE_BATCH_PEAK_BYTES);
const _: () = assert!(BLOOM_CACHE_BIT_ARRAY_BYTES == 2048 * 128 * 1024);
const _: () = assert!(BLOOM_CACHE_MEMORY_BUDGET_BYTES >= BLOOM_CACHE_BIT_ARRAY_BYTES);
const _: () = assert!(ARGON2_MEMORY_BUDGET_BYTES % ARGON2_MEMORY_BYTES == 0);
const _: () = assert!(TURBO_PUT_GLOBAL_PERMITS > 0 && TURBO_GET_GLOBAL_PERMITS > 0);
const _: () = assert!(TURBO_EVENTS_GLOBAL_PERMITS > 0);
const _: () = assert!(budget_fits(
    CONTAINER_MEMORY_BYTES,
    DECLARED_MEMORY_BUDGET_BYTES,
    RUNTIME_MEMORY_RESERVE_BYTES,
    CONTAINER_VCPU_MILLICORES,
));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployed_budget_fits_with_runtime_reserve() {
        assert!(validate_runtime_budget().is_ok());
        assert_eq!(CONTAINER_MEMORY_BYTES, 1024 * 1024 * 1024);
        assert_eq!(CONTAINER_VCPU_MILLICORES, 250);
        assert_eq!(BLOOM_CACHE_BIT_ARRAY_BYTES, 256 * MEMORY_BUDGET_UNIT_BYTES);
        assert_eq!(BLOOM_CACHE_METADATA_BYTES, 2 * MEMORY_BUDGET_UNIT_BYTES);
        assert_eq!(
            BLOOM_CACHE_MEMORY_BUDGET_BYTES,
            258 * MEMORY_BUDGET_UNIT_BYTES
        );
        let declared_bytes = DECLARED_MEMORY_BUDGET_BYTES;
        let reserve_bytes = RUNTIME_MEMORY_RESERVE_BYTES;
        assert!(budget_fits(
            CONTAINER_MEMORY_BYTES,
            declared_bytes,
            reserve_bytes,
            CONTAINER_VCPU_MILLICORES,
        ));
    }

    #[test]
    fn over_budget_mutation_is_rejected() {
        assert!(!budget_fits(
            CONTAINER_MEMORY_BYTES,
            CONTAINER_MEMORY_BYTES,
            RUNTIME_MEMORY_RESERVE_BYTES,
            CONTAINER_VCPU_MILLICORES,
        ));
        assert!(!budget_fits(
            CONTAINER_MEMORY_BYTES,
            DECLARED_MEMORY_BUDGET_BYTES,
            CONTAINER_MEMORY_BYTES + 1,
            CONTAINER_VCPU_MILLICORES,
        ));
    }

    #[test]
    fn plaintext_read_peak_is_bounded() {
        let object_bytes = 64 * MEMORY_BUDGET_UNIT_BYTES;
        assert_eq!(object_bytes * 2, 128 * MEMORY_BUDGET_UNIT_BYTES);
        assert!(object_bytes * 2 <= CAS_READ_GLOBAL_BUDGET_BYTES);
    }

    #[test]
    fn byok_read_peak_is_bounded() {
        let object_bytes = 64 * MEMORY_BUDGET_UNIT_BYTES;
        assert_eq!(
            object_bytes * CAS_READ_COPY_MULTIPLIER,
            192 * MEMORY_BUDGET_UNIT_BYTES
        );
        assert!(
            object_bytes * CAS_READ_COPY_MULTIPLIER + CAS_BATCH_PARSE_METADATA_BYTES
                <= CAS_READ_GLOBAL_BUDGET_BYTES
        );
    }

    #[test]
    fn batch_request_parse_peaks_are_inside_the_read_and_write_slices() {
        assert_eq!(CAS_READ_BATCH_PEAK_BYTES, 22 * MEMORY_BUDGET_UNIT_BYTES);
        assert_eq!(CAS_WRITE_BATCH_PEAK_BYTES, 22 * MEMORY_BUDGET_UNIT_BYTES);
        assert_eq!(CAS_READ_SINGLE_PEAK_BYTES, 196 * MEMORY_BUDGET_UNIT_BYTES);
        let read_peak_bytes = CAS_READ_SINGLE_PEAK_BYTES + CAS_READ_BATCH_PEAK_BYTES;
        assert!(budget_fits(
            CAS_READ_GLOBAL_BUDGET_BYTES,
            read_peak_bytes,
            0,
            CONTAINER_VCPU_MILLICORES,
        ));
    }

    #[test]
    fn write_peaks_are_reserved_before_body_buffering() {
        assert_eq!(CAS_WRITE_SINGLE_PEAK_BYTES, 20 * MEMORY_BUDGET_UNIT_BYTES);
        assert_eq!(CAS_WRITE_BATCH_PEAK_BYTES, 22 * MEMORY_BUDGET_UNIT_BYTES);
        assert_eq!(CAS_WRITE_GLOBAL_BUDGET_BYTES, 38 * MEMORY_BUDGET_UNIT_BYTES);
        assert!(
            CAS_WRITE_SINGLE_PEAK_BYTES.max(CAS_WRITE_BATCH_PEAK_BYTES)
                <= CAS_WRITE_GLOBAL_BUDGET_BYTES
        );
        assert_eq!(BLOOM_CACHE_BIT_ARRAY_BYTES, 2048 * 128 * 1024);
    }
}
