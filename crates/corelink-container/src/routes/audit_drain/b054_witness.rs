// B-054 client/verifier for the independently administered head witness.
// Every byte received from the witness is hostile. Both signed payloads are
// checked against their exact RFC-8785 encoding, pinned witness identity and
// root-authorized Ed25519 key registry before a caller may mutate D1.

use rand::{rngs::OsRng, RngCore as _};
use serde::Deserialize;
use std::{collections::BTreeMap, time::Duration};
use url::Url;

const WITNESS_DOMAIN: &[u8] = b"corelink/audit-chain/head-witness/v1\0";
const HEAD_RECORD_DOMAIN: &[u8] = b"corelink/audit-chain/head-record/v1\0";
const RECEIPT_DOMAIN: &[u8] = b"corelink/audit-chain/head-witness-receipt/v1\0";
const LATEST_DOMAIN: &[u8] = b"corelink/audit-chain/head-witness-latest/v1\0";
const MAX_WITNESS_RESPONSE_BYTES: usize = 32 * 1024;
const JS_SAFE_MAX: u64 = 9_007_199_254_740_991;
const ZERO_HASH_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Clone)]
pub(crate) struct WitnessClient {
    http: reqwest::Client,
    origin: Url,
    append_token: Arc<Zeroizing<String>>,
    witness_id: String,
    public_keys: Arc<BTreeMap<u64, ed25519_dalek::VerifyingKey>>,
}

