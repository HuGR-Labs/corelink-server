# Migrations/ root directory — historical context (K.2)

The SQL files in this directory are **historical artifacts** from the
pre-D1 era of the CoreLink control plane (Postgres-on-Neon). They are
**NOT** part of the current D1 schema and are **NOT** applied by
`wrangler.toml` / `corelink-d1-migrations` / `scripts/d1-migration-validate.py`.

| File | Era | Current role |
|------|-----|---------------|
| `0001_init.sql` | Neon era (WI-S03-001) | **deprecated** — the canonical D1 schema lives in `migrations/d1/`. Referenced only by `corelink-auth/tests/schema_migration_canonical.rs` and the embedded canonical-text constant in `corelink-auth/src/schema.rs` (`include_str!`). The D1 schema mirrors the entities declared here but as D1 SQL (D1 uses DATE functions etc, not Postgres). |
| `002_auth_tables.sql` | Neon era (WI-S03-005) | **embedded in `corelink-auth` as the canonical text regression** — see `MIGRATION_002_AUTH_TABLES` in `crates/corelink-auth/src/schema.rs`. The Rust source of truth for the auth schema is `corelink-auth/src/schema.rs` (which itself embeds this file via `include_str!` for the in-process test-mode schema). Do NOT move. |
| `013_admin_op_log.sql` | Neon era (WI-S13-002) | Already migrated to `migrations/d1/` (see current D1 canonical DDL). The Neon-era copy here is preserved as a cross-reference for the original spec, not as a live migration. |
| `013_rotation_state.sql` | Neon era (WI-S13-003) | Same: already migrated to D1; copy here is historical. |
| `N4__sub_processor_tables.sql` | Neon era (WI-S11-005) | Already migrated to D1; copy here is historical. |
| `neon/0001_audit_events_shadow.sql` | Neon shadow schema (WI-S09-005) | **Live canonical shadow schema** for the Neon-backed audit shadow. Embedded via `include_str!` in `corelink-audit-chain/tests/harness/pg_container.rs`. Do NOT move. |
| `neon/0002_audit_events_shadow_with_check.sql` | Neon shadow v2 | Same: live canonical, referenced from `corelink-audit-chain/src/neon_shadow.rs` and tests. Do NOT move. |

**K.1 — 0044 collision (grandfathered):** the 2026-05-14 `0044_drata_evidence_sent.sql` and `0044_stripe_webhook_events_processed.sql` collision is **intentional** per PR #1353. Both are applied in production and the D1 ledger keys on the full filename, so renaming would desync the ledger. The `scripts/check_migrations_additive.py` gate now fails on any FUTURE ordinal collision but grandfathers this single pair explicitly.

**K.2 — Why these files are NOT moved into `migrations/_archive/`:** moving breaks the embedded-canonical-text regression in `corelink-auth` and `corelink-audit-chain` (both use `include_str!` to read these files as compile-time constants). The cleanest "archive" is the **comment in this README** plus the OKF concept `auth/auth-schema-canonical-text` that already names this directory as the Neon control-plane historical home.

**K.3 — `TODO.md` (repo root):** moved into `docs/_archive/2026-04-todo-mvp.md` with a one-line note that BACKLOG.md is the current source of truth. The 12 stale files (SQL, Neon shadow, 0044 collision) are the unwashed dishes of the pre-D1 era; this README is the sink.
