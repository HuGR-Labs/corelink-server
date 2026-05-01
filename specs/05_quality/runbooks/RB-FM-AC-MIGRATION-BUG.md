---
id: "RB-FM-AC-MIGRATION-BUG"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "schema", "migration", "d1", "ac"]
---

# RB-FM-AC-MIGRATION-BUG — D1 `ac_meta` Migration Bug Detected in Prod

> **FM:** AC schema migration bug (data-loss exposure) | **SLA:** mitigate < 30 min (read-only stop) | **CRITICAL** — never `DROP TABLE`.

## Quando aplicar

- CHECK constraint inadvertidamente removida (ex: `chk_ac_region` accept `'gru'` que nunca foi ratificado).
- Migration framework hash drift detectada (file modified pós-applied).
- INSERT/UPDATE rejeitado em produção com SQLite error que sugere schema mismatch.
- Bug detectado em pré-prod via prop test que **só dispararia em prod** com migration errada.

## Detecção

- `corelink.d1.ac_meta.check_violation_total{constraint=*}` > 0 sustained (alert SEV-2).
- `wrangler d1 migrations list CORELINK_DB --env prod` mostra hash mismatch vs file no repo.
- Customer report de 5xx sustained em GET/UPDATE AC paths.
- CI diff `wrangler d1 migrations list staging` vs `production` divergente.

## Comunicação

- **SEV-2** (potencial CRITICAL se data preservation at risk).
- Page Architect + DBA-on-call (HIGH_RISK lane).
- Status page: degraded if customer-impacting.
- Skip customer notification até confirmar exposure (ver §Forensics).

## Mitigação imediata (≤ 30 min)

### Cenário A: bug é apenas regressão funcional (sem data leak)

1. **Verify rows preserved**: `wrangler d1 execute CORELINK_DB --env prod --command "SELECT COUNT(*) FROM ac_meta;"` — count deve ser estável.
2. **Disable AC writes** via degrade flag (`PAT-DEGRADE-001` cache-only):
   - GET path continua (read-only fallback).
   - UPDATE path retorna `503 COR_AC_DEPRECATED`.
3. Capture full schema dump for forensics: `wrangler d1 export CORELINK_DB --env prod --output /tmp/ac_meta_pre_rollback.sql`.

### Cenário B: rollback via dummy migration `0003_ac_meta_deprecated.sql`

```sql
-- migrations/d1/0003_ac_meta_deprecated.sql
-- ROLLBACK marker: ac_meta entries are read-only; UPDATE rejected by handler.
-- ⚠️ DO NOT add DROP TABLE; data must be preserved for manual recovery.
CREATE TABLE IF NOT EXISTS ac_meta_deprecation (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    deprecated_at INTEGER NOT NULL,
    reason      TEXT    NOT NULL
);
INSERT OR REPLACE INTO ac_meta_deprecation (id, deprecated_at, reason)
VALUES (1, <unix_ms_now>, '<bug short description>');
```

1. Apply via `scripts/migrate_d1.sh production <region>`.
2. Update worker handler config to read `ac_meta_deprecation` table on startup; if row present, every AC handler short-circuits to `503 COR_AC_DEPRECATED`.
3. Bazel/Buck2 clients see 503 + retry with backoff; build still succeeds (cache miss + execute).

### Cenário C: bug em CHECK constraint contents

ADR-0036 Rule 3 mandates a NEW migration via SQLite 12-step recipe;
**NEVER** raw `ALTER`. Procedure:

1. Draft `migrations/d1/0003_ac_meta_v2.sql` per ADR-0036 §Rule 3 example.
2. Stage in dual-write window (handler writes to both `ac_meta` + `ac_meta_v2`); confirm parity for ≥ 24 h.
3. Cutover: handler reads from `ac_meta_v2`; `ac_meta` becomes read-only legacy.
4. Retain old table ≥ 7 days for rollback.

## Mitigação completa (≤ 4 h)

1. Patch the underlying bug (handler code, migration SQL, framework drift).
2. Re-run schema simulator + property tests in `crates/corelink-ac-schema` to confirm regression coverage.
3. Re-deploy with new migration; validate `scripts/check_ac_infra.sh production` exits 0.
4. Re-enable AC writes via degrade flag clear.
5. Spot-check 100 random tenants: `SELECT COUNT(*) FROM ac_meta WHERE tenant_id IN (...)` matches pre-rollback census.

## Forensics

1. Audit log: who applied the bad migration? When?
2. Schema diff: `git log -p migrations/d1/0002_ac_meta.sql` vs `wrangler d1 migrations list`.
3. CI gate logs: was `scripts/check_migrations_additive.py` red? Did anyone bypass?
4. If hash drift: which commit modified the file post-apply?

## Notificação obrigatória

- **Internal**: post-mortem mandatory; ADR-0036 governance review.
- **Customer**: not required unless data exposure confirmed (cross-tenant OR data loss).
- **DPA / ANPD**: only if confirmed data exposure (see RB-BREACH-NOTIF).

## Anti-scope (NEVER do these)

- ❌ `DROP TABLE ac_meta` — data loss; irreversible.
- ❌ Raw `wrangler d1 execute --command "ALTER TABLE …"` — bypasses framework hash validation.
- ❌ Manual schema patch outside the migrations/ directory — audit trail loss.
- ❌ Skip the dual-write window during cutover — handler will see partial state.

## Post-mortem

5-Why mandatory; ADR-0036 governance review (was the rule clear enough?). Output: ADR amendment OR new ADR (e.g. ADR-0036b) tightening the migration framework gates.

## Cross-references

- `specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md` — governance rules.
- `migrations/d1/0002_ac_meta.sql` — canonical migration text.
- `scripts/migrate_d1.sh`, `scripts/check_ac_infra.sh`, `scripts/check_migrations_additive.py`.
- `crates/corelink-ac-schema` — schema simulator (regression test bed).
- `RB-FM-AC-BUCKET-LEAK` — sibling runbook for R2 bucket ACL drift.
- `RB-FM-303` — AC cross-tenant runbook (severer; data exposure).
