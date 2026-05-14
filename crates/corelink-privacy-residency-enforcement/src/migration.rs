//! Region migration request — POST /v1/admin/tenant/region-migration.
//!
//! Implements 30d cooldown per `privacy_model.md §7.2` + ADR-S11-011.
//! Reuses the step-up MFA pattern from WI-S11-001 (`CTRL-AUTH-010`).
//!
//! # Monotonic constraint (INV-DATA-RESIDENCY)
//!
//! `tenant.primary_region` is monotonic: once set at signup, it can only
//! change via this formal request path with Privacy Officer + Compliance review.
//! The 30-day cooldown ensures no accidental or coerced rapid migration.

use crate::{error::ResidencyViolation, Region};
use serde::{Deserialize, Serialize};

/// Status of a region migration request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    /// Ticket created; awaiting Privacy Officer + Compliance review.
    Pending,
    /// Both Privacy Officer and Compliance have approved.
    Approved,
    /// Migration was rejected by reviewer.
    Rejected,
    /// Migration has been executed (data moved).
    Completed,
}

/// A region migration request ticket (D1 `region_migration_request` table).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionMigrationRequest {
    /// Unique ticket ID (UUIDv4).
    pub ticket_id: String,
    /// Tenant requesting the migration.
    pub tenant_id: String,
    /// Current region.
    pub from_region: Region,
    /// Target region.
    pub to_region: Region,
    /// Justification text provided by tenant admin.
    pub justification: String,
    /// Attestation string (e.g. "I understand region migration is irreversible").
    pub attestation: String,
    /// Ticket creation timestamp (ms since epoch).
    pub created_at_ms: u64,
    /// Current status.
    pub status: MigrationStatus,
}

/// In-memory store for region migration tickets (test/staging only).
#[derive(Debug, Default)]
pub struct InMemoryMigrationStore {
    tickets: std::sync::Mutex<Vec<RegionMigrationRequest>>,
}

impl InMemoryMigrationStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Submit a new migration request. Validates 30d cooldown.
    ///
    /// # Arguments
    ///
    /// - `current_region_set_at_ms`: timestamp when `primary_region` was last set.
    /// - `now_ms`: current timestamp.
    ///
    /// Returns `Err(ResidencyViolation::MigrationCooldownNotElapsed)` if
    /// `now_ms - current_region_set_at_ms < 30 days`.
    pub fn submit(
        &self,
        ticket: RegionMigrationRequest,
        current_region_set_at_ms: u64,
        now_ms: u64,
    ) -> Result<String, ResidencyViolation> {
        const COOLDOWN_MS: u64 = 30 * 24 * 60 * 60 * 1000; // 30 days in ms

        let elapsed_ms = now_ms.saturating_sub(current_region_set_at_ms);
        if elapsed_ms < COOLDOWN_MS {
            let remaining_ms = COOLDOWN_MS - elapsed_ms;
            let days_remaining = remaining_ms.div_ceil(86_400_000);
            return Err(ResidencyViolation::MigrationCooldownNotElapsed { days_remaining });
        }

        let id = ticket.ticket_id.clone();
        self.tickets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(ticket);
        Ok(id)
    }

    /// Get a ticket by ID.
    pub fn get(&self, ticket_id: &str) -> Option<RegionMigrationRequest> {
        self.tickets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|t| t.ticket_id == ticket_id)
            .cloned()
    }

    /// All tickets for a tenant.
    pub fn for_tenant(&self, tenant_id: &str) -> Vec<RegionMigrationRequest> {
        self.tickets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|t| t.tenant_id == tenant_id)
            .cloned()
            .collect()
    }
}
