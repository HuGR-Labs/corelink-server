//! `BatchReadBlobs` two-pass pipeline — module-private types + phase
//! helpers 4a / 4b / 4c / 4d. Step 4e (response composition) lives in
//! the sibling [`super::batch_read_compose`] module so each file stays
//! within the L2.10 file-size discipline (≤ 500 LOC HARD CAP).
//!
//! Per WI-S02-002 + ADR-0028 the two-pass design is memory-bounded by
//! construction:
//!
//!   Pass 1: parallel D1 lookups for `(presence, size_bytes,
//!           declared-size-cross-check)`. NO body bytes materialised.
//!   Pass 2: parallel R2 GET only for slots that fit (single-blob cap +
//!           running aggregate cap). Per slot we additionally cross-
//!           check `body.len() == size_bytes` (R2 corruption guard).

use bytes::Bytes;
use corelink_hash::Digest;
use corelink_meta::MetaStore;
use corelink_cas::r2_storage::R2Backend;
use corelink_worker::TenantCtx as StorageTenantCtx;
use tonic::{Code, Status};
use uuid::Uuid;

use crate::capabilities::MAX_BATCH_TOTAL_SIZE_BYTES;
use crate::error_map::{
    MetaErrorMapping, R2ErrorMapping, COR_CAS_BATCH_SIZE_EXCEEDED,
    COR_CAS_DIGEST_FUNCTION_UNSUPPORTED, GRPC_INVALID_ARGUMENT,
};
use crate::find_missing::MAX_FIND_MISSING_BATCH_SIZE;
use crate::proto::reapi::{BatchReadBlobsRequest, Digest as ProtoDigest};
use crate::read::MissReason;

use super::helpers::make_status;

// ---------------------------------------------------------------------------
// `batch_read_blobs` internal types (Wave 33 Stream A2.1a hoist)
//
// Hoisted from inside `batch_read_blobs` to module scope so the function can
// be decomposed into per-phase helpers without each helper carrying nested
// type definitions or visibility gymnastics. All remain module-private.
// ---------------------------------------------------------------------------

/// 4a — per-input-slot pre-validation outcome (digest hex + size_bytes).
#[derive(Clone)]
pub(super) enum SlotState {
    /// Digest hex was malformed; surface INVALID_ARGUMENT per-blob.
    BadDigest {
        proto_digest: Option<ProtoDigest>,
        message: String,
    },
    /// Well-formed; needs the D1 metadata pass.
    Pending {
        proto_digest: ProtoDigest,
        digest: Digest,
    },
}

/// 4b — per-row D1 lookup outcome. Per ADR-0028 the wire response for
/// every miss arm is uniform `NOT_FOUND`, but the audit-emit envelope MUST
/// disambiguate NeverExisted (low-severity) from Tombstoned (legitimate
/// post-GC) so the S-09 forensics pipeline can distinguish ordinary
/// cache-miss probes from genuine tombstone-read attempts.
#[derive(Clone, Copy, Debug)]
pub(super) enum MetaPass1Result {
    /// Row absent OR exists in another tenant's scope (uniform per
    /// ADR-0028 — no oracle).
    NeverExisted,
    /// Row exists but `deleted_at` is set.
    Tombstoned,
    /// Row exists + alive; size_bytes is the canonical size.
    Alive { size_bytes: u64 },
}

/// 4c — per-slot decision after the D1 pass; drives which entries fall
/// into Pass 2's R2 fetch + composition phases.
#[derive(Clone)]
pub(super) enum Decision {
    BadDigest {
        proto_digest: Option<ProtoDigest>,
        message: String,
        grpc_code: i32,
    },
    Miss {
        proto_digest: ProtoDigest,
        digest: Digest,
        reason: MissReason,
    },
    ExceedsSingleCap {
        proto_digest: ProtoDigest,
    },
    ExceedsAggregate {
        proto_digest: ProtoDigest,
    },
    CallerSizeMismatch {
        proto_digest: ProtoDigest,
        row_size: u64,
    },
    MetaTransport {
        proto_digest: ProtoDigest,
        mapping: crate::error_map::ErrorMapping,
    },
    Fetch {
        proto_digest: ProtoDigest,
        digest: Digest,
        row_size: u64,
    },
}

/// 4d — Pass 2 R2 fetch outcome. Typed enum so the orphan condition
/// (alive blob_meta + `R2Error::NotFound`) is structurally distinguishable
/// from generic Err mappings (codex round-3 P2 SEAL fix).
#[derive(Clone)]
pub(super) enum FetchOutcome {
    Body(Bytes),
    R2Orphan,
    Other(crate::error_map::ErrorMapping),
}