impl std::fmt::Debug for WitnessClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WitnessClient")
            .field("origin", &self.origin)
            .field("append_token", &"<redacted>")
            .field("witness_id", &self.witness_id)
            .field("public_key_count", &self.public_keys.len())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessRecord {
    head_message_b64: String,
    head_record_hash: String,
    head_signature_b64: String,
    previous_witness_hash: String,
    region: String,
    tenant_id: String,
    witness_sequence: u64,
    witness_version: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessHeadV2 {
    epoch_id: u64,
    epoch_ledger_hash: String,
    epoch_ledger_sequence: u64,
    head_hash: String,
    head_message_version: u8,
    next_sequence: u64,
    region: String,
    signing_key_id: u64,
    tenant_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum EpochLedgerJcs {
    Genesis(EpochGenesisJcs),
    Transition(EpochTransitionJcs),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EpochGenesisJcs {
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EpochTransitionJcs {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessReceiptJcs {
    committed_at_ms: u64,
    head_record_hash: String,
    previous_witness_hash: String,
    receipt_version: u8,
    region: String,
    tenant_id: String,
    witness_id: String,
    witness_key_id: u64,
    witness_record_hash: String,
    witness_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessReceipt {
    receipt_jcs_b64: String,
    receipt_signature_b64: String,
    witness_jcs_b64: String,
    witness_key_id: u64,
    witness_record_hash: String,
    witness_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredWitnessReceipt {
    receipt_jcs_b64: String,
    receipt_signature_b64: String,
    witness_jcs_b64: String,
    witness_key_id: u64,
    witness_record_hash: String,
    witness_sequence: u64,
    tenant_id: String,
    region: String,
}

#[derive(Debug, Serialize)]
struct LatestRequest<'a> {
    challenge_b64: &'a str,
    latest_request_version: u8,
    region: &'a str,
    tenant_id: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LatestEnvelope {
    latest_jcs_b64: String,
    latest_signature_b64: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LatestJcs {
    challenge_b64: String,
    latest: Option<StoredWitnessReceipt>,
    latest_version: u8,
    observed_at_ms: u64,
    witness_key_id: u64,
    region: String,
    tenant_id: String,
    witness_id: String,
}

#[derive(Debug, Serialize)]
struct AppendRequest<'a> {
    append_request_version: u8,
    expected_latest: Option<ExpectedLatest<'a>>,
    witness_jcs_b64: &'a str,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct ExpectedLatest<'a> {
    witness_record_hash: &'a str,
    witness_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedWitness {
    record: WitnessRecord,
    witness_jcs: Vec<u8>,
    receipt_jcs: Vec<u8>,
    receipt_signature_b64: String,
    witness_record_hash: String,
    witness_key_id: u64,
    receipt: WitnessReceiptJcs,
}

/// Stored witness evidence as read from D1. Construction is intentionally
/// separate from verification: callers cannot obtain a [`VerifiedWitness`]
/// without checking canonical bytes, every signed binding and the pinned
/// witness key registry held by [`WitnessClient`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct B054StoredWitnessReceiptInput {
    pub(crate) witness_jcs: Vec<u8>,
    pub(crate) receipt_jcs: Vec<u8>,
    pub(crate) receipt_signature_b64: String,
    pub(crate) witness_key_id: u64,
    pub(crate) witness_record_hash: String,
    pub(crate) witness_sequence: u64,
}

/// Non-secret archive projection of fully verified witness evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct B054WitnessAnchor {
    pub(crate) tenant_id: String,
    pub(crate) region: String,
    pub(crate) witness_sequence: u64,
    pub(crate) witness_record_hash: String,
    pub(crate) previous_witness_hash: String,
    pub(crate) head_record_hash: String,
    pub(crate) head_hash: String,
    pub(crate) head_next_sequence: u64,
    pub(crate) epoch_id: u64,
    pub(crate) epoch_ledger_sequence: u64,
    pub(crate) epoch_ledger_hash: String,
    pub(crate) signing_key_id: u64,
    pub(crate) head_jcs: Vec<u8>,
    pub(crate) head_signature_b64: String,
}

impl VerifiedWitness {
    /// Project only authenticated, non-secret fields needed by archival. The
    /// signed v2 head is parsed again from the exact bytes already validated by
    /// `verify_receipt`, retaining the opaque trust boundary for callers.
    pub(crate) fn archive_anchor(&self) -> Result<B054WitnessAnchor, String> {
        let head_jcs = decode_canonical_base64(&self.record.head_message_b64, None)?;
        let head: WitnessHeadV2 = serde_json::from_slice(&head_jcs)
            .map_err(|_| "verified witness head became malformed")?;
        if serde_jcs::to_vec(&head).map_err(|_| "verified witness head JCS failure")? != head_jcs {
            return Err("verified witness head lost canonical form".to_owned());
        }
        Ok(B054WitnessAnchor {
            tenant_id: self.record.tenant_id.clone(),
            region: self.record.region.clone(),
            witness_sequence: self.record.witness_sequence,
            witness_record_hash: self.witness_record_hash.clone(),
            previous_witness_hash: self.record.previous_witness_hash.clone(),
            head_record_hash: self.record.head_record_hash.clone(),
            head_hash: head.head_hash,
            head_next_sequence: head.next_sequence,
            epoch_id: head.epoch_id,
            epoch_ledger_sequence: head.epoch_ledger_sequence,
            epoch_ledger_hash: head.epoch_ledger_hash,
            signing_key_id: head.signing_key_id,
            head_jcs,
            head_signature_b64: self.record.head_signature_b64.clone(),
        })
    }
}

fn canonical_partition(tenant_id: &str, region: &str) -> bool {
    let tenant = uuid::Uuid::parse_str(tenant_id).ok();
    tenant.is_some_and(|id| {
        id.get_version_num() == 7
            && id.get_variant() == uuid::Variant::RFC4122
            && id.hyphenated().to_string() == tenant_id
    }) && matches!(region, "wnam" | "enam" | "weur" | "sam" | "apac" | "afr")
}

fn canonical_witness_id(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    value.len() <= 128
        && first.is_ascii_alphanumeric()
        && bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn decode_canonical_base64(value: &str, exact_len: Option<usize>) -> Result<Vec<u8>, String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| "witness returned malformed base64".to_owned())?;
    if exact_len.is_some_and(|length| decoded.len() != length)
        || base64::engine::general_purpose::STANDARD.encode(&decoded) != value
    {
        return Err("witness returned non-canonical base64".to_owned());
    }
    Ok(decoded)
}

/// Decode the two BLOB encodings emitted by D1's HTTP API. A string is always
/// canonical padded base64, never treated as raw JSON/text; accepting both
/// interpretations would make the signed ledger bytes storage-shape-dependent.
fn decode_d1_blob(value: &Value, field: &str, max_len: usize) -> Result<Vec<u8>, String> {
    let bytes = match value {
        Value::Array(items) => {
            let mut bytes = Vec::with_capacity(items.len());
            for item in items {
                let byte = item
                    .as_u64()
                    .and_then(|number| u8::try_from(number).ok())
                    .ok_or_else(|| format!("{field} contains a non-byte BLOB element"))?;
                bytes.push(byte);
            }
            bytes
        }
        Value::String(encoded) => {
            let bytes = decode_canonical_base64(encoded, None)
                .map_err(|_| format!("{field} is not canonical base64 BLOB data"))?;
            bytes
        }
        _ => return Err(format!("{field} has unsupported D1 BLOB encoding")),
    };
    if bytes.is_empty() || bytes.len() > max_len {
        return Err(format!("{field} BLOB length is outside its bound"));
    }
    Ok(bytes)
}

fn canonical_decode<T>(value: &str, max_len: usize) -> Result<(T, Vec<u8>), String>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let decoded = decode_canonical_base64(value, None)?;
    if decoded.len() > max_len {
        return Err("witness signed payload exceeds size limit".to_owned());
    }
    let parsed: T = serde_json::from_slice(&decoded)
        .map_err(|_| "witness signed payload is malformed".to_owned())?;
    let canonical = serde_jcs::to_vec(&parsed)
        .map_err(|_| "witness signed payload cannot be canonicalized".to_owned())?;
    if canonical != decoded {
        return Err("witness signed payload is not exact RFC-8785 JCS".to_owned());
    }
    Ok((parsed, decoded))
}

fn length_prefixed(domain: &[u8], payload: &[u8]) -> Result<Vec<u8>, String> {
    let length = u64::try_from(payload.len()).map_err(|_| "witness payload length overflow")?;
    let mut message = Vec::with_capacity(domain.len() + 8 + payload.len());
    message.extend_from_slice(domain);
    message.extend_from_slice(&length.to_be_bytes());
    message.extend_from_slice(payload);
    Ok(message)
}

fn hash_domain_payload(domain: &[u8], payload: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(payload);
    hasher.finalize().to_hex().to_string()
}

fn verify_signature(
    keys: &BTreeMap<u64, ed25519_dalek::VerifyingKey>,
    key_id: u64,
    domain: &[u8],
    payload: &[u8],
    signature_b64: &str,
) -> Result<(), String> {
    let key = keys
        .get(&key_id)
        .ok_or_else(|| format!("witness response uses unknown key id {key_id}"))?;
    let signature = decode_canonical_base64(signature_b64, Some(64))?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| "witness signature has wrong length".to_owned())?;
    key.verify(
        &length_prefixed(domain, payload)?,
        &Signature::from_bytes(&signature),
    )
    .map_err(|_| "witness signature verification failed".to_owned())
}

fn validate_witness_record(record: &WitnessRecord) -> Result<(), String> {
    if record.witness_version != 1
        || !canonical_partition(&record.tenant_id, &record.region)
        || record.previous_witness_hash.len() != 64
        || record.witness_sequence > JS_SAFE_MAX
        || !record
            .previous_witness_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("witness record contract mismatch".to_owned());
    }
    let head_jcs = decode_canonical_base64(&record.head_message_b64, None)?;
    let head: WitnessHeadV2 =
        serde_json::from_slice(&head_jcs).map_err(|_| "witness head is malformed".to_owned())?;
    if serde_jcs::to_vec(&head).map_err(|_| "witness head JCS failure")? != head_jcs
        || head.head_message_version != 2
        || head.signing_key_id == 0
        || head.signing_key_id > JS_SAFE_MAX
        || head.epoch_id > JS_SAFE_MAX
        || head.epoch_ledger_sequence > JS_SAFE_MAX
        || head.next_sequence > JS_SAFE_MAX
        || head.tenant_id != record.tenant_id
        || head.region != record.region
        || !is_lower_hex_32(&head.head_hash)
        || !is_lower_hex_32(&head.epoch_ledger_hash)
    {
        return Err("witness head is not exact canonical v2".to_owned());
    }
    let head_signature = decode_canonical_base64(&record.head_signature_b64, Some(64))?;
    let mut material = length_prefixed(HEAD_RECORD_DOMAIN, &head_jcs)?;
    material.extend_from_slice(&head_signature);
    if !is_lower_hex_32(&record.head_record_hash)
        || hash_domain_payload(&[], &material) != record.head_record_hash
    {
        return Err("witness record hash mismatch".to_owned());
    }
    Ok(())
}

fn verify_receipt(
    receipt: &WitnessReceipt,
    expected_witness_jcs: Option<&[u8]>,
    witness_id: &str,
    keys: &BTreeMap<u64, ed25519_dalek::VerifyingKey>,
) -> Result<VerifiedWitness, String> {
    let witness_jcs = decode_canonical_base64(&receipt.witness_jcs_b64, None)?;
    if expected_witness_jcs.is_some_and(|expected| expected != witness_jcs) {
        return Err("witness receipt names different witness bytes".to_owned());
    }
    let witness_record_hash = hash_domain_payload(WITNESS_DOMAIN, &witness_jcs);
    let record: WitnessRecord = serde_json::from_slice(&witness_jcs)
        .map_err(|_| "witness record is malformed".to_owned())?;
    if serde_jcs::to_vec(&record).map_err(|_| "witness record JCS failure")? != witness_jcs {
        return Err("witness record is not exact RFC-8785 JCS".to_owned());
    }
    validate_witness_record(&record)?;
    let (signed, receipt_jcs): (WitnessReceiptJcs, Vec<u8>) =
        canonical_decode(&receipt.receipt_jcs_b64, 16 * 1024)?;
    if signed.receipt_version != 1
        || signed.committed_at_ms > JS_SAFE_MAX
        || signed.witness_key_id > JS_SAFE_MAX
        || signed.witness_sequence > JS_SAFE_MAX
        || signed.witness_id != witness_id
        || signed.witness_key_id != receipt.witness_key_id
        || signed.witness_record_hash != witness_record_hash
        || receipt.witness_record_hash != witness_record_hash
        || signed.witness_sequence != record.witness_sequence
        || receipt.witness_sequence != record.witness_sequence
        || signed.head_record_hash != record.head_record_hash
        || signed.previous_witness_hash != record.previous_witness_hash
        || signed.tenant_id != record.tenant_id
        || signed.region != record.region
    {
        return Err("witness receipt fields do not bind the record".to_owned());
    }
    verify_signature(
        keys,
        receipt.witness_key_id,
        RECEIPT_DOMAIN,
        &receipt_jcs,
        &receipt.receipt_signature_b64,
    )?;
    Ok(VerifiedWitness {
        record,
        witness_jcs,
        receipt_jcs,
        receipt_signature_b64: receipt.receipt_signature_b64.clone(),
        witness_record_hash,
        witness_key_id: receipt.witness_key_id,
        receipt: signed,
    })
}

fn verify_v2_checkpoint_witness(
    checkpoint: &HeadCheckpoint,
    latest: &VerifiedWitness,
    signing_seed: &[u8; 32],
    tenant_id: &str,
    region: &str,
) -> Result<(), String> {
    let epoch_id = checkpoint
        .epoch_id
        .ok_or("v2 checkpoint missing epoch_id")?;
    let ledger_sequence = checkpoint
        .epoch_ledger_sequence
        .ok_or("v2 checkpoint missing epoch_ledger_sequence")?;
    let ledger_hash = checkpoint
        .epoch_ledger_hash
        .as_deref()
        .ok_or("v2 checkpoint missing epoch_ledger_hash")?;
    let witness_sequence = checkpoint
        .head_witness_sequence
        .ok_or("v2 checkpoint missing witness sequence")?;
    let witness_hash = checkpoint
        .head_witness_hash
        .as_deref()
        .ok_or("v2 checkpoint missing witness hash")?;
    let signing_key_id = checkpoint
        .signing_key_id
        .ok_or("v2 checkpoint missing signing key id")?;
    let signature = checkpoint
        .head_signature
        .as_deref()
        .ok_or("v2 checkpoint missing signature")?;
    if checkpoint.head_message_version != Some(2)
        || latest.record.tenant_id != tenant_id
        || latest.record.region != region
        || latest.record.witness_sequence != witness_sequence
        || latest.witness_record_hash != witness_hash
    {
        return Err("D1 v2 head does not equal external witness latest".to_owned());
    }
    let head_jcs = canonical_head_v2_bytes(
        tenant_id,
        region,
        &checkpoint.head_hex,
        checkpoint.next_sequence,
        epoch_id,
        ledger_sequence,
        ledger_hash,
        signing_key_id,
    )?;
    let witnessed_head_jcs = decode_canonical_base64(&latest.record.head_message_b64, None)?;
    if witnessed_head_jcs != head_jcs || latest.record.head_signature_b64 != signature {
        return Err("external latest names different v2 head bytes/signature".to_owned());
    }
    if !verify_head_v2(
        signing_seed,
        signing_key_id,
        tenant_id,
        region,
        &checkpoint.head_hex,
        checkpoint.next_sequence,
        epoch_id,
        ledger_sequence,
        ledger_hash,
        signature,
    ) {
        return Err("D1 v2 head signature verification failed".to_owned());
    }
    Ok(())
}

fn authenticate_active_epoch(
    active: &ActiveEpoch,
    signing_seed: &[u8; 32],
    current_signing_key_id: u64,
    tenant_id: &str,
    region: &str,
) -> Result<(), String> {
    if active.ledger_signing_key_id != current_signing_key_id
        || hash_domain_payload(
            b"corelink/audit-chain/epoch-ledger/v1\0",
            &active.ledger_jcs,
        ) != active.ledger_hash
    {
        return Err("active epoch ledger hash/signing-key mismatch".to_owned());
    }
    let entry: EpochLedgerJcs = serde_json::from_slice(&active.ledger_jcs)
        .map_err(|_| "active epoch ledger JCS malformed".to_owned())?;
    if serde_jcs::to_vec(&entry).map_err(|_| "active epoch ledger JCS failure")?
        != active.ledger_jcs
    {
        return Err("active epoch ledger is not exact RFC-8785 JCS".to_owned());
    }
    let key = ErasureSigningKey::from_seed(
        current_signing_key_id,
        region_for_key(region),
        0,
        0,
        *signing_seed,
    )
    .public_key()
    .verifying_key;
    let signature: [u8; 64] = decode_canonical_base64(&active.ledger_signature_b64, Some(64))?
        .try_into()
        .map_err(|_| "active epoch ledger signature length")?;
    key.verify(&active.ledger_jcs, &Signature::from_bytes(&signature))
        .map_err(|_| "active epoch ledger signature verification failed")?;

    let valid = match entry {
        EpochLedgerJcs::Genesis(entry) => {
            active.epoch.epoch_id() == 0
                && active.epoch.algorithm().id() == 0
                && entry.algorithm_id == 0
                && entry.checkpoint_type == "epoch-genesis"
                && entry.checkpoint_version == 1
                && entry.epoch_id == 0
                && entry.ledger_sequence == active.ledger_sequence
                && entry.ledger_version == 1
                && entry.link_key_id.is_none()
                && entry.previous_ledger_hash == ZERO_HASH_HEX
                && entry.region == region
                && entry.signing_key_id == current_signing_key_id
                && entry.start_prev_hash == ZERO_HASH_HEX
                && entry.start_sequence == 0
                && entry.tenant_id == tenant_id
        }
        EpochLedgerJcs::Transition(entry) => {
            active.epoch.epoch_id() > 0
                && active.epoch.algorithm().id() == 1
                && entry.algorithm_id == 1
                && entry.checkpoint_type == "epoch-transition"
                && entry.checkpoint_version == 1
                && entry.epoch_id == active.epoch.epoch_id()
                && entry.from_epoch_id.checked_add(1) == Some(entry.epoch_id)
                && entry.from_head_hash == entry.to_start_prev_hash
                && entry.from_next_sequence == entry.to_start_sequence
                && entry.ledger_sequence == active.ledger_sequence
                && entry.ledger_version == 1
                && Some(entry.link_key_id) == active.epoch.link_key_id()
                && is_lower_hex_32(&entry.previous_ledger_hash)
                && entry.region == region
                && entry.signing_key_id == current_signing_key_id
                && entry.to_start_prev_hash == active.epoch.start_prev_hash().to_hex()
                && entry.to_start_sequence == active.epoch.start_sequence()
                && entry.tenant_id == tenant_id
        }
    };
    if !valid {
        return Err("active epoch projection differs from signed ledger entry".to_owned());
    }
    Ok(())
}

fn validate_witness_origin(raw: &str) -> Result<Url, String> {
    let origin = Url::parse(raw).map_err(|_| "AUDIT_WITNESS_URL is not a URL")?;
    let host = origin.host_str().ok_or("AUDIT_WITNESS_URL has no host")?;
    if origin.scheme() != "https"
        || origin.username() != ""
        || origin.password().is_some()
        || origin.query().is_some()
        || origin.fragment().is_some()
        || origin.path() != "/"
        || origin.port().is_some()
        || host.parse::<std::net::IpAddr>().is_ok()
        || host != host.to_ascii_lowercase()
    {
        return Err("AUDIT_WITNESS_URL must be a pinned HTTPS DNS origin".to_owned());
    }
    Ok(origin)
}

fn is_lower_hex_32(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn parse_witness_public_keys(
    raw: &str,
) -> Result<BTreeMap<u64, ed25519_dalek::VerifyingKey>, String> {
    struct UniquePublicKeys(BTreeMap<String, String>);
    impl<'de> serde::Deserialize<'de> for UniquePublicKeys {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = UniquePublicKeys;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("a unique map of witness key ids to public keys")
                }
                fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::MapAccess<'de>,
                {
                    let mut entries = BTreeMap::new();
                    while let Some((id, key)) = map.next_entry::<String, String>()? {
                        if entries.insert(id, key).is_some() {
                            return Err(serde::de::Error::custom(
                                "duplicate witness public-key id",
                            ));
                        }
                    }
                    Ok(UniquePublicKeys(entries))
                }
            }
            deserializer.deserialize_map(Visitor)
        }
    }
    let entries = serde_json::from_str::<UniquePublicKeys>(raw)
        .map_err(|_| "AUDIT_WITNESS_PUBLIC_KEYS_JSON is malformed")?
        .0;
    if entries.is_empty() {
        return Err("AUDIT_WITNESS_PUBLIC_KEYS_JSON is empty".to_owned());
    }
    let mut keys = BTreeMap::new();
    for (id_text, encoded) in entries {
        let id = id_text
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0 && *id <= JS_SAFE_MAX && id.to_string() == id_text)
            .ok_or("witness public-key id is not canonical positive decimal")?;
        let bytes: [u8; 32] = decode_canonical_base64(&encoded, Some(32))?
            .try_into()
            .map_err(|_| "witness public key has wrong length")?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
            .map_err(|_| "witness public key is invalid")?;
        keys.insert(id, key);
    }
    Ok(keys)
}

