---
id: "RB-AUDIT-ARCHIVE-ABSENT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-23"
updated: "2026-08-23"
sprint: "R-PREP-WAVE-19"
parent_wi: "WI-S09-008"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-AUDIT-EXPORT-INTEGRITY"
  - "security_model"
tags: ["runbook", "audit", "archive", "absence", "sev-0", "soc2", "cc7.2", "monitoring"]
---

# RB-AUDIT-ARCHIVE-ABSENT — the offsite audit archive stopped being written

> **Status:** DRAFT. Owner: Gustavo Schneiter.
>
> **Trigger:** PagerDuty SEV-0, `class=archive-absent`,
> `component=corelink-audit-chain`, dedup key
> `audit-archive-absent-<YYYY-MM-DD>`, dispatched by
> `.github/workflows/audit-archive-lag.yml` (hourly cron).
>
> **Severity:** SEV-0. The sealed chain still exists in D1, but D1 is
> mutable; the R2 copy under the 7-year Object Lock is the immutable
> evidence tier (CTRL-AUDIT-001). While the archiver is stopped, every
> newly sealed event has exactly one mutable copy.

## 1. What the page means

It means **both** of these were true at the same sample:

1. At least one row in `audit_outbox` is **sealed** (`emitted_at IS NOT NULL`)
   and **unarchived** (`archived_at IS NULL`) and was enqueued more than
   **T = 3 hours** ago.
2. `MAX(archived_at)` across the whole `audit_outbox` table is older than T,
   or is `NULL` (nothing has ever been archived).

It does **not** mean the chain is broken. Chain-integrity breaks are a
different page (`class=integrity-break`, see `RB-AUDIT-CHAIN-VERIFY.md` /
`audit-chain-daily-verify.yml`). This page is about **absence**: sealed rows
that should have become NDJSON chunks in `corelink-audit-weur` and did not.

**Why two clauses.** Either clause alone is a false alarm generator:

- Clause 1 alone fires for the entire duration of the historical backlog
  drain (56,026 sealed rows existed unarchived when migration
  `0099_audit_outbox_archived_at.sql` landed). A long tail is healthy.
- Clause 2 alone fires during any genuinely quiet period, because
  `MAX(archived_at)` ages on its own when there is simply nothing to archive.

Together they describe one state only: **pending work the archiver is
demonstrably not touching.**

The job summary of every run — pass or fail — prints both measurements, so
the backlog tail is observable without waiting for a page.

## 2. Reproduce the measurement by hand

The producer is `POST /_internal/audit/archive`
(`crates/corelink-container/src/routes/audit_archive.rs`), driven hourly by
`apps/signup-worker/src/webhooks/audit_archive_cron.ts`. Chunks land in the
R2 bucket `corelink-audit-weur`. The watermark column is
`audit_outbox.archived_at` (unix epoch ms, nullable).

Database: `corelink-prod-d1`, id `d64742ea-e102-40b2-a844-ff02e3f94562`.

Run each query against prod D1. The `<T_MS>` placeholder is
`now_unix_ms - 3*3600*1000` — compute it with
`python3 -c 'import time;print(int(time.time()*1000)-3*3600*1000)'`.

**Clause 1 — pending work older than T:**

```sql
SELECT COUNT(*) AS pending_old, MIN(enqueued_at) AS oldest_pending_ms
FROM audit_outbox
WHERE emitted_at IS NOT NULL AND archived_at IS NULL
  AND enqueued_at < <T_MS>;
```

**Clause 2 — the archiver's last demonstrated write:**

```sql
SELECT MAX(archived_at) AS last_archived_ms FROM audit_outbox;
```

**Backlog shape (context, not part of the predicate):**

```sql
SELECT COUNT(*) AS sealed_total,
       SUM(CASE WHEN archived_at IS NULL THEN 1 ELSE 0 END) AS unarchived_total
FROM audit_outbox WHERE emitted_at IS NOT NULL;
```

Via `wrangler`:

```bash
worker/node_modules/.bin/wrangler d1 execute corelink-prod-d1 --remote \
  --command "SELECT MAX(archived_at) FROM audit_outbox;"
```

Or via the same Cloudflare D1 HTTP API the workflow uses (note the REST API
binds JSON numbers as **REAL** — inline integer literals, or wrap binds in
`CAST(? AS INTEGER)`):

