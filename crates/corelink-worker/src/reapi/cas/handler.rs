//! Canonical SplitBlob/SpliceBlob handler trait + Impl re-exports.
//!
//! The full handler implementation (trait surface, builder, impl, inner
//! flow methods, error taxonomies, outcome shapes) lives in
//! [`super::split_splice`] — single-file cohesion mirrors how the
//! `reapi::ac::handler` module is laid out (trait + Impl + flow inner
//! methods all in the same file). This module exists so the WI §13
//! artifact table's "Handler trait + Impl"
//! (`crates/corelink-worker/src/reapi/cas/handler.rs`) row maps to a
//! real file and so call sites that mirror the
//! `corelink_worker::reapi::ac::handler::*` import path can fluently
//! `use corelink_worker::reapi::cas::handler::*`.

pub use super::split_splice::{
    Clock, FakeClock, FinalizeSplitOutcome, InitSplitOutcome, SpliceError, SpliceOutcome,
    SplitError, SplitSpliceHandler, SplitSpliceHandlerBuilder, SplitSpliceHandlerImpl, SystemClock,
    MAX_CHUNK_BYTES,
};