// ---------------------------------------------------------------------------
// Phase helpers (Wave 33 Stream A2.1b extraction)
// ---------------------------------------------------------------------------

/// Step 2 + 2b + 3 — request-level pre-validation.
///
/// Returns the canonical inline-cap + single-blob inline-cap (4 MiB) and
/// the moved `digests` vec on success; surfaces top-level
/// `INVALID_ARGUMENT` / `FAILED_PRECONDITION` / `OUT_OF_RANGE` statuses on
/// digest-function negotiation / `acceptable_compressors` / batch-size
/// breaches respectively.
pub(super) fn batch_read_validate_request(
    inner: BatchReadBlobsRequest,
) -> Result<(Vec<ProtoDigest>, u64, u64), Status> {
    // Step 2 — digest-function negotiation (BLAKE3 only; same contract as
    // BatchUpdateBlobs).
    if inner.digest_function != 0
        && inner.digest_function != crate::capabilities::DigestFunction::Blake3 as i32
    {
        return Err(make_status(
            Code::InvalidArgument,
            COR_CAS_DIGEST_FUNCTION_UNSUPPORTED,
            "BatchReadBlobsRequest.digest_function declares a non-BLAKE3 hash; CoreLink advertises BLAKE3 only via Capabilities.GetCapabilities",
            0,
        ));
    }

    // Step 2b — `acceptable_compressors` honor (codex round-2 P2 SEAL fix).
    // REAPI v2 §`BatchReadBlobsRequest`: an empty `acceptable_compressors`
    // list MUST be treated as "client accepts the default IDENTITY
    // encoding"; a non-empty list restricts the server's permitted
    // response encodings. CoreLink S-01 advertises IDENTITY only via
    // `CacheCapabilities` (no ZSTD/DEFLATE/BROTLI server-side
    // compression yet). If a client supplies a non-empty list that does
    // NOT include `Compressor.IDENTITY` (= 0) the server cannot satisfy
    // the request — surface `FAILED_PRECONDITION` with a hint pointing
    // at `GetCapabilities`. (Compressed batch responses ship with the
    // multipart write surface in WI-S05-005 / the `compressed-blobs`
    // ByteStream resources.)
    if !inner.acceptable_compressors.is_empty() && !inner.acceptable_compressors.contains(&0) {
        return Err(make_status(
            Code::FailedPrecondition,
            crate::error_map::COR_CAS_COMPRESSOR_UNSUPPORTED,
            "BatchReadBlobsRequest.acceptable_compressors must include Compressor.IDENTITY (= 0) or be empty; CoreLink S-01 supports IDENTITY only (compressed batch responses ship in WI-S05-005)",
            0,
        ));
    }

    // Step 3 — batch-size cap (canonical 1000 — same as FindMissingBlobs;
    // REAPI v2 recommendation).
    if inner.digests.len() > MAX_FIND_MISSING_BATCH_SIZE {
        return Err(make_status(
            Code::OutOfRange,
            COR_CAS_BATCH_SIZE_EXCEEDED,
            "BatchReadBlobsRequest carries more digests than the canonical CoreLink batch cap (1000)",
            inner.digests.len() as u64,
        ));
    }

    let inline_cap_bytes: u64 = u64::try_from(MAX_BATCH_TOTAL_SIZE_BYTES).unwrap_or(u64::MAX);
    let single_blob_inline_cap_bytes: u64 = inline_cap_bytes; // 4 MiB
    Ok((inner.digests, inline_cap_bytes, single_blob_inline_cap_bytes))
}

/// 4a. Pre-validate per-input-slot digests (hex + size_bytes); decode hex
/// per-index. Bad slots flagged with the per-slot diagnostic so Pass 1
/// only fans out for well-formed `Pending` slots.
pub(super) fn batch_read_prevalidate_slots(digests: Vec<ProtoDigest>) -> Vec<SlotState> {
    let n_input = digests.len();
    let mut slots: Vec<SlotState> = Vec::with_capacity(n_input);
    for pd in digests.into_iter() {
        let parsed_digest = Digest::from_hex(&pd.hash);
        let pd_size = pd.size_bytes;
        if pd_size < 0 {
            slots.push(SlotState::BadDigest {
                proto_digest: Some(pd),
                message: "digest.size_bytes is negative".to_owned(),
            });
            continue;
        }
        match parsed_digest {
            Err(_) => {
                slots.push(SlotState::BadDigest {
                    proto_digest: Some(pd),
                    message: "digest hex is malformed (expected 64 lowercase hex chars)"
                        .to_owned(),
                });
            }
            Ok(d) => {
                slots.push(SlotState::Pending {
                    proto_digest: pd,
                    digest: d,
                });
            }
        }
    }
    slots
}

