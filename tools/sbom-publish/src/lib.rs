//! `sbom-publish` — CoreLink SBOM CycloneDX 1.5+ pipeline library.
//!
//! # Overview
//!
//! Generates, validates, timestamps, and ingests SBOMs as part of CoreLink's
//! supply-chain hardening initiative (WI-S12-002, S-12 HIGH_RISK lane).
//!
//! ## Submodules
//!
//! - [`ntia`] — NTIA minimum elements validator (7 fields).
//! - [`purl`] — PURL normalisation (cargo → crates alias + workspace discriminator).
//! - [`tsa`]  — RFC 3161 TSA timestamp client (Sigstore TSA).
//! - [`dt`]   — Dependency-Track ingestion client (retry + fallback queue).
//! - [`publisher`] — [`SbomPublisher`] trait + [`DefaultSbomPublisher`] impl.
//! - [`metrics`] — 5 Prometheus metrics counters / histograms.
//! - [`error`]   — [`SbomError`] taxonomy.
//!
//! ## Invariant
//!
//! **INV-SUPPLY-SBOM-PRESENT** — every release must have an SBOM attached.
//! The CLI binary exits non-zero and blocks release publication when any
//! pipeline stage fails (generation, NTIA strict, TSA, or DT ingestion),
//! with the sole exception of DT outage which triggers fallback-queue and
//! continues (per §9.7 design decision).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod dt;
pub mod error;
pub mod metrics;
pub mod ntia;
pub mod publisher;
pub mod purl;
pub mod tsa;

pub use error::SbomError;
pub use ntia::{validate_ntia_json, NtiaValidation, ValidationMode};
pub use publisher::{DefaultSbomPublisher, ReleaseMetadata, SbomFormat, SbomPublisher, SignedSbom};
pub use tsa::TsrToken;
