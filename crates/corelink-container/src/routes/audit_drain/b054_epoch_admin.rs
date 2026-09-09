// B-054 administrative boundary for registry provisioning and epoch cutover.
//
// Its cryptographic core is pure: it canonicalizes and verifies caller-supplied
// artifacts, then returns parameterized D1 batches. The bounded handler around it
// provisions and confirms registries before any witness append and performs the
// witnessed cutover while holding the partition lease.

#[allow(
    dead_code,
    reason = "wired incrementally by the bounded B054 admin endpoint"
)]
mod b054_epoch_admin {
    use super::*;

    const B054_ADMIN_LEDGER_DOMAIN: &[u8] = b"corelink/audit-chain/epoch-ledger/v1\0";
    const B054_ADMIN_LINK_COMMITMENT_DOMAIN: &[u8] =
        b"corelink/audit-chain/link-key-commitment/v1\0";
    const B054_ADMIN_MAX_JCS_BYTES: usize = 64 * 1024;
    const B054_ADMIN_MAX_LEGACY_ROWS: usize = 10_000;
    const B054_ADMIN_MAX_LEGACY_BYTES: usize = 16 * 1024 * 1024;

    #[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
    #[serde(deny_unknown_fields)]
    struct B054SigningRegistryJcs {
        algorithm: String,
        key_type: String,
        public_key_b64: String,
        registered_at_ms: u64,
        registry_type: String,
        registry_version: u8,
        signing_key_id: u64,
        trust_root_key_id: String,
    }

    #[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
    #[serde(deny_unknown_fields)]
    struct B054LinkRegistryJcs {
        algorithm_id: u8,
        key_commitment_hex: String,
        key_type: String,
        link_key_id: u64,
        registered_at_ms: u64,
        registry_type: String,
        registry_version: u8,
        signing_key_id: u64,
    }

    #[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
    #[serde(deny_unknown_fields)]
    struct B054E0LedgerJcs {
        algorithm_id: u8,
        checkpoint_type: String,
        checkpoint_version: u8,
        epoch_id: u64,
        ledger_sequence: u64,
        ledger_version: u8,
        link_key_id: Option<u64>,
        previous_ledger_hash: String,
        region: String,
        signing_key_id: u64,
        start_prev_hash: String,
        start_sequence: u64,
        tenant_id: String,
    }

    #[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
    #[serde(deny_unknown_fields)]
    struct B054E1LedgerJcs {
        algorithm_id: u8,
        checkpoint_type: String,
        checkpoint_version: u8,
        epoch_id: u64,
        from_epoch_id: u64,
        from_head_hash: String,
        from_next_sequence: u64,
        ledger_sequence: u64,
        ledger_version: u8,
        link_key_id: u64,
        previous_ledger_hash: String,
        region: String,
        signing_key_id: u64,
        tenant_id: String,
        to_start_prev_hash: String,
        to_start_sequence: u64,
    }

    /// Opaque, root-authenticated signing-key registration. Construction verifies
    /// exact RFC-8785 bytes and a signature from the selected OOB trust root.
    #[derive(Clone, Debug)]
    pub(super) struct B054AuthenticatedSigningKey {
        record: B054SigningRegistryJcs,
        registry_jcs: Vec<u8>,
        registry_signature_b64: String,
        verifying_key: ed25519_dalek::VerifyingKey,
    }

    /// Opaque, signing-key-authenticated link-key commitment.
    #[derive(Clone, Debug)]
    pub(super) struct B054AuthenticatedLinkKey {
        record: B054LinkRegistryJcs,
        registry_jcs: Vec<u8>,
        registry_signature_b64: String,
    }

    /// Exact legacy rows verified from sequence zero through the signed v1 head.
    /// The row bodies stay private; the bootstrap batch only needs bounded snapshot
    /// invariants because the caller holds the partition lease through cutover.
    #[derive(Clone, Debug)]
    pub(super) struct B054VerifiedLegacyPrefix {
        tenant_id: String,
        region: String,
        head_hash: String,
        next_sequence: u64,
        head_signature_b64: String,
        signing_key_id: u64,
        row_count: u64,
    }

    /// E0 checkpoint authenticated against the registered signing key, exact
    /// signed E0 ledger and challenged external latest witness.
    #[derive(Clone, Debug)]
    pub(super) struct B054AuthenticatedE0Checkpoint {
        checkpoint: HeadCheckpoint,
        tenant_id: String,
        region: String,
    }

    #[derive(Clone, Debug)]
    pub(super) struct B054LegacySealedRow {
        pub(super) sequence_number: u64,
        pub(super) prev_hash_hex: String,
        pub(super) chain_hash_hex: String,
        pub(super) canonical_jcs: Vec<u8>,
    }

