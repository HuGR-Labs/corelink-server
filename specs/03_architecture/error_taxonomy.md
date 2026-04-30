---
id: "ERROR-TAXONOMY"
type: "architecture"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.3.1"
created: "2026-04-24"
updated: "2026-04-29"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "errors", "taxonomy", "sdk", "customer-facing", "canonical"]
---

# Error Taxonomy — Customer-Facing Error Catalog (Nível 3 canonical)

> **Propósito**: fonte canônica para `error_code → http_status → SDK exception class → customer-visible message → next-action`. Sem este catalog, S-15 SDK + S-18 docs + S-16 UI divergem.
>
> Adicionado em Lote 9.4 endereçando Opus H-03 strategic recommendation S-4 — "customer-facing UX silenciado em sprints CRITICAL".
>
> Consumido por: S-15 (CLI/SDK), S-16 (admin UI error toasts), S-18 (public docs error catalog), S-13 (admin plane), S-19 (onboarding), todos sprints que retornam erros HTTP.
>
> **Regra**: novo error code requires PR adicionando entry aqui + S-15 SDK + S-18 docs simultâneos. Sem entry = não pode shipar error code novo.

---

## Sumário

1. [Princípios de design](#1-princípios-de-design)
2. [Schema canônico](#2-schema-canônico)
3. [Catalog](#3-catalog)
4. [Customer-visible message style](#4-customer-visible-message-style)
5. [Next-action discipline](#5-next-action-discipline)
6. [SDK exception class mapping](#6-sdk-exception-class-mapping)
7. [Adding new errors](#7-adding-new-errors)

---

## 0. Cross-doc consistency note (Lote 9.5b)

**Lote 9.5b R3-09 fix**: `observability_model.md §X` define enum legada `AUTH_*`, `CAS_*`, `TENANT_*`, `RATE_*` para classificação interna de error_code em logs structurados. Esta taxonomia (`COR_*`) é **customer-facing** e SDK-facing. Mapping canonical:

| Customer-facing (este doc) | Internal log enum (observability_model.md) |
|---|---|
| `COR_AUTH_*` | `AUTH_*` |
| `COR_CAS_*` | `CAS_*` |
| `COR_AC_*` | `CAS_*` (subset) |
| `COR_RATE_*` | `RATE_*` |
| `COR_QUOTA_*` (em `COR_RATE_TENANT_QUOTA`) | `RATE_*` |
| `COR_BILLING_*` | `BILLING_*` (planned em Lote 9.5+) |
| `COR_BYOK_*` | `BYOK_*` (planned) |
| `COR_PRIVACY_*` (DSR/CONSENT/RESIDENCY) | `PRIVACY_*` |
| `COR_ADMIN_*` | `ADMIN_*` |
| `COR_MULTIPART_*` | `CAS_*` (subset) |
| `COR_ONBOARD_*` | `ONBOARD_*` (new — para S-19) |
| `COR_SERVICE_*` | (no mapping — generic) |

**CI gate planned (Lote 9.5+)**: `scripts/check_error_taxonomy.py` valida domain coverage + onboard domain presence se S-19 ativo + observability_model cross-doc compatibility check.

---

## 1. Princípios de design

1. **Customer-actionable**: cada error tem uma `next_action` que o customer pode tomar (não "internal error").
2. **Stable identifier**: `error_code` é stable across versions; HTTP status pode mudar mas error_code não.
3. **Per-language idiomatic**: SDK exception classes são Pythonic / Go-idiomatic / JS-Promise-rejecting; mas wrap o mesmo error_code.
4. **Localized message**: customer-facing message tem 3 locales (en/pt-BR/es) at GA; mais locales pós-GA.
5. **Cardinality budget**: error_code values são bounded (~50-100 enumerated); nunca dynamic.
6. **No PII em error**: erro nunca inclui PII em payload (CTRL-PRIV-001 alignment); tenant_id é permitido só em X-Request-Id header.
7. **Retry semantics declared**: cada error é `retryable: bool` + `retry_strategy: exponential|linear|never`.

---

## 2. Schema canônico

```yaml
error_code: COR_<DOMAIN>_<SHORT>  # e.g. COR_CAS_DIGEST_MISMATCH
http_status: 4xx | 5xx
retryable: bool
retry_strategy: exponential | linear | never | after_delay
sdk_exception:
  python: corelink.exceptions.<ClassName>
  go: corelink.<ClassName>Error
  js: CoreLinkErrors.<ClassName>
customer_message:
  en: "..."
  pt-BR: "..."
  es: "..."
next_action: "..."
canonical_source: <sprint or canonical doc reference>
introduced_in_sprint: S-XX
```

---

## 3. Catalog

### 3.1 CAS errors (S-01, S-02)

| error_code | HTTP | retryable | SDK exception | customer_message (en) | next_action |
|---|---|---|---|---|---|
| `COR_CAS_DIGEST_MISMATCH` | 409 | never | `DigestMismatchError` | "Provided digest does not match content hash" | Recompute digest from body and retry; do not retry with same payload |
| `COR_CAS_BLOB_NOT_FOUND` | 404 | never | `BlobNotFoundError` | "Blob with digest <X> not found" | Verify digest is correct or upload first |
| `COR_CAS_TENANT_FORBIDDEN` | 403 | never | `TenantForbiddenError` | "Access to this resource is forbidden" | Check PAT scope (e.g. missing `cache-r`); **NÃO retornado por CAS read handlers para cross-tenant blob access** — per ADR-0028 (S-02 GA freeze): cross-tenant CAS reads return 404 uniform `COR_CAS_BLOB_NOT_FOUND` to fechar enumeration oracle; 403 reservado para PAT scope failures (S-03 auth middleware) |
| `COR_CAS_BLOB_TOO_LARGE` | 413 | never | `BlobTooLargeError` | "Blob exceeds maximum size for your tier" | Use multipart upload (S-05) or upgrade tier |
| `COR_CAS_BATCH_TOO_LARGE` | 413 | never | `BatchTooLargeError` | "BatchUpdateBlobs aggregate exceeds MaxBatchTotalSizeBytes (4 MiB)" | Fragment the batch into smaller groups OR use ByteStream::Write for the oversized blob (canonical capability advertised via `Capabilities.GetCapabilities`) |
| `COR_CAS_BATCH_SIZE_EXCEEDED` | 413 | never | `BatchSizeExceededError` | "FindMissingBlobs/BatchReadBlobs request carries more digests than the canonical 1000 batch cap" | Fragment the digest list into chunks of ≤ 1000 entries per request (REAPI v2 conformance recommendation) |
| `COR_CAS_BAD_RESOURCE_NAME` | 400 | never | `BadResourceNameError` | "ByteStream resource_name is malformed or missing required segments" | Build the resource_name as `<instance>/uploads/<uuid>/blobs/<digest>/<size_bytes>` per REAPI v2 §`ByteStream` ABI |
| `COR_CAS_DIGEST_FUNCTION_UNSUPPORTED` | 400 | never | `DigestFunctionUnsupportedError` | "BatchUpdateBlobs.digest_function declares a hash the server does not advertise" | Read `Capabilities.GetCapabilities` first; CoreLink S-01 advertises BLAKE3 only |
| `COR_CAS_COMPRESSOR_UNSUPPORTED` | 412 | never | `CompressorUnsupportedError` | "BatchReadBlobs.acceptable_compressors must include Compressor.IDENTITY (= 0) or be empty" | Set `acceptable_compressors = []` (defaults to IDENTITY) OR include `Compressor.IDENTITY` (= 0) in the list. CoreLink S-01 supports IDENTITY only; compressed batch responses (ZSTD/DEFLATE/BROTLI) ship in WI-S05-005 once `CacheCapabilities.supported_compressors` advertises them. |
| `COR_CAS_BAD_DIGEST` | 400 | never | `BadDigestError` | "BatchUpdateBlobs digest is malformed (length / hex / size_bytes mismatch)" | Recompute digest hex (64 lowercase chars) and ensure `digest.size_bytes == data.len()` |
| `COR_CAS_QUOTA_EXCEEDED` | 429 | after_delay | `QuotaExceededError` | "Storage quota reached" | Free space via deletion or upgrade tier; check `Retry-After` header |

### 3.2 AC errors (S-04)

| error_code | HTTP | retryable | SDK exception | customer_message | next_action |
|---|---|---|---|---|---|
| `COR_AC_ACTION_NOT_FOUND` | 404 | never | `ActionNotFoundError` | "Action result not in cache" | Execute action and call UpdateActionResult to populate |
| `COR_AC_TTL_EXPIRED` | 410 | never | `ActionExpiredError` | "Action result has expired" | Re-execute action |
| `COR_AC_MERKLE_INVALID` | 422 | never | `MerkleVerificationError` | "Merkle tree verification failed" | Verify ActionResult proto integrity; possible cache poisoning |
| `COR_AC_OUTPUTS_MISSING` | 422 | never | `OutputsMissingError` | "ActionResult references missing or tombstoned blobs" | Re-execute action to repopulate referenced blobs |
| `COR_AC_SIG_INVALID` | 422 | never | `SignatureInvalidError` | "Action result signature verification failed" | Possible cache tampering; re-execute action and report to support |
| `COR_AC_BACKEND_UNAVAILABLE` | 503 | exponential | `BackendUnavailableError` | "Action cache backend temporarily unavailable" | Retry with exponential backoff per `Retry-After` |
| `COR_AC_RESULT_HASH_MISMATCH` | 409 | never | `ResultHashMismatchError` | "Existing cached result hash differs from upload" | Possible non-deterministic build; investigate compiler determinism |
| `COR_AC_DIGEST_MISMATCH` | 422 | never | `DigestMismatchError` | "URL digest does not match request body" | Recompute action digest from canonical Action proto |
| `COR_AC_PAYLOAD_TOO_LARGE` | 413 | never | `PayloadTooLargeError` | "ActionResult exceeds 1 MiB limit" | Reduce output_files metadata size or chunk via CAS |
| `COR_AC_INTERNAL` | 500 | exponential | `InternalServerError` | "Internal error processing action cache request" | Retry; if persistent, contact support with `request_id` |
| `COR_AC_DEPRECATED` | 503 | never | `ACDeprecatedError` | "Action cache temporarily disabled (rollback)" | Retry after `Retry-After`; check status page |
| `COR_AC_KEY_ID_UNKNOWN` | 422 | never | `KeyIdUnknownError` | "Action result signed with rotated-out key" | Re-execute action; older entries past rotation grace are invalidated |

### 3.3 Auth errors (S-03)

| error_code | HTTP | retryable | SDK exception | customer_message | next_action |
|---|---|---|---|---|---|
| `COR_AUTH_PAT_INVALID` | 401 | never | `InvalidTokenError` | "Authentication token is invalid or expired" | Generate new PAT in dashboard |
| `COR_AUTH_PAT_REVOKED` | 401 | never | `TokenRevokedError` | "This PAT has been revoked" | Generate new PAT; check audit log |
| `COR_AUTH_MFA_REQUIRED` | 401 | never | `MFARequiredError` | "MFA verification required for this operation" | Re-authenticate with MFA |
| `COR_AUTH_MFA_STALE` | 401 | never | `MFAStaleError` | "MFA verification expired" | Re-authenticate (last MFA was > 30 min ago) |
| `COR_AUTH_SCOPE_INSUFFICIENT` | 403 | never | `ScopeInsufficientError` | "PAT scope does not allow this operation" | Generate PAT with required scope |

### 3.4 Rate limit errors (S-08)

| error_code | HTTP | retryable | SDK exception | customer_message | next_action |
|---|---|---|---|---|---|
| `COR_RATE_TENANT_QUOTA` | 429 | after_delay | `RateLimitedError` (within_quota=false) | "Tenant has exceeded plan quota" | Upgrade tier OR wait until `Retry-After` (typically days-until-month-reset) |
| `COR_RATE_PER_IP` | 429 | after_delay | `RateLimitedError` | "Too many requests from your IP" | Wait `Retry-After` seconds or contact support if NAT |
| `COR_RATE_PER_PAT` | 429 | after_delay | `RateLimitedError` | "PAT-level rate limit reached" | Reduce concurrent requests or generate additional PAT |
| `COR_RATE_GLOBAL_CIRCUIT` | 503 | exponential | `ServiceDegradedError` | "Service temporarily degraded" | Retry with exponential backoff per `Retry-After` |
| `COR_ABUSE_DETECTED` | 429 | never | `AbuseDetectedError` | "Unusual usage pattern detected; appeal available" | Use `POST /v1/admin/abuse_appeal` for human review (S-08 R-S08 / LGPD Art. 20) |

### 3.5 Privacy + DSR errors (S-11)

| error_code | HTTP | retryable | SDK exception | customer_message | next_action |
|---|---|---|---|---|---|
| `COR_DSR_RATE_LIMITED` | 429 | after_delay | `DSRRateLimitedError` | "DSR request limit reached (10/day)" | Wait 24h or contact privacy@corelink.dev |
| `COR_DSR_NOT_FOUND` | 404 | never | `DSRNotFoundError` | "DSR request not found" | Check `dsr_request_id` in JWT receipt |
| `COR_DSR_ALREADY_PROCESSED` | 409 | never | `DSRAlreadyProcessedError` | "This DSR has already been processed" | Check status via `GET /v1/privacy/dsr/{id}/status` |
| `COR_CONSENT_NOT_GRANTED` | 409 | never | `ConsentNotGrantedError` | "Cannot revoke consent that was never granted" | Verify `wording_id` matches an active consent |
| `COR_RESIDENCY_VIOLATION` | 403 | never | `ResidencyError` | "Cross-region data access prohibited" | Use endpoint matching tenant region (`<tenant>.<region>.corelink.dev`) |

### 3.6 Billing errors (S-10)

| error_code | HTTP | retryable | SDK exception | customer_message | next_action |
|---|---|---|---|---|---|
| `COR_BILLING_DPA_NOT_SIGNED` | 402 | never | `DPANotSignedError` | "DPA must be signed before service activation" | Sign DPA at `<dashboard>/onboarding/dpa` |
| `COR_BILLING_PAYMENT_REQUIRED` | 402 | never | `PaymentRequiredError` | "Payment method required" | Add payment method in Stripe Customer Portal |
| `COR_BILLING_OVERDUE` | 402 | after_delay | `OverdueError` | "Account past due" | Resolve via Stripe Customer Portal |
| `COR_BILLING_REPLAY_FORBIDDEN` | 403 | never | `ReplayForbiddenError` | "Invoice replay requires billing_admin role" | Request role from tenant admin |

### 3.7 BYOK errors (S-14)

| error_code | HTTP | retryable | SDK exception | customer_message | next_action |
|---|---|---|---|---|---|
| `COR_BYOK_KEY_NOT_FOUND` | 404 | never | `BYOKKeyNotFoundError` | "Customer-managed key not configured" | Configure BYOK in admin panel |
| `COR_BYOK_KMS_UNAVAILABLE` | 503 | exponential | `KMSUnavailableError` | "Customer KMS provider unavailable" | Verify KMS provider status; check IAM policy allows CoreLink |
| `COR_BYOK_ACCESS_REVOKED` | 403 | never | `BYOKAccessRevokedError` | "Customer revoked CMK access — service degraded" | Re-grant CoreLink access in your KMS within 5 min, OR continue with revocation flow |
| `COR_BYOK_DECRYPT_FAILED` | 500 | never | `DecryptError` | "Decryption failed" | Verify CMK is rotated correctly; check KMS audit log |

### 3.8 Admin / config errors (S-13)

| error_code | HTTP | retryable | SDK exception | customer_message | next_action |
|---|---|---|---|---|---|
| `COR_ADMIN_DUAL_APPROVAL_REQUIRED` | 403 | never | `DualApprovalRequiredError` | "Operation requires dual approval" | Send approval request to second admin |
| `COR_ADMIN_DUAL_APPROVAL_COLLUSION` | 403 | never | `ApprovalRotationError` | "Approver rotation required" | Find approver who hasn't approved last 3 ops in 24h (NIST AC-2(7)) |
| `COR_ADMIN_CONFIG_VERSION_STALE` | 409 | never | `ConfigVersionStaleError` | "Config has been updated by another admin" | Refresh and retry |

### 3.9 Multipart errors (S-05)

| error_code | HTTP | retryable | SDK exception | customer_message | next_action |
|---|---|---|---|---|---|
| `COR_MULTIPART_PART_MISSING` | 422 | never | `PartMissingError` | "Multipart part `<n>` not received" | Re-upload the missing part |
| `COR_MULTIPART_MERKLE_INVALID` | 422 | never | `MerkleInvalidError` | "Merkle tree of parts is invalid" | Verify all parts uploaded with correct hashes |
| `COR_MULTIPART_TIMEOUT` | 408 | linear | `MultipartTimeoutError` | "Multipart upload session expired" | Restart multipart upload |
| `COR_MULTIPART_BLOB_TOO_LARGE` | 413 | never | `BlobTooLargeError` | "Blob exceeds 160 GiB single multipart cap" | Use stitched multipart flow (WI-S05-006) for blobs > 160 GiB |
| `COR_MULTIPART_BACKEND_UNAVAILABLE` | 503 | exponential | `BackendUnavailableError` | "Multipart backend (R2) temporarily unavailable" | Retry with exponential backoff per `Retry-After` |
| `COR_MULTIPART_CONCURRENCY_LIMITED` | 429 | after_delay | `ConcurrencyLimitedError` | "Concurrent multipart sessions limit reached for tenant" | Reduce concurrent uploads or wait `Retry-After` seconds |
| `COR_MULTIPART_ALGO_UNSUPPORTED` | 422 | never | `ChunkerAlgoUnsupportedError` | "Chunker algorithm not supported (Fixed2MiB / FastCDC2MiB only)" | Use one of the supported algorithms |
| `COR_MULTIPART_SIG_INVALID` | 422 | never | `ManifestSigInvalidError` | "Manifest signature verification failed" | Possible cache tampering; re-upload original blob; report to support |
| `COR_MULTIPART_CHUNK_MISSING` | 422 | never | `ChunkMissingError` | "Manifest references missing or tombstoned chunk" | Re-upload original blob (chunks may have been GC'd) |
| `COR_MULTIPART_MANIFEST_NOT_FOUND` | 404 | never | `ManifestNotFoundError` | "Manifest with digest not in storage" | Verify digest is correct or use SplitBlob to populate |

### 3.10 Onboarding / Customer Lifecycle (S-19)

Domínio adicionado em Lote 9.5b endereçando Opus R3 R3-10 + Codex R3-10.

| error_code | HTTP | retryable | SDK exception | customer_message (en) | next_action |
|---|---|---|---|---|---|
| `COR_ONBOARD_EMAIL_VERIFICATION_PENDING` | 403 | after_delay | `EmailVerifyPendingError` | "Email verification required to complete signup" | Check inbox for verification email; retry após click |
| `COR_ONBOARD_DPA_NOT_SIGNED` | 402 | never | `DPANotSignedError` | "DPA must be signed before service activation" | Visit `<dashboard>/onboarding/dpa` to accept |
| `COR_ONBOARD_DPA_VERSION_BUMPED` | 409 | never | `DPAReSignRequiredError` | "Updated DPA requires re-acceptance" | Re-accept DPA at `<dashboard>/onboarding/dpa`; 30d grace before degrade |
| `COR_ONBOARD_TENANT_PROVISIONING_FAILED` | 500 | exponential | `TenantProvisioningError` | "Account setup failed" | Retry signup; if persistent, contact support@corelink.dev with request_id |
| `COR_ONBOARD_STRIPE_LINK_FAILED` | 500 | exponential | `StripeLinkError` | "Could not link payment provider" | Retry; check Stripe status; contact support if persistent |
| `COR_ONBOARD_SIGNUP_RATE_LIMITED` | 429 | after_delay | `SignupRateLimitedError` | "Too many signup attempts from this IP" | Wait `Retry-After` seconds; contact support if NAT |
| `COR_ONBOARD_INCOMPLETE_FLOW` | 409 | never | `IncompleteOnboardingError` | "Onboarding incomplete; resume required" | Continue at `<dashboard>/onboarding/resume` |

### 3.11 Service / infrastructure errors

| error_code | HTTP | retryable | SDK exception | customer_message | next_action |
|---|---|---|---|---|---|
| `COR_SERVICE_DEGRADED` | 503 | exponential | `ServiceDegradedError` | "Service temporarily degraded; full functionality returns shortly" | Retry per `Retry-After`; check status page |
| `COR_REGION_FAILOVER` | 503 | linear | `RegionFailoverError` | "Region temporarily unavailable; failing over" | Retry; SDK auto-routes to secondary region (S-14) |
| `COR_INTERNAL` | 500 | never | `InternalError` | "An internal error occurred (request_id=<X>)" | Contact support@corelink.dev with request_id |

---

## 4. Customer-visible message style

- Imperative + actionable.
- < 80 chars typically.
- Avoid jargão técnico ("Merkle tree" OK em developer docs; em UI/email é "integrity check failed").
- Include actionable hint when possible (`Retry-After`, `request_id`, dashboard link).
- Never PII or internal state names ("D1 row not found" → "Resource not found").
- Translatable: messages em `legal/i18n/error_taxonomy/<locale>.yaml`.

---

## 5. Next-action discipline

Every error MUST have one of:
- **Self-service** action (user can resolve via API/dashboard alone).
- **Communication-needed** action (contact support / privacy / legal).
- **Wait** action (retry-after; SDK handles).

Never `next_action: "An error occurred"` ou `next_action: "internal_error"`.

---

## 6. SDK exception class mapping

### Python (corelink-py via pyO3)
```python
class CoreLinkError(Exception):
    error_code: str
    http_status: int
    retryable: bool
    request_id: str | None

class DigestMismatchError(CoreLinkError): ...
class BlobNotFoundError(CoreLinkError): ...
# etc.
```

### Go (corelink-go via cgo)
```go
type CoreLinkError struct {
    ErrorCode  string
    HTTPStatus int
    Retryable  bool
    RequestID  string
}

type DigestMismatchError struct{ *CoreLinkError }
// etc.
```

### JS/TS (@corelink/client via WASM)
```typescript
export class CoreLinkError extends Error {
  errorCode: string;
  httpStatus: number;
  retryable: boolean;
  requestId?: string;
}

export class DigestMismatchError extends CoreLinkError {}
// etc.
```

---

## 7. Adding new errors

1. PR adicionando entry em §3 catalog matching schema §2.
2. SDK exception class adicionada em 3 languages (S-15 ownership).
3. Localized messages em 3 locales adicionadas (S-18 ownership).
4. CI gate `scripts/check_error_taxonomy.py` (criar Lote 9.5):
   - Verifica que cada error referenciado em código tem entry no catalog.
   - Verifica que SDK exceptions cobrem 100% dos error_codes.
   - Verifica que docs cobrem 100% dos error_codes.
5. Backwards-compatibility: nunca remover error_code; mark como `deprecated_in_sprint: S-XX` se obsoleto; remoção real após 12 meses (semver major bump).

---

**Ownership matrix**:

- Schema canonical: este doc.
- SDK exception classes: S-15.
- Localized messages: S-18 + per-sprint adições conforme errors são introduzidos.
- UI rendering of errors: S-16.
- Error injection chaos test: S-17.
- Error appearance em audit log: CTRL-AUDIT-003 (S-09).

**CI gate (planned Lote 9.5)**:
- `scripts/check_error_taxonomy.py` — verifica integridade catalog ↔ SDK ↔ docs.

---

**Fim error_taxonomy.md v0.3.1.**

---

## Changelog

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo (Lote 9.4) | Catalog inicial. |
| 0.2.0 | 2026-04-29 | Gustavo (WI-S01-005 SEAL Lote — codex round-1 P2 fix) | Added 4 new CAS codes referenced by WI-S01-005 §23 + `corelink-reapi` `error_map.rs`: `COR_CAS_BATCH_TOO_LARGE` (HTTP 413, gRPC RESOURCE_EXHAUSTED — `BatchUpdateBlobs` aggregate exceeds `MaxBatchTotalSizeBytes` 4 MiB; distinguishable at SDK level from per-blob `COR_CAS_BLOB_TOO_LARGE`); `COR_CAS_BAD_RESOURCE_NAME` (HTTP 400, gRPC INVALID_ARGUMENT — `ByteStream::Write` resource_name unparseable); `COR_CAS_DIGEST_FUNCTION_UNSUPPORTED` (HTTP 400 — server advertises BLAKE3 only in S-01); `COR_CAS_BAD_DIGEST` (HTTP 400 — digest hex / `size_bytes` mismatch). All four mapped to canonical gRPC codes per WI §9.6. |
| 0.3.0 | 2026-04-29 | Gustavo (WI-S02-002 SEAL Lote 11.2) | Added `COR_CAS_BATCH_SIZE_EXCEEDED` (HTTP 413, gRPC OUT_OF_RANGE) for `FindMissingBlobs` / `BatchReadBlobs` requests carrying more than the canonical 1000-digest batch cap. Distinct from `COR_CAS_BATCH_TOO_LARGE` (`RESOURCE_EXHAUSTED`, aggregate-bytes cap on `BatchUpdateBlobs`). Per WI-S02-002 §6.1.5 + REAPI v2 conformance recommendation. |
| 0.3.1 | 2026-04-29 | Gustavo (WI-S02-002 SEAL Lote 11.2 — codex round-2 P2 fix) | Added `COR_CAS_COMPRESSOR_UNSUPPORTED` (HTTP 412, gRPC FAILED_PRECONDITION) for `BatchReadBlobs` requests where `acceptable_compressors` is non-empty AND does NOT include `Compressor.IDENTITY` (= 0). REAPI v2 §`BatchReadBlobsRequest` semantics: an empty `acceptable_compressors` list means the client accepts the default IDENTITY encoding; a non-empty list MUST include IDENTITY in S-01 because CoreLink does NOT advertise compressed batch encodings yet (`CacheCapabilities.supported_compressors` and `supported_batch_update_compressors` are both empty until ZSTD/DEFLATE/BROTLI ship in WI-S05-005). Distinct from `COR_CAS_DIGEST_FUNCTION_UNSUPPORTED` (digest-function negotiation, not compressor negotiation) so dashboard / log triage can bucket compressor-negotiation failures separately. |
