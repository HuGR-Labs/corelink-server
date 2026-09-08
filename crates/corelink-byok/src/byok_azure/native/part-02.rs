#[async_trait]
impl KmsProvider for AzureKeyVaultRealProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AzureKeyVault
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        // Premium HSM = FIPS 140-2 L2 (CMVP #3516). Managed HSM is
        // logically L3 in the compliance matrix; the orchestrator is the
        // source of truth on tier policy.
        FipsLevel::Fips140_2_L2
    }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&Value>,
    ) -> Result<WrappedDek, BYOKError> {
        if key_id.provider != KmsProviderKind::AzureKeyVault {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "wrong_provider");
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultRealProvider received key_id with wrong provider: {:?}",
                key_id.provider
            )));
        }
        let ctx = encryption_context.ok_or_else(|| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "aad_missing");
            BYOKError::EncryptionContextMissing
        })?;
        let (_ec_map, canonical_aad) = canonicalize_aad_to_string_map(ctx).inspect_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "aad_canonicalize");
        })?;

        debug!(
            target: "corelink.byok.azure",
            key = %key_id.key_arn_or_id,
            region = %self.region,
            "Azure KV wrap_dek"
        );

        if self.mock_mode {
            return Ok(mock_wrap(dek, key_id, ctx, &canonical_aad));
        }

        let resource = Self::parse_resource(&key_id.key_arn_or_id).inspect_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "malformed_arn");
        })?;

        // Step 1: generate inner AES-256-GCM key + nonce.
        let inner_key = Self::gen_inner_key()?;
        let nonce = Self::gen_nonce()?;

        // Step 2: encrypt DEK with AES-256-GCM, AAD = JCS(ctx).
        let inner_ct = Self::aes_gcm_encrypt(&inner_key, &nonce, &dek.bytes, &canonical_aad)?;

        // Step 3: wrap inner_key via Azure wrapKey (RSA-OAEP-256).
        let token = self.bearer().await.inspect_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "entra_token_failure");
        })?;
        let url = self.wrapkey_url(&resource);
        let body = WrapRequest {
            alg: WRAP_ALG_RSA_OAEP_256,
            value: b64url_encode(&inner_key),
        };
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                emit_audit("wrap_dek", &key_id.key_arn_or_id, "http_send_error");
                BYOKError::Provider(format!("Azure KV wrapkey POST: {e}"))
            })?;
        if !resp.status().is_success() {
            return Err(map_http_error(resp, key_id, "wrap_dek").await);
        }

        let wr: WrapResponse = resp.json().await.map_err(|e| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "http_body_parse");
            BYOKError::Provider(format!("Azure KV wrapkey JSON: {e}"))
        })?;
        let azure_wrapped_inner = b64url_decode(&wr.value).inspect_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "b64_decode_failed");
        })?;

        // Step 4: ciphertext layout = [u32 BE outer-len] || outer || inner_ct.
        let mut ciphertext = Vec::with_capacity(4 + azure_wrapped_inner.len() + inner_ct.len());
        let outer_len_u32: u32 = azure_wrapped_inner.len().try_into().map_err(|_| {
            emit_audit("wrap_dek", &key_id.key_arn_or_id, "outer_len_overflow");
            BYOKError::EnvelopeError("outer wrapped key length > u32::MAX".to_string())
        })?;
        ciphertext.extend_from_slice(&outer_len_u32.to_be_bytes());
        ciphertext.extend_from_slice(&azure_wrapped_inner);
        ciphertext.extend_from_slice(&inner_ct);

        Ok(WrappedDek {
            provider: KmsProviderKind::AzureKeyVault,
            key_id: key_id.clone(),
            ciphertext,
            encryption_context: Some(ctx.clone()),
        })
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.provider != KmsProviderKind::AzureKeyVault {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "wrong_provider",
            );
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultRealProvider received WrappedDek with wrong provider: {:?}",
                wrapped.provider
            )));
        }
        let ctx = wrapped.encryption_context.as_ref().ok_or_else(|| {
            emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "aad_missing");
            BYOKError::EncryptionContextMissing
        })?;
        let (_ec_map, canonical_aad) = canonicalize_aad_to_string_map(ctx).inspect_err(|_| {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "aad_canonicalize",
            );
        })?;

        debug!(
            target: "corelink.byok.azure",
            key = %wrapped.key_id.key_arn_or_id,
            "Azure KV unwrap_dek"
        );

        if self.mock_mode {
            return mock_unwrap(wrapped, &canonical_aad);
        }

        let resource = Self::parse_resource(&wrapped.key_id.key_arn_or_id).inspect_err(|_| {
            emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "malformed_arn");
        })?;

        // Parse ciphertext layout: [u32 BE outer-len] || outer || inner_ct.
        if wrapped.ciphertext.len() < 4 + NONCE_LEN + TAG_LEN {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "ciphertext_too_short",
            );
            return Err(BYOKError::EnvelopeError(format!(
                "Azure KV: ciphertext too short ({} bytes)",
                wrapped.ciphertext.len()
            )));
        }
        let (len_bytes, rest) = wrapped.ciphertext.split_at(4);
        let outer_len = u32::from_be_bytes([
            *len_bytes.first().unwrap_or(&0),
            *len_bytes.get(1).unwrap_or(&0),
            *len_bytes.get(2).unwrap_or(&0),
            *len_bytes.get(3).unwrap_or(&0),
        ]) as usize;
        if rest.len() < outer_len + NONCE_LEN + TAG_LEN {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "ciphertext_underflow",
            );
            return Err(BYOKError::EnvelopeError(format!(
                "Azure KV: ciphertext underflow (outer_len={outer_len}, rest={})",
                rest.len()
            )));
        }
        let (azure_wrapped_inner, inner_ct) = rest.split_at(outer_len);

        // Step 1: unwrap inner_key via Azure unwrapKey (RSA-OAEP-256).
        let token = self.bearer().await.inspect_err(|_| {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "entra_token_failure",
            );
        })?;
        let url = self.unwrapkey_url(&resource);
        let body = UnwrapRequest {
            alg: WRAP_ALG_RSA_OAEP_256,
            value: b64url_encode(azure_wrapped_inner),
        };
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                emit_audit(
                    "unwrap_dek",
                    &wrapped.key_id.key_arn_or_id,
                    "http_send_error",
                );
                BYOKError::Provider(format!("Azure KV unwrapkey POST: {e}"))
            })?;
        if !resp.status().is_success() {
            return Err(map_http_error(resp, &wrapped.key_id, "unwrap_dek").await);
        }

        let ur: UnwrapResponse = resp.json().await.map_err(|e| {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "http_body_parse",
            );
            BYOKError::Provider(format!("Azure KV unwrapkey JSON: {e}"))
        })?;
        let inner_key_bytes = b64url_decode(&ur.value).inspect_err(|_| {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "b64_decode_failed",
            );
        })?;
        if inner_key_bytes.len() != INNER_KEY_LEN {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "inner_key_length",
            );
            return Err(BYOKError::EnvelopeError(format!(
                "Azure KV: inner key length {} != {}",
                inner_key_bytes.len(),
                INNER_KEY_LEN
            )));
        }
        let mut inner_key = [0u8; INNER_KEY_LEN];
        inner_key.copy_from_slice(&inner_key_bytes);

        // Step 2: decrypt AES-GCM with AAD = JCS(ctx). Tag mismatch ⇒
        // AadMismatch.
        let plaintext =
            Self::aes_gcm_decrypt(&inner_key, inner_ct, &canonical_aad).inspect_err(|e| {
                if matches!(e, BYOKError::AadMismatch) {
                    emit_audit(
                        "unwrap_dek",
                        &wrapped.key_id.key_arn_or_id,
                        "invalid_ciphertext_aad_mismatch",
                    );
                } else {
                    emit_audit("unwrap_dek", &wrapped.key_id.key_arn_or_id, "aes_gcm_error");
                }
            })?;

        if plaintext.len() != 32 {
            emit_audit(
                "unwrap_dek",
                &wrapped.key_id.key_arn_or_id,
                "dek_length_invalid",
            );
            return Err(BYOKError::DekLengthInvalid {
                got: plaintext.len(),
            });
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&plaintext);
        Ok(Dek { bytes })
    }

    async fn check_access(&self, key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        if key_id.provider != KmsProviderKind::AzureKeyVault {
            emit_audit("check_access", &key_id.key_arn_or_id, "wrong_provider");
            return Err(BYOKError::EnvelopeError(format!(
                "AzureKeyVaultRealProvider check_access: wrong provider {:?}",
                key_id.provider
            )));
        }

        if self.mock_mode {
            return Ok(KmsAccessStatus::Ok);
        }

        let resource = Self::parse_resource(&key_id.key_arn_or_id).inspect_err(|_| {
            emit_audit("check_access", &key_id.key_arn_or_id, "malformed_arn");
        })?;
        let token = self.bearer().await.inspect_err(|_| {
            emit_audit("check_access", &key_id.key_arn_or_id, "entra_token_failure");
        })?;
        let url = self.getkey_url(&resource);

        let resp = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| {
                emit_audit("check_access", &key_id.key_arn_or_id, "http_send_error");
                BYOKError::Provider(format!("Azure KV GetKey: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let api_err = parse_api_error(&body);
            return Ok(map_get_error_to_access(
                status.as_u16(),
                &api_err,
                &key_id.key_arn_or_id,
            ));
        }

        let kb: KeyBundle = resp.json().await.map_err(|e| {
            emit_audit("check_access", &key_id.key_arn_or_id, "http_body_parse");
            BYOKError::Provider(format!("Azure KV GetKey JSON: {e}"))
        })?;

        Ok(map_bundle_to_access(&kb))
    }
}
