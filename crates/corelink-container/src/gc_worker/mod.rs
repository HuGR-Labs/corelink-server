//! Production GC worker implementations.
//!
//! Real implementations of [`crate::run::GcRunStore`], [`crate::audit::GcAuditSink`],
//! [`crate::degrade::DegradeProbe`], [`crate::metrics::GcMetricsObserver`] and
//! [`crate::worker::GcWorker`] backed by real D1/R2.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use corelink_gc::{
    audit::{GcAuditRecord, GcAuditSink, GcEventType},
    degrade::{DegradeKind, DegradeProbe},
    error::GcError,
    mark::{InMemoryMarkPhase, MarkConfig, MarkPhase},
    metrics::{GcMetricsObserver, NoOpGcMetrics},
    reconcile::{InMemoryReconcilePhase, ReconcileConfig},
    run::{
        CheckpointDeltas, FailureContext, GcPhase, GcRun, GcRunStore, GcRunStoreError,
        GcRegion, GcRun as GcRunStruct, GcStatus, RunId,
    },
    sweep::{InMemorySweepPhase, SweepConfig},
    physical_delete::{InMemoryPhysicalDeletePhase, PhysicalDeleteConfig},
    reconcile::{InMemoryReconcilePhase, ReconcileConfig as ReconcileConfig2},
    worker::GcWorker,
};

use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;

use crate::gc_worker::d1_gc_run_store::D1GcRunStore;
use crate::gc_worker::d1_degrade_probe::D1DegradeProbe;
use crate::gc_worker::d1_gc_audit_sink::D1GcAuditSink;
use crate::gc_worker::d1_reachable_set_source::D1ReachableSetSource;
use crate::gc_worker::d1_gc_candidates_store::D1GcCandidatesStore;
use crate::gc_worker::d1_blob_meta_store::D1BlobMetaStore;
use crate::gc_worker::d1_ac_reference_index::D1AcReferenceIndex;
use crate::gc_worker::d1_blob_meta_purge_store::D1BlobMetaPurgeStore;
use crate::gc_worker::d1_refcount_source::D1RefcountSource;
use crate::gc_worker::d1_blob_meta_refcount_store::D1BlobMetaRefcountStore;
use crate::gc_worker::d1_gc_audit_sink::D1GcAuditSink;
use crate::gc_worker::d1_degrade_probe::D1DegradeProbe;
use crate::gc_worker::RealGcMetricsObserver;

use corelink_gc::{
    audit::{GcAuditRecord, GcAuditSink, GcEventType},
    degrade::{DegradeKind, DegradeProbe},
    error::GcError,
    metrics::{GcMetricsObserver, NoOpGcMetrics},
    run::{
        CheckpointDeltas, FailureContext, GcPhase, GcRun, GcRunStore, GcRunStoreError,
        GcRegion, GcRun as GcRunStruct, GcStatus, RunId,
    },
    worker::GcWorker,
};

use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;

use crate::gc_worker::d1_gc_run_store::D1GcRunStore;
use crate::gc_worker::d1_degrade_probe::D1DegradeProbe;
use crate::gc_worker::d1_gc_audit_sink::D1GcAuditSink;
use crate::gc_worker::d1_reachable_set_source::D1ReachableSetSource;
use crate::gc_worker::d1_gc_candidates_store::D1GcCandidatesStore;
use crate::gc_worker::d1_blob_meta_store::D1BlobMetaStore;
use crate::gc_worker::d1_ac_reference_index::D1AcReferenceIndex;
use crate::gc_worker::d1_blob_meta_purge_store::D1BlobMetaPurgeStore;
use crate::gc_worker::d1_refcount_source::D1RefcountSource;
use crate::gc_worker::d1_blob_meta_refcount_store::D1BlobMetaRefcountStore;
use crate::gc_worker::real_gc_worker::RealGcWorker;
use crate::gc_worker::real_reconcile_phase::RealReconcilePhase;
use crate::gc_worker::real_physical_delete_phase::RealPhysicalDeletePhase;
use crate::gc_worker::d1_gc_audit_sink::D1GcAuditSink;
use crate::gc_worker::d1_degrade_probe::D1DegradeProbe;
use crate::gc_worker::RealGcMetricsObserver;

use crate::gc_worker::{
    d1_gc_run_store::D1GcRunStore,
    d1_degrade_probe::D1DegradeProbe,
    d1_gc_audit_sink::D1GcAuditSink,
    d1_reachable_set_source::D1ReachableSetSource,
    d1_gc_candidates_store::D1GcCandidatesStore,
    d1_blob_meta_store::D1BlobMetaStore,
    d1_ac_reference_index::D1AcReferenceIndex,
    d1_blob_meta_purge_store::D1BlobMetaPurgeStore,
    d1_refcount_source::D1RefcountSource,
    d1_blob_meta_refcount_store::D1BlobMetaRefcountStore,
    real_gc_worker::RealGcWorker,
    real_reconcile_phase::RealReconcilePhase,
    real_physical_delete_phase::RealPhysicalDeletePhase,
    d1_gc_audit_sink::D1GcAuditSink,
    d1_degrade_probe::D1DegradeProbe,
    RealGcMetricsObserver,
};

use corelink_gc::{
    audit::{GcAuditRecord, GcAuditSink, GcEventType},
    degrade::{DegradeKind, DegradeProbe},
    error::GcError,
    metrics::{GcMetricsObserver, NoOpGcMetrics},
    run::{
        CheckpointDeltas, FailureContext, GcPhase, GcRun, GcRunStore, GcRunStoreError,
        GcRegion, GcRun as GcRunStruct, GcStatus, RunId,
    },
    worker::GcWorker,
};

// Re-export the real implementations
pub use crate::gc_worker::{
    d1_gc_run_store::D1GcRunStore,
    d1_degrade_probe::D1DegradeProbe,
    d1_gc_audit_sink::D1GcAuditSink,
    d1_reachable_set_source::D1ReachableSetSource,
    d1_gc_candidates_store::D1GcCandidatesStore,
    d1_blob_meta_store::D1BlobMetaStore,
    d1_ac_reference_index::D1AcReferenceIndex,
    d1_blob_meta_purge_store::D1BlobMetaPurgeStore,
    d1_refcount_source::D1RefcountSource,
    d1_blob_meta_refcount_store::D1BlobMetaRefcountStore,
    real_gc_worker::RealGcWorker,
    real_reconcile_phase::RealReconcilePhase,
    real_physical_delete_phase::RealPhysicalDeletePhase,
    d1_gc_audit_sink::D1GcAuditSink,
    d1_degrade_probe::D1DegradeProbe,
    RealGcMetricsObserver,
};