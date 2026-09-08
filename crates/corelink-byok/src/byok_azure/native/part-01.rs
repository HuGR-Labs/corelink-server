// ---------------------------------------------------------------------
// REST wire types (Key Vault 7.4).
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct WrapRequest<'a> {
    alg: &'a str,
    /// Base64-URL-no-padding encoded plaintext key (Key Vault spec).
    value: String,
}

#[derive(Debug, Deserialize)]
struct WrapResponse {
    /// Base64-URL-no-padding encoded ciphertext.
    value: String,
    #[serde(default)]
    #[allow(dead_code)]
    kid: String,
}

#[derive(Debug, Serialize)]
struct UnwrapRequest<'a> {
    alg: &'a str,
    /// Base64-URL-no-padding encoded ciphertext.
    value: String,
}

#[derive(Debug, Deserialize)]
struct UnwrapResponse {
    /// Base64-URL-no-padding encoded plaintext.
    value: String,
}

#[derive(Debug, Default, Deserialize)]
struct KeyBundle {
    #[serde(default)]
    attributes: KeyAttributes,
    #[serde(default)]
    key: Option<JsonWebKey>,
}

#[derive(Debug, Deserialize, Default)]
struct KeyAttributes {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    #[allow(dead_code)]
    exp: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct JsonWebKey {
    #[serde(default)]
    #[allow(dead_code)]
    kty: String,
    #[serde(default)]
    key_ops: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorEnvelope {
    #[serde(default)]
    error: ApiErrorInner,
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorInner {
    #[serde(default)]
    code: String,
    #[serde(default)]
    message: String,
}

fn parse_api_error(body: &str) -> ApiErrorInner {
    serde_json::from_str::<ApiErrorEnvelope>(body)
        .map(|e| e.error)
        .unwrap_or_default()
}

/// Key Vault uses base64url-no-pad encoding for binary fields.
fn b64url_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn b64url_decode(s: &str) -> Result<Vec<u8>, BYOKError> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| BYOKError::Provider(format!("base64url decode: {e}")))
}

/// Emit a fail-CLOSED audit event BEFORE the error bubbles up.
///
/// The orchestrator subscribes to `target =
/// "corelink.byok.azure.audit"` and turns each event into a BLAKE3-
/// linked audit record (see `corelink-audit-chain`). The `audit = true`
/// field is the canonical marker used by the subscriber to distinguish
/// audit events from regular `tracing` output.
pub(super) fn emit_audit(op: &str, key: &str, reason: &str) {
    warn!(
        target: "corelink.byok.azure.audit",
        audit = true,
        op = op,
        key = key,
        reason = reason,
        "BYOK Azure Key Vault audit event"
    );
}

fn map_bundle_to_access(kb: &KeyBundle) -> KmsAccessStatus {
    if matches!(kb.attributes.enabled, Some(false)) {
        return KmsAccessStatus::Revoked;
    }
    if let Some(jwk) = kb.key.as_ref() {
        if !jwk.key_ops.is_empty()
            && (!jwk
                .key_ops
                .iter()
                .any(|o| o.eq_ignore_ascii_case("wrapKey"))
                || !jwk
                    .key_ops
                    .iter()
                    .any(|o| o.eq_ignore_ascii_case("unwrapKey")))
        {
            return KmsAccessStatus::Revoked;
        }
    }
    KmsAccessStatus::Ok
}

fn map_get_error_to_access(http_status: u16, api: &ApiErrorInner, key: &str) -> KmsAccessStatus {
    match http_status {
        401 | 403 => {
            emit_audit("check_access", key, "access_denied");
            KmsAccessStatus::Revoked
        }
        404 => {
            emit_audit("check_access", key, "not_found");
            KmsAccessStatus::NotFound
        }
        429 => {
            emit_audit("check_access", key, "throttled");
            KmsAccessStatus::Throttled
        }
        _ => {
            emit_audit("check_access", key, "provider_error");
            warn!(http = http_status, code = %api.code, "Azure KV GetKey error");
            KmsAccessStatus::ApiError(http_status)
        }
    }
}

async fn map_http_error(resp: reqwest::Response, key_id: &KmsKeyId, op: &str) -> BYOKError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let api = parse_api_error(&body);
    let code_lc = api.code.to_ascii_lowercase();

