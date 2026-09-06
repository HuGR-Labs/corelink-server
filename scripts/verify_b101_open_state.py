#!/usr/bin/env python3
"""Verify the *unfinished* side of every B-101 proposal.

This is deliberately not a regression/acceptance test.  B-101 records the
canonical backlog only; the WP which closes a finding owns its behavioural
test.  Until that WP lands, this guard proves two real repository facts:

* the source/configuration surface named by the finding still exists at a
  concrete declaration or executable route; and
* no source-specific closure witness has been installed yet (for items whose
  implementation backlog status is still ``open``).

The latter is an explicit negative capability: adding the closure witness
without changing the backlog status makes this command fail.  It prevents an
OPEN guard from becoming a permanent green after the implementation lands.
When invoked without ``--id``, already-``done`` implementation items are
excluded; their owning closure verifier is responsible for the positive gate.
Comments are ignored for source anchors, and every path is checked as a
regular file inside the supplied repository root.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REGISTRY = Path("reports/audit-finding-decisions/proposals-v1.json")
PROPOSAL_IDS = tuple(f"B-{number}" for number in range(171, 244))
_VIEW_CACHE: dict[str, tuple[tuple[int, int], tuple[str, str, str]]] = {}


class OpenStateError(ValueError):
    """The proposed item is not provably unfinished or its instrument is invalid."""


@dataclass(frozen=True)
class Rule:
    # One or more concrete source/config files.  Globs are intentionally not
    # used: an exact path is easier to review and cannot silently broaden.
    artifacts: tuple[str, ...]
    # A declaration/route/config section which must occur outside comments.
    anchor: str
    # A closure witness is produced by the owning WP, never by B-101.
    closure_witness: str
    # Tokens which must still be absent from the relevant live config section.
    absent: tuple[str, ...] = ()


# Each item has a second, source-specific executable contract in addition to
# its declaration anchor.  Keeping these contracts explicit is intentional:
# a copied function name, a comment, or a string literal must not be enough to
# satisfy an OPEN finding.  The patterns are evaluated against the complete
# source surface (with comments/literals masked below), except where the
# finding is itself a SQL/Markdown/configuration contract.
REQUIRED_CODE: dict[str, tuple[str, ...]] = {
    "B-171": (r"tenant_prefix\(self\.tdk\.as_ref\(\),\s*tenant\)\?", r"R2S3Client::blob_key"),
    "B-172": (r"MAX_MANIFEST_BYTES", r"ManifestOversized"),
    "B-173": (r"last_active_ms", r"OCI_SESSION_IDLE_TIMEOUT_MS", r"inflight_bytes"),
    "B-174": (r"tenant_new", r"RENAME\s+TO\s+tenant"),
    "B-175": (r'"wnam"', r'"enam"', r'"weur"', r'"apac"', r"isMacroRegion"),
    "B-176": (r"signature_ed25519", r"canonical_payload_jcs", r"INSERT\s+OR\s+IGNORE"),
    "B-177": (r"build_with_factory", r"Router"),
    "B-178": (r"requestQuotaEnabled", r"tierResult\.d1Error", r"requestKvKey|REQUEST"),
    # This contract must describe the real OCI pass-through, not a toy h.set.
    "B-179": (r"route\.routeKind\s*===\s*\"oci_v2\"", r"ociStub\.fetch", r"new\s+Request\(request"),
    "B-180": (r"CORELINK_OCI_TOKEN_KEY", r"HUGR_OCI_TOKEN_KEY", r"legacy|fallback"),
    "B-181": (r"tierResult\.d1Error", r"isMutating", r"fail|closed|ok"),
    "B-182": (r"expires_ms\s*=\s*0", r"unixepoch", r"revoked_at_ms"),
    "B-183": (r"InvalidPat", r"Backend", r"argon2|Argon2"),
    "B-184": (r"expires_ms", r"revoked_at_ms", r"primaryDb|readDb"),
    "B-185": (r"authorization", r"CLERK_ISSUER_URL", r"issuer"),
    "B-186": (r"NEXT_PUBLIC_E2E_TEST_MODE", r"NODE_ENV", r"production"),
    "B-187": (r"RunnerTier", r"tenant_tiers", r"shadow_charge"),
    "B-188": (r"x-corelink-tenant-id", r"is_reserved_sentinel", r"StatusCode::UNAUTHORIZED"),
    "B-189": (r"verify_locks|single.?flight", r"VERIFY_CACHE_TTL", r"PatVerifier"),
    "B-190": (r"tenant_prefix\(self\.tdk\.as_ref\(\),\s*tenant\)\?", r"digest", r"blob_key"),
    "B-191": (r"object_key", r"async\s+fn\s+put", r"async\s+fn\s+get"),
    # B-192 is intentionally checked across both the authenticated extractor
    # and the Turbo storage keying surface below.
    "B-192": (
        r"is_reserved_sentinel\(raw\)",
        r"is_canonical_tenant_id\(raw\)",
        r"Uuid::try_parse",
        r"derive_prefix\(tdk,\s*uid\)",
    ),
    "B-193": (r"verify_against_bytes\(&session\.buf\)", r"moat\.put|\.put\(", r"OciDigest::parse"),
    "B-194": (r"delete_locks", r"head_object|head", r"delete_object|delete"),
    "B-195": (r"CREATE\s+TABLE\s+IF\s+NOT\s+EXISTS", r"PRIMARY\s+KEY"),
    "B-196": (r"resolveEraseAuthKey", r"\.all\(\)|LIMIT", r"dsr_requested"),
    "B-197": (r"residency_decision", r"ResidencyDecision::Reject", r"container_colo"),
    "B-198": (r"PROVISIONED_MACROS", r"isMacroRegion", r'"apac"'),
    "B-199": (r'"sam"', r'"apac"', r'"afr"', r"isMacroRegion"),
    "B-200": (r"DEADLINE_MS", r"requested", r"completed|completion"),
    "B-201": (r"ac_list_prefix", r"list_objects_v2", r"count_ac_remaining"),
    "B-202": (r"bucket", r"cas_region|region", r"R2S3Client"),
    "B-203": (r'"wnam"', r'"enam"', r'"weur"', r'"apac"', r"PROVISIONED_MACROS"),
    "B-204": (r"BYOK", r"FIPS", r"ATTESTATION-MATRIX"),
    "B-205": (r"BURST_MARGIN", r"requestKvKey", r"KV_REQUEST_PREFIX"),
    "B-206": (r"setAlarm", r"reaper", r"alarm"),
    "B-207": (r"blockConcurrencyWhile", r"lifecycle", r"container"),
    "B-208": (r"isValidHex", r"sha.?256|SHA.?256", r"expected_root"),
    "B-209": (r"Authorization|authorization", r"fetch", r"consent"),
    "B-210": (r"getSessionToken", r"tenantId", r"configure"),
    "B-211": (r"getToken", r"Authorization|authorization", r"fetchImpl"),
    "B-212": (r"useState", r"audit|Audit", r"loading|error"),
    "B-213": (r"requestHeaders", r"content-security-policy", r"x-nonce"),
    "B-214": (r"tenant_org_map|insertTenantOrgMap", r"user\.created|user_created", r"email"),
    "B-215": (r"colo", r"enam", r"afr|apac|sam"),
    "B-216": (r"erasure|DSR", r"clerkUserId", r"CONFIG_DB|prepare"),
    "B-217": (r"DsrQueuedV1", r"consumer|ERASE", r"fetch|queue"),
    "B-218": (r"svix-signature|svixSignature", r"CORELINK_INTERNAL_AUTH_KEY|CLERK_WEBHOOK", r"autoProvision"),
    "B-219": (r"BACKEND_COUNT", r"SUCCESSFUL|success", r"verified-empty|empty"),
    "B-220": (r"AuthTenant", r"SENTINELS|sentinel", r"tenant"),
    "B-221": (r"mint", r"throttle|rate", r"requestId"),
    "B-222": (r"autoProvisionFromClerkEvent", r"insertTenantOrgMap", r"pat|PAT"),
    "B-223": (r"openapi", r"servers", r"Production"),
    "B-224": (r"ANALYTICS_ENDPOINT", r"method:\s*\"POST\"", r"ANALYTICS_INGEST_KEY"),
    "B-225": (r"chunk_len", r"MAX|capacity|quota", r"session"),
    "B-226": (r"Idempotency|event_id", r"Failed|Inserted|Updated", r"DlqError"),
    "B-227": (r"EnvFilter", r"try_from_default_env", r"corelink_server"),
    "B-228": (r"sanitize|control|header|principal", r"build_with_factory", r"Router"),
    "B-229": (r"CORELINK_API_BASE", r"ENVIRONMENT", r"\[vars\]"),
    "B-230": (r"ENVIRONMENT", r"R2_S3_ENDPOINT", r"\[env\.prod\]"),
    "B-231": (r"includedRequests", r"includedCasGb", r"free"),
    "B-232": (r"BREW_SERVICE_PRINCIPAL", r"CasReadHandler", r"CasWriteHandler"),
    "B-233": (r"tenant_id", r"now_ms", r"correlation_id"),
    "B-234": (r"idempotency", r"materializer", r"audit"),
    "B-235": (r"HashMap", r"Mutex", r"WebhookDlqRow"),
    "B-236": (r"MATRIX_ROW_RE", r"backtick|split", r"names"),
    "B-237": (r"Duration::from_secs\(5\)", r"VERIFY_CACHE_TTL", r"cache"),
    "B-238": (r"verify_attestation_signature", r"schema_version", r"Region"),
    "B-239": (r"len\(\)\s*==\s*64", r"is_ascii_digit", r"wording_id"),
    "B-240": (r"METHOD_NOT_ALLOWED", r"internal|auth", r"tenant"),
    "B-241": (r"resolve-tenant|tenant_lookup", r"InternalConsumer", r"POST|pathSuffix"),
    "B-242": (r"emailHashFor", r"emailHashLegacy", r"salt"),
    # Raw-source checks for B-243 are applied inside the real function below;
    # this catches INSERT-vs-INSERT OR IGNORE mutations and bind drift.
    "B-243": (r"INSERT\s+OR\s+IGNORE\s+INTO\s+tenant_org_map", r"VALUES\s*\(\?1,\s*\?2,\s*\?3\)", r"bind\(params\.clerkOrgId,\s*params\.tenantId,\s*params\.nowMs\)"),
}


# The anchors are source-specific executable declarations, not finding titles
# and not comments.  A few findings are configuration omissions; those use a
# real section anchor plus ``absent`` rather than a prose marker.
RULES: dict[str, Rule] = {
    # The R2 facade includes this bounded part; inspect the implementation
    # unit directly so open-state mutations do not depend on include layout.
    "B-171": Rule(("crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs",), r"\bfn\s+r2_key\s*\(", "tests/audit/b101/closures/B-171.py"),
    "B-172": Rule(("crates/corelink-adapter-host/src/oci/server/handlers.rs",), r"\basync\s+fn\s+dispatch_manifest\s*\(", "tests/audit/b101/closures/B-172.py"),
    "B-173": Rule(("crates/corelink-container/src/routes/oci.rs",), r"\bstruct\s+UploadSession\b", "tests/audit/b101/closures/B-173.py"),
    "B-174": Rule(("migrations/d1/0064_tenant_tier_max.sql",), r"\bALTER\s+TABLE\b", "tests/audit/b101/closures/B-174.py"),
    "B-175": Rule(("worker/src/region-map.ts",), r"\bexport\s+const\s+PROVISIONED_MACROS\b", "tests/audit/b101/closures/B-175.py"),
    "B-176": Rule(("crates/corelink-container/src/routes/dsr/attestation.rs",), r"\bfn\s+sign_and_persist\s*\(", "tests/audit/b101/closures/B-176.py"),
    "B-177": Rule(("crates/corelink-container/src/routes.rs",), r"\bpub\s+fn\s+build\s*\(\s*\)\s*->\s*Router", "tests/audit/b101/closures/B-177.py"),
    "B-178": Rule(("worker/src/lib/quota.ts",), r"\bexport\s+async\s+function\s+checkRequestQuota\s*\(", "tests/audit/b101/closures/B-178.py"),
    "B-179": Rule(("worker/src/index_special_routes.ts",), r"\bh\.set\(\s*", "tests/audit/b101/closures/B-179.py"),
    "B-180": Rule(("crates/corelink-container/src/routes/oci.rs",), r"\bpub\s+const\s+OCI_TOKEN_KEY_ENV\b", "tests/audit/b101/closures/B-180.py"),
    "B-181": Rule(("worker/src/lib/quota.ts",), r"\bexport\s+async\s+function\s+checkStorageQuota\s*\(", "tests/audit/b101/closures/B-181.py"),
    "B-182": Rule(("crates/corelink-container/src/adapter_pat_lookup.rs",), r"\bpub\(super\)\s+const\s+PAT_LOOKUP_SQL\s*:", "tests/audit/b101/closures/B-182.py"),
    "B-183": Rule(("crates/corelink-container/src/adapter_pat_lookup.rs",), r"\bpub\s+enum\s+VerifyError\b", "tests/audit/b101/closures/B-183.py"),
    "B-184": Rule(("worker/src/lib/pat_verify_cache.ts",), r"\bconst\s+PAT_ROW_SQL\b", "tests/audit/b101/closures/B-184.py"),
    "B-185": Rule(("worker/src/lib/clerk_auth.ts",), r"\bexport\s+async\s+function\s+verifyClerkSessionAndResolveTenant\s*\(", "tests/audit/b101/closures/B-185.py"),
    "B-186": Rule(("apps/admin-ui/src/middleware.ts",), r"\bconst\s+isE2E\s*=", "tests/audit/b101/closures/B-186.py"),
    "B-187": Rule(("crates/corelink-runner-aggregate/src/lib.rs",), r"\bpub\s+fn\s+aggregate_runner_usage\s*\(", "tests/audit/b101/closures/B-187.py"),
    "B-188": Rule(("crates/corelink-container/src/auth_tenant.rs",), r"\bimpl<S:\s*Send\s*\+\s*Sync>\s+FromRequestParts", "tests/audit/b101/closures/B-188.py"),
    "B-189": Rule(("crates/corelink-container/src/native_pat_gate.rs",), r"\bpub\s+struct\s+NativePatGate\b", "tests/audit/b101/closures/B-189.py"),
    "B-190": Rule(("crates/corelink-container/src/storage/r2_s3.rs",), r"\bfn\s+r2_key\s*\(", "tests/audit/b101/closures/B-190.py"),
    "B-191": Rule(("crates/corelink-container/src/storage/r2_kv.rs",), r"\bpub\s+trait\s+KvBackend\b", "tests/audit/b101/closures/B-191.py"),
    "B-192": Rule(("crates/corelink-container/src/auth_tenant.rs",), r"\bimpl<S:\s*Send\s*\+\s*Sync>\s+FromRequestParts", "tests/audit/b101/closures/B-192.py"),
    "B-193": Rule(("crates/corelink-container/src/routes/oci.rs",), r"\basync\s+fn\s+finalize_upload\s*\(", "tests/audit/b101/closures/B-193.py"),
    "B-194": Rule(("crates/corelink-container/src/storage/r2_s3.rs",), r"\bfn\s+delete_if_present\s*\(", "tests/audit/b101/closures/B-194.py"),
    "B-195": Rule(("migrations/d1/0044_stripe_webhook_events_processed.sql", "migrations/d1/0044_drata_evidence_sent.sql"), r"\bCREATE\s+TABLE\b", "tests/audit/b101/closures/B-195.py"),
    "B-196": Rule(("apps/signup-worker/src/webhooks/dsr_verify_cron.ts",), r"\bexport\s+async\s+function\s+runDsrVerifySweep\s*\(", "tests/audit/b101/closures/B-196.py"),
    "B-197": Rule(("crates/corelink-container/src/routes/residency.rs",), r"\bpub\s+async\s+fn\s+residency_guard\s*\(", "tests/audit/b101/closures/B-197.py"),
    "B-198": Rule(("worker/src/region-map.ts",), r"\bexport\s+const\s+PROVISIONED_MACROS\b", "tests/audit/b101/closures/B-198.py"),
    "B-199": Rule(("worker/src/region-map.ts",), r"\bexport\s+function\s+isMacroRegion\s*\(", "tests/audit/b101/closures/B-199.py"),
    "B-200": Rule(("apps/signup-worker/src/webhooks/dsr_verify_cron.ts",), r"\bconst\s+DEADLINE_MS\s*=", "tests/audit/b101/closures/B-200.py"),
    "B-201": Rule(("crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs",), r"\bfn\s+list_and_delete_ac\s*\(", "tests/audit/b101/closures/B-201.py"),
    "B-202": Rule(("crates/corelink-container/src/storage/r2_s3.rs",), r"\bpub\s+struct\s+R2S3Client\b", "tests/audit/b101/closures/B-202.py"),
    "B-203": Rule(("worker/src/region-map.ts",), r"\bexport\s+const\s+PROVISIONED_MACROS\b", "tests/audit/b101/closures/B-203.py"),
    "B-204": Rule(("marketing/sales/legal-questionnaires/EVIDENCE-PACK-INDEX.md",), r"^\|\s*30\s*\|", "tests/audit/b101/closures/B-204.py"),
    "B-205": Rule(("worker/src/lib/quota_request_cache.ts",), r"\bexport\s+function\s+burstMargin\s*\(", "tests/audit/b101/closures/B-205.py"),
    "B-206": Rule(("worker/src/durable_object.ts",), r"\basync\s+alarm\s*\(", "tests/audit/b101/closures/B-206.py"),
    "B-207": Rule(("worker/src/durable_object.ts",), r"\bblockConcurrencyWhile\s*\(", "tests/audit/b101/closures/B-207.py"),
    "B-208": Rule(("apps/admin-ui/src/lib/audit/verify-proof.ts",), r"\bexport\s+async\s+function\s+verifyAuditProof\s*\(", "tests/audit/b101/closures/B-208.py"),
    "B-209": Rule(("apps/admin-ui/src/lib/consent-api.ts",), r"\bexport\s+function\s+createConsentApi\s*\(", "tests/audit/b101/closures/B-209.py"),
    "B-210": Rule(("apps/admin-ui/src/app/[locale]/onboarding/actions.ts",), r"\bexport\s+async\s+function\s+configureTenantAction\s*\(", "tests/audit/b101/closures/B-210.py"),
    "B-211": Rule(("apps/admin-ui/src/lib/admin-client.ts",), r"\bexport\s+class\s+AdminClient\b", "tests/audit/b101/closures/B-211.py"),
    "B-212": Rule(("apps/admin-ui/src/app/[locale]/(authenticated)/customer/audit/visualization/page.tsx",), r"\bexport\s+default\s+function\s+CustomerAuditVisualizationPage\s*\(", "tests/audit/b101/closures/B-212.py"),
    "B-213": Rule(("apps/admin-ui/src/middleware.ts",), r"\bexport\s+default\s+async\s+function\s+middleware\s*\(", "tests/audit/b101/closures/B-213.py"),
    "B-214": Rule(("apps/signup-worker/src/webhooks/clerk.ts",), r"\bexport\s+async\s+function\s+autoProvisionFromClerkEvent\s*\(", "tests/audit/b101/closures/B-214.py"),
    "B-215": Rule(("apps/signup-worker/src/webhooks/clerk.ts",), r"\bexport\s+async\s+function\s+autoProvisionFromClerkEvent\s*\(", "tests/audit/b101/closures/B-215.py"),
    # M3 moved the executable deletion/erasure route into its dedicated
    # module; clerk.ts remains the public facade/re-export surface.
    "B-216": Rule(("apps/signup-worker/src/webhooks/clerk_erasure.ts",), r"\bexport\s+async\s+function\s+handleUserDeleted\s*\(", "tests/audit/b101/closures/B-216.py"),
    "B-217": Rule(("apps/signup-worker/src/webhooks/dsr_consumer.ts",), r"\bexport\s+async\s+function\s+processErasureMessage\s*\(", "tests/audit/b101/closures/B-217.py"),
    "B-218": Rule(("apps/signup-worker/src/webhooks/clerk.ts",), r"\bexport\s+async\s+function\s+handleClerkWebhook\s*\(", "tests/audit/b101/closures/B-218.py"),
    "B-219": Rule(("crates/corelink-container/src/routes/dsr/attestation.rs",), r"\bfn\s+verified_evidence_segments\s*\(", "tests/audit/b101/closures/B-219.py"),
    "B-220": Rule(("crates/corelink-container/src/auth_tenant.rs",), r"\bpub\s+struct\s+AuthTenant\b", "tests/audit/b101/closures/B-220.py"),
    "B-221": Rule(("worker/src/lib/session_exchange.ts",), r"\bexport\s+async\s+function\s+handleSessionExchange\s*\(", "tests/audit/b101/closures/B-221.py"),
    "B-222": Rule(("apps/signup-worker/src/webhooks/clerk.ts",), r"\bexport\s+async\s+function\s+autoProvisionFromClerkEvent\s*\(", "tests/audit/b101/closures/B-222.py"),
    "B-223": Rule(("worker/src/lib/openapi_devenv.ts",), r"\bexport\s+const\s+devenvOpenApiSpec\b", "tests/audit/b101/closures/B-223.py"),
    "B-224": Rule(("apps/signup-worker/src/lib/analytics-server.ts",), r"\bexport\s+async\s+function\s+emit\s*\(", "tests/audit/b101/closures/B-224.py"),
    "B-225": Rule(("crates/corelink-container/src/routes/oci.rs",), r"\basync\s+fn\s+append_chunk\s*\(", "tests/audit/b101/closures/B-225.py"),
    "B-226": Rule(("crates/corelink-stripe-real/src/dlq.rs",), r"\bpub\s+trait\s+WebhookDlqStore\b", "tests/audit/b101/closures/B-226.py"),
    "B-227": Rule(("crates/corelink-container/src/main.rs",), r"\btracing_subscriber::", "tests/audit/b101/closures/B-227.py"),
    "B-228": Rule(("crates/corelink-container/src/routes.rs",), r"\bpub\s+fn\s+build\s*\(\s*\)\s*->\s*Router", "tests/audit/b101/closures/B-228.py"),
    "B-229": Rule(("apps/signup-worker/wrangler.toml",), r"^\[vars\]\s*$", "tests/audit/b101/closures/B-229.py", ("CLERK_WEBHOOK_SECRET",)),
    "B-230": Rule(("wrangler.toml",), r"^\[env\.prod\]\s*$", "tests/audit/b101/closures/B-230.py", ("R2_CHUNK_BUCKET", "R2_CHUNK_REGION")),
    "B-231": Rule(("apps/docs/src/lib/pricing.ts",), r"\bexport\s+const\s+TIER_RATE_CARD\b", "tests/audit/b101/closures/B-231.py"),
    "B-232": Rule(("crates/corelink-container/src/routes/brew.rs",), r"\bpub\s+fn\s+router\s*\(", "tests/audit/b101/closures/B-232.py"),
    "B-233": Rule(("crates/corelink-tier-selection/src/tenant.rs",), r"\bpub\s+struct\s+TenantCtx\b", "tests/audit/b101/closures/B-233.py"),
    "B-234": Rule(("crates/corelink-stripe-real/src/webhook_dispatch.rs",), r"\bpub\s+struct\s+WebhookDispatcher\b", "tests/audit/b101/closures/B-234.py"),
    "B-235": Rule(("crates/corelink-stripe-real/src/dlq.rs",), r"\bpub\s+struct\s+InMemoryWebhookDlqStore\b", "tests/audit/b101/closures/B-235.py"),
    "B-236": Rule(("scripts/validate_secrets_matrix.py",), r"\bdef\s+parse_matrix\s*\(", "tests/audit/b101/closures/B-236.py"),
    "B-237": Rule(("crates/corelink-container/src/native_pat_gate.rs",), r"\bpub\s+const\s+VERIFY_CACHE_TTL\b", "tests/audit/b101/closures/B-237.py"),
    "B-238": Rule(("crates/corelink-erasure-attestation/src/lib.rs",), r"\bpub\s+use\s+region::Region\s*;", "tests/audit/b101/closures/B-238.py"),
    "B-239": Rule(("crates/corelink-container/src/routes/dpa_accept.rs",), r"\bfn\s+is_sha256_hex64\s*\(", "tests/audit/b101/closures/B-239.py"),
    "B-240": Rule(("worker/src/lib/tenant_lookup.ts",), r"\bexport\s+async\s+function\s+handleTenantLookup\s*\(", "tests/audit/b101/closures/B-240.py"),
    "B-241": Rule(("worker/src/index.ts",), r"\bexport\s+function\s+internalConsumerForPath\s*\(", "tests/audit/b101/closures/B-241.py"),
    "B-242": Rule(("apps/signup-worker/src/webhooks/clerk.ts",), r"\bfunction\s+emailHashCandidates\s*\(", "tests/audit/b101/closures/B-242.py"),
    "B-243": Rule(("apps/signup-worker/src/lib/d1.ts",), r"\bexport\s+async\s+function\s+insertTenantOrgMap\s*\(", "tests/audit/b101/closures/B-243.py"),
}


def _scan_source(text: str, *, mask_strings: bool) -> str:
    """Mask comments (and optionally literals) without changing line offsets.

    This is a deliberately small lexical scanner rather than a collection of
    regex substitutions.  In particular, ``// fn r2_key()`` and
    ``"fn r2_key()"`` are both inert, while a URL/string containing ``/*``
    cannot consume the executable code that follows it.
    """
    out = list(text)
    i = 0
    line_start = True
    while i < len(text):
        c = text[i]
        n = text[i + 1] if i + 1 < len(text) else ""
        if c == "\n":
            line_start = True
            i += 1
            continue
        if c in " \t\r":
            i += 1
            continue
        # Line comments in Rust/TS/JS, SQL and Python/TOML.  Preserve Rust
        # attributes (#[...]) and ordinary hash characters in code.
        hash_comment = c == "#" and line_start and n != "["
        if (c == "/" and n == "/") or (c == "-" and n == "-") or hash_comment:
            j = i
            while j < len(text) and text[j] != "\n":
                out[j] = " "
                j += 1
            i = j
            line_start = False
            continue
        if c == "/" and n == "*":
            out[i] = out[i + 1] = " "
            i += 2
            while i < len(text):
                if text[i] == "\n":
                    line_start = True
                    i += 1
                    continue
                if text[i] == "*" and i + 1 < len(text) and text[i + 1] == "/":
                    out[i] = out[i + 1] = " "
                    i += 2
                    break
                out[i] = " "
                i += 1
            continue
        # Rust raw strings.  Their delimiters are not meaningful to the
        # source-level predicates, so mask the complete literal as one unit.
        # Never hand the unbounded suffix to ``re.match`` here: slicing it at
        # every character turns a 250-KB source file into quadratic work.
        raw_match = re.match(r"(?:br|r)(#+)\"", text[i : i + 64]) if c == "r" or c == "b" else None
        if raw_match:
            hashes = raw_match.group(1)
            end_marker = "\"" + hashes
            j = i + raw_match.end()
            for k in range(i, j):
                if text[k] != "\n":
                    out[k] = " "
            end = text.find(end_marker, j)
            end = len(text) if end < 0 else end + len(end_marker)
            for k in range(j, end):
                if text[k] != "\n":
                    out[k] = " "
            i = end
            line_start = False
            continue
        # Rust lifetimes (e.g. ``'static``) are not character/string
        # literals.  Treat a single quote followed by an identifier as code
        # unless a closing quote is close enough to be a character literal.
        if c == "'" and (n.isalpha() or n == "_") and text.find("'", i + 1, i + 8) < 0:
            line_start = False
            i += 1
            continue
        if c in "\"'`":
            quote = c
            j = i + 1
            while j < len(text):
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == quote:
                    j += 1
                    break
                j += 1
            if mask_strings:
                for k in range(i, min(j, len(text))):
                    if text[k] != "\n":
                        out[k] = " "
            i = j
            line_start = False
            continue
        line_start = False
        i += 1
    return "".join(out)


def _strip_comments(text: str) -> str:
    """Compatibility helper: remove comments, retain literals."""
    return _scan_source(text, mask_strings=False)


def _line_comment_source(text: str) -> str:
    """Comment stripper for SQL/TOML/Python data files.

    These formats do not contain the Rust lifetime ambiguity handled by the
    code scanner, and preserving their quoted values is part of the contract.
    """
    text = re.sub(r"(?m)//[^\n]*", "", text)
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    text = re.sub(r"(?m)^\s*(?:#|--)[^\n]*", "", text)
    return text


def _mask_non_code(text: str) -> str:
    """Return source with both comments and string literals masked."""
    return _scan_source(text, mask_strings=True)


def _source_views(path: Path) -> tuple[str, str, str]:
    """Return (raw, comment_free, code_only), cached per path and content."""
    raw = path.read_text(encoding="utf-8")
    stat = path.stat()
    key = str(path)
    cached_entry = _VIEW_CACHE.get(key)
    signature = (stat.st_mtime_ns, stat.st_size)
    if cached_entry is not None and cached_entry[0] == signature:
        return cached_entry[1]
    if cached_entry is None or cached_entry[0] != signature:
        if path.suffix in {".sql", ".toml", ".py"}:
            literal_view = _line_comment_source(raw)
            code_view = literal_view
        elif path.suffix == ".md":
            literal_view = raw
            code_view = raw
        else:
            literal_view = _strip_comments(raw)
            code_view = _mask_non_code(raw)
        cached = (raw, literal_view, code_view)
        # Keep the cache bounded while allowing one complete verifier run to
        # avoid rescanning r2_s3/index.ts for their several related findings.
        if len(_VIEW_CACHE) > 96:
            _VIEW_CACHE.clear()
        _VIEW_CACHE[key] = (signature, cached)
    return cached


def _backlog_status(root: Path, identifier: str) -> str:
    """Read the implementation status without importing the backlog gate."""
    backlog = (root / "BACKLOG.md").read_text(encoding="utf-8")
    section = re.search(
        rf"(?ms)^### {re.escape(identifier)}\b.*?(?=^### B-\d+\b|\Z)", backlog
    )
    if section is None:
        return "open"
    match = re.search(r"^status:\s+(open|done|parked)\s*$", section.group(0), re.MULTILINE)
    return match.group(1) if match else "open"


def _function_body(text: str, declaration: str, checked_text: str | None = None) -> str:
    """Extract the balanced-brace body beginning at a real declaration."""
    checked = _mask_non_code(text) if checked_text is None else checked_text
    match = re.search(declaration, checked, flags=re.MULTILINE)
    if not match:
        return ""
    # The first brace after a declaration can belong to a destructured
    # parameter (notably ``params: { ... }`` in TypeScript).  Find the body
    # brace only after the declaration's parameter list has closed.
    opening = -1
    # Some declaration patterns consume the opening ``(`` while others stop
    # before an impl/type body; derive the initial depth from the match.
    parens = checked[match.start() : match.end()].count("(") - checked[match.start() : match.end()].count(")")
    for index in range(match.end(), len(checked)):
        token = checked[index]
        if token == "(":
            parens += 1
        elif token == ")":
            parens = max(0, parens - 1)
        elif token == "{" and parens == 0:
            opening = index
            break
    if opening < 0:
        return ""
    depth = 0
    for index in range(opening, len(checked)):
        if checked[index] == "{":
            depth += 1
        elif checked[index] == "}":
            depth -= 1
            if depth == 0:
                return text[opening : index + 1]
    return ""


def _require_patterns(
    identifier: str,
    text: str,
    *,
    raw_literals: bool = False,
    literal_view: str | None = None,
    checked_view: str | None = None,
) -> None:
    """Require every source-specific semantic clause for an item."""
    checked = _mask_non_code(text) if checked_view is None else checked_view
    # A few contracts are intrinsically literal/configuration contracts (SQL,
    # env names, route discriminants).  They still use comment-free input, so
    # comments cannot act as witnesses, but their quoted values remain visible.
    literal_view = _strip_comments(text) if literal_view is None else literal_view
    view = literal_view if raw_literals else checked
    missing = [pattern for pattern in REQUIRED_CODE[identifier] if re.search(pattern, view, re.IGNORECASE | re.DOTALL) is None]
    if missing and not raw_literals:
        # Identifiers and operators belong in the masked code view; quoted
        # literals belong in the comment-free view.  Accept each clause in
        # whichever lexical class it actually occupies.
        missing = [
            pattern
            for pattern in missing
            if re.search(pattern, literal_view, re.IGNORECASE | re.DOTALL) is None
        ]
    if missing:
        raise OpenStateError(f"{identifier}: semantic contract clause missing: {missing[0]}")


def _verify_special_semantics(root: Path, identifier: str) -> None:
    """Verify scopes whose meaning spans a route or more than one artifact."""
    if identifier == "B-171":
        path = _safe_file(root, "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs", identifier)
        raw, _literal, code = _source_views(path)
        body = _function_body(raw, r"\bfn\s+r2_key\s*\(", code)
        if not body:
            raise OpenStateError(f"{identifier}: r2_key executable scope missing")
        body_code = _mask_non_code(body)
        _require_patterns(identifier, body, literal_view=body_code, checked_view=body_code)
    elif identifier == "B-179":
        path = _safe_file(root, "worker/src/index_special_routes.ts", identifier)
        raw = path.read_text(encoding="utf-8")
        literal_view = _strip_comments(raw)
        route = re.search(r"if\s*\(\s*route\.routeKind\s*===\s*\"oci_v2\".*?\n", literal_view)
        if not route:
            raise OpenStateError(f"{identifier}: OCI pass-through route scope missing")
        # Bound the predicate to the actual OCI branch.  A copied h.set in an
        # unrelated helper or a string bait elsewhere cannot satisfy it.
        end = literal_view.find("return applyCors(ociResp", route.end())
        region_end = end if end >= 0 else len(raw)
        region = raw[route.start() : region_end]
        region_code = _mask_non_code(region)

        def require_forward_setter(request_name: str, fetch_expression: str) -> None:
            request_start = re.search(
                rf"\bconst\s+{re.escape(request_name)}Req\s*=\s*new\s+Request\(request,\s*\{{",
                region_code,
            )
            if request_start is None:
                raise OpenStateError(
                    f"{identifier}: {request_name} OCI Request scope missing"
                )
            fetch = re.search(
                rf"\bawait\s+{re.escape(fetch_expression)}\.fetch\(\s*{re.escape(request_name)}Req\s*\)",
                region_code[request_start.end() :],
            )
            if fetch is None:
                raise OpenStateError(
                    f"{identifier}: {request_name} OCI fetch boundary missing"
                )
            request_region = region_code[
                request_start.start() : request_start.end() + fetch.start()
            ]
            # Literals are masked, so this structural predicate cannot be
            # satisfied by comments or a string containing a copied setter.
            if re.search(
                r"h\.set\(\s*,\s*request\.headers\.get\(\s*\)\s*\?\?\s*\s*\)",
                request_region,
            ) is None:
                raise OpenStateError(
                    f"{identifier}: {request_name} OCI client-ip setter missing in forwarding branch"
                )

        require_forward_setter("regional", "regionalBinding")
        require_forward_setter("oci", "ociStub")
        _require_patterns(identifier, region, raw_literals=True)
    elif identifier == "B-192":
        auth_path = _safe_file(root, "crates/corelink-container/src/auth_tenant.rs", identifier)
        auth_raw, auth_literal, auth_code = _source_views(auth_path)
        auth_body = _function_body(auth_raw, r"\bimpl<S:\s*Send\s*\+\s*Sync>\s+FromRequestParts", auth_code)
        if not auth_body:
            raise OpenStateError(f"{identifier}: AuthTenant extractor scope missing")
        # These are executable extractor clauses, not string-valued config;
        # keep literals masked so an attacker constant cannot become a witness.
        auth_view = _mask_non_code(auth_body)
        for clause in (
            r"is_reserved_sentinel\(raw\)",
            r"is_canonical_tenant_id\(raw\)",
            r"if\s+!is_canonical_tenant_id\(raw\)",
            r"Ok\(AuthTenant\(raw\.to_owned\(\)\)\)",
        ):
            if re.search(clause, auth_view) is None:
                raise OpenStateError(f"{identifier}: AuthTenant semantic clause missing: {clause}")
        kv_path = _safe_file(root, "crates/corelink-container/src/storage/r2_kv.rs", identifier)
        kv_raw, _kv_literal, kv_code = _source_views(kv_path)
        kv_body = _function_body(kv_raw, r"\bfn\s+object_key\s*\(", kv_code)
        if not kv_body:
            raise OpenStateError(f"{identifier}: Turbo/R2 object-key scope missing")
        kv_view = _mask_non_code(kv_body)
        for clause in (
            r"self\.tdk\.as_ref\(\)\.ok_or_else\(non_derivable_tenant_err\)",
            r"Uuid::try_parse\(tenant\)",
            r"uid\.to_string\(\)\s*!=\s*tenant",
            r"derive_prefix\(tdk,\s*uid\)",
            r"format!\(",
        ):
            if re.search(clause, kv_view) is None:
                raise OpenStateError(f"{identifier}: object-key semantic clause missing: {clause}")
        if re.search(r"\bpad16\s*\(", kv_view) is not None:
            raise OpenStateError(f"{identifier}: legacy pad16 fallback is executable")
    elif identifier == "B-243":
        path = _safe_file(root, "apps/signup-worker/src/lib/d1.ts", identifier)
        raw, _literal, code = _source_views(path)
        body = _function_body(raw, r"\bexport\s+async\s+function\s+insertTenantOrgMap\s*\(", code)
        if not body:
            raise OpenStateError(f"{identifier}: tenant_org_map writer scope missing")
        _require_patterns(identifier, body, raw_literals=True)


def _safe_file(root: Path, relative: str, proposal_id: str) -> Path:
    path = root / relative
    try:
        resolved_root = root.resolve(strict=True)
        resolved = path.resolve(strict=True)
        resolved.relative_to(resolved_root)
    except (OSError, RuntimeError, ValueError) as exc:
        raise OpenStateError(f"{proposal_id}: artifact escapes or is missing: {relative}") from exc
    if not path.is_file() or path.is_symlink():
        raise OpenStateError(f"{proposal_id}: artifact is not a regular non-symlink file: {relative}")
    return path


def _section(text: str, anchor: str) -> str:
    """Return the TOML section containing an exact section heading."""
    match = re.search(anchor, text, flags=re.MULTILINE)
    if not match:
        return ""
    start = match.end()
    next_section = re.search(r"(?m)^\s*\[[^\n]+\]\s*$", text[start:])
    return text[start : start + next_section.start()] if next_section else text[start:]


def verify(root: Path = ROOT, proposal_id: str | None = None) -> dict[str, int]:
    if set(RULES) != set(PROPOSAL_IDS):
        raise OpenStateError("open-state rule set must be exactly B-171..B-243")
    registry_path = root / REGISTRY
    try:
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise OpenStateError(f"cannot load proposal registry: {exc}") from exc
    records = registry.get("proposals") if isinstance(registry, dict) else None
    if not isinstance(records, list) or len(records) != len(PROPOSAL_IDS):
        raise OpenStateError("proposal registry must contain exactly 73 records")
    by_id = {record.get("id"): record for record in records if isinstance(record, dict)}
    # B-101's proposal registry remains the immutable census (records are
    # intentionally still `open`), while implementation backlog items may now
    # be closed by their own inverted verifier. On an all-items invocation,
    # inspect only implementation items that are still open; an explicit
    # `--id` remains strict and is useful for testing the negative open guard.
    selected = (
        tuple(identifier for identifier in PROPOSAL_IDS if _backlog_status(root, identifier) == "open")
        if proposal_id is None
        else (proposal_id,)
    )
    if any(identifier not in RULES for identifier in selected):
        raise OpenStateError(f"unknown proposal id: {proposal_id}")

    for identifier in selected:
        record = by_id.get(identifier)
        if not record or record.get("owner") != "tl" or record.get("status") != "open":
            raise OpenStateError(f"{identifier}: open verifier requires owner=tl and status=open")
        rule = RULES[identifier]
        witness = root / rule.closure_witness
        if witness.exists() or witness.is_symlink():
            raise OpenStateError(f"{identifier}: closure witness exists; update the item before claiming OPEN")
        for relative in rule.artifacts:
            path = _safe_file(root, relative, identifier)
            raw, literal_view, code_view = _source_views(path)
            checked = raw if path.suffix == ".md" else code_view
            if re.search(rule.anchor, checked, flags=re.MULTILINE) is None:
                raise OpenStateError(f"{identifier}: executable/config anchor missing in {relative}")
            if rule.absent:
                live = _section(literal_view, rule.anchor) if path.suffix == ".toml" else literal_view
                present = [token for token in rule.absent if token in live]
                if present:
                    raise OpenStateError(f"{identifier}: unresolved live config token(s) are now present: {present}")
            if identifier not in {"B-171", "B-179", "B-192", "B-243"}:
                # Markdown/config contracts need their literals; source code
                # clauses are checked in the masked view first and may fall
                # back to the comment-free view for string-valued facts.
                _require_patterns(
                    identifier,
                    raw,
                    raw_literals=path.suffix in {".md", ".toml"},
                    literal_view=literal_view,
                    checked_view=code_view,
                )
        _verify_special_semantics(root, identifier)
    return {"records": len(selected), "unfinished": len(selected)}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--id")
    args = parser.parse_args(argv)
    try:
        report = verify(Path(args.root).resolve(), args.id)
    except (OSError, UnicodeDecodeError, OpenStateError) as exc:
        print(f"B-101 open-state guard: FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"B-101 open-state guard: PASS: {report['records']} unfinished item(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
