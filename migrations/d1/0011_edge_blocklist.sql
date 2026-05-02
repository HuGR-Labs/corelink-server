-- CoreLink D1 (Cloudflare SQLite) — migration 0011 for `edge_blocklist`
-- (S-08 Rate Limiting multi-camada: per-IP edge CIDR blocklist + audit
-- trail; WI-S08-002 EdgePolicy + InMemoryEdgePolicy + CidrBlocklist).
--
-- Canonical sources:
--   - specs/04_sprints/S08/work_items/WI-S08-002-cf-edge-per-ip-rules-cidr-blocklist.md §6.1.3
--   - specs/04_sprints/S08/_spec_contract.md §4 CAP-RATE-002 + §8 INV-AVAIL-ISOLATION
--   - specs/03_architecture/invariant_registry.md INV-AVAIL-ISOLATION (§3.8) + INV-AUDIT-APPEND-ONLY
--   - specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md
--
-- Schema rationale:
--   - The CF Ruleset Engine + CF managed List `corelink_ip_blocklist`
--     are the production enforcement replicas (eventual consistency
--     ≤ 5 min via reconcile DO; deferred to WI-S08-006 PRR ship gate).
--     This SQL table is the durable source-of-truth: every admin
--     `add_block` / `remove_block` / `suggest_block` mutation lands here
--     with full audit metadata BEFORE the CF List push fires.
--   - cidr_text stored as canonical normalised form: IPv4 `<dotted>/<prefix>`
--     (e.g. `192.0.2.0/24`); IPv6 `<lowercased-zero-suppressed>/<prefix>`
--     (e.g. `2001:db8::/32`). The Rust domain layer canonicalises before
--     INSERT; the SQL layer treats the text as opaque (PK uniqueness is
--     a textual identity check, NOT a semantic CIDR equivalence check).
--   - cidr_family ∈ {4, 6} CHECK + CHECK envelope on prefix bounds
--     `[0, 32]` for IPv4 and `[0, 128]` for IPv6.
--   - reason CHECK lists the canonical 4-literal taxonomy
--     {Sustained4xx, DDoSPattern, ManualAdmin, Compliance} per WI §6.1
--     BlockReason enum.
--   - source CHECK lists the canonical 2-literal taxonomy
--     {Manual, AutomatedSuggestionApproved} per WI §6.1.
--   - expires_at_ms NULL for permanent admin blocks; explicit
--     `expires_at_ms > added_at_ms` envelope CHECK.
--
-- Invariants enforced at storage layer:
--   - INV-AVAIL-ISOLATION (HIGH): edge enforcement layer; per-IP
--     dimension; no tenant_id at the blocklist (pre-auth boundary).
--     Cross-tenant collateral architecturally impossible at the edge.
--   - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+): every admin mutation must
--     produce an audit_outbox row in the same D1 batch as the
--     edge_blocklist mutation. The fail-closed envelope is enforced at
--     the InMemoryEdgePolicy layer (audit emit BEFORE state mutation;
--     audit failure aborts the operation; production wiring rolls back
--     the D1 batch on emit failure).
--   - Humane automated response (LGPD Art. 20 / GDPR Art. 22; spec
--     contract §7.10.s08.3): `source = 'AutomatedSuggestionApproved'`
--     means the row was admin-approved AFTER the suggest_block
--     observation; edge_blocklist NEVER receives auto-applied blocks
--     at this WI's boundary (the suggestion stays in
--     ip_blocklist_suggestions until admin approval lands).
--
-- Conventions (mirror migrations/d1/0001..0010):
--   - All timestamps stored as INTEGER Unix epoch milliseconds.
--   - Migration is idempotent via `CREATE TABLE IF NOT EXISTS` /
--     `CREATE INDEX IF NOT EXISTS`.
--   - Migrations are additive-only per scripts/check_migrations_additive.py
--     CI gate.
--   - admin_id stored as canonical UUIDv7 TEXT form.
--
-- D1 SQL correctness gates (Lote 10.4bis P0 lessons; pre-deploy CI):
--   - CHECK constraints inlined in CREATE TABLE (SQLite/D1 does NOT support
--     `ALTER TABLE … ADD CONSTRAINT chk_*`; only inline at CREATE TABLE per
--     ADR-0036 Rule 1).
--   - BEGIN/COMMIT NOT included (`wrangler d1 migrations apply` uses an
--     implicit transaction).
--
-- Backfill plan: NONE. The table starts empty; rows are inserted by the
-- admin endpoint (S-13 dependency; staging stub OK at this WI per spec).
--
-- Migration runner: see scripts/migrate_d1.sh.