    pub(super) struct B054LegacyPrefixVerification<'a> {
        pub(super) tenant_id: &'a str,
        pub(super) region: &'a str,
        pub(super) head_hash: &'a str,
        pub(super) next_sequence: u64,
        pub(super) head_signature_b64: &'a str,
        pub(super) stored_signing_key_id: u64,
        pub(super) signing_key: &'a B054AuthenticatedSigningKey,
        pub(super) rows: &'a [B054LegacySealedRow],
    }

    #[derive(Clone, Debug)]
    pub(super) struct B054SignedArtifact {
        pub(super) jcs: Vec<u8>,
        pub(super) signature_b64: String,
    }

    #[derive(Debug)]
    pub(super) struct B054AdminTransaction {
        pub(super) statements: Vec<D1BatchStatement>,
    }

    include!("b054_epoch_admin_authorities.rs");

    fn b054_admin_canonical_jcs<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
        let bytes = serde_jcs::to_vec(value).map_err(|error| format!("B054 JCS: {error}"))?;
        if bytes.is_empty() || bytes.len() > B054_ADMIN_MAX_JCS_BYTES {
            return Err("B054 JCS length outside administrative bound".to_owned());
        }
        Ok(bytes)
    }

    fn b054_admin_parse_exact<T>(bytes: &[u8], label: &str) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned + Serialize,
    {
        if bytes.is_empty() || bytes.len() > B054_ADMIN_MAX_JCS_BYTES {
            return Err(format!("{label} length outside administrative bound"));
        }
        let value: T = serde_json::from_slice(bytes).map_err(|_| format!("{label} malformed"))?;
        if b054_admin_canonical_jcs(&value)? != bytes {
            return Err(format!("{label} is not exact RFC-8785 JCS"));
        }
        Ok(value)
    }

    fn b054_admin_signature(value: &str) -> Result<Signature, String> {
        let bytes: [u8; 64] = decode_canonical_base64(value, Some(64))?
            .try_into()
            .map_err(|_| "B054 Ed25519 signature length mismatch")?;
        Ok(Signature::from_bytes(&bytes))
    }

    fn b054_admin_verify_raw_signature(
        key: &ed25519_dalek::VerifyingKey,
        message: &[u8],
        signature_b64: &str,
        label: &str,
    ) -> Result<(), String> {
        key.verify(message, &b054_admin_signature(signature_b64)?)
            .map_err(|_| format!("{label} signature verification failed"))
    }

    fn b054_admin_i64(value: u64, label: &str) -> Result<i64, String> {
        if value > JS_SAFE_MAX {
            return Err(format!("{label} exceeds JavaScript-safe integer range"));
        }
        i64::try_from(value).map_err(|_| format!("{label} exceeds D1 integer range"))
    }

    fn b054_admin_jcs_text<'a>(bytes: &'a [u8], label: &str) -> Result<&'a str, String> {
        std::str::from_utf8(bytes).map_err(|_| format!("{label} is not UTF-8"))
    }

    fn b054_admin_root_id(value: &str) -> bool {
        !value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            })
    }

    /// Parse pinned OOB roots without permitting duplicate JSON members or key ids.
    /// D1 registry rows are never roots of trust and are deliberately not accepted.
    pub(super) fn b054_parse_trust_root_public_keys(
        raw: &str,
    ) -> Result<std::collections::BTreeMap<String, ed25519_dalek::VerifyingKey>, String> {
        struct UniqueRoots(std::collections::BTreeMap<String, String>);
        impl<'de> serde::Deserialize<'de> for UniqueRoots {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct Visitor;
                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = UniqueRoots;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("a unique map of trust-root ids to Ed25519 public keys")
                    }
                    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
                    where
                        A: serde::de::MapAccess<'de>,
                    {
                        let mut roots = std::collections::BTreeMap::new();
                        while let Some((id, key)) = map.next_entry::<String, String>()? {
                            if roots.insert(id, key).is_some() {
                                return Err(serde::de::Error::custom("duplicate trust-root id"));
                            }
                        }
                        Ok(UniqueRoots(roots))
                    }
                }
                deserializer.deserialize_map(Visitor)
            }
        }

        let entries = serde_json::from_str::<UniqueRoots>(raw)
            .map_err(|_| "B054 trust-root registry JSON malformed")?
            .0;
        if entries.is_empty() {
            return Err("B054 trust-root registry is empty".to_owned());
        }
        let mut roots = std::collections::BTreeMap::new();
        for (id, encoded) in entries {
            if !b054_admin_root_id(&id) {
                return Err("B054 trust-root id is non-canonical".to_owned());
            }
            let bytes: [u8; 32] = decode_canonical_base64(&encoded, Some(32))?
                .try_into()
                .map_err(|_| "B054 trust-root public key length mismatch")?;
            let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
                .map_err(|_| "B054 trust-root public key invalid")?;
            roots.insert(id, key);
        }
        Ok(roots)
    }

    pub(super) fn b054_signing_registry_jcs(
        signing_key_id: u64,
        public_key_b64: &str,
        trust_root_key_id: &str,
        registered_at_ms: u64,
    ) -> Result<Vec<u8>, String> {
        b054_admin_i64(signing_key_id, "signing key id")?;
        b054_admin_i64(registered_at_ms, "signing registration time")?;
        if signing_key_id == 0 || !b054_admin_root_id(trust_root_key_id) {
            return Err("B054 signing registry identity invalid".to_owned());
        }
        let _: [u8; 32] = decode_canonical_base64(public_key_b64, Some(32))?
            .try_into()
            .map_err(|_| "B054 signing public key length mismatch")?;
        b054_admin_canonical_jcs(&B054SigningRegistryJcs {
            algorithm: "ed25519-v1".to_owned(),
            key_type: "audit-chain-head-signing".to_owned(),
            public_key_b64: public_key_b64.to_owned(),
            registered_at_ms,
            registry_type: "audit-chain-signing-key-registry".to_owned(),
            registry_version: 1,
            signing_key_id,
            trust_root_key_id: trust_root_key_id.to_owned(),
        })
    }

    pub(super) fn b054_authenticate_signing_registry(
        artifact: B054SignedArtifact,
        trust_roots: &std::collections::BTreeMap<String, ed25519_dalek::VerifyingKey>,
    ) -> Result<B054AuthenticatedSigningKey, String> {
        let record: B054SigningRegistryJcs =
            b054_admin_parse_exact(&artifact.jcs, "B054 signing registry JCS")?;
        let expected = b054_signing_registry_jcs(
            record.signing_key_id,
            &record.public_key_b64,
            &record.trust_root_key_id,
            record.registered_at_ms,
        )?;
        if expected != artifact.jcs
            || record.algorithm != "ed25519-v1"
            || record.key_type != "audit-chain-head-signing"
            || record.registry_type != "audit-chain-signing-key-registry"
            || record.registry_version != 1
        {
            return Err("B054 signing registry contract mismatch".to_owned());
        }
        let root = trust_roots
            .get(&record.trust_root_key_id)
            .ok_or("B054 signing registry names unknown OOB trust root")?;
        b054_admin_verify_raw_signature(
            root,
            &artifact.jcs,
            &artifact.signature_b64,
            "B054 signing registry root",
        )?;
        let public: [u8; 32] = decode_canonical_base64(&record.public_key_b64, Some(32))?
            .try_into()
            .map_err(|_| "B054 signing public key length mismatch")?;
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public)
            .map_err(|_| "B054 signing public key invalid")?;
        Ok(B054AuthenticatedSigningKey {
            record,
            registry_jcs: artifact.jcs,
            registry_signature_b64: artifact.signature_b64,
            verifying_key,
        })
    }

    /// Idempotent pre-witness provisioning. An exact row is a no-op; a conflicting
    /// id/public key reaches 0109's no-replace trigger and rolls the batch back.
    pub(super) fn b054_build_signing_registry_transaction(
        key: &B054AuthenticatedSigningKey,
    ) -> Result<B054AdminTransaction, String> {
        let r = &key.record;
        let id = b054_admin_i64(r.signing_key_id, "signing key id")?;
        let at = b054_admin_i64(r.registered_at_ms, "signing registration time")?;
        let jcs = b054_admin_jcs_text(&key.registry_jcs, "B054 signing registry JCS")?;
        let commit_id = uuid::Uuid::new_v4().to_string();
        let exact = "signing_key_id=?1 AND algorithm='ed25519-v1' AND public_key_b64=?2 AND trust_root_key_id=?3 AND registry_version=1 AND registry_jcs=CAST(?4 AS BLOB) AND registry_signature_b64=?5 AND registered_at_ms=?6";
        Ok(B054AdminTransaction {
        statements: vec![
            D1BatchStatement::new(
                format!("INSERT INTO audit_chain_signing_key_registry (signing_key_id,algorithm,public_key_b64,trust_root_key_id,registry_version,registry_jcs,registry_signature_b64,registered_at_ms) SELECT ?1,'ed25519-v1',?2,?3,1,CAST(?4 AS BLOB),?5,?6 WHERE NOT EXISTS (SELECT 1 FROM audit_chain_signing_key_registry WHERE {exact})"),
                vec![json!(id), json!(r.public_key_b64), json!(r.trust_root_key_id), json!(jcs), json!(key.registry_signature_b64), json!(at)],
            ),
            D1BatchStatement::new(
                format!("INSERT INTO audit_chain_v2_tx_assert(commit_id,assertion) SELECT ?7,CASE WHEN EXISTS (SELECT 1 FROM audit_chain_signing_key_registry WHERE {exact}) THEN 1 ELSE 0 END"),
                vec![json!(id), json!(r.public_key_b64), json!(r.trust_root_key_id), json!(jcs), json!(key.registry_signature_b64), json!(at), json!(commit_id)],
            ),
            D1BatchStatement::new(
                "DELETE FROM audit_chain_v2_tx_assert WHERE commit_id=?1",
                vec![json!(commit_id)],
            ),
        ],
    })
    }

    pub(super) fn b054_link_registry_jcs(
        link_key_id: u64,
        key_commitment_hex: &str,
        signing_key_id: u64,
        registered_at_ms: u64,
    ) -> Result<Vec<u8>, String> {
        if link_key_id == 0 || signing_key_id == 0 || !is_lower_hex_32(key_commitment_hex) {
            return Err("B054 link registry identity/commitment invalid".to_owned());
        }
        b054_admin_i64(link_key_id, "link key id")?;
        b054_admin_i64(signing_key_id, "signing key id")?;
        b054_admin_i64(registered_at_ms, "link registration time")?;
        b054_admin_canonical_jcs(&B054LinkRegistryJcs {
            algorithm_id: 1,
            key_commitment_hex: key_commitment_hex.to_owned(),
            key_type: "audit-chain-link".to_owned(),
            link_key_id,
            registered_at_ms,
            registry_type: "audit-chain-link-key-registry".to_owned(),
            registry_version: 1,
            signing_key_id,
        })
    }

    pub(super) fn b054_authenticate_link_registry(
        artifact: B054SignedArtifact,
        signing_key: &B054AuthenticatedSigningKey,
    ) -> Result<B054AuthenticatedLinkKey, String> {
        let record: B054LinkRegistryJcs =
            b054_admin_parse_exact(&artifact.jcs, "B054 link registry JCS")?;
        let expected = b054_link_registry_jcs(
            record.link_key_id,
            &record.key_commitment_hex,
            record.signing_key_id,
            record.registered_at_ms,
        )?;
        if expected != artifact.jcs
            || record.signing_key_id != signing_key.record.signing_key_id
            || record.algorithm_id != 1
            || record.registry_version != 1
            || record.key_type != "audit-chain-link"
            || record.registry_type != "audit-chain-link-key-registry"
        {
            return Err("B054 link registry contract mismatch".to_owned());
        }
        b054_admin_verify_raw_signature(
            &signing_key.verifying_key,
            &artifact.jcs,
            &artifact.signature_b64,
            "B054 link registry",
        )?;
        Ok(B054AuthenticatedLinkKey {
            record,
            registry_jcs: artifact.jcs,
            registry_signature_b64: artifact.signature_b64,
        })
    }

    pub(super) fn b054_build_link_registry_transaction(
        link: &B054AuthenticatedLinkKey,
        signing: &B054AuthenticatedSigningKey,
    ) -> Result<B054AdminTransaction, String> {
        if link.record.signing_key_id != signing.record.signing_key_id {
            return Err("B054 link registry signing key mismatch".to_owned());
        }
        let r = &link.record;
        let link_id = b054_admin_i64(r.link_key_id, "link key id")?;
        let signing_id = b054_admin_i64(r.signing_key_id, "signing key id")?;
        let at = b054_admin_i64(r.registered_at_ms, "link registration time")?;
        let jcs = b054_admin_jcs_text(&link.registry_jcs, "B054 link registry JCS")?;
        let signing_jcs = b054_admin_jcs_text(&signing.registry_jcs, "B054 signing registry JCS")?;
        let commit_id = uuid::Uuid::new_v4().to_string();
        let exact = "link_key_id=?1 AND algorithm_id=1 AND key_commitment_hex=?2 AND registry_version=1 AND registry_jcs=CAST(?3 AS BLOB) AND registry_signature_b64=?4 AND signing_key_id=?5 AND registered_at_ms=?6";
        let signer =
            "signing_key_id=?5 AND registry_jcs=CAST(?7 AS BLOB) AND registry_signature_b64=?8";
        let params = || {
            vec![
                json!(link_id),
                json!(r.key_commitment_hex),
                json!(jcs),
                json!(link.registry_signature_b64),
                json!(signing_id),
                json!(at),
                json!(signing_jcs),
                json!(signing.registry_signature_b64),
            ]
        };
        Ok(B054AdminTransaction {
        statements: vec![
            D1BatchStatement::new(
                format!("INSERT INTO audit_chain_link_key_registry (link_key_id,algorithm_id,key_commitment_hex,registry_version,registry_jcs,registry_signature_b64,signing_key_id,registered_at_ms) SELECT ?1,1,?2,1,CAST(?3 AS BLOB),?4,?5,?6 WHERE EXISTS (SELECT 1 FROM audit_chain_signing_key_registry WHERE {signer}) AND NOT EXISTS (SELECT 1 FROM audit_chain_link_key_registry WHERE {exact})"),
                params(),
            ),
            D1BatchStatement::new(
                format!("INSERT INTO audit_chain_v2_tx_assert(commit_id,assertion) SELECT ?9,CASE WHEN EXISTS (SELECT 1 FROM audit_chain_signing_key_registry WHERE {signer}) AND EXISTS (SELECT 1 FROM audit_chain_link_key_registry WHERE {exact}) THEN 1 ELSE 0 END"),
                { let mut p = params(); p.push(json!(commit_id)); p },
            ),
            D1BatchStatement::new("DELETE FROM audit_chain_v2_tx_assert WHERE commit_id=?1", vec![json!(commit_id)]),
        ],
    })
    }

    pub(super) fn b054_e0_ledger_jcs(
        tenant_id: &str,
        region: &str,
        signing_key_id: u64,
    ) -> Result<Vec<u8>, String> {
        if !canonical_partition(tenant_id, region) || signing_key_id == 0 {
            return Err("B054 E0 partition/signing key invalid".to_owned());
        }
        b054_admin_i64(signing_key_id, "signing key id")?;
        b054_admin_canonical_jcs(&B054E0LedgerJcs {
            algorithm_id: 0,
            checkpoint_type: "epoch-genesis".to_owned(),
            checkpoint_version: 1,
            epoch_id: 0,
            ledger_sequence: 0,
            ledger_version: 1,
            link_key_id: None,
            previous_ledger_hash: ZERO_HASH_HEX.to_owned(),
            region: region.to_owned(),
            signing_key_id,
            start_prev_hash: ZERO_HASH_HEX.to_owned(),
            start_sequence: 0,
            tenant_id: tenant_id.to_owned(),
        })
    }

    pub(super) fn b054_e1_ledger_jcs(
        tenant_id: &str,
        region: &str,
        head_hash: &str,
        next_sequence: u64,
        previous_ledger_hash: &str,
        link_key_id: u64,
        signing_key_id: u64,
    ) -> Result<Vec<u8>, String> {
        if !canonical_partition(tenant_id, region)
            || !is_lower_hex_32(head_hash)
            || !is_lower_hex_32(previous_ledger_hash)
            || link_key_id == 0
            || signing_key_id == 0
        {
            return Err("B054 E1 transition input invalid".to_owned());
        }
        b054_admin_i64(next_sequence, "transition sequence")?;
        b054_admin_i64(link_key_id, "link key id")?;
        b054_admin_i64(signing_key_id, "signing key id")?;
        b054_admin_canonical_jcs(&B054E1LedgerJcs {
            algorithm_id: 1,
            checkpoint_type: "epoch-transition".to_owned(),
            checkpoint_version: 1,
            epoch_id: 1,
            from_epoch_id: 0,
            from_head_hash: head_hash.to_owned(),
            from_next_sequence: next_sequence,
            ledger_sequence: 1,
            ledger_version: 1,
            link_key_id,
            previous_ledger_hash: previous_ledger_hash.to_owned(),
            region: region.to_owned(),
            signing_key_id,
            tenant_id: tenant_id.to_owned(),
            to_start_prev_hash: head_hash.to_owned(),
            to_start_sequence: next_sequence,
        })
    }

    fn b054_admin_ledger_hash(jcs: &[u8]) -> String {
        hash_domain_payload(B054_ADMIN_LEDGER_DOMAIN, jcs)
    }

    fn b054_admin_verify_artifact(
        artifact: &B054SignedArtifact,
        key: &B054AuthenticatedSigningKey,
        label: &str,
    ) -> Result<(), String> {
        b054_admin_verify_raw_signature(
            &key.verifying_key,
            &artifact.jcs,
            &artifact.signature_b64,
            label,
        )
    }

    pub(super) fn b054_verify_legacy_prefix(
        input: B054LegacyPrefixVerification<'_>,
    ) -> Result<B054VerifiedLegacyPrefix, String> {
        let B054LegacyPrefixVerification {
            tenant_id,
            region,
            head_hash,
            next_sequence,
            head_signature_b64,
            stored_signing_key_id,
            signing_key,
            rows,
        } = input;
        if !canonical_partition(tenant_id, region)
            || !is_lower_hex_32(head_hash)
            || rows.len() > B054_ADMIN_MAX_LEGACY_ROWS
            || u64::try_from(rows.len()).ok() != Some(next_sequence)
            || stored_signing_key_id != signing_key.record.signing_key_id
        {
            return Err("B054 legacy prefix shape mismatch".to_owned());
        }
        let total = rows.iter().try_fold(0usize, |total, row| {
            total
                .checked_add(row.canonical_jcs.len())
                .ok_or("B054 legacy prefix byte count overflow")
        })?;
        if total > B054_ADMIN_MAX_LEGACY_BYTES {
            return Err("B054 legacy prefix exceeds verification bound".to_owned());
        }
        let mut previous = [0_u8; 32];
        for (index, row) in rows.iter().enumerate() {
            let index = u64::try_from(index).map_err(|_| "B054 legacy index overflow")?;
            if row.sequence_number != index
                || row.prev_hash_hex != hex::encode(previous)
                || !is_lower_hex_32(&row.chain_hash_hex)
                || row.canonical_jcs.is_empty()
            {
                return Err("B054 legacy row continuity mismatch".to_owned());
            }
            let canonical_value: Value = serde_json::from_slice(&row.canonical_jcs)
                .map_err(|_| "B054 legacy row canonical_jcs is malformed")?;
            if serde_jcs::to_vec(&canonical_value)
                .map_err(|_| "B054 legacy row canonicalization failed")?
                != row.canonical_jcs
            {
                return Err("B054 legacy row is not exact RFC-8785 JCS".to_owned());
            }
            let mut hasher = blake3::Hasher::new();
            hasher.update(&previous);
            hasher.update(&row.canonical_jcs);
            let computed = *hasher.finalize().as_bytes();
            if hex::encode(computed) != row.chain_hash_hex {
                return Err("B054 legacy row link mismatch".to_owned());
            }
            previous = computed;
        }
        if hex::encode(previous) != head_hash {
            return Err("B054 legacy signed head differs from verified row tail".to_owned());
        }
        let head_jcs = canonical_head_bytes(tenant_id, region, head_hash, next_sequence)?;
        b054_admin_verify_raw_signature(
            &signing_key.verifying_key,
            &head_jcs,
            head_signature_b64,
            "B054 legacy v1 head",
        )?;
        Ok(B054VerifiedLegacyPrefix {
            tenant_id: tenant_id.to_owned(),
            region: region.to_owned(),
            head_hash: head_hash.to_owned(),
            next_sequence,
            head_signature_b64: head_signature_b64.to_owned(),
            signing_key_id: stored_signing_key_id,
            row_count: next_sequence,
        })
    }

    pub(super) fn b054_authenticate_e0_checkpoint(
        checkpoint: &HeadCheckpoint,
        tenant_id: &str,
        region: &str,
        signing_key: &B054AuthenticatedSigningKey,
        e0_ledger: &B054SignedArtifact,
        challenged_latest: &VerifiedWitness,
    ) -> Result<B054AuthenticatedE0Checkpoint, String> {
        let ledger_hash = checkpoint
            .epoch_ledger_hash
            .as_deref()
            .ok_or("B054 E0 checkpoint missing ledger hash")?;
        let signature = checkpoint
            .head_signature
            .as_deref()
            .ok_or("B054 E0 checkpoint missing head signature")?;
        let witness_sequence = checkpoint
            .head_witness_sequence
            .ok_or("B054 E0 checkpoint missing witness sequence")?;
        let witness_hash = checkpoint
            .head_witness_hash
            .as_deref()
            .ok_or("B054 E0 checkpoint missing witness hash")?;
        if !canonical_partition(tenant_id, region)
            || checkpoint.head_message_version != Some(2)
            || checkpoint.epoch_id != Some(0)
            || checkpoint.epoch_ledger_sequence != Some(0)
            // 0109 has UNIQUE(tenant,region,start_sequence). An empty E0 and
            // E1 would both start at zero, so transition must stop before the
            // witness until a forward migration represents zero-length epochs.
            || checkpoint.signing_key_id != Some(signing_key.record.signing_key_id)
            || !is_lower_hex_32(&checkpoint.head_hex)
            || !is_lower_hex_32(ledger_hash)
        {
            return Err("B054 E0 checkpoint shape mismatch".to_owned());
        }
        let canonical_ledger =
            b054_e0_ledger_jcs(tenant_id, region, signing_key.record.signing_key_id)?;
        if e0_ledger.jcs != canonical_ledger
            || b054_admin_ledger_hash(&e0_ledger.jcs) != ledger_hash
        {
            return Err("B054 E0 checkpoint ledger mismatch".to_owned());
        }
        b054_admin_verify_artifact(e0_ledger, signing_key, "B054 E0 ledger")?;
        let head_jcs = canonical_head_v2_bytes(
            tenant_id,
            region,
            &checkpoint.head_hex,
            checkpoint.next_sequence,
            0,
            0,
            ledger_hash,
            signing_key.record.signing_key_id,
        )?;
        b054_admin_verify_raw_signature(
            &signing_key.verifying_key,
            &head_jcs,
            signature,
            "B054 current E0 head",
        )?;
        if challenged_latest.record.tenant_id != tenant_id
            || challenged_latest.record.region != region
            || challenged_latest.record.witness_sequence != witness_sequence
            || challenged_latest.witness_record_hash != witness_hash
            || decode_canonical_base64(&challenged_latest.record.head_message_b64, None)?
                != head_jcs
            || challenged_latest.record.head_signature_b64 != signature
        {
            return Err("B054 E0 checkpoint differs from challenged witness latest".to_owned());
        }
        Ok(B054AuthenticatedE0Checkpoint {
            checkpoint: checkpoint.clone(),
            tenant_id: tenant_id.to_owned(),
            region: region.to_owned(),
        })
    }

    fn b054_admin_receipt_params(witness: &VerifiedWitness) -> Result<Vec<Value>, String> {
        Ok(vec![
            json!(witness.record.tenant_id),
            json!(witness.record.region),
            json!(b054_admin_i64(
                witness.record.witness_sequence,
                "witness sequence"
            )?),
            json!(witness.witness_record_hash),
            json!(witness.record.previous_witness_hash),
            json!(witness.record.head_record_hash),
            json!(b054_admin_jcs_text(&witness.witness_jcs, "witness JCS")?),
            json!(b054_admin_jcs_text(&witness.receipt_jcs, "receipt JCS")?),
            json!(witness.receipt_signature_b64),
            json!(witness.receipt.witness_id),
            json!(b054_admin_i64(witness.witness_key_id, "witness key id")?),
            json!(b054_admin_i64(
                witness.receipt.committed_at_ms,
                "witness committed time"
            )?),
        ])
    }

    fn b054_admin_receipt_insert(witness: &VerifiedWitness) -> Result<D1BatchStatement, String> {
        Ok(D1BatchStatement::new(
        "INSERT INTO audit_chain_witness_receipt (tenant_id,region,witness_sequence,witness_record_hash,previous_witness_hash,head_record_hash,witness_jcs,receipt_jcs,receipt_signature_b64,witness_id,witness_key_id,committed_at_ms) SELECT ?1,?2,?3,?4,?5,?6,CAST(?7 AS BLOB),CAST(?8 AS BLOB),?9,?10,?11,?12 WHERE NOT EXISTS (SELECT 1 FROM audit_chain_witness_receipt WHERE tenant_id=?1 AND region=?2 AND witness_sequence=?3 AND witness_record_hash=?4 AND previous_witness_hash=?5 AND head_record_hash=?6 AND witness_jcs=CAST(?7 AS BLOB) AND receipt_jcs=CAST(?8 AS BLOB) AND receipt_signature_b64=?9 AND witness_id=?10 AND witness_key_id=?11 AND committed_at_ms=?12)",
        b054_admin_receipt_params(witness)?,
    ))
    }

    /// Build the post-witness, single-batch E0 bootstrap. Registry provisioning is
    /// intentionally absent: `signing_key` must already exist byte-for-byte in D1.
    #[allow(clippy::too_many_arguments, reason = "explicit cryptographic boundary")]
    pub(super) fn b054_build_e0_bootstrap_transaction(
        legacy: &B054VerifiedLegacyPrefix,
        signing_key: &B054AuthenticatedSigningKey,
        e0_ledger: &B054SignedArtifact,
        v2_head: &B054SignedArtifact,
        witness: &VerifiedWitness,
        now_ms: u64,
        lease_holder: &str,
    ) -> Result<B054AdminTransaction, String> {
        if legacy.signing_key_id != signing_key.record.signing_key_id
            || witness.record.witness_sequence != 0
            || witness.record.previous_witness_hash != ZERO_HASH_HEX
            || witness.record.tenant_id != legacy.tenant_id
            || witness.record.region != legacy.region
        {
            return Err("B054 E0 bootstrap preflight mismatch".to_owned());
        }
        let expected_e0 = b054_e0_ledger_jcs(
            &legacy.tenant_id,
            &legacy.region,
            signing_key.record.signing_key_id,
        )?;
        if e0_ledger.jcs != expected_e0 {
            return Err("B054 E0 ledger bytes differ from canonical contract".to_owned());
        }
        b054_admin_verify_artifact(e0_ledger, signing_key, "B054 E0 ledger")?;
        let ledger_hash = b054_admin_ledger_hash(&e0_ledger.jcs);
        let expected_head = canonical_head_v2_bytes(
            &legacy.tenant_id,
            &legacy.region,
            &legacy.head_hash,
            legacy.next_sequence,
            0,
            0,
            &ledger_hash,
            signing_key.record.signing_key_id,
        )?;
        if v2_head.jcs != expected_head {
            return Err("B054 E0 v2 head bytes differ from canonical contract".to_owned());
        }
        b054_admin_verify_artifact(v2_head, signing_key, "B054 E0 v2 head")?;
        if decode_canonical_base64(&witness.record.head_message_b64, None)? != v2_head.jcs
            || witness.record.head_signature_b64 != v2_head.signature_b64
        {
            return Err("B054 E0 witness does not bind prepared head".to_owned());
        }

        let tenant = &legacy.tenant_id;
        let region = &legacy.region;
        let key_id = b054_admin_i64(signing_key.record.signing_key_id, "signing key id")?;
        let next = b054_admin_i64(legacy.next_sequence, "legacy next sequence")?;
        let now = b054_admin_i64(now_ms, "bootstrap time")?;
        let ledger_jcs = b054_admin_jcs_text(&e0_ledger.jcs, "E0 ledger JCS")?;
        let signing_jcs = b054_admin_jcs_text(&signing_key.registry_jcs, "signing registry JCS")?;
        let commit_id = uuid::Uuid::new_v4().to_string();
        let mut statements = vec![
        D1BatchStatement::new(
            "INSERT INTO audit_chain_epoch_ledger (tenant_id,region,ledger_sequence,ledger_version,entry_type,epoch_id,previous_ledger_hash,ledger_hash,entry_jcs,signature_b64,signing_key_id,created_at_ms) SELECT ?1,?2,0,1,'epoch-genesis',0,?3,?4,CAST(?5 AS BLOB),?6,?7,?8 WHERE NOT EXISTS (SELECT 1 FROM audit_chain_epoch_ledger WHERE tenant_id=?1 AND region=?2 AND ledger_sequence=0 AND epoch_id=0 AND previous_ledger_hash=?3 AND ledger_hash=?4 AND entry_jcs=CAST(?5 AS BLOB) AND signature_b64=?6 AND signing_key_id=?7)",
            vec![json!(tenant),json!(region),json!(ZERO_HASH_HEX),json!(ledger_hash),json!(ledger_jcs),json!(e0_ledger.signature_b64),json!(key_id),json!(now)],
        ),
        D1BatchStatement::new(
            "INSERT INTO audit_chain_epoch (tenant_id,region,epoch_id,algorithm_id,link_key_id,state,start_sequence,start_prev_hash,predecessor_epoch_id,open_ledger_sequence,open_ledger_hash) SELECT ?1,?2,0,0,NULL,'active',0,?3,NULL,0,?4 WHERE NOT EXISTS (SELECT 1 FROM audit_chain_epoch WHERE tenant_id=?1 AND region=?2 AND epoch_id=0 AND algorithm_id=0 AND link_key_id IS NULL AND state='active' AND start_sequence=0 AND start_prev_hash=?3 AND predecessor_epoch_id IS NULL AND open_ledger_sequence=0 AND open_ledger_hash=?4)",
            vec![json!(tenant),json!(region),json!(ZERO_HASH_HEX),json!(ledger_hash)],
        ),
        D1BatchStatement::new(
            "UPDATE audit_chain_head SET head_message_version=2,epoch_id=0,epoch_ledger_sequence=0,epoch_ledger_hash=?1,head_signature=?2,head_signed_at_ms=?3,signing_key_id=?4,head_witness_sequence=0,head_witness_hash=?5,updated_at=?3 WHERE tenant_id=?6 AND region=?7 AND head_hash=?8 AND next_sequence=?9 AND head_signature=?10 AND signing_key_id=?4 AND epoch_id IS NULL AND head_message_version IS NULL AND epoch_ledger_sequence IS NULL AND epoch_ledger_hash IS NULL AND head_witness_sequence IS NULL AND head_witness_hash IS NULL",
            vec![json!(ledger_hash),json!(v2_head.signature_b64),json!(now),json!(key_id),json!(witness.witness_record_hash),json!(tenant),json!(region),json!(legacy.head_hash),json!(next),json!(legacy.head_signature_b64)],
        ),
    ];
        statements.push(b054_admin_receipt_insert(witness)?);
        statements.push(D1BatchStatement::new(
        "INSERT INTO audit_chain_v2_tx_assert(commit_id,assertion) SELECT ?1,CASE WHEN EXISTS (SELECT 1 FROM audit_chain_signing_key_registry WHERE signing_key_id=?2 AND registry_jcs=CAST(?3 AS BLOB) AND registry_signature_b64=?4) AND EXISTS (SELECT 1 FROM audit_chain_epoch_ledger WHERE tenant_id=?5 AND region=?6 AND ledger_sequence=0 AND epoch_id=0 AND ledger_hash=?7 AND entry_jcs=CAST(?8 AS BLOB) AND signature_b64=?9 AND signing_key_id=?2) AND EXISTS (SELECT 1 FROM audit_chain_epoch WHERE tenant_id=?5 AND region=?6 AND epoch_id=0 AND state='active' AND algorithm_id=0 AND link_key_id IS NULL AND start_sequence=0 AND start_prev_hash=?10 AND open_ledger_sequence=0 AND open_ledger_hash=?7) AND EXISTS (SELECT 1 FROM audit_chain_head WHERE tenant_id=?5 AND region=?6 AND head_hash=?11 AND next_sequence=?12 AND head_message_version=2 AND epoch_id=0 AND epoch_ledger_sequence=0 AND epoch_ledger_hash=?7 AND head_signature=?13 AND signing_key_id=?2 AND head_witness_sequence=0 AND head_witness_hash=?14) AND EXISTS (SELECT 1 FROM audit_chain_witness_receipt WHERE tenant_id=?5 AND region=?6 AND witness_sequence=0 AND witness_record_hash=?14 AND receipt_signature_b64=?15) AND EXISTS (SELECT 1 FROM audit_drain_lease WHERE tenant_id=?5 AND region=?6 AND holder=?16) AND (SELECT COUNT(*) FROM audit_outbox WHERE tenant_id=?5 AND region=?6 AND emitted_at IS NOT NULL)=?12 AND (?12=0 OR EXISTS (SELECT 1 FROM audit_outbox WHERE tenant_id=?5 AND region=?6 AND emitted_at IS NOT NULL AND sequence_number=?12-1 AND chain_hash=?11)) AND NOT EXISTS (SELECT 1 FROM audit_outbox WHERE tenant_id=?5 AND region=?6 AND emitted_at IS NOT NULL AND (sequence_number IS NULL OR prev_hash IS NULL OR chain_hash IS NULL OR canonical_jcs IS NULL)) THEN 1 ELSE 0 END",
        vec![json!(commit_id),json!(key_id),json!(signing_jcs),json!(signing_key.registry_signature_b64),json!(tenant),json!(region),json!(ledger_hash),json!(ledger_jcs),json!(e0_ledger.signature_b64),json!(ZERO_HASH_HEX),json!(legacy.head_hash),json!(next),json!(v2_head.signature_b64),json!(witness.witness_record_hash),json!(witness.receipt_signature_b64),json!(lease_holder)],
    ));
        statements.push(D1BatchStatement::new(
            "DELETE FROM audit_chain_v2_tx_assert WHERE commit_id=?1",
            vec![json!(commit_id)],
        ));
        Ok(B054AdminTransaction { statements })
    }

    /// Build the post-witness E0->E1 transaction. The link registry is only checked,
    /// never inserted here, so a global-key race cannot occur after linearization.
    #[allow(clippy::too_many_arguments, reason = "explicit cryptographic boundary")]
    pub(super) fn b054_build_e0_to_e1_transaction(
        authenticated_e0: &B054AuthenticatedE0Checkpoint,
        signing_key: &B054AuthenticatedSigningKey,
        link_key: &B054AuthenticatedLinkKey,
        transition: &B054SignedArtifact,
        v2_head: &B054SignedArtifact,
        witness: &VerifiedWitness,
        now_ms: u64,
        lease_holder: &str,
    ) -> Result<B054AdminTransaction, String> {
        let expected = &authenticated_e0.checkpoint;
        let tenant_id = authenticated_e0.tenant_id.as_str();
        let region = authenticated_e0.region.as_str();
        let old_ledger_hash = expected
            .epoch_ledger_hash
            .as_deref()
            .ok_or("B054 E0 head missing ledger hash")?;
        let old_witness_hash = expected
            .head_witness_hash
            .as_deref()
            .ok_or("B054 E0 head missing witness hash")?;
        let old_signature = expected
            .head_signature
            .as_deref()
            .ok_or("B054 E0 head missing signature")?;
        let old_witness_sequence = expected
            .head_witness_sequence
            .ok_or("B054 E0 head missing witness sequence")?;
        if !canonical_partition(tenant_id, region)
            || expected.head_message_version != Some(2)
            || expected.epoch_id != Some(0)
            || expected.epoch_ledger_sequence != Some(0)
            || expected.signing_key_id != Some(signing_key.record.signing_key_id)
            || link_key.record.signing_key_id != signing_key.record.signing_key_id
            || witness.record.tenant_id != tenant_id
            || witness.record.region != region
            || witness.record.witness_sequence
                != old_witness_sequence
                    .checked_add(1)
                    .ok_or("B054 witness sequence overflow")?
            || witness.record.previous_witness_hash != old_witness_hash
        {
            return Err("B054 E0->E1 preflight mismatch".to_owned());
        }
        let expected_transition = b054_e1_ledger_jcs(
            tenant_id,
            region,
            &expected.head_hex,
            expected.next_sequence,
            old_ledger_hash,
            link_key.record.link_key_id,
            signing_key.record.signing_key_id,
        )?;
        if transition.jcs != expected_transition {
            return Err("B054 E1 ledger bytes differ from canonical contract".to_owned());
        }
        b054_admin_verify_artifact(transition, signing_key, "B054 E1 ledger")?;
        let new_ledger_hash = b054_admin_ledger_hash(&transition.jcs);
        let expected_head = canonical_head_v2_bytes(
            tenant_id,
            region,
            &expected.head_hex,
            expected.next_sequence,
            1,
            1,
            &new_ledger_hash,
            signing_key.record.signing_key_id,
        )?;
        if v2_head.jcs != expected_head {
            return Err("B054 E1 v2 head bytes differ from canonical contract".to_owned());
        }
        b054_admin_verify_artifact(v2_head, signing_key, "B054 E1 v2 head")?;
        if decode_canonical_base64(&witness.record.head_message_b64, None)? != v2_head.jcs
            || witness.record.head_signature_b64 != v2_head.signature_b64
        {
            return Err("B054 E1 witness does not bind prepared head".to_owned());
        }

        let key_id = b054_admin_i64(signing_key.record.signing_key_id, "signing key id")?;
        let link_id = b054_admin_i64(link_key.record.link_key_id, "link key id")?;
        let next = b054_admin_i64(expected.next_sequence, "head next sequence")?;
        let old_wseq = b054_admin_i64(old_witness_sequence, "old witness sequence")?;
        let new_wseq = b054_admin_i64(witness.record.witness_sequence, "new witness sequence")?;
        let now = b054_admin_i64(now_ms, "transition time")?;
        let ledger_jcs = b054_admin_jcs_text(&transition.jcs, "E1 ledger JCS")?;
        let link_jcs = b054_admin_jcs_text(&link_key.registry_jcs, "link registry JCS")?;
        let commit_id = uuid::Uuid::new_v4().to_string();
        let mut statements = vec![
        D1BatchStatement::new(
            "INSERT INTO audit_chain_epoch_ledger (tenant_id,region,ledger_sequence,ledger_version,entry_type,epoch_id,previous_ledger_hash,ledger_hash,entry_jcs,signature_b64,signing_key_id,created_at_ms) SELECT ?1,?2,1,1,'epoch-transition',1,?3,?4,CAST(?5 AS BLOB),?6,?7,?8 WHERE NOT EXISTS (SELECT 1 FROM audit_chain_epoch_ledger WHERE tenant_id=?1 AND region=?2 AND ledger_sequence=1 AND epoch_id=1 AND previous_ledger_hash=?3 AND ledger_hash=?4 AND entry_jcs=CAST(?5 AS BLOB) AND signature_b64=?6 AND signing_key_id=?7)",
            vec![json!(tenant_id),json!(region),json!(old_ledger_hash),json!(new_ledger_hash),json!(ledger_jcs),json!(transition.signature_b64),json!(key_id),json!(now)],
        ),
        D1BatchStatement::new(
            "UPDATE audit_chain_epoch SET state='closed',closed_at_ms=?1,end_sequence_exclusive=?2,end_head_hash=?3 WHERE tenant_id=?4 AND region=?5 AND epoch_id=0 AND state='active' AND algorithm_id=0 AND link_key_id IS NULL AND start_sequence=0 AND start_prev_hash=?6 AND predecessor_epoch_id IS NULL AND open_ledger_sequence=0 AND open_ledger_hash=?7",
            vec![json!(now),json!(next),json!(expected.head_hex),json!(tenant_id),json!(region),json!(ZERO_HASH_HEX),json!(old_ledger_hash)],
        ),
        D1BatchStatement::new(
            "INSERT INTO audit_chain_epoch (tenant_id,region,epoch_id,algorithm_id,link_key_id,state,start_sequence,start_prev_hash,predecessor_epoch_id,open_ledger_sequence,open_ledger_hash) SELECT ?1,?2,1,1,?3,'active',?4,?5,0,1,?6 WHERE NOT EXISTS (SELECT 1 FROM audit_chain_epoch WHERE tenant_id=?1 AND region=?2 AND epoch_id=1 AND algorithm_id=1 AND link_key_id=?3 AND state='active' AND start_sequence=?4 AND start_prev_hash=?5 AND predecessor_epoch_id=0 AND open_ledger_sequence=1 AND open_ledger_hash=?6)",
            vec![json!(tenant_id),json!(region),json!(link_id),json!(next),json!(expected.head_hex),json!(new_ledger_hash)],
        ),
        D1BatchStatement::new(
            "UPDATE audit_chain_head SET epoch_id=1,epoch_ledger_sequence=1,epoch_ledger_hash=?1,head_signature=?2,head_signed_at_ms=?3,head_witness_sequence=?4,head_witness_hash=?5,updated_at=?3 WHERE tenant_id=?6 AND region=?7 AND head_hash=?8 AND next_sequence=?9 AND head_message_version=2 AND epoch_id=0 AND epoch_ledger_sequence=0 AND epoch_ledger_hash=?10 AND head_signature=?11 AND signing_key_id=?12 AND head_witness_sequence=?13 AND head_witness_hash=?14",
            vec![json!(new_ledger_hash),json!(v2_head.signature_b64),json!(now),json!(new_wseq),json!(witness.witness_record_hash),json!(tenant_id),json!(region),json!(expected.head_hex),json!(next),json!(old_ledger_hash),json!(old_signature),json!(key_id),json!(old_wseq),json!(old_witness_hash)],
        ),
    ];
        statements.push(b054_admin_receipt_insert(witness)?);
        statements.push(D1BatchStatement::new(
        "INSERT INTO audit_chain_v2_tx_assert(commit_id,assertion) SELECT ?1,CASE WHEN EXISTS (SELECT 1 FROM audit_chain_link_key_registry WHERE link_key_id=?2 AND key_commitment_hex=?3 AND registry_jcs=CAST(?4 AS BLOB) AND registry_signature_b64=?5 AND signing_key_id=?6) AND EXISTS (SELECT 1 FROM audit_chain_epoch_ledger WHERE tenant_id=?7 AND region=?8 AND ledger_sequence=1 AND epoch_id=1 AND previous_ledger_hash=?9 AND ledger_hash=?10 AND entry_jcs=CAST(?11 AS BLOB) AND signature_b64=?12 AND signing_key_id=?6) AND EXISTS (SELECT 1 FROM audit_chain_epoch WHERE tenant_id=?7 AND region=?8 AND epoch_id=0 AND state='closed' AND end_sequence_exclusive=?13 AND end_head_hash=?14 AND open_ledger_hash=?9) AND EXISTS (SELECT 1 FROM audit_chain_epoch WHERE tenant_id=?7 AND region=?8 AND epoch_id=1 AND state='active' AND algorithm_id=1 AND link_key_id=?2 AND start_sequence=?13 AND start_prev_hash=?14 AND predecessor_epoch_id=0 AND open_ledger_sequence=1 AND open_ledger_hash=?10) AND EXISTS (SELECT 1 FROM audit_chain_head WHERE tenant_id=?7 AND region=?8 AND head_hash=?14 AND next_sequence=?13 AND head_message_version=2 AND epoch_id=1 AND epoch_ledger_sequence=1 AND epoch_ledger_hash=?10 AND head_signature=?15 AND signing_key_id=?6 AND head_witness_sequence=?16 AND head_witness_hash=?17) AND EXISTS (SELECT 1 FROM audit_chain_witness_receipt WHERE tenant_id=?7 AND region=?8 AND witness_sequence=?16 AND witness_record_hash=?17 AND receipt_signature_b64=?18) AND EXISTS (SELECT 1 FROM audit_drain_lease WHERE tenant_id=?7 AND region=?8 AND holder=?19) THEN 1 ELSE 0 END",
        vec![json!(commit_id),json!(link_id),json!(link_key.record.key_commitment_hex),json!(link_jcs),json!(link_key.registry_signature_b64),json!(key_id),json!(tenant_id),json!(region),json!(old_ledger_hash),json!(new_ledger_hash),json!(ledger_jcs),json!(transition.signature_b64),json!(next),json!(expected.head_hex),json!(v2_head.signature_b64),json!(new_wseq),json!(witness.witness_record_hash),json!(witness.receipt_signature_b64),json!(lease_holder)],
    ));
        statements.push(D1BatchStatement::new(
            "DELETE FROM audit_chain_v2_tx_assert WHERE commit_id=?1",
            vec![json!(commit_id)],
        ));
        Ok(B054AdminTransaction { statements })
    }

    include!("b054_epoch_admin_part2.rs");
}

pub(crate) use b054_epoch_admin::{
    b054_archive_authenticate_link_registry, b054_archive_authenticate_signing_registry,
    b054_archive_load_manifest_signer, b054_archive_parse_trust_roots, B054ArchiveManifestSigner,
    B054ArchiveSigningKey, B054ArchiveTrustRoots,
};
