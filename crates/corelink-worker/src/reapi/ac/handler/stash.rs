//! `ActionResult` sideband persistence — `persist_action_result` /
//! `recover_action_result` methods + `ActionResultStash` canonical
//! key generator.
//!
//! Split from monolith `reapi/ac/handler.rs` (wave-33 stage 2.PRE-A.4).

use corelink_tenant_path::TenantPrefix;

use super::super::audit::AuditSink;
use super::super::merkle::MerkleVerifier;
use super::super::meta::AcMetaStore;
use super::super::outputs::OutputsCheck;
use super::super::sig::Signer;
use super::super::types::ActionResult;
use super::builder::ActionCacheHandlerImpl;
use super::envelope_store::AcEnvelopeStore;
use crate::cache::kv::KvBackend;
use crate::Region;

impl<M, E, V, S, O, A, K> ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Persist the [`ActionResult`] bytes alongside the envelope —
    /// in-memory implementation; production stashes them inside the
    /// envelope JSON.
    pub(super) async fn persist_action_result(
        &self,
        region: Region,
        prefix: &TenantPrefix,
        action_hex: &str,
        action_result: &ActionResult,
    ) -> Result<(), String> {
        // Per-instance stash keyed by the canonical envelope path —
        // tenant isolation by construction (the `region/prefix/hex`
        // tuple is tenant-leftmost). The stash is scoped to this
        // handler instance so concurrent in-process tests cannot
        // collide on shared keys (F-001 closure 2026-05-01).
        //
        // Production wiring inlines the `ActionResult` proto bytes
        // inside the envelope JSON; the fake `InMemoryAcEnvelopeStore`
        // delegates the proto round-trip back here so the property
        // tests can assert the canonical shape without spinning up a
        // real proto codec.
        let key = ActionResultStash::canonical(region, prefix, action_hex);
        self.action_result_stash
            .lock()
            .await
            .insert(key, action_result.clone());
        Ok(())
    }

    /// Recover the [`ActionResult`] bytes from the sibling store.
    pub(super) async fn recover_action_result(
        &self,
        region: Region,
        prefix: &TenantPrefix,
        action_hex: &str,
    ) -> Result<ActionResult, String> {
        let key = ActionResultStash::canonical(region, prefix, action_hex);
        let guard = self.action_result_stash.lock().await;
        guard
            .get(&key)
            .cloned()
            .ok_or_else(|| "action_result missing from stash".to_string())
    }
}

/// Canonical key generator for the per-instance [`ActionResult`]
/// sibling stash (`region/prefix/hex` shape preserves tenant
/// isolation by construction). Production routes the proto bytes
/// through the envelope JSON inline; the in-memory fake delegates
/// the round-trip back to a per-handler-instance map (see
/// `ActionCacheHandlerImpl::action_result_stash`).
pub(super) struct ActionResultStash;

impl ActionResultStash {
    pub(super) fn canonical(region: Region, prefix: &TenantPrefix, hex: &str) -> String {
        format!(
            "ac-{}/{}/{}.action_result",
            region.bucket_suffix(),
            prefix.as_str(),
            hex
        )
    }
}