-- ---------------------------------------------------------------------------
-- edge_blocklist — durable source-of-truth for per-IP / CIDR blocklist.
--
-- ONE row per `cidr_text` (canonical normalised form). The Rust
-- InMemoryCidrBlocklist mirrors this table 1:1 in process memory for
-- O(N) longest-prefix-match scan; the production CF List replica is
-- pushed via reconcile DO (deferred to WI-S08-006 PRR ship gate per
-- `trait-abstraction-defer` charter pattern).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS edge_blocklist (
  -- Canonical normalised CIDR text (PK; e.g. "192.0.2.0/24" or
  -- "2001:db8::/32").
  cidr_text          TEXT     NOT NULL,

  -- Address family: 4 (IPv4) or 6 (IPv6).
  cidr_family        INTEGER  NOT NULL,

  -- Prefix length in bits; 0..=32 for IPv4, 0..=128 for IPv6.
  cidr_prefix_len    INTEGER  NOT NULL,

  -- Block reason (canonical 4-literal taxonomy):
  --   - Sustained4xx              — admin-approved suggestion from
  --                                 5min × > 10000 RPS × 100% 4xx pattern.
  --   - DDoSPattern               — admin-approved volumetric pattern.
  --   - ManualAdmin               — admin manual add (e.g. compliance,
  --                                 law-enforcement request, internal
  --                                 abuse triage).
  --   - Compliance                — admin manual add for compliance
  --                                 (sanctions, geo-fencing).
  reason             TEXT     NOT NULL,

  -- Wall-clock when the block was added (Unix ms; immutable).
  added_at_ms        INTEGER  NOT NULL,

  -- Optional expiry (Unix ms). NULL for permanent admin blocks; CHECK
  -- envelope `expires_at_ms > added_at_ms` when present.
  expires_at_ms      INTEGER,

  -- Admin who added the block (canonical UUIDv7 admin_id; AdminCtx
  -- extracted at S-13 admin layer; never trusted from the request body).
  added_by_admin_id  TEXT     NOT NULL,

  -- Provenance source (canonical 2-literal taxonomy):
  --   - Manual                          — admin direct add.
  --   - AutomatedSuggestionApproved     — admin approved a sustained-abuse
  --                                       suggestion (LGPD Art. 20 humane
  --                                       review; spec contract §7.10.s08.3).
  source             TEXT     NOT NULL,

  -- Self-service appeal URL (sprint contract §7.10.s08.3 humane response;
  -- LGPD Art. 20 alignment). Populated by the admin endpoint per WI §6.1.
  appeal_url         TEXT     NOT NULL,

  -- Watermark: when the row was last successfully synced to the CF
  -- List replica. NULL until the first reconcile pass after INSERT.
  cf_list_synced_at  INTEGER,

  -- Soft-delete watermark (Unix ms). NULL = active; otherwise the row
  -- is retained for the audit trail but excluded from enforcement.
  deleted_at_ms      INTEGER,

  PRIMARY KEY (cidr_text),

  -- chk_edge_cidr_family_canonical: closed canonical family list.
  CHECK (cidr_family IN (4, 6)),
  -- chk_edge_prefix_len_v4_envelope.
  CHECK (cidr_prefix_len >= 0),
  CHECK (
    (cidr_family = 4 AND cidr_prefix_len <= 32)
    OR (cidr_family = 6 AND cidr_prefix_len <= 128)
  ),
  -- chk_edge_reason_canonical: closed canonical 4-literal list.
  CHECK (reason IN (
    'Sustained4xx',
    'DDoSPattern',
    'ManualAdmin',
    'Compliance'
  )),
  -- chk_edge_source_canonical: closed canonical 2-literal list.
  CHECK (source IN ('Manual', 'AutomatedSuggestionApproved')),
  -- chk_edge_added_at_non_negative.
  CHECK (added_at_ms >= 0),
  -- chk_edge_expires_after_added (NULL allowed = permanent).
  CHECK (expires_at_ms IS NULL OR expires_at_ms > added_at_ms),
  -- chk_edge_deleted_after_added (NULL allowed = active).
  CHECK (deleted_at_ms IS NULL OR deleted_at_ms >= added_at_ms),
  -- chk_edge_cidr_text_non_empty.
  CHECK (length(cidr_text) >= 1)
);

-- Index: enforcement scan — every reconcile pass + every cold-start
-- reload reads `WHERE deleted_at_ms IS NULL ORDER BY cidr_family,
-- cidr_prefix_len DESC` (longest-prefix-match favours the most-specific
-- entries first).
CREATE INDEX IF NOT EXISTS idx_edge_blocklist_active_family_prefix
  ON edge_blocklist(cidr_family, cidr_prefix_len)
  WHERE deleted_at_ms IS NULL;

-- Index: reconcile drift detection — surface rows that have NEVER
-- successfully synced to the CF List replica (cf_list_synced_at IS NULL)
-- within the lookback window.
CREATE INDEX IF NOT EXISTS idx_edge_blocklist_unsynced
  ON edge_blocklist(cf_list_synced_at)
  WHERE deleted_at_ms IS NULL;
