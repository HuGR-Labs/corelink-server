#[cfg(test)]
mod b054_witness_tests {
    use super::*;

    const TENANT: &str = "00000000-0000-7000-8000-00000000aaaa";

    fn must<T, E>(result: Result<T, E>) -> T
    where
        E: std::fmt::Debug,
    {
        match result {
            Ok(value) => value,
            Err(error) => unreachable!("test fixture construction failed: {error:?}"),
        }
    }

    #[test]
    fn witness_origin_is_an_exact_https_dns_origin() {
        assert!(validate_witness_origin("https://witness.security.example").is_ok());
        for invalid in [
            "http://witness.security.example",
            "https://user@witness.security.example",
            "https://witness.security.example/path",
            "https://witness.security.example?x=1",
            "https://127.0.0.1",
            "https://witness.security.example:8443",
        ] {
            assert!(
                validate_witness_origin(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn witness_public_key_registry_rejects_duplicate_or_noncanonical_ids() {
        let key = base64::engine::general_purpose::STANDARD.encode(
            ed25519_dalek::SigningKey::from_bytes(&[7_u8; 32])
                .verifying_key()
                .to_bytes(),
        );
        assert!(parse_witness_public_keys(&format!(r#"{{"1":"{key}"}}"#)).is_ok());
        assert!(
            parse_witness_public_keys(&format!(r#"{{"1":"{key}","\u0031":"{key}"}}"#)).is_err()
        );
        assert!(parse_witness_public_keys(&format!(r#"{{"01":"{key}"}}"#)).is_err());
    }

    #[test]
    fn d1_blob_decoder_accepts_only_bounded_bytes_or_canonical_base64() {
        assert_eq!(
            must(decode_d1_blob(&json!([123, 125]), "entry_jcs", 8)),
            b"{}"
        );
        assert_eq!(
            must(decode_d1_blob(&json!("e30="), "entry_jcs", 8)),
            b"{}"
        );
        assert!(decode_d1_blob(&json!("{}"), "entry_jcs", 8).is_err());
        assert!(decode_d1_blob(&json!([256]), "entry_jcs", 8).is_err());
        assert!(decode_d1_blob(&json!([]), "entry_jcs", 8).is_err());
        assert!(decode_d1_blob(&json!([1, 2, 3]), "entry_jcs", 2).is_err());
    }

    fn signed_receipt() -> (
        WitnessReceipt,
        Vec<u8>,
        String,
        BTreeMap<u64, ed25519_dalek::VerifyingKey>,
    ) {
        let head_seed = [0x11_u8; 32];
        let witness_key = ed25519_dalek::SigningKey::from_bytes(&[0x22_u8; 32]);
        let head_hash = "ab".repeat(32);
        let ledger_hash = "cd".repeat(32);
        let head_jcs = must(canonical_head_v2_bytes(
            TENANT,
            "weur",
            &head_hash,
            7,
            1,
            2,
            &ledger_hash,
            1,
        ));
        let head_signature = must(sign_head_v2(
            &head_seed,
            1,
            TENANT,
            "weur",
            &head_hash,
            7,
            1,
            2,
            &ledger_hash,
        ));
        let mut head_material = must(length_prefixed(HEAD_RECORD_DOMAIN, &head_jcs));
        head_material
            .extend_from_slice(&must(decode_canonical_base64(&head_signature, Some(64))));
        let record = WitnessRecord {
            head_message_b64: base64::engine::general_purpose::STANDARD.encode(&head_jcs),
            head_record_hash: hash_domain_payload(&[], &head_material),
            head_signature_b64: head_signature,
            previous_witness_hash: "34".repeat(32),
            region: "weur".to_owned(),
            tenant_id: TENANT.to_owned(),
            witness_sequence: 9,
            witness_version: 1,
        };
        let witness_jcs = must(serde_jcs::to_vec(&record));
        let witness_hash = hash_domain_payload(WITNESS_DOMAIN, &witness_jcs);
        let receipt_jcs = must(serde_jcs::to_vec(&WitnessReceiptJcs {
            committed_at_ms: 10,
            head_record_hash: record.head_record_hash,
            previous_witness_hash: record.previous_witness_hash,
            receipt_version: 1,
            region: record.region,
            tenant_id: record.tenant_id,
            witness_id: "security-witness-1".to_owned(),
            witness_key_id: 7,
            witness_record_hash: witness_hash.clone(),
            witness_sequence: 9,
        }));
        let signature = witness_key.sign(&must(length_prefixed(RECEIPT_DOMAIN, &receipt_jcs)));
        let receipt = WitnessReceipt {
            receipt_jcs_b64: base64::engine::general_purpose::STANDARD.encode(&receipt_jcs),
            receipt_signature_b64: base64::engine::general_purpose::STANDARD
                .encode(signature.to_bytes()),
            witness_jcs_b64: base64::engine::general_purpose::STANDARD.encode(&witness_jcs),
            witness_key_id: 7,
            witness_record_hash: witness_hash,
            witness_sequence: 9,
        };
        (
            receipt,
            witness_jcs,
            "security-witness-1".to_owned(),
            BTreeMap::from([(7, witness_key.verifying_key())]),
        )
    }

    #[test]
    fn exact_receipt_verifies_and_any_signature_or_record_drift_fails() {
        let (receipt, witness_jcs, witness_id, keys) = signed_receipt();
        assert!(verify_receipt(&receipt, Some(&witness_jcs), &witness_id, &keys).is_ok());
        let mut bad = receipt.clone();
        bad.witness_record_hash = "ff".repeat(32);
        assert!(verify_receipt(&bad, Some(&witness_jcs), &witness_id, &keys).is_err());
        assert!(verify_receipt(&receipt, Some(&witness_jcs), "other-witness", &keys).is_err());
        assert!(verify_receipt(&receipt, Some(b"{}"), &witness_id, &keys).is_err());
    }
}
