#!/usr/bin/env python3
"""
Validate cross-references nos specs.

Para cada tipo de ID (EVT, CTRL, PAT, FM, INV, FF-HR, SLO, RB, ADR), extrai:

- **Definitions**: onde o ID é canonicamente DEFINIDO (cabeçalho ou tabela canônica do canonical source).
- **Uses**: onde o ID é referenciado em qualquer doc.

Reporta:

- **Dangling**: USE sem DEFINITION correspondente (referencia ID que não existe).
- **Orphan**: DEFINITION sem USE (catalogado mas nunca referenciado — pode ser dead code).

Uso:
    python3 scripts/validate_references.py                    # report normal
    python3 scripts/validate_references.py --warn-orphans     # também emite warnings de orphans
    python3 scripts/validate_references.py --json             # output em JSON

Exit code:
    0 se nenhum dangling.
    1 se há dangling references.

Endereça findings S-05/F-04 (PAT dangling), S-10 (CTRL dangling), S-16 (PAT-AUTHZ-002),
S-15 (FF-HR-011), F-04 (CTRL/PAT) do audit Lote 3+4.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SPECS_DIR = REPO_ROOT / "specs"

# ID patterns: (pattern, is_definition_anchor)
# Definitions são detectadas dentro de canonical sources nos paths abaixo.
ID_PATTERNS = {
    "EVT": re.compile(r"\bEVT-\d{3}\b"),
    "CTRL": re.compile(r"\bCTRL-[A-Z]+(?:-[A-Z]+)*-\d{3}\b"),  # v3 Lote 7.2: aceita multi-segmento (CTRL-PRIV-CONSENT-001, H-01)
    "PAT": re.compile(r"\bPAT-[A-Z][A-Z0-9-]+-\d{3}\b"),
    "FM": re.compile(r"\bFM-\d{3}\b"),
    "INV": re.compile(r"\bINV-[A-Za-z][A-Za-z0-9_-]+\b"),  # v2 Lote 6.3: aceita CamelCase legados (G-04)
    "FF-HR": re.compile(r"\bFF-HR-\d{3}\b"),
    "SLO": re.compile(r"\bSLO-[A-Z][A-Z0-9-]+\b"),
    "RB": re.compile(r"\bRB-[A-Z][A-Z0-9-]*[A-Z0-9]\b"),
    "ADR": re.compile(r"\bADR-\d{4}\b"),
    "WAIVER": re.compile(r"\bWAIVER-\d{8}-\d{3}\b"),
    "FF-LR": re.compile(r"\bFF-LR-\d{3}\b"),
}

# Canonical source files for each ID type (where definitions live).
DEFINITION_SOURCES = {
    "EVT": ["00_framework.md"],
    "CTRL": [
        "03_architecture/security_model.md",
        "03_architecture/privacy_model.md",
        "03_architecture/key_management.md",
    ],
    "PAT": ["03_architecture/resilience_patterns.md"],
    "FM": ["03_architecture/failure_modes.md"],
    "INV": ["03_architecture/invariant_registry.md"],
    "FF-HR": ["00_framework.md"],
    "SLO": ["03_architecture/slo_catalog.md"],
    "RB": ["05_quality/runbooks/"],  # directory: any .md inside
    "ADR": ["03_architecture/adrs/"],
    "WAIVER": ["_waivers/"],
    "FF-LR": ["00_framework.md"],
}

# Skip these dirs entirely (audits, archives, csv, etc).
# _templates also skipped because they contain example placeholders, not real references.
SKIP_DIRS = {"_audits", "_archive", "_schemas", "_templates"}

# Detect definitions via "anchored" patterns in canonical sources.
# A definition is recognized when an ID appears at start of:
# - markdown header (### / #### / ##### with the ID)
# - table cell that starts with `**ID**` or `| ID |`
# - bullet `- **ID**` or `- ID`
# - bold `**SLO-XXX**:` or `**FM-001**:` (often used for SLO/PRR/runbook headings)
DEFINITION_ANCHORS = [
    re.compile(r"^#{1,5}\s+(?:[A-Z]+-?\d*[:\s]+)?\*?\*?([A-Z]+(?:-[A-Z0-9_]+)+)\*?\*?"),  # ### EVT-001 or # RB-FM-051 — title
    re.compile(r"^\|\s*\*?\*?([A-Z]+(?:-[A-Z0-9_]+)+)\*?\*?\s*[\(\|]"),  # | EVT-001 | or | **EVT-001** ( ...
    re.compile(r"^-\s+\*?\*?([A-Z]+(?:-[A-Z0-9_]+)+)\*?\*?"),  # - **EVT-001** or - EVT-001
    re.compile(r"^\*?\*?([A-Z]+(?:-[A-Z0-9_]+)+)\*?\*?\s*:"),  # **SLO-AVAIL-CP**:
    re.compile(r'^id:\s*"([A-Z]+(?:-[A-Z0-9_]+)+)"'),  # YAML front matter id field
    re.compile(r"^\*\*([A-Z]+(?:-[A-Z0-9_]+)+)\*\*\s*$"),  # **SLO-AVAIL-CP** standalone (no colon)
]

# Alias expiry tracking (Lote 7.3 endereça M-01):
# Aliases legacy CamelCase expiram em 2026-10-24 conforme invariant_registry §5.
# Após essa data, validator deve FALHAR em vez de whitelistar silenciosamente.
import datetime as _dt
_TODAY = _dt.date.today()
_ALIAS_EXPIRY_DATE = _dt.date(2026, 10, 24)
_LEGACY_INV_ALIASES_EXPIRED = _TODAY > _ALIAS_EXPIRY_DATE

# IDs whitelisted (canonical IDs of canonical-source docs themselves; not "uses").
WHITELIST_IDS = {
    "FRAMEWORK-00",
    "REMOTE-CACHE-PRODUCT-PROFILE",
    "STORAGE-SEMANTICS-MATRIX",
    "AUTH-MODEL",
    "SECURITY-MODEL",
    "PRIVACY-MODEL",
    "OBSERVABILITY-MODEL",
    "FAILURE-MODES",
    "RESILIENCE-PATTERNS",
    "SLO-CATALOG",
    "COMPLIANCE-MATRIX",
    "DATA-MODEL",
    "INVARIANT-REGISTRY",
    "KEY-MANAGEMENT",
    "LIA-TEMPLATE",
    # INV-KEY-* canonicalized em invariant_registry.md §3.13 (Lote 9.4 / ADR-0018);
    # whitelist removida — agora resolve via registry.
    # Aliases históricos canonical em invariant_registry.md §5
    "INV-DATA-AC-REFS-EXIST",
    "INV-DATA-AUDIT-CHAIN",
    "INV-DATA-BLOB-HASH",
    "INV-DATA-BLOB-NO-ZOMBIE",
    "INV-DATA-REFCOUNT",
    "INV-DATA-TENANT-ISOLATION",
    "INV-DATA-MONOTONIC-TS",
    "INV-DATA-BILLING-RECONCILE",
    "INV-DATA-ERASURE-COMPLETE",
    # Sprint-level meta-invariants (não técnicas; não no invariant_registry):
    "INV-DATA-CLASSIFICATION",  # S-00 planning invariant
    "INV-SCOPE-DISCIPLINE",     # S-00 planning invariant
    # Legacy CamelCase aliases — EXPIRAM EM 2026-10-24 (invariant_registry §5).
    # Após essa data, _LEGACY_INV_ALIASES_EXPIRED = True e estes IDs NÃO
    # estarão mais na whitelist (validator falhará em uses).
    # Pre-expiry: todos na whitelist.
    # Post-expiry: só mantém os que são de dependências externas imóveis.
    *([] if _LEGACY_INV_ALIASES_EXPIRED else [
        "INV-TenantIsolation",
        "INV-AuditLogImmutability",
        "INV-CASIdempotency",
        "INV-QuotaEnforcement",
        "INV-DigestVerification",
        "INV-DataResidency",
    ]),
    # Template placeholders
    "INV-AAA",
    "INV-BBB",
    "INV-XXX-",
    "INV-YYY",
    "INV-ZZZ",
    "INV-XXX",
    "INV-XXX-name",   # template placeholder em framework examples
    "INV-LIFECYCLE-001",  # framework-internal example
    "INV-GC",  # plural-form mention
    "INV-SUPPLY",  # plural-form mention em §4.3 ("INV-SUPPLY-*")
    "INV-DEDUP",  # line-wrap artifact: "INV-DEDUP-\nCONSISTENCY" → captures short form (PRR-S07 §A)
    "INV-EVICT",  # line-wrap artifact: "INV-EVICT-\nSOFT-DELETE-FIRST" → captures short form (PRR-S07 §3)
    "INV-AUDIT-CHAIN",  # informal short-form reference to INV-AUDIT-APPEND-ONLY (PRR-S03; alias in §5)
    "INV-OBS",  # plural-form mention (S20 sprint.md §8 "INV-OBS-* invariants")
    "INV-RATE-LIMIT",  # plural-form mention; canonical IDs are INV-RATE-LIMIT-PROPORTIONALITY (PRR-S08)
    "INV-CAS-SIDE-CHANNEL",  # short-form mention em S-02 §6 (full ID INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE)
    "INV-AC",  # plural-form mention em S-04 §8 (registry §3.3 INV-AC-* pattern)
    "INV-AUTH",  # plural-form mention em registry §3.14 ("INV-AUTH-*" pattern intro)
    "INV-AUTH-PAT",  # plural-form mention em registry §3.14 + §3.13 (PAT-related INVs)
    "INV-MULTIPART",  # plural-form mention em registry §3.16 intro ("INV-MULTIPART-* pattern")
    "INV-LRU",  # plural-form mention em S-07 §3.18 + WI-S07-004 narrative (registry §3.18 INV-LRU-* pattern)
    # Forward-looking INVs introduced em sprint WIs; serão promovidas a invariant_registry.md em respective sprint implementation:
    "INV-AUTH-CLOCK-SKEW-BOUND",       # S-03 WI-S03-001
    "INV-AUTH-ISS-EXACT-MATCH",        # S-03 WI-S03-001
    "INV-AUTH-JWT-VALIDATE-RS256-ONLY",# S-03 WI-S03-001
    "INV-AUTH-KID-RESOLUTION",         # S-03 WI-S03-001
    "INV-AUTH-PAT-HASH-ARGON2ID-2024", # S-03 WI-S03-002
    "INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED",  # S-03 WI-S03-002
    "INV-AUTH-PAT-SALT-PER-TOKEN",     # S-03 WI-S03-002
    "INV-AUTH-PAT-SCOPE-DB-IS-SOT",    # S-03 WI-S03-002
    "INV-AUTH-PAT-VERIFY-CONSTANT-TIME",  # S-03 WI-S03-002
    "INV-NEG-CACHE-MONOTONIC",         # S-02 WI-S02-005 (P0 fix Lote 10.2bis)
    "INV-NO-BODY-IN-LOGS",             # S-01 WI-S01-005 (P0 fix Lote 10.2bis)
    "INV-NO-PII-IN-LOGS",              # S-03 WI-S03-001
    "INV-AUTH-TENANTCTX-IMMUTABLE",    # S-03 WI-S03-003
    "INV-AUTH-5-LAYER-ORDERING",       # S-03 WI-S03-003
    "INV-AUTH-SESSION-CACHE-KEY-CT",   # S-03 WI-S03-003
    "INV-AUTH-SCOPE-MIDDLEWARE-LEVEL", # S-03 WI-S03-003
    "INV-AUTH-AUDIT-PRE-POST-ORDERING",# S-03 WI-S03-003
    "INV-AUTH-REVOCATION-IDEMPOTENT",  # S-03 WI-S03-004
    "INV-AUTH-REVOCATION-SLO-60S",     # S-03 WI-S03-004
    "INV-AUTH-D1-IS-SOT",              # S-03 WI-S03-004
    "INV-AUTH-MASS-REVOKE-ATOMIC",     # S-03 WI-S03-004
    "INV-AUTH-PROPAGATION-AT-LEAST-ONCE",  # S-03 WI-S03-004
    "INV-AUTH-SCHEMA-RLS-DEFAULT-ON",      # S-03 WI-S03-005
    "INV-AUTH-PII-ENCRYPTED",              # S-03 WI-S03-005
    "INV-AUTH-MIGRATION-ADDITIVE",         # S-03 WI-S03-005
    "INV-AUTH-CASCADE-DSR-COMPLETE",       # S-03 WI-S03-005
    "INV-AUTH-AUDIT-PSEUDONYMIZATION",     # S-03 WI-S03-005
    "INV-AUTH-WEBAUTHN-UV-REQUIRED-ADMIN", # S-03 WI-S03-006
    "INV-AUTH-WEBAUTHN-ATTESTATION-VERIFIED",  # S-03 WI-S03-006
    "INV-AUTH-WEBAUTHN-SIGN-COUNT-MONOTONIC",  # S-03 WI-S03-006
    "INV-AUTH-WEBAUTHN-ORIGIN-EXACT",      # S-03 WI-S03-006
    "INV-AUTH-WEBAUTHN-RP-ID-CANONICAL",   # S-03 WI-S03-006
    "INV-AUDIT-NO-RAW-PII",                # S-03 WI-S03-007
    "INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER",  # S-03 WI-S03-007
    "INV-AUDIT-CHAIN-HASH-DETERMINISTIC",  # S-03 WI-S03-007
    "INV-AUDIT-EVENT-TYPE-EXHAUSTIVE",     # S-03 WI-S03-007
    "INV-AUDIT-RETENTION-HINT-ACCURATE",   # S-03 WI-S03-007
    # Forward-looking AC INVs (S-04 Lote 10.4 — promovidas em registry §3.15 em Lote 10.4bis P0 fix):
    "INV-AC-MERKLE-VALID",                 # S-04 WI-S04-003
    "INV-AC-MERKLE-DETERMINISTIC",         # S-04 WI-S04-003
    "INV-AC-BOUNDED-PARSER",               # S-04 WI-S04-003
    "INV-AC-CYCLE-FREE",                   # S-04 WI-S04-003
    "INV-AC-DUAL-SIDE-VERIFY",             # S-04 WI-S04-003
    "INV-AC-DIGEST-SIGNED",                # S-04 WI-S04-004
    "INV-AC-SIG-CONSTANT-TIME",            # S-04 WI-S04-004
    "INV-AC-SIG-INFO-FIXED",               # S-04 WI-S04-004
    "INV-AC-KEY-ROTATION-GRACE",           # S-04 WI-S04-004
    "INV-AC-TDK-ZEROIZED",                 # S-04 WI-S04-004
    "INV-AC-CANONICAL-BYTES-STABLE",       # S-04 WI-S04-004
    "INV-AC-IDEMPOTENT",                   # S-04 WI-S04-001
    "INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE",  # S-04 WI-S04-001
    "INV-AC-RESULT-HASH-IMMUTABLE",        # S-04 WI-S04-001
    "INV-AC-EVICT-TENANT-SCOPED",          # S-04 WI-S04-005
    "INV-AC-EVICT-CONSISTENCY",            # S-04 WI-S04-005
    "INV-AC-TTL-MONOTONIC",                # S-04 WI-S04-005
    "INV-DATA-AC-REFS-EXIST",              # data_model.md §4.2 alias of INV-AC-OUTPUTS-VALID
    # Forward-looking Multipart INVs (S-05 Lote 10.5 — promovidas em registry §3.16 em Lote 10.5bis P0 fix):
    "INV-MULTIPART-IDEMPOTENT",                  # S-05 WI-S05-001 + WI-S05-003 + WI-S05-004
    "INV-MULTIPART-MANIFEST-SIGNED",             # S-05 WI-S05-001 + WI-S05-005
    "INV-MULTIPART-CONCURRENCY-BOUNDED",         # S-05 WI-S05-001 + WI-S05-003
    "INV-MULTIPART-CHUNK-DETERMINISTIC",         # S-05 WI-S05-002
    "INV-MULTIPART-BOUNDED-PARSER",              # S-05 WI-S05-002 + WI-S05-005
    "INV-MULTIPART-STREAMING-MEMORY",            # S-05 WI-S05-002
    "INV-MULTIPART-ORPHAN-DETECTABLE",           # S-05 WI-S05-003
    "INV-MULTIPART-PATH-TENANT-SCOPED",          # S-05 WI-S05-003
    "INV-MULTIPART-STATE-MONOTONIC",             # S-05 WI-S05-004
    "INV-MULTIPART-PATH-KEY-MATERIALIZED",       # S-05 WI-S05-004
    "INV-MULTIPART-MANIFEST-VALID",              # S-05 WI-S05-005
    "INV-MULTIPART-DUAL-SIDE-VERIFY",            # S-05 WI-S05-005
    "INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST",  # S-05 WI-S05-005
    # Forward-looking GC INVs (S-06 Lote 10.6 — promovidas em registry §3.17 preemptivamente):
    "INV-GC-IDEMPOTENT-RERUN",                   # S-06 WI-S06-001
    "INV-GC-SINGLE-RUNNING-PER-TENANT-REGION",   # S-06 WI-S06-001
    "INV-GC-PHASE-MONOTONIC",                    # S-06 WI-S06-001
    "INV-GC-MARK-STARTED-AT-IMMUTABLE",          # S-06 WI-S06-001
    "INV-GC-DEGRADE-MODE-PROBE-PER-BATCH",       # S-06 WI-S06-001
    "INV-GC-MARK-STARTED-AT-ATOMIC",             # S-06 WI-S06-002
    "INV-GC-REACHABLE-SET-COMPLETE",             # S-06 WI-S06-002
    "INV-GC-MARK-TENANT-SCOPED",                 # S-06 WI-S06-002
    "INV-GC-MARK-PHASE-BUDGETED",                # S-06 WI-S06-002
    "INV-GC-MARK-D1-BOUNDED-BATCH",              # S-06 WI-S06-002
    "INV-GC-SWEEP-AUDIT-FAIL-CLOSED",            # S-06 WI-S06-003
    "INV-GC-SWEEP-IDEMPOTENT",                   # S-06 WI-S06-003
    "INV-GC-SWEEP-TENANT-SCOPED",                # S-06 WI-S06-003
    "INV-GC-GRACE-RESPECTED",                    # S-06 WI-S06-003
    "INV-GC-PHYSICAL-DELETE-IDEMPOTENT",         # S-06 WI-S06-004
    "INV-GC-GRACE-BOUNDARY-STRICT",              # S-06 WI-S06-004
    "INV-GC-R2-D1-ORDERING",                     # S-06 WI-S06-004
    "INV-GC-DSR-BYPASS-AUTHORIZED",              # S-06 WI-S06-004
    "INV-GC-RECONCILE-AUTO-FIX-BOUNDED",         # S-06 WI-S06-005
    "INV-GC-RECONCILE-AUDIT-FAIL-CLOSED",        # S-06 WI-S06-005
    "INV-GC-CI-GATE-ENFORCED",                   # S-06 WI-S06-006
    "INV-GC-PROPERTY-TEST-CROSS-VALIDATED",      # S-06 WI-S06-006
    "INV-GC-30D-SUSTAINED-VERIFICATION",         # S-06 WI-S06-006
    # Patterns referenciados forward-looking em S-02 (definidos em resilience_patterns.md mas missing entry-anchor)
    "PAT-CIRCUIT-BREAKER-001",
    "PAT-INVALIDATE-001",  # S-03 v1.1 forward-looking pattern (revocation propagation)
    "PAT-MIGRATION-IDEM-001",  # S-01 v1.1 (D1 idempotent migration; resilience_patterns.md anchor pendente)
    # ADRs forward-looking (a serem criadas durante respectivos sprints implementation)
    "ADR-0021",  # S-04: AC digest signing HKDF vs Ed25519
    "ADR-0022",  # S-05: chunk size vs multipart part size decoupling
    "ADR-0023",  # S-02 WI-S02-004: constant-time defense via timing padding middleware
    "ADR-0024",  # S-03 WI-S03-001: Clerk JWKS cache strategy (lazy refresh em KID miss)
    "ADR-0025",  # S-03 WI-S03-002: Argon2id calibration target + deploy validation gate
    "ADR-0026",  # S-03 WI-S03-002: PatScopes bitset u64 layout + future migration u128
    "ADR-0027",  # S-01 WI-S01-005: Dual-write reconciliation contract (R2-first + audit outbox + GC sweep)
    "ADR-0028",  # S-02 WI-S02-005: MissReason → HTTP 404 uniform freeze (S-02 GA; 410 Gone deferido S-06)
    "ADR-0029",  # S-03 WI-S03-003: TenantCtx immutability + session cache strategy + audit pre/post-emit ordering
    "ADR-0030",  # S-03 WI-S03-004: Revocation propagation (DO + Queue + ≤ 60s SLO; combined 120s stale window)
    "ADR-0031",  # S-03 WI-S03-005: Auth domain Neon Postgres schema + pgcrypto + RLS + DSR cascade
    "ADR-0032",  # S-03 WI-S03-006: WebAuthn Level 3 + AAGUID allowlist + step-up flow design
    "ADR-0033",  # S-03 WI-S03-007: Audit event taxonomy EVT-047 + CloudEvents 1.0 + chain hash alignment
    "ADR-0034",  # S-03 WI-S03-008 (Lote 10.3bis): Solo-tier PRR waiver (staffing reality dual-hat com expiry)
    "ADR-0035",  # S-04 WI-S04-001: AC handler invariants (TenantCtx-only; warn-only outputs check em GET; R2-first then D1; 100 batch cap)
    "ADR-0036",  # S-04 WI-S04-002: AC schema design + migration governance + R2 bucket provisioning policy + region addition workflow
    "ADR-0037",  # S-04 WI-S04-003: AC Merkle protocol (BLAKE3 + RFC 6962 domain sep + bounded parser + deterministic build + dual-side verify)
    "ADR-0038",  # S-05 WI-S05-001: SplitBlob/SpliceBlob handler invariants (TenantCtx-only; bounded concurrency 4/tenant; streaming pipeline; manifest sig domain separation)
    "ADR-0039",  # S-05 WI-S05-002: Chunker public API stability + semver + FastCDC mask seeds versioning policy
    "ADR-0040",  # S-05 WI-S05-004: Multipart D1 sharding strategy (per-tenant_tier OR per-region; trigger 80% of D1 10 GB hard limit)
    "ADR-0041",  # S-05 WI-S05-005: Manifest public API stability + sig domain separation policy
    "ADR-0042",  # S-06 WI-S06-001: GC worker scheduler design + degrade-mode contract
    "ADR-0043",  # S-01 WI-S01-001: HMAC tenant prefix algorithm choice (HMAC-SHA256 vs HMAC-BLAKE3; FIPS compliance)
    "ADR-0044",  # S-12 WI-S12-003: Deploy gate hard non-bypassable + Cosign keyless OIDC (Fulcio chain + Rekor inclusion + fail-CLOSED audit)
    "FM-XXX",
    "ADR-XXXX",
    "ADR-YYYY",
    "ADR-ZZZZ",
    "ADR-YYYY-REPLACE",
    "ADR-XXXX-REPLACE",
    "ADR-XXXX-RELATED",
    # SLO usage context names (derived from SLO names)
    "SLO-CAS-GET",  # short form of SLO-AVAIL-CAS-GET
    "SLO-CORRECT-ISO",
    # CTRL placeholder em template
    "CTRL-XXX",
    # CTRL-KEY-030..032: placeholders Fase 2 BYOE (key_management §5.4)
    "CTRL-KEY-030",
    "CTRL-KEY-031",
    "CTRL-KEY-032",
    # Runbook placeholder (template exemplo)
    "RB-XXX",
    # Forward-looking runbooks introduced em sprint WIs (criados durante respective sprint implementation):
    "RB-FM-OUTBOX-DRAIN",     # S-01 WI-S01-005 (P0 fix Lote 10.2bis)
    "RB-FM-SIGSTORE-OUTAGE",  # S-01 WI-S01-007 (P1 fix Lote 10.2bis)
    "RB-FM-AUTH-CACHE-MISS-STORM",  # S-03 WI-S03-003
    "RB-FM-REVOKE-LAG",              # S-03 WI-S03-004
    "RB-FM-REVOKE-DRIFT",            # S-03 WI-S03-004
    "RB-FM-NEON-OUTAGE",             # S-03 WI-S03-005
    "RB-FM-KEY-ROTATION-DRIFT",      # S-03 WI-S03-005
    "RB-FM-WEBAUTHN-MDS-OUTAGE",     # S-03 WI-S03-006
    "RB-FM-AC-TTL-DRIFT",            # S-04 WI-S04-005
    "RB-FM-AC-TTL-STORM",            # S-04 WI-S04-005
    "RB-FM-AC-MIGRATION-BUG",        # S-04 WI-S04-002
    "RB-FM-AC-BUCKET-LEAK",          # S-04 WI-S04-002
    "RB-FM-AC-CACHE-MISS-STORM",     # S-04 WI-S04-001 forward-looking
    "RB-FM-MULTIPART-MIGRATION-BUG", # S-05 WI-S05-004 forward-looking
    "RB-FM-302",                     # S-06 WI-S06-005 refcount drift forward
    "RB-FM-GC-WORKER-STALL",         # S-06 WI-S06-001 forward
    "RB-EMAIL-HASH-KEY-ROTATION",    # S-03 WI-S03-005 Lote 10.3-tris P0-R5-003 forward stub
    "RB-PATH-TDK-RETENTION",         # S-04 WI-S04-004 Lote 10.4-tris P0-R5-006 forward stub
    # SLO forward-stubs (defined em sprint implementation; whitelisted at spec time):
    # SLO-DEDUP-RATIO promoted from forward-stub → canonical em slo_catalog.md §4.8.1 (Lote 10.7bis P1-1 fix)
    "RB-FM",                         # plural-form mention "RB-FM-*" em ADR-0042 + WI-S06-007
    # SLOs em formato sem header standalone (definidos em corpo do §4.X mas não como anchor)
    "SLO-DEPLOY-SAFE",
    "SLO-INCIDENTS",
    "SLO-LAT-CAS-PUT-MULTIPART",
    "SLO-TENANT",
    "SLO-XXX",
    "SLO-FRESH-PAT-REVOKE",  # S-03 v1.1 forward-looking SLO (a ser definido em slo_catalog.md durante S-03 implementation)
    "SLO-FRESH-GC",  # S-06 v1.1 forward-looking SLO (mark phase freshness)
    "SLO-CORRECT-GC",  # S-06 v1.1 forward-looking SLO (GC correctness gate)
    "SLO-AVAIL-AUTH",  # S-03 v1.1 forward-looking SLO (auth path availability)
    "SLO-SUPPLY-CVE-DETECTION",  # S-12 forward-looking SLO (CVE alert delivery ≤ 15 min p99)
    "SLO-SUPPLY-DEPLOY-VERIFY-LATENCY",  # S-12 forward-looking SLO (Cosign deploy verify ≤ 5s p99)
    # R6-2 runbook title fragments (not real SLO IDs; extracted from RB-SLO-* names via regex)
    "SLO-AVAIL-DATA-PLANE",   # fragment of RB-SLO-AVAIL-DATA-PLANE
    "SLO-CORRECT-VIOLATION",  # fragment of RB-SLO-CORRECT-VIOLATION
    "SLO-DEDUP-DEGRADATION",  # fragment of RB-SLO-DEDUP-DEGRADATION
    "SLO-LATENCY-INVESTIGATION",  # fragment of RB-SLO-LATENCY-INVESTIGATION
    "SLO-ID",                 # generic placeholder in escalation template
    "CTRL-AUTH-014",  # S-12 forward-looking CTRL (quarterly secret rotation; canonical in security_model.md)
    "RB-AUTH-014",  # S-12 forward-looking RB (CF API token emergency rotation)
    "RB-FM-156",  # S-12 dep maintainer malicious (criado durante WI-S12-007)
    "RB-FM-157",  # S-12 typosquatting (criado durante WI-S12-007)
    # ADRs exemplo no framework (não são deployments reais)
    "ADR-0002",
    "ADR-0007",
    "ADR-0015",
    # Lote 9.1+9.2 SOTA expansion (referências forward-looking; controles/PATs são canonical TBD)
    "CTRL-AUTHZ-005",     # S-10 billing role-protected replay
    "CTRL-CRYPTO-005",    # S-14 BYOK envelope encryption
    "CTRL-OBS-001",       # S-09 observability stack canonical CTRL
    "PAT-AUTO-ROLLBACK-001",  # S-13 progressive rollout
    "PAT-DEDUP-CHECK-001",    # S-07 dedup property test
    "PAT-FAILOVER-001",       # S-14 region failover
    "PAT-RETRY-IDEMPOTENT-001",  # S-11 erasure replay
    # FMs novos catalogados em sprint contracts (a serem promovidos a failure_modes.md)
    "FM-157",  # typosquat (S-12)
    "FM-160",  # auth invalid (S-15/S-16/S-19)
}

# IDs com prefixo wildcard (qualquer ID que comece com este prefixo é whitelist).
# Útil para padrões legados ou exemplos nos templates.
WHITELIST_PREFIXES = (
    "ADR-XXXX",
    "ADR-YYYY",
    "WI-S",  # Work Items são exemplos comuns
    "ST-",   # Sub-tasks
    "PRR-",  # PRRs são exemplos
)


def iter_spec_files() -> list[Path]:
    files = []
    for path in sorted(SPECS_DIR.rglob("*.md")):
        rel = path.relative_to(SPECS_DIR)
        if any(part in SKIP_DIRS for part in rel.parts):
            continue
        files.append(path)
    return files


def is_definition_source(rel_path: str, id_type: str) -> bool:
    sources = DEFINITION_SOURCES.get(id_type, [])
    for src in sources:
        if src.endswith("/"):
            if rel_path.startswith(src):
                return True
        elif rel_path == src or rel_path.endswith("/" + src):
            return True
    return False


def extract_definitions(text: str, id_type: str) -> set[str]:
    """Find IDs of given type that appear in 'definition position'."""
    pattern = ID_PATTERNS[id_type]
    defs = set()
    for line in text.splitlines():
        line_stripped = line.strip()
        for anchor in DEFINITION_ANCHORS:
            m = anchor.match(line_stripped)
            if m:
                candidate = m.group(1)
                if pattern.fullmatch(candidate):
                    defs.add(candidate)
        # Also: H1/H2 with ID anywhere (looser)
        if line_stripped.startswith("# "):
            for m in pattern.finditer(line_stripped):
                defs.add(m.group(0))
    return defs


def extract_uses(text: str, id_type: str) -> set[str]:
    pattern = ID_PATTERNS[id_type]
    return set(pattern.findall(text))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--warn-orphans", action="store_true",
                        help="Also report orphan IDs (defined but never used).")
    parser.add_argument("--json", action="store_true",
                        help="Output report as JSON.")
    parser.add_argument("--id-types", nargs="*",
                        choices=list(ID_PATTERNS.keys()),
                        help="Limit check to these ID types.")
    args = parser.parse_args()

    id_types = args.id_types or list(ID_PATTERNS.keys())

    # definitions[id_type] = {ID: [files where defined]}
    definitions: dict[str, dict[str, list[str]]] = defaultdict(lambda: defaultdict(list))
    # uses[id_type] = {ID: [files where used]}
    uses: dict[str, dict[str, list[str]]] = defaultdict(lambda: defaultdict(list))

    files = iter_spec_files()
    for path in files:
        rel = str(path.relative_to(SPECS_DIR))
        text = path.read_text(encoding="utf-8")
        for id_type in id_types:
            ids_in_file = extract_uses(text, id_type)
            for id_ in ids_in_file:
                uses[id_type][id_].append(rel)
            if is_definition_source(rel, id_type):
                defs = extract_definitions(text, id_type)
                for id_ in defs:
                    definitions[id_type][id_].append(rel)

    # Compute dangling and orphans
    dangling: dict[str, dict[str, list[str]]] = defaultdict(dict)
    orphans: dict[str, dict[str, list[str]]] = defaultdict(dict)

    for id_type in id_types:
        all_uses = set(uses[id_type].keys())
        all_defs = set(definitions[id_type].keys())

        # Dangling: used but not defined
        for id_ in all_uses - all_defs - WHITELIST_IDS:
            dangling[id_type][id_] = uses[id_type][id_]

        # Orphans: defined but not used outside the canonical source itself
        for id_ in all_defs - WHITELIST_IDS:
            non_def_uses = [
                f for f in uses[id_type][id_]
                if not is_definition_source(f, id_type)
            ]
            if not non_def_uses:
                orphans[id_type][id_] = definitions[id_type][id_]

    if args.json:
        report = {
            "dangling": {k: dict(v) for k, v in dangling.items() if v},
            "orphans": {k: dict(v) for k, v in orphans.items() if v},
            "definitions_count": {k: len(v) for k, v in definitions.items()},
            "uses_count": {k: len(v) for k, v in uses.items()},
        }
        print(json.dumps(report, indent=2, ensure_ascii=False))
    else:
        print(f"=== Cross-reference report ({len(files)} docs analisados) ===\n")

        for id_type in id_types:
            n_defs = len(definitions[id_type])
            n_uses = len(uses[id_type])
            print(f"[{id_type}] definitions={n_defs}  uses={n_uses}")

        print()
        any_dangling = False
        for id_type in id_types:
            if dangling[id_type]:
                any_dangling = True
                print(f"❌ DANGLING [{id_type}] — usado mas não definido:")
                for id_, files_ in sorted(dangling[id_type].items()):
                    print(f"  {id_}")
                    for f in files_[:3]:
                        print(f"    in {f}")
                    if len(files_) > 3:
                        print(f"    ... +{len(files_)-3} arquivos")
                print()

        if not any_dangling:
            print("✅ Nenhuma dangling reference detectada.\n")

        if args.warn_orphans:
            for id_type in id_types:
                if orphans[id_type]:
                    print(f"⚠️  ORPHANS [{id_type}] — definido mas nunca referenciado fora do canonical source:")
                    for id_, files_ in sorted(orphans[id_type].items()):
                        print(f"  {id_} (defined in {files_[0]})")
                    print()

    return 1 if any(dangling[k] for k in id_types) else 0


if __name__ == "__main__":
    sys.exit(main())