    // Defensive: Azure may surface `BadParameter`/`decrypt`-mention as
    // a cryptographic mismatch (e.g. tampered outer wrap). The inner
    // AES-GCM tag protects the AAD, so this branch is only reached when
    // Azure rejected the outer wrap key.
    if (status.as_u16() == 400 || status.as_u16() == 403)
        && (code_lc.contains("badparameter")
            && api.message.to_ascii_lowercase().contains("decrypt"))
    {
        emit_audit(op, &key_id.key_arn_or_id, "invalid_ciphertext_aad_mismatch");
        return BYOKError::AadMismatch;
    }

    match status.as_u16() {
        401 | 403 => {
            emit_audit(op, &key_id.key_arn_or_id, "access_denied");
            BYOKError::CmkRevoked {
                provider: KmsProviderKind::AzureKeyVault,
                key_id: key_id.key_arn_or_id.clone(),
            }
        }
        404 => {
            emit_audit(op, &key_id.key_arn_or_id, "not_found");
            BYOKError::CmkRevoked {
                provider: KmsProviderKind::AzureKeyVault,
                key_id: key_id.key_arn_or_id.clone(),
            }
        }
        429 => {
            emit_audit(op, &key_id.key_arn_or_id, "throttled");
            BYOKError::Provider(format!("Azure KV {op}: throttled"))
        }
        _ => {
            emit_audit(op, &key_id.key_arn_or_id, "provider_error");
            warn!(http = %status, op = %op, "Azure KV provider error");
            // Do not leak the raw SDK message — it may contain key
            // metadata the caller did not pass in.
            BYOKError::Provider(format!("Azure KV {op}: provider error"))
        }
    }
}

// ── Mock-mode wrap / unwrap (in-process, AAD-binding enforced) ───────

fn mock_wrap(dek: &Dek, key_id: &KmsKeyId, ctx: &Value, canonical_aad: &[u8]) -> WrappedDek {
    let fp = aad_fingerprint(canonical_aad);
    let mut ct = Vec::with_capacity(8 + 32);
    ct.extend_from_slice(&fp);
    for b in &dek.bytes {
        ct.push(b ^ 0xCC);
    }
    WrappedDek {
        provider: KmsProviderKind::AzureKeyVault,
        key_id: key_id.clone(),
        ciphertext: ct,
        encryption_context: Some(ctx.clone()),
    }
}

fn mock_unwrap(wrapped: &WrappedDek, canonical_aad: &[u8]) -> Result<Dek, BYOKError> {
    if wrapped.ciphertext.len() != 40 {
        return Err(BYOKError::EnvelopeError(format!(
            "Azure KV real (mock): wrong ciphertext length {} (expected 40)",
            wrapped.ciphertext.len()
        )));
    }
    let expected = aad_fingerprint(canonical_aad);
    let stored = wrapped.ciphertext.get(..8).ok_or_else(|| {
        BYOKError::EnvelopeError("Azure KV real (mock): ciphertext too short".to_string())
    })?;
    // Constant-time fingerprint compare.
    if stored.ct_eq(&expected).unwrap_u8() == 0 {
        return Err(BYOKError::AadMismatch);
    }
    let body = wrapped.ciphertext.get(8..).ok_or_else(|| {
        BYOKError::EnvelopeError("Azure KV real (mock): ciphertext too short".to_string())
    })?;
    let mut bytes = [0u8; 32];
    for (out, &b) in bytes.iter_mut().zip(body.iter()) {
        *out = b ^ 0xCC;
    }
    Ok(Dek { bytes })
}