/// 4b. Pass 1 — bounded-parallel D1 lookups for size_bytes + miss-reason.
/// Per ADR-0028 the wire response for every miss arm is uniform
/// `NOT_FOUND`, but the audit-emit envelope MUST disambiguate
/// NeverExisted (low-severity) from Tombstoned (legitimate post-GC) so the
/// S-09 forensics pipeline can distinguish ordinary cache-miss probes from
/// genuine tombstone-read attempts (codex round-2 P2 SEAL fix).
///
/// Returns a Vec indexed by input slot; `None` for slots that were
/// `BadDigest`; `Some(Err)` for D1 transport errors; `Some(Ok(_))` for
/// a successful row lookup.
pub(super) async fn batch_read_pass1_d1<M>(
    meta: &M,
    tenant_id: Uuid,
    slots: &[SlotState],
    n_input: usize,
) -> Vec<Option<Result<MetaPass1Result, crate::error_map::ErrorMapping>>>
where
    M: MetaStore + 'static,
{
    use futures::stream::StreamExt;
    const BOUNDED_CONCURRENCY: usize = 16;
    let pending_indices: Vec<(usize, Digest)> = slots
        .iter()
        .enumerate()
        .filter_map(|(idx, s)| match s {
            SlotState::Pending { digest, .. } => Some((idx, *digest)),
            SlotState::BadDigest { .. } => None,
        })
        .collect();
    let mut size_results: Vec<Option<Result<MetaPass1Result, crate::error_map::ErrorMapping>>> =
        (0..n_input).map(|_| None).collect();
    let mut size_stream = futures::stream::iter(pending_indices)
        .map(move |(idx, digest)| async move {
            let key = corelink_meta::BlobMetaKey::new(tenant_id, digest);
            let row_opt = meta.get(&key).await;
            let mapped: Result<MetaPass1Result, crate::error_map::ErrorMapping> = match row_opt {
                Err(e) => Err(e.mapping()),
                Ok(None) => Ok(MetaPass1Result::NeverExisted),
                Ok(Some(row)) => {
                    if row.is_alive() {
                        Ok(MetaPass1Result::Alive {
                            size_bytes: row.size_bytes,
                        })
                    } else {
                        Ok(MetaPass1Result::Tombstoned)
                    }
                }
            };
            (idx, mapped)
        })
        .buffer_unordered(BOUNDED_CONCURRENCY);
    while let Some((idx, mapped)) = size_stream.next().await {
        if let Some(slot) = size_results.get_mut(idx) {
            *slot = Some(mapped);
        }
    }
    size_results
}