```bash
curl -s -X POST \
  -H "Authorization: Bearer $CF_API_TOKEN" -H "Content-Type: application/json" \
  --data '{"sql":"SELECT MAX(archived_at) AS last_archived_ms FROM audit_outbox","params":[]}' \
  "https://api.cloudflare.com/client/v4/accounts/$CF_ACCOUNT_ID/d1/database/d64742ea-e102-40b2-a844-ff02e3f94562/query"
```

## 3. The three most likely causes, in order

### 3.1 The erase auth key is unbound (most likely)

Both audit crons call the container over `/_internal/*` with the erase auth
key. When it is not bound, the signup-worker **skips the call and says so**:

```
[audit-archive-cron] skipped=true reason=erase-auth-key-unbound
```

(the drain cron emits the sibling line `[audit-drain-cron] skipped=true
reason=erase-auth-key-unbound`).

**Check:** `wrangler tail` the signup-worker, or read its recent logs, and
grep for `erase-auth-key-unbound`.

**Fix:** re-bind `CORELINK_ERASE_AUTH_KEY` on the signup-worker (write-only
secret; the value lives only in `.env.local`). Set it with `printf`, never
`echo` — a trailing newline yields 401/403. This key has **no fallback** to
the shared internal key; the path is fail-closed by design.

### 3.2 The route is not mounted after a container roll

`main.rs` mounts `POST /_internal/audit/archive` only when the erase auth key
**and** the D1 binding **and** the R2 client are all present. Otherwise it
logs, at startup:

```
/_internal/audit/archive route NOT mounted (fail-CLOSED)
```

and the cron's POST gets a 404 from a container that is otherwise healthy.

**Check:** container startup logs for `route mounted` vs `route NOT mounted`,
plus a direct probe of the endpoint. Remember that a Worker deploy does **not**
restart a live DO-container — only a NEW image pin replaces instances, or the
manual `/_internal/admin/recycle-system` lever.

**Fix:** correct the missing binding, roll a new image pin, and confirm the
mount line appears in the new container's startup log. Do not judge by the
first cron tick after a roll: a tick can precede the container roll and report
zero, which looks like a broken route.

### 3.3 A partition is failing

The handler walks per-`(tenant_id, region)` partitions and returns
`partitions_failed` in its JSON response; a non-zero count downgrades the
HTTP status, and the container logs
`audit/archive: partition failed — rows left UNARCHIVED`. The cron reads
`partitions_failed` out of the response.

This is the case where the archiver **is** running — so `MAX(archived_at)`
may keep advancing from the healthy partitions while one tenant's rows never
drain. That combination will **not** trip this page (clause 2 stays false),
which is why `partitions_failed` must be checked independently.

**Check:** call the endpoint once by hand and read `partitions_failed` plus
the per-partition error in the container log.

**Fix:** depends on the underlying error (R2 auth/permission on
`corelink-audit-weur`, a residency/region mismatch, a malformed
`canonical_jcs`). Treat a persistently failing partition as its own SEV-1.

## 4. Verify recovery

1. Re-run `.github/workflows/audit-archive-lag.yml` via `workflow_dispatch`.
   The job summary must show `MAX(archived_at)` inside the last T and the
   verdict `OK`.
2. Confirm chunks actually landed: list `corelink-audit-weur` under
   `audit/<YYYY>/<MM>/<DD>/` for today.
3. Let the next `audit-chain-daily-verify` run confirm the new chunks are
   chain-valid — absence recovery is not integrity proof.
4. Resolve the PagerDuty incident with dedup key
   `audit-archive-absent-<YYYY-MM-DD>`.

## 5. Related

- `.github/workflows/audit-archive-lag.yml` — this page's source.
- `.github/workflows/audit-chain-daily-verify.yml` — the **corruption**
  monitor (verifies chunks it finds; blind to chunks that never existed).
- `migrations/d1/0099_audit_outbox_archived_at.sql` — the watermark column
  and the archiver's partial work-queue index.
- `crates/corelink-container/src/routes/audit_archive.rs` — the producer.
- `apps/signup-worker/src/webhooks/audit_archive_cron.ts` — the driver.
- `docs/knowledge/compliance/audit-chain.md` — architecture concept.