impl WitnessClient {
    fn new(
        origin: &str,
        token: String,
        witness_id: String,
        public_keys_json: &str,
    ) -> Result<Self, String> {
        if token.len() < 32 || !canonical_witness_id(&witness_id) {
            return Err("witness client identity/token configuration is invalid".to_owned());
        }
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .build()
            .map_err(|error| format!("build witness HTTP client: {error}"))?;
        Ok(Self {
            http,
            origin: validate_witness_origin(origin)?,
            append_token: Arc::new(Zeroizing::new(token)),
            witness_id,
            public_keys: Arc::new(parse_witness_public_keys(public_keys_json)?),
        })
    }

    async fn bounded_body(response: reqwest::Response) -> Result<Vec<u8>, String> {
        if response
            .content_length()
            .is_some_and(|length| length > MAX_WITNESS_RESPONSE_BYTES as u64)
        {
            return Err("witness response exceeds size limit".to_owned());
        }
        let mut response = response;
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "read witness response failed")?
        {
            if body.len().saturating_add(chunk.len()) > MAX_WITNESS_RESPONSE_BYTES {
                return Err("witness response exceeds size limit".to_owned());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    pub(crate) async fn latest(
        &self,
        tenant_id: &str,
        region: &str,
    ) -> Result<Option<VerifiedWitness>, String> {
        if !canonical_partition(tenant_id, region) {
            return Err("non-canonical witness partition".to_owned());
        }
        let mut challenge = [0_u8; 32];
        OsRng.fill_bytes(&mut challenge);
        let challenge_b64 = base64::engine::general_purpose::STANDARD.encode(challenge);
        let request = LatestRequest {
            challenge_b64: &challenge_b64,
            latest_request_version: 1,
            region,
            tenant_id,
        };
        let body =
            serde_jcs::to_vec(&request).map_err(|_| "canonicalize witness latest request")?;
        let response = self
            .http
            .post(
                self.origin
                    .join("v1/audit-chain/head-witness/latest")
                    .map_err(|_| "join witness URL")?,
            )
            .bearer_auth(&**self.append_token)
            .header(reqwest::header::CONTENT_TYPE, "application/jcs+json")
            .body(body)
            .send()
            .await
            .map_err(|error| format!("witness latest transport: {error}"))?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(format!(
                "witness latest returned HTTP {}",
                response.status()
            ));
        }
        let envelope: LatestEnvelope = serde_json::from_slice(&Self::bounded_body(response).await?)
            .map_err(|_| "witness latest envelope malformed")?;
        let (signed, latest_jcs): (LatestJcs, Vec<u8>) =
            canonical_decode(&envelope.latest_jcs_b64, 24 * 1024)?;
        if signed.latest_version != 1
            || signed.observed_at_ms > JS_SAFE_MAX
            || signed.witness_key_id == 0
            || signed.witness_key_id > JS_SAFE_MAX
            || signed.challenge_b64 != challenge_b64
            || signed.tenant_id != tenant_id
            || signed.region != region
            || signed.witness_id != self.witness_id
        {
            return Err("witness latest does not bind request/pinned identity".to_owned());
        }
        verify_signature(
            &self.public_keys,
            signed.witness_key_id,
            LATEST_DOMAIN,
            &latest_jcs,
            &envelope.latest_signature_b64,
        )?;
        let Some(stored) = signed.latest else {
            return Ok(None);
        };
        if stored.tenant_id != tenant_id || stored.region != region || stored.witness_key_id == 0 {
            return Err("witness latest stored partition mismatch".to_owned());
        }
        let receipt = WitnessReceipt {
            receipt_jcs_b64: stored.receipt_jcs_b64,
            receipt_signature_b64: stored.receipt_signature_b64,
            witness_jcs_b64: stored.witness_jcs_b64,
            witness_key_id: stored.witness_key_id,
            witness_record_hash: stored.witness_record_hash,
            witness_sequence: stored.witness_sequence,
        };
        verify_receipt(&receipt, None, &self.witness_id, &self.public_keys).map(Some)
    }

    /// Authenticate receipt material loaded from mutable storage against the
    /// independently pinned witness keys. No caller-supplied boolean or D1 key
    /// row can manufacture the returned opaque value.
    pub(crate) fn verify_stored_receipt(
        &self,
        stored: B054StoredWitnessReceiptInput,
    ) -> Result<VerifiedWitness, String> {
        let receipt = WitnessReceipt {
            receipt_jcs_b64: base64::engine::general_purpose::STANDARD.encode(stored.receipt_jcs),
            receipt_signature_b64: stored.receipt_signature_b64,
            witness_jcs_b64: base64::engine::general_purpose::STANDARD.encode(stored.witness_jcs),
            witness_key_id: stored.witness_key_id,
            witness_record_hash: stored.witness_record_hash,
            witness_sequence: stored.witness_sequence,
        };
        verify_receipt(&receipt, None, &self.witness_id, &self.public_keys)
    }

    /// Verify an ordered historical receipt segment and its hash links. The
    /// first item may be any sequence (archive loaders commonly verify a
    /// suffix); every subsequent item must be its exact successor.
    pub(crate) fn verify_historical_receipt_chain(
        &self,
        stored: Vec<B054StoredWitnessReceiptInput>,
        tenant_id: &str,
        region: &str,
    ) -> Result<Vec<VerifiedWitness>, String> {
        if stored.is_empty() || !canonical_partition(tenant_id, region) {
            return Err(
                "witness receipt history is empty or partition is non-canonical".to_owned(),
            );
        }
        let mut verified: Vec<VerifiedWitness> = Vec::with_capacity(stored.len());
        for input in stored {
            let current = self.verify_stored_receipt(input)?;
            if current.record.tenant_id != tenant_id || current.record.region != region {
                return Err("witness receipt history crosses partitions".to_owned());
            }
            if let Some(previous) = verified.last() {
                let expected_sequence = previous
                    .record
                    .witness_sequence
                    .checked_add(1)
                    .ok_or("witness receipt history sequence overflow")?;
                if current.record.witness_sequence != expected_sequence
                    || current.record.previous_witness_hash != previous.witness_record_hash
                {
                    return Err("witness receipt history is not contiguous".to_owned());
                }
            }
            verified.push(current);
        }
        Ok(verified)
    }

    async fn append(
        &self,
        head_jcs: &[u8],
        head_signature_b64: &str,
        tenant_id: &str,
        region: &str,
        expected: &VerifiedWitness,
    ) -> Result<VerifiedWitness, String> {
        self.append_from(
            head_jcs,
            head_signature_b64,
            tenant_id,
            region,
            Some(expected),
        )
        .await
    }

    /// Bootstrap the first witnessed head. The challenged latest read must
    /// already have returned signed `latest:null`; the witness enforces the
    /// same genesis predicate atomically at compare-and-append.
    async fn append_genesis(
        &self,
        head_jcs: &[u8],
        head_signature_b64: &str,
        tenant_id: &str,
        region: &str,
    ) -> Result<VerifiedWitness, String> {
        self.append_from(head_jcs, head_signature_b64, tenant_id, region, None)
            .await
    }

    async fn append_from(
        &self,
        head_jcs: &[u8],
        head_signature_b64: &str,
        tenant_id: &str,
        region: &str,
        expected: Option<&VerifiedWitness>,
    ) -> Result<VerifiedWitness, String> {
        let signature = decode_canonical_base64(head_signature_b64, Some(64))?;
        let mut head_record = length_prefixed(HEAD_RECORD_DOMAIN, head_jcs)?;
        head_record.extend_from_slice(&signature);
        let head_record_hash = hash_domain_payload(&[], &head_record);
        let (witness_sequence, previous_witness_hash) = match expected {
            Some(expected) => (
                expected
                    .record
                    .witness_sequence
                    .checked_add(1)
                    .ok_or("witness sequence overflow")?,
                expected.witness_record_hash.clone(),
            ),
            None => (0, ZERO_HASH_HEX.to_owned()),
        };
        let record = WitnessRecord {
            head_message_b64: base64::engine::general_purpose::STANDARD.encode(head_jcs),
            head_record_hash,
            head_signature_b64: head_signature_b64.to_owned(),
            previous_witness_hash,
            region: region.to_owned(),
            tenant_id: tenant_id.to_owned(),
            witness_sequence,
            witness_version: 1,
        };
        let witness_jcs = serde_jcs::to_vec(&record).map_err(|_| "canonicalize witness record")?;
        let witness_record_hash = hash_domain_payload(WITNESS_DOMAIN, &witness_jcs);
        let witness_jcs_b64 = base64::engine::general_purpose::STANDARD.encode(&witness_jcs);
        let request = AppendRequest {
            append_request_version: 1,
            expected_latest: expected.map(|expected| ExpectedLatest {
                witness_record_hash: &expected.witness_record_hash,
                witness_sequence: expected.record.witness_sequence,
            }),
            witness_jcs_b64: &witness_jcs_b64,
        };
        let body =
            serde_jcs::to_vec(&request).map_err(|_| "canonicalize witness append request")?;
        let response = self
            .http
            .post(
                self.origin
                    .join("v1/audit-chain/head-witness/compare-and-append")
                    .map_err(|_| "join witness URL")?,
            )
            .bearer_auth(&**self.append_token)
            .header(reqwest::header::CONTENT_TYPE, "application/jcs+json")
            .header("idempotency-key", &witness_record_hash)
            .body(body)
            .send()
            .await
            .map_err(|error| format!("witness append transport/commit-unknown: {error}"))?;
        if response.status() != reqwest::StatusCode::CREATED {
            return Err(format!(
                "witness append returned HTTP {}",
                response.status()
            ));
        }
        let receipt: WitnessReceipt = serde_json::from_slice(&Self::bounded_body(response).await?)
            .map_err(|_| "witness receipt envelope malformed")?;
        verify_receipt(
            &receipt,
            Some(&witness_jcs),
            &self.witness_id,
            &self.public_keys,
        )
    }
}

include!("b054_witness_runtime.rs");