/// 4c. Decide per-slot which bodies to fetch (running aggregate cap in
/// input order; missed/oversize/aggregate-cap-rejected slots flagged
/// here so Pass 2 only does R2 GETs for hits that fit).
pub(super) fn batch_read_decide(
    slots: &[SlotState],
    size_results: Vec<Option<Result<MetaPass1Result, crate::error_map::ErrorMapping>>>,
    inline_cap_bytes: u64,
    single_blob_inline_cap_bytes: u64,
) -> Vec<Decision> {
    let n_input = slots.len();
    let mut running: u64 = 0;
    let mut decisions: Vec<Decision> = Vec::with_capacity(n_input);
    for (idx, slot) in slots.iter().enumerate() {
        match slot {
            SlotState::BadDigest {
                proto_digest,
                message,
            } => {
                decisions.push(Decision::BadDigest {
                    proto_digest: proto_digest.clone(),
                    message: message.clone(),
                    grpc_code: GRPC_INVALID_ARGUMENT,
                });
            }
            SlotState::Pending {
                proto_digest,
                digest,
            } => {
                let mapped = match size_results.get(idx).and_then(|s| s.clone()) {
                    Some(v) => v,
                    None => {
                        // Defensive — should never happen as Pass 1 fills
                        // every Pending slot. Treat as transport failure
                        // for safety.
                        decisions.push(Decision::MetaTransport {
                            proto_digest: proto_digest.clone(),
                            mapping: crate::error_map::ErrorMapping {
                                taxonomy_code: crate::error_map::COR_INTERNAL,
                                grpc_code: crate::error_map::GRPC_INTERNAL,
                                message: "BatchReadBlobs Pass 1 yielded no result for slot",
                            },
                        });
                        continue;
                    }
                };
                match mapped {
                    Err(mapping) => {
                        decisions.push(Decision::MetaTransport {
                            proto_digest: proto_digest.clone(),
                            mapping,
                        });
                    }
                    Ok(MetaPass1Result::NeverExisted) => {
                        decisions.push(Decision::Miss {
                            proto_digest: proto_digest.clone(),
                            digest: *digest,
                            reason: MissReason::NeverExisted,
                        });
                    }
                    Ok(MetaPass1Result::Tombstoned) => {
                        decisions.push(Decision::Miss {
                            proto_digest: proto_digest.clone(),
                            digest: *digest,
                            reason: MissReason::Tombstoned,
                        });
                    }
                    Ok(MetaPass1Result::Alive {
                        size_bytes: row_size,
                    }) => {
                        // Caller-supplied size cross-check (codex round-1 P1).
                        let declared = u64::try_from(proto_digest.size_bytes).unwrap_or(u64::MAX);
                        if declared != row_size {
                            decisions.push(Decision::CallerSizeMismatch {
                                proto_digest: proto_digest.clone(),
                                row_size,
                            });
                            continue;
                        }
                        // Single-blob inline cap.
                        if row_size > single_blob_inline_cap_bytes {
                            decisions.push(Decision::ExceedsSingleCap {
                                proto_digest: proto_digest.clone(),
                            });
                            continue;
                        }
                        // Aggregate cap (running, in input order).
                        let after = running.saturating_add(row_size);
                        if after > inline_cap_bytes {
                            decisions.push(Decision::ExceedsAggregate {
                                proto_digest: proto_digest.clone(),
                            });
                            continue;
                        }
                        running = after;
                        decisions.push(Decision::Fetch {
                            proto_digest: proto_digest.clone(),
                            digest: *digest,
                            row_size,
                        });
                    }
                }
            }
        }
    }
    decisions
}

/// 4d. Pass 2 — bounded-parallel R2 GETs ONLY for `Fetch` decisions.
/// Worst-case in-flight bytes: `FETCH_CONCURRENCY × single-blob 4 MiB cap
/// = 64 MiB`; the aggregate-cap trim means total RETURNED bytes never
/// exceed 4 MiB — the in-flight working set stays bounded under the
/// 50 MiB ceiling. Production deployment caps further via the binding
/// adapter's R2 client connection pool.
pub(super) async fn batch_read_pass2_r2<B>(
    reader: &corelink_worker::storage::r2::R2Reader<B>,
    storage_ctx: &StorageTenantCtx,
    decisions: &[Decision],
    n_input: usize,
) -> Vec<Option<FetchOutcome>>
where
    B: R2Backend + 'static,
{
    use futures::stream::StreamExt;
    const FETCH_CONCURRENCY: usize = 8;
    let fetch_targets: Vec<(usize, Digest)> = decisions
        .iter()
        .enumerate()
        .filter_map(|(idx, d)| match d {
            Decision::Fetch { digest, .. } => Some((idx, *digest)),
            _ => None,
        })
        .collect();
    let mut fetched: Vec<Option<FetchOutcome>> = (0..n_input).map(|_| None).collect();
    let mut fetch_stream = futures::stream::iter(fetch_targets)
        .map(move |(idx, digest)| async move {
            let res = reader.get(storage_ctx, &digest).await;
            let mapped: FetchOutcome = match res {
                Ok(b) => FetchOutcome::Body(b),
                // R2 orphan row — alive blob_meta + R2 NotFound. Per
                // ADR-0028 surface as uniform 404 on the wire (GC
                // reconciler in S-06 repairs the orphan window) AND tag
                // the slot so the response-composition phase emits the
                // canonical SEV-2 `r2_orphan_detected` audit.
                Err(corelink_worker::storage::error::R2Error::NotFound) => FetchOutcome::R2Orphan,
                Err(e) => FetchOutcome::Other(e.mapping()),
            };
            (idx, mapped)
        })
        .buffer_unordered(FETCH_CONCURRENCY);
    while let Some((idx, mapped)) = fetch_stream.next().await {
        if let Some(slot) = fetched.get_mut(idx) {
            *slot = Some(mapped);
        }
    }
    fetched
}
