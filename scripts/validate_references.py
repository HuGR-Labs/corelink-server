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
    "RB": ["05_quality/runbooks/", "_runbooks/", "05_runbooks/"],  # directory: any .md inside (DEBT-006: _runbooks/ + 05_runbooks/ added 2026-05-15)
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
    re.compile(r"^\*\*([A-Z]+(?:-[A-Z0-9_]+)+)\*\*\s*\("),  # **SLO-ADMIN-X** (description...) — DEBT-006: SLO catalog uses this pattern
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
    "INV-BACKUP",  # plural-form mention em registry §3.20 intro ("INV-BACKUP-*" pattern) — DEBT-004 promotion
    "INV-BILLING-PORTAL",  # plural-form mention em registry §3.21 intro ("INV-BILLING-PORTAL-*" pattern) — DEBT-004 promotion
    "INV-BODY",  # plural-form mention em registry §3.22 intro ("INV-BODY-*" pattern) — DEBT-004 promotion
    "INV-HANDLER-SLI",  # plural-form mention em registry §3.23 intro ("INV-HANDLER-SLI-*" pattern) — DEBT-004 promotion
    "INV-OBS-EXPORT",  # plural-form mention em registry §3.24 intro ("INV-OBS-EXPORT-*" pattern) — DEBT-004 promotion
    "INV-OFFBOARDING",  # plural-form mention em registry §3.25 intro ("INV-OFFBOARDING-*" pattern) — DEBT-004 promotion
    "INV-ROLLOUT",  # plural-form mention em registry §3.26 intro ("INV-ROLLOUT-*" pattern) — DEBT-004 promotion
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
    "RB-XX",     # NOISE: template placeholder in GA-GATE-GO-NOGO-TEMPLATE.md
    "RB-YYY",    # NOISE: template placeholder in IR-TABLETOP-EVIDENCE / RB-TABLETOP-TEMPLATE
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
    "RB-DRY-RUN-ASSERTION",          # wave-16 Lote 10.6 R5 review proposed PRINC-RB-DRY-RUN-ASSERTION citation; review-doc-only finding, not yet canonical
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
    "FM-249",  # auth replay storm (S-03 PRR-S03 forward-looking; promoted in S-03 impl)
    # === DEBT-006 (2026-05-15): Bulk PLANNED + NOISE allowlist additions ===
    # PLANNED CTRLs — Vendor due-diligence cross-walks reference controls owned by external
    # vendors (Clerk/Cloudflare/PagerDuty/Slack/HubSpot/Stripe). Documented in
    # _compliance/vendor-dd/*.md; will be promoted to security_model.md/privacy_model.md
    # as part of S-20 TPRM finalization. Justification: vendor controls are *their* CTRL
    # IDs traced into our compliance matrix; not native HuGR controls.
    "CTRL-ACCESS-001",        # DD-CLERK, DD-CLOUDFLARE
    "CTRL-AVAIL-001",         # DD-CLOUDFLARE
    "CTRL-BCP-DR-009",        # DD-PAGERDUTY
    "CTRL-COMM-001",          # DD-SLACK
    "CTRL-COMM-003",          # DD-HUBSPOT
    "CTRL-COMM-004",          # DD-SLACK
    "CTRL-COMM-005",          # DD-SLACK
    "CTRL-COMPL-002",         # DD-STRIPE
    "CTRL-COMPL-007",         # DD-SLACK
    "CTRL-COMPL-009",         # DD-HUBSPOT
    "CTRL-DATA-001",          # DD-CLOUDFLARE
    "CTRL-IDENT-001",         # DD-CLERK
    "CTRL-IR-002",            # DD-PAGERDUTY
    "CTRL-IR-003",            # DD-PAGERDUTY
    "CTRL-IR-004",            # DD-PAGERDUTY
    "CTRL-IR-005",            # DD-SLACK
    "CTRL-OBS-005",           # DD-PAGERDUTY
    "CTRL-OBS-006",           # DD-SLACK
    "CTRL-PRIV-018",          # DD-STRIPE
    # PLANNED CTRLs — forward-looking native controls (promoted during respective sprint impl):
    "CTRL-ADMIN-001",         # S-13 admin signup (RB-FM-SIGNUP-FAILED)
    "CTRL-ADMIN-002",         # S-13 dual-approval (slo_catalog.md SLO-ADMIN-DUAL-APPROVAL-LATENCY)
    "CTRL-ADMIN-006",         # S-13 config-propagation
    "CTRL-ADMIN-007",         # S-13 rollback recovery
    "CTRL-ANTI-FRAUD-001",    # S-19 insider threat (TT-04)
    "CTRL-AUTH-013",          # S-15 tenant-offboarding
    "CTRL-CHAOS-001",         # S-17 DR drill scheduler (WI-S17-002)
    "CTRL-CRYPT-001",         # S-19 PRR (typo of CTRL-CRYPTO-001? promoted in S-19 impl)
    "CTRL-DATA-RESIDENCY-001",# S-14 tenant region pinning (WI-S14-002)
    "CTRL-DEP-AUDIT-001",     # S-12 supply chain (TT-05)
    "CTRL-KEY-013",           # S-14 BYOK key wrap (PRR-S14)
    "CTRL-KEY-014",           # S-14 BYOK key unwrap (PRR-S14)
    "CTRL-MULTIPART-002",     # S-05 multipart (asvs checklist)
    "CTRL-ONBOARD-001",       # S-19 onboarding
    "CTRL-ONBOARD-002",       # S-19 onboarding
    "CTRL-ONBOARD-005",       # S-19 onboarding
    "CTRL-ONBOARD-006",       # S-19 onboarding
    "CTRL-PRIV-RESIDENCY-001",# S-14 residency (GDPR audit)
    "CTRL-SECRETS-DRIFT-001", # S-13 secret rotation (SOC2 rollup)
    "CTRL-SUPPLY-COSIGN-001", # S-12 Cosign verify (RB-SUPPLY-REKOR-OUTAGE)
    "CTRL-WEBHOOK-001",       # S-10 webhook signing (TT-03)
    "CTRL-WEBHOOK-002",       # S-10 webhook DLQ (TT-03)
    "CTRL-WEBHOOK-003",       # S-10 webhook idempotency (TT-03)
    # PLANNED PATs:
    "PAT-DNS-001",            # S-09 DNS resilience (pentest evidence)
    "PAT-PRIV-001",           # S-17 privacy pattern (sprint.md)
    "PAT-SAGA-001",           # S-19 saga pattern (sprint.md)
    "PAT-SAGA-ATOMIC-001",    # S-20 saga atomic (PRR-S20-GA)
    # PLANNED INVs — promoted to invariant_registry.md during respective sprint impl:
    "INV-ADMIN-CONFIG-CAS",                  # S-13 admin config (slo_catalog)
    "INV-AUDIT-HASH-CHAIN-CONTINUOUS",       # S-09 audit chain (GDPR audit)
    "INV-AUDIT-MINIMIZATION",                # S-09 audit minimization (GDPR audit)
    "INV-AUDIT-PSEUDONYM-DETERMINISTIC",     # S-09 audit pseudonym (GDPR audit)
    "INV-AVAIL-DOS",                         # S-09 DoS availability (PRR-S09)
    "INV-BACKUP-FRESH",                      # S-15 backup freshness (RB-CANONICAL-DRIFT)
    "INV-BACKUP-INTEGRITY-SAMPLE-CAP",       # S-15 backup integrity
    "INV-BACKUP-RESTORE-EPHEMERAL",          # S-15 backup restore
    "INV-BYOK-CMK-ERASURE-ATOMICITY",        # S-14 BYOK erasure
    "INV-BYOK-CMK-NEVER-LEAVES-CUSTOMER",    # S-14 BYOK customer key (DD-AWS-KMS)
    "INV-CACHE-001",                         # legacy short-form (DD-CLOUDFLARE)
    "INV-CAS-DIGEST-INTEGRITY",              # Lote 10.9bis wave 17 — renamed to canonical INV-CAS-INTEGRITY (registry §3.2); historical alias retained for review-doc references (R4/R5 closure footnotes)
    "INV-CONSENT-NO-FAIL-OPEN",              # S-11 consent (GDPR audit)
    "INV-DATA-CRYPTO-001",                   # legacy short-form (DD-CLOUDFLARE)
    "INV-DSR-AUDIT-FAIL-CLOSED",             # S-11/S-15 DSR
    "INV-DSR-ERASURE-12-BACKEND",            # S-15 DSR erasure
    "INV-DSR-MFA-DESTRUCTIVE",               # S-15 DSR MFA
    "INV-DSR-RECEIPT-90D",                   # S-15 DSR receipt
    "INV-DSR-TENANT-ISOLATION",              # S-15 DSR isolation
    "INV-DSR-VERIFIED-CLOCK",                # S-15 DSR clock
    # INV-EXEC-IDEMPOTENT: removed Lote 10.9bis wave 17 — promoted to registry §3.12 (S-09 row, HIGH)
    "INV-ISO-CONSTANT-TIME-404",             # S-09 isolation
    "INV-ISO-NO-CROSS-LEAK",                 # S-09 isolation
    # INV-LGPD-AUTO-SUSPEND-FORBIDDEN: removed Lote 10.9bis wave 17 — promoted to registry §3.12 (S-09 row, HIGH; LGPD Art. 20 + GDPR Art. 22)
    "INV-OFFBOARDING-AUDIT-COMPLETE",        # S-15 tenant offboarding
    "INV-OFFBOARDING-GRACE-RESPECTED",       # S-15 tenant offboarding
    "INV-PRIVACY-PSEUDONYMIZE-ON-ERASURE",   # S-15 privacy
    "INV-RESIDENCY-FAIL-CLOSED",             # S-14 residency
    "INV-ROLLOUT-AUTO-ROLLBACK",             # S-13 rollout
    "INV-S17-CHAOS-STAGING-ONLY",            # S-17 sprint contract
    "INV-S17-ONCALL-FATIGUE-AUTOROTATE",     # S-17 sprint contract
    "INV-S17-OPS-EXCLUSIVITY",               # S-17 sprint contract
    "INV-S17-SEV1-DRILL-PAUSE",              # S-17 sprint contract
    "INV-SUB-PROCESSOR",                     # S-11 sprint contract (plural-form)
    "INV-SUB-PROCESSOR-BROADCAST",           # S-11 sub-processor change broadcast
    "INV-WEBHOOK-DLQ-IDEMPOTENT-001",        # S-10 webhook DLQ (DD-STRIPE)
    # NOISE INVs — false positives from validator regex (English words / line-wrap artifacts):
    "INV-ID",                                # NOISE: regex catches "INV-ID" English phrase in RB-CANONICAL-DRIFT
    "INV-IDs",                               # NOISE: plural-form English mention in RB-CANONICAL-DRIFT
    "INV-CRITICAL",                          # NOISE: "INV-CRITICAL" qualifier in GA-GATE-CRITERIA prose
    "INV-level",                             # NOISE: "INV-level specs" English in S20 spec contract
    "INV-AUTH-WEBAUTHN-ORIGIN-EXACT-style",  # NOISE: pentest narrative "ORIGIN-EXACT-style", not a real INV
    "INV-BLAKE3-256-LOWER-HEX-64",           # wave-16 Lote 10.6 R4 review P3-002-1 proposed canonical citation (cross-ref recommendation; not yet promoted to registry)
    "INV-GC-001-violation",                  # NOISE: wave-16 Lote 10.6 R5 review narrative "INV-GC-001 violation" parsed as compound; real ref is INV-GC-001
    # PLANNED SLOs — forward-looking from sprint catalogs (promoted in slo_catalog.md during impl):
    "SLO-ADMIN",                             # S-13 plural-form/PRR-S13
    "SLO-AVAIL",                             # S-20 plural-form (sprint.md "SLO-AVAIL-*")
    "SLO-AVAIL-CAS-GET-FAST-BURN",           # S-09 multi-burn-rate alerts
    "SLO-AVAIL-CAS-GET-SLOW-BURN",           # S-09 multi-burn-rate alerts
    "SLO-AVAIL-FAST-BURN",                   # S-09 DASH-SLO-BURNDOWN
    "SLO-BACKUP-VERIFICATION",               # S-15 backup verification SLO (referenced in slo_catalog but not anchor)
    "SLO-BURNDOWN",                          # S-09 burndown dashboard
    "SLO-BYOK-CMK-DETECT",                   # S-14 BYOK detection
    "SLO-BYOK-DEK-CACHE-TTL",                # S-14 BYOK DEK cache
    "SLO-BYOK-DEK-EVICT",                    # S-14 BYOK DEK evict
    "SLO-BYOK-DETECTION",                    # S-14 BYOK detection (PRR-S14)
    "SLO-BYOK-KILL-SWITCH",                  # S-14 kill switch
    "SLO-BYOK-KILL-SWITCH-TOTAL",            # S-14 kill switch total
    "SLO-BYOK-MATRIX-AVAILABILITY",          # S-14 BYOK matrix
    "SLO-BYOK-MATRIX-WEEKLY",                # S-14 BYOK matrix weekly
    "SLO-BYOK-UNWRAP-LATENCY",               # S-14 BYOK unwrap
    "SLO-BYOK-WRAP-LATENCY",                 # S-14 BYOK wrap
    "SLO-FRESH",                             # S-20 plural-form "SLO-FRESH-*"
    "SLO-INCIDENT-RESPONSE",                 # S-20 incident response (plural)
    "SLO-INCIDENT-RESPONSE-SYNTHETIC-PAGE",  # S-20 synthetic page response
    "SLO-LAT",                               # S-20 plural-form "SLO-LAT-*"
    "SLO-LAT-DSR-RECEIPT",                   # S-15 DSR receipt latency
    "SLO-LAT-SIGNUP",                        # S-19 signup latency (GA-GATE-CRITERIA)
    "SLO-LATENCY-BREACH",                    # S-09 latency breach RB
    "SLO-LATENCY-P99-CAS-PUT",               # S-09 latency P99
    "SLO-ONBOARD",                           # S-19 onboarding plural
    "SLO-ONBOARD-ATOMICITY",                 # S-19 onboarding atomicity
    "SLO-ONBOARD-DPA-RECEIPT-VERIFIABILITY", # S-19 DPA receipt
    "SLO-ONBOARD-ENTERPRISE-AUTO-REPLY",     # S-19 enterprise auto-reply
    "SLO-ONBOARD-SIGNUP-DURATION",           # S-19 signup duration
    "SLO-REGION-FAILOVER-LATENCY",           # S-14 region failover
    "SLO-REGION-REPLICATION-LAG",            # S-14 region replication lag
    "SLO-REPLICATION-LAG",                   # S-14 replication lag (DR dashboard)
    "SLO-REPLICATION-LAG-P99",               # S-14 active failover spec
    "SLO-SUPPLY-LICENSE-REVIEW",             # S-12 license review
    "SLO-SUPPLY-RUSTSEC-TRIAGE",             # S-12 rustsec triage
    # PLANNED RBs — forward-looking runbook stubs (referenced from dashboards / vendor-dd /
    # sprint WIs; created during respective sprint impl OR are stubs created at sprint-end).
    # Justification: dashboards typically link to RBs that will exist when the sprint that
    # owns the alert is implemented. Pre-spec for these RBs.
    "RB-ABUSE-001", "RB-ABUSE-002", "RB-ABUSE-003", "RB-ABUSE-004", "RB-ABUSE-LGPD-001",  # S-08 abuse
    "RB-AUDIT-CHAIN-001", "RB-AUDIT-CHAIN-INTEGRITY-VIOLATION", "RB-AUDIT-CHAIN-STALL",   # S-09 audit chain
    "RB-AUDIT-CHAIN-TAMPER-RESPONSE", "RB-AUDIT-LGPD-001", "RB-AUDIT-LOCK-VIOLATION",     # S-09 audit chain
    "RB-AUTH-EMERGENCY",                                   # S-15 auth emergency (DD-CLERK)
    "RB-AUTH-REPLAY-INVESTIGATE",                          # S-03 auth replay (DASH-RATELIMIT-ABUSE)
    "RB-BILLING-DLQ-DRAIN",                                # S-10 billing DLQ (DASH-BILLING)
    "RB-BREACH-NOTIFICATION",                              # S-15 alias of RB-BREACH-NOTIF (GA-GATE-CRITERIA)
    "RB-BYOK-KEK-REVOKED-INCIDENT", "RB-BYOK-KILL-SWITCH", # S-14 BYOK
    "RB-BYOK-KILL-SWITCH-DRILL", "RB-BYOK-PROVIDER-OUTAGE",# S-14 BYOK
    "RB-BYOK-ROTATION-OVERDUE", "RB-BYOK-VAULT-CERT-RENEWAL",# S-14 BYOK
    "RB-CAPACITY-EXPAND-D1", "RB-CAPACITY-EXPAND-R2",      # S-09 capacity
    "RB-CAS-DIGEST-001", "RB-CAS-INTEGRITY-VIOLATION",     # S-09 CAS integrity
    "RB-CHANGE-WINDOW",                                    # S-13 change window
    "RB-CIRCUIT-BREAKER-OPEN",                             # S-09 circuit breaker
    "RB-CONSENT-PROPAGATION-FAILURE",                      # S-11 consent
    "RB-COST-REGRESSION-INVESTIGATE",                      # S-09 cost
    "RB-CVE-TRIAGE",                                       # S-12 CVE triage (PCI-DSS)
    "RB-DDOS-MITIGATION",                                  # S-08 DDoS (DASH-RATELIMIT)
    "RB-DPA-VERSION-BUMP",                                 # S-20 DPA version bump
    "RB-DR-DRILL-FAILURE",                                 # S-17 DR drill failure
    "RB-DRILL-OVERDUE-RECOVERY",                           # S-17 drill overdue
    "RB-DSR",                                              # S-15 plural-form "RB-DSR-*"
    "RB-DSR-FAILURE-RECOVERY", "RB-DSR-FULFILLMENT",       # S-15 DSR
    "RB-DSR-RECEIPT-FAILURE",                              # S-15 DSR
    "RB-EDGE-BLOCKLIST-001", "RB-EDGE-BLOCKLIST-002",      # S-08 edge blocklist
    "RB-ENTERPRISE-INCIDENT-COMMS",                        # S-19 enterprise comms
    "RB-ERASURE-VERIFY",                                   # S-14 erasure attestation
    "RB-ERROR-BUDGET-EXHAUSTED",                           # S-09 error budget
    "RB-EVIDENCE-FRESHNESS-RECOVERY",                      # S-16 evidence freshness
    "RB-FM-305-2026-04", "RB-FM-305-2026-05",              # PM postmortem date-stamped variants
    "RB-FM-401",                                           # S-08 retry storm (referenced)
    "RB-FM-AUDIT-BREAK",                                   # S-09 audit break (pentest)
    "RB-FM-DPA-LEGAL-CHALLENGE",                           # S-19 DPA legal
    "RB-FM-ENTERPRISE-HANDOFF-PARTIAL",                    # S-19 enterprise handoff
    "RB-GAP-REGISTER-REVIEW",                              # S-16 gap register
    "RB-GLOBAL-CIRCUIT-001", "RB-GLOBAL-CIRCUIT-002", "RB-GLOBAL-CIRCUIT-003",  # S-08 global circuit
    "RB-INCIDENT-COMMS",                                   # S-09 incident comms
    "RB-INCIDENT-ESCALATION-MATRIX",                       # S-20 escalation matrix
    "RB-INCIDENT-RESPONSE",                                # S-09 IR (PCI-DSS)
    "RB-INVOICE-GENERATION-FAILURE",                       # S-10 invoice gen (DASH-BILLING)
    "RB-ISOLATION-001",                                    # S-08 isolation
    "RB-MULTI-REGION-OUTAGE",                              # S-14 multi-region outage
    "RB-OFFBOARDING",                                      # S-15 alias of RB-TENANT-OFFBOARDING (ISO27001-GAP)
    "RB-PERSONNEL-OFFBOARDING",                            # S-13 personnel offboarding (PCI-DSS)
    "RB-QUOTA-001", "RB-QUOTA-002", "RB-QUOTA-003",        # S-08 quota
    "RB-REGION-OUTAGE",                                    # S-14 region outage
    "RB-REGULATOR-INQUIRY",                                # S-11 regulator inquiry
    "RB-RELIABILITY-REVIEW-PREP",                          # S-09 reliability review
    "RB-REPLICATION-LAG-INVESTIGATE",                      # S-14 replication lag
    "RB-RESIDENCY-VIOLATION-RESPONSE",                     # S-14 residency violation
    "RB-REVENUE-RECONCILIATION",                           # S-10 revenue reconciliation
    "RB-ROTATION-EMERGENCY",                               # S-13 emergency rotation
    "RB-SEV1-IC-CHAIR",                                    # S-20 IC chair
    "RB-SLI-DISTINCTION-001",                              # S-08 SLI distinction
    "RB-SLO",                                              # S-09 plural-form "RB-SLO-*"
    "RB-SLO-AVAIL-CAS-GET-FAST-BURN", "RB-SLO-AVAIL-CAS-GET-SLOW-BURN",  # S-09 burn rate RBs
    "RB-SLO-AVAIL-FAST-BURN", "RB-SLO-LATENCY-BREACH",     # S-09 burn rate RBs
    "RB-STRIPE-CREDENTIAL-ROTATION",                       # S-10 Stripe credential rotation
    "RB-STRIPE-WEBHOOK-FAILURE",                           # S-10 Stripe webhook (DASH-BILLING)
    "RB-SYNTHETIC-CANARY-FAILURE",                         # S-09 synthetic canary
    "RB-TENANT",                                           # S-15 plural-form "RB-TENANT-*"
    "RB-TENANT-ABUSE-RESPONSE",                            # S-08 tenant abuse
    "RB-TENANT-ISOLATION-BREACH",                          # S-15 tenant isolation breach
    "RB-UPSTREAM-DEGRADATION",                             # S-09 upstream degradation
    "RB-WAIVER-EXPIRY-RENEWAL",                            # S-16 waiver renewal
    # NOISE RBs — template placeholders:
    "RB-FM-XX",                              # NOISE: template placeholder in WI-S14-009 example
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
