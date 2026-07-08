# CoreLink — Production Forensics Guide

> **Status:** ACTIVE — first responder reference for post-incident investigation.
> **Owner:** SRE Lead (rotating).
> **Audience:** Any engineer paged into an incident war room.
> **Companion runbooks:** `specs/_runbooks/RB-*.md` (failure-mode specific).
> **Last reviewed:** 2026-05-15.

This guide is the canonical "Debugging Incidents 101" for CoreLink.
It captures the **generic forensic workflow** that applies to every
incident, regardless of failure mode. Individual runbooks
(`specs/_runbooks/RB-*.md`) handle their specific scenarios; this guide
ties them together and is what you read first when you join the war
room and do not yet know which runbook applies.

> **Rule of thumb:** if you are 10 minutes in and still cannot name the
> failure mode, you are still in this guide. Once you can name it, you
> jump to the matching `RB-*` and follow its checklist.

---

## Table of contents

1. [First contact](#1-first-contact)
2. [Time-bracketing](#2-time-bracketing)
3. [Audit chain replay](#3-audit-chain-replay)
4. [D1 forensics](#4-d1-forensics)
5. [KV / DO state inspection](#5-kv--do-state-inspection)
6. [Cross-region correlation](#6-cross-region-correlation)
7. [BYOK envelope inspection (test/staging ONLY)](#7-byok-envelope-inspection-teststaging-only)
8. [Common diagnostic patterns](#8-common-diagnostic-patterns)
9. [Tools cheatsheet](#9-tools-cheatsheet)
10. [Evidence preservation](#10-evidence-preservation)
11. [Cross-link to runbooks](#11-cross-link-to-runbooks)

---

## 1. First contact

You just got paged or pulled into Slack. Goal of this section: be a
useful war room member within 5 minutes.

### 1.1 Log into the war room

- Slack channel: `#incident-active` (pinned bridge). If a dedicated
  `#inc-YYYYMMDD-N` channel has been spun up, join that too.
- Voice bridge: Zoom link in the `#incident-active` topic. Mic muted
  by default; unmute only to commit to an action.
- Status page admin: <https://status.corelink.humangr.com/admin> — read-only
  unless you are IC.

### 1.2 Claim incident command (only if no IC exists)

- Look at `#incident-active` topic. If it shows `IC: <name>`, that
  person is in charge — defer to them.
- If `IC: none` or topic is stale (>10 min since last update with no
  named IC), claim it: `/ic claim INC-YYYYMMDD-N`. Set topic.
- IC duties: drive timeline, **delegate** technical work, own status
  page updates. See `specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md`
  for the full operator playbook.

### 1.3 Baseline metrics screenshot

Before you change anything, capture the "now" snapshot — this becomes
evidence for the retro and a reference for "did it get better?".

- Open the **Incident Overview** Grafana dashboard
  (`grafana.corelink.humangr.com/d/incident-overview`).
- Screenshot the SLO panels at the current timestamp. Save to
  `incidents/INC-YYYYMMDD-N/00-baseline-grafana-<ts>.png` (see §10).
- Note p50/p95/p99 latency, error rate, and which SLO is breached
  (the dashboard highlights breaches in red).

### 1.4 Status page — set it

- If user-visible impact: switch the relevant component to
  **Degraded performance** or **Partial outage** within 5 minutes of
  detection. Wording: short, factual, no speculation on cause.
- If unsure whether users are affected: post a **Monitoring** update
  ("We're investigating elevated error rates on …"). Better to
  over-communicate than under-.
- Update cadence after the initial post: every 30 minutes during the
  investigation, even if "still investigating, no new info."

### 1.5 Identify which runbook applies

- Failure-mode known (eg "Stripe webhooks stuck") → jump to the
  matching `RB-*` (table in §11).
- Failure-mode unknown → continue to §2.

---

## 2. Time-bracketing

You cannot debug an incident if you do not know **when it started**.
Time-bracketing is the cheapest, highest-signal step in every
investigation.

### 2.1 Find the SLO breach window from Prometheus

Run the helper script (described in §9):

```bash
python3 scripts/forensics/time-bracket-slos.py \
    --start "2026-05-15T14:00:00Z" \
    --end   "2026-05-15T15:00:00Z"
```

It queries Grafana for every SLO defined in
`specs/03_architecture/slo_catalog.md` and prints which ones breached,
when, and how severely. Output is JSON; pipe through `jq` for tables.

Manual query (if the script is unavailable):

```promql
# CAS read p99 latency SLO breach window
histogram_quantile(0.99,
  sum(rate(corelink_cas_read_latency_seconds_bucket{env="prod"}[5m]))
  by (le, region)
) > 0.5  # SLO target = 500ms
```

The earliest timestamp where this is `true` is your incident start
candidate. Walk back 5 minutes to capture the leading edge.

### 2.2 Correlate with deploy log

```bash
# All prod deploys in the bracket window
gh api repos/HumanGuardrail/corelink-server/deployments \
    --jq '.[] | select(.environment == "production")' \
    | jq 'select(.created_at >= "2026-05-15T13:00:00Z" \
                 and .created_at <= "2026-05-15T15:30:00Z")'
```

If you find a deploy **within ±10 minutes of the breach onset**, that
is your prime suspect. Open
`specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` and consider a rollback
before going deeper into root cause.

### 2.3 Correlate with change log

```bash
# Config / feature-flag changes in the bracket window
wrangler d1 execute corelink-prod \
    --remote \
    --command "SELECT changed_at, actor, change_type, summary
               FROM config_change_log
               WHERE changed_at >= 1715781600000
                 AND changed_at <= 1715785200000
               ORDER BY changed_at DESC;"
```

Look for flag flips, quota updates, BYOK rotations, anything human.
Pair every change with its `change_id` for the retro.

### 2.4 Output of this step

By end of §2 you should be able to write one sentence on the IC
channel:

> "Incident started 14:23Z. Affected SLO: CAS-READ-P99 in `wnam`. No
> deploys in window. Config change `cfg-2026-05-15-0042` at 14:18Z
> raised tenant-T quota by 100x — investigating."

---

## 3. Audit chain replay

The audit chain (`crates/corelink-audit-chain/`) is the
hash-chained, append-only log of every state mutation. Re-playing it
against a snapshot lets you reproduce the exact sequence of events
that led to the observed state, without touching prod.

### 3.1 Export the relevant window

> **Note:** `corelink-cli` does not currently expose an `audit export`
> subcommand (the 7 canonical subcommands are `ls / get / put / stat /
> bench / doctor / version` — see `crates/corelink-cli/src/main.rs`).
> Until a dedicated operator subcommand lands, export is via the
> wrangler shim below.

```bash
# Export 30 minutes of audit events for tenant T, around incident start
TENANT="01HX...UUIDv7..."
START_MS=1715781600000   # incident start
END_MS=1715783400000     # +30 min

wrangler d1 execute corelink-prod --remote --json \
    --command "SELECT id, tenant_id, request_id, event_type,
                      payload_json, enqueued_at, emitted_at
               FROM audit_outbox
               WHERE tenant_id = '${TENANT}'
                 AND enqueued_at >= ${START_MS}
                 AND enqueued_at <  ${END_MS}
               ORDER BY enqueued_at ASC;" \
    > incidents/INC-YYYYMMDD-N/audit-export.json
```

### 3.2 Verify the chain head

Before replay, confirm the export is **gap-free** (every
`sequence_number` is contiguous; no out-of-order events):

```bash
python3 scripts/forensics/audit-gap-detector.py \
    incidents/INC-YYYYMMDD-N/audit-export.json
```

Exits 0 if the export is contiguous; exits 1 listing every gap or
out-of-order event. A gap during the incident window is itself a
finding — capture it.

### 3.3 Logical replay against a staging snapshot

> The replay engine for *billing* events lives in
> `crates/corelink-billing-replay/`; it is the only first-class
> replay engine in-tree at GA. For non-billing events, replay is
> **logical** (you read the chain and reason about state transitions
> by hand) rather than mechanical.

```bash
# Restore a staging snapshot at incident start
./scripts/cold-restore-drill.sh \
    --target staging-forensics \
    --as-of  "${START_MS}"

# Replay billing events through the canonical replay engine
cargo run -p corelink-billing-replay --release -- \
    --input  incidents/INC-YYYYMMDD-N/audit-export.json \
    --target staging-forensics \
    --mode   dry-run
```

`--mode dry-run` writes nothing; it simulates each event and reports
which would have been **accepted**, **denied**, or **idempotent-skip**
(the four canonical `ReplayDecision` variants — see
`crates/corelink-billing-replay/src/event.rs`).

For non-billing replay: read the chain export by hand,
event-by-event, into a markdown narrative under
`incidents/INC-YYYYMMDD-N/replay-narrative.md`. Tedious; effective.

---

## 4. D1 forensics

D1 (Cloudflare's distributed SQLite) holds tenant state, audit
outbox, Stripe webhook idempotency, and dozens of other tables. SQL
is the primary forensic tool.

### 4.1 Canonical tables you will hit

| Table                                  | What it holds                                | Migration |
|----------------------------------------|----------------------------------------------|-----------|
| `audit_outbox`                         | Pending audit events (drained to S-09 chain) | `0001`    |
| `blob_meta`                            | CAS object metadata                          | `0001`    |
| `ac_meta`                              | REAPI Action Cache metadata                  | `0002`    |
| `stripe_webhook_events_processed`      | Webhook idempotency dedup                    | `0044`    |
| `stripe_webhook_dlq`                   | Webhook DLQ (replay candidates)              | `0045`    |
| `tenant_storage_state`                 | Per-tenant quota / usage                     | `0008`    |
| `quota_reservations`                   | In-flight reservations                       | `0009`    |
| `quota_fsm_state`                      | Quota state machine                          | `0020`    |
| `billing_reconciliation_drift`         | Reconciliation drift rows                    | `0019`    |
| `usage_event_idem`                     | Usage event idempotency                      | `0017`    |
| `dsr_erasure_log`                      | DSR (GDPR/LGPD) erasure trail                | `0022`    |
| `config_change_log`                    | Operator config changes                      | `0024`    |
| `billing_replay_audit`                 | Billing replay audit trail                   | `0021`    |

Schemas: `migrations/d1/*.sql` (all idempotent, additive-only).

### 4.2 `wrangler d1 execute` — the workhorse

```bash
# Always use --remote on prod; --local hits the dev sqlite.
wrangler d1 execute corelink-prod --remote --json \
    --command "SELECT count(*) FROM audit_outbox WHERE emitted_at IS NULL;"
```

Pre-canned query patterns:

**Audit outbox — pending drain backlog** (S-09 chain stuck):

```sql
SELECT count(*)             AS pending,
       min(enqueued_at)     AS oldest_ms,
       max(enqueued_at)     AS newest_ms
FROM   audit_outbox
WHERE  emitted_at IS NULL;
```

Healthy: pending < 100, max age < 60s. Stuck: pending growing
monotonically. See §8.3 + `RB-WEBHOOK-DLQ-REPLAY` for drain recovery.

**Stripe webhook — was this `evt_…` already processed?**

```sql
SELECT event_id, event_type, processed_at_ms, outcome
FROM   stripe_webhook_events_processed
WHERE  event_id = 'evt_1Nf...';
```

If 1 row → idempotency caught the retry (expected). If 0 rows → first
delivery; check `stripe_webhook_dlq` next.

**Tenant state snapshot** (use `tenant-state-snapshot.sh` — §9):

```sql
SELECT * FROM tenant_storage_state WHERE tenant_id = '<UUID>';
SELECT * FROM quota_reservations   WHERE tenant_id = '<UUID>';
SELECT * FROM quota_fsm_state      WHERE tenant_id = '<UUID>';
```

### 4.3 Safety rules

- **NEVER** run `UPDATE` / `DELETE` / `INSERT` via `wrangler d1
  execute` during an incident. State mutations go through the
  application layer or via `migrations/d1/*.sql` reviewed PRs only.
- If you think you need a write, **stop** and bring it to the IC. Ad
  hoc writes are the #1 way to turn a 30-minute incident into a
  3-day cleanup.
- Read queries should always project specific columns, not `SELECT *`,
  to keep tail logs reviewable.

---

## 5. KV / DO state inspection

KV and Durable Objects hold ephemeral / sharded state that is **not**
in D1: rate-limit buckets, idempotency caches, quota DO snapshots,
config DO leases.

### 5.1 What the CLI currently exposes

> **GA-state caveat:** at GA, `corelink doctor` runs a **fixed 8-check
> health suite** (see `crates/corelink-cli/src/commands/doctor_cmd.rs`
> + `crates/corelink-cli/src/doctor.rs`). It does **not** yet have
> per-store sub-commands like `doctor ratelimit` or `doctor quota`.
> The "what would I want?" shape below documents the **target** API;
> until those land, the read paths are direct D1 queries (§4) for any
> state mirrored to D1, and operator-only Cloudflare dashboard reads
> for pure DO state.
>
> Tracking: post-GA follow-up — split `doctor` into per-store
> read-only inspection subcommands. Do not invent flags that the
> binary does not implement.

### 5.2 Rate-limit DO state

Token-bucket DOs are keyed by `<tenant_id>:<bucket_name>`. Read paths
available today:

```bash
# 1) From the D1 mirror (recommended — read-only, audit-safe)
wrangler d1 execute corelink-prod --remote --json \
    --command "SELECT tenant_id, bucket, tokens, refill_rate_per_s, updated_at_ms
               FROM ratelimit_buckets
               WHERE tenant_id = '<UUID>'
                 AND bucket    = 'cas_put';"

# 2) From the Cloudflare dashboard
#    Workers → Durable Objects → ratelimit_v1 → "<UUID>:cas_put"
#    (operator-only; SSO + 2FA gated)
```

### 5.3 Quota DO state

```bash
# D1 mirror — preferred forensic source
wrangler d1 execute corelink-prod --remote --json \
    --command "SELECT * FROM quota_fsm_state WHERE tenant_id = '<UUID>';"
```

Cross-check D1 `quota_fsm_state` against any DO-side state observed
in the Cloudflare dashboard — they should match within the
eventual-consistency window (≤ 5s).

### 5.3 Idempotency KV

KV is **eventually consistent globally**. Reads from
`<region>:idempotency:<key>` may be stale up to 60s. For
forensics:

```bash
wrangler kv:key get --namespace-id <NS_ID> \
    "idempotency:<request_id>" --remote
```

If you see a `null` here but the application path swears it wrote one,
the leading suspect is the 60s eventual-consistency window — confirm
by re-reading after 60s before declaring a bug.

### 5.4 Config DO leases

Config DO holds the live config snapshot + a lease timestamp. Read
via the D1 mirror (`config_change_log` + the live snapshot row):

```bash
# Most recent config change applied
wrangler d1 execute corelink-prod --remote --json \
    --command "SELECT changed_at, actor, change_type, summary
               FROM config_change_log
               ORDER BY changed_at DESC LIMIT 5;"
```

If the most recent `changed_at` is > 30s old AND the deploy pipeline
shows a config push within that window, the Config DO propagation is
likely stuck → `specs/_runbooks/RB-SECRETS-DRIFT.md`.

---

## 6. Cross-region correlation

CoreLink is a multi-region edge-first system. A single request may
touch **edge worker (region A) → DO (region B) → R2 (region C) →
D1 (region B)**, each hop emitting its own log line.

### 6.1 Tracing headers

Every external request gets:

- `traceparent` (W3C Trace Context 2020 — see
  `crates/corelink-tracing/src/context.rs`): 16-byte trace_id,
  8-byte span_id, 1-byte flags. **Canonical correlator across hops.**
- `x-corelink-request-id`: UUIDv7 set at edge ingress; the audit
  outbox `request_id` column traces back to this.
- `x-corelink-tenant-id`: tenant UUID (UUIDv7).
- `cf-ray`: Cloudflare's per-edge-hit id (1-hop only, not propagated).

### 6.2 Following a single request_id

```bash
# Pull every log line with this request_id, all regions, JSON shape
gcloud logging read \
    "jsonPayload.request_id = \"<UUID>\"
     AND timestamp >= \"2026-05-15T14:00:00Z\"" \
    --format json \
    --project corelink-prod \
    > incidents/INC-YYYYMMDD-N/req-<UUID>.json

jq -r '.[] | "\(.timestamp) [\(.jsonPayload.region)] \
              \(.jsonPayload.service) \(.jsonPayload.event)"' \
    incidents/INC-YYYYMMDD-N/req-<UUID>.json \
    | sort
```

You now have the full request timeline, region-by-region, in
chronological order.

### 6.3 Following a trace_id (cross-service)

`trace_id` is the right correlator when one external request fans out
to multiple internal services. Search Grafana Tempo
(`tempo.corelink.humangr.com`) by `trace_id`; every span across every service
that participated will appear.

### 6.4 Active-failover incidents

If the incident is region-level (one region degraded, another
healthy), the canonical playbook is
`specs/_runbooks/RB-ACTIVE-FAILOVER.md`. Cross-region forensics adds:

- Was the failover triggered? Check `corelink_active_failover_state`
  metric — value `1` means a region is fenced.
- Did traffic shift? `corelink_edge_requests_total{region=...}` should
  drop to ~0 on the fenced region within 60s.

---

## 7. BYOK envelope inspection (test/staging ONLY)

> **HARD RULE — read this before reading anything else in this
> section.** Production code paths **NEVER** decrypt customer DEKs.
> Customer-managed envelopes are unwrapped only inside the customer's
> KMS (AWS KMS / Azure Key Vault / GCP KMS), under the customer's
> IAM. The CoreLink control plane stores only the *wrapped* envelope
> + metadata. **Do not attempt to unwrap a production envelope, ever,
> for any reason.** If you genuinely need plaintext to debug, that
> is a customer-side action and goes through
> `RB-CUSTOMER-SUPPORT-T-90`.

### 7.1 What you CAN do in production (forensics-safe)

- Read envelope **metadata**: which CMK alias signed it, when, by
  whom (operator identity), envelope generation number.
- Compute a **digest match**: hash the wrapped envelope locally,
  compare to the audit chain record. Does the on-disk envelope match
  what the chain says was stored? This catches tampering without
  ever touching plaintext.

```bash
# Read envelope metadata (no plaintext access) via D1 mirror
wrangler d1 execute corelink-prod --remote --json \
    --command "SELECT envelope_id, cmk_alias, cmk_provider,
                      wrapped_at_ms, operator_id,
                      envelope_generation, wrapped_digest_blake3
               FROM byok_envelope_meta
               WHERE tenant_id = '<UUID>'
                 AND envelope_id = '<ENV-ID>';"
```

Fields you get: `cmk_alias`, `cmk_provider`, `wrapped_at_ms`,
`operator_id`, `envelope_generation`, `wrapped_digest_blake3`. Fields
you do **not** get: the wrapped bytes themselves, the DEK, any key
material. The wrapped bytes live in R2 under a tenant-scoped prefix
and require a dual-control read flow (see
`crates/corelink-dual-approval/`) that is explicitly out-of-scope for
unilateral on-call forensics.

### 7.2 What you CAN do in staging (where customer KMS is mocked)

In staging, the BYOK matrix test fixtures
(`crates/corelink-byok-matrix-test/`) use mock CMKs under our control.
You can unwrap a staging envelope to verify the unwrap path:

```bash
# STAGING ONLY — refuses to run against prod
cargo run -p corelink-byok-matrix-test --release -- \
    unwrap-envelope \
    --env staging \
    --envelope-id <ENV-ID>
```

The binary verifies it is bound to `staging` (refuses prod env vars)
before doing anything. If you see it running in prod, **stop the world**
— that's an SEV-1 incident in itself.

### 7.3 Revocation forensics

If BYOK revocation is suspected (eg "tenant says they revoked but our
side still has access"):

```sql
SELECT * FROM byok_revocation_events
WHERE  tenant_id = '<UUID>'
ORDER BY emitted_at DESC LIMIT 10;
```

Cross-check with the audit chain (§3) — every revocation must emit
`corelink.byok.revoked` and `corelink.byok.access_denied` (the
follow-up that confirms our side stopped using the key).

---

## 8. Common diagnostic patterns

Four real failure modes seen during pre-GA chaos drills. Each maps to
a runbook for the full recovery procedure; this section captures the
**diagnostic signature** so you can recognize it fast.

### 8.1 Stuck DSR worker

**Signature:** `dsr_erasure_log` has rows with `state = 'in_progress'`
and `started_at > 30 min ago`. The DSR worker DO is wedged.

**Quick diagnosis:**

```sql
SELECT id, tenant_id, started_at,
       (strftime('%s','now')*1000 - started_at)/1000 AS age_seconds,
       state
FROM   dsr_erasure_log
WHERE  state = 'in_progress'
ORDER BY started_at ASC;
```

Any row with `age_seconds > 1800` is stuck.

**Full recovery:** `specs/_runbooks/RB-DSR-GDPR.md` §4 (resume) or §5
(manual completion).

### 8.2 Stale Stripe sub state

**Signature:** Stripe customer portal shows a subscription as
`active`, but our `tenant_storage_state` shows the tenant as
`suspended_billing`. Webhook drift.

**Quick diagnosis:**

```sql
SELECT t.tenant_id, t.billing_state,
       w.event_id, w.event_type, w.processed_at_ms
FROM   tenant_storage_state t
LEFT JOIN stripe_webhook_events_processed w
       ON w.tenant_id = t.tenant_id
WHERE  t.tenant_id = '<UUID>'
ORDER BY w.processed_at_ms DESC
LIMIT 10;
```

If the most recent webhook is > 1 hour old and Stripe shows recent
events, deliveries are stuck.

**Replay the missing event** with the helper script:

```bash
./scripts/forensics/replay-stripe-webhook.sh \
    --event-id evt_1Nf... \
    --env      staging   # NEVER prod without IC sign-off
```

**Full recovery:** `specs/_runbooks/RB-WEBHOOK-DLQ-REPLAY.md`.

### 8.3 Audit chain gap

**Signature:** `audit-gap-detector.py` (§3.2) reports missing
`sequence_number` values, or `audit_outbox` has rows with
`emitted_at IS NULL` and `enqueued_at < now() - 5 min`.

**Quick diagnosis:**

```sql
SELECT min(enqueued_at) AS oldest_pending,
       max(enqueued_at) AS newest_pending,
       count(*)         AS pending_total
FROM   audit_outbox
WHERE  emitted_at IS NULL;
```

If `now() - oldest_pending > 5 min` → drain worker stuck.

**Full recovery:** `specs/_runbooks/RB-WEBHOOK-DLQ-REPLAY.md` (DLQ path)
or `RB-CHAOS-CATALOG.md` (chaos scenario `audit-drain-stuck`).

### 8.4 Region failover incomplete

**Signature:** Active-failover triggered, but
`corelink_edge_requests_total{region=<fenced>}` is still > 0 after
60s. Some traffic is bypassing the fence.

**Quick diagnosis:** check the failover state in the D1 mirror:

```bash
wrangler d1 execute corelink-prod --remote --json \
    --command "SELECT region, fenced, fenced_at_ms, fenced_by
               FROM global_circuit_state
               WHERE fenced = 1;"
```

If the fenced region is listed but `corelink_edge_requests_total`
for that region continues > 0 after 60s, the edge worker's config
refresh is stuck — see `specs/_runbooks/RB-SECRETS-DRIFT.md` for
config-DO drift recovery.

**Full recovery:** `specs/_runbooks/RB-ACTIVE-FAILOVER.md` §6.

---

## 9. Tools cheatsheet

### 9.1 `wrangler`

```bash
# D1 read (always --remote in prod, always read-only during incident)
wrangler d1 execute corelink-prod --remote --json --command "..."

# KV read
wrangler kv:key get --namespace-id <NS_ID> "<key>" --remote

# Tail a worker live (CAUTION — high volume in prod)
wrangler tail corelink-edge --env production --format json | jq '...'
```

### 9.2 `cargo run -p corelink-cli --`

The CLI exposes 7 canonical subcommands (see
`crates/corelink-cli/src/main.rs`):

```bash
cargo run -p corelink-cli --release -- ls       # list blobs by tenant
cargo run -p corelink-cli --release -- get      # fetch blob by digest
cargo run -p corelink-cli --release -- put      # upload (DO NOT use in prod incident)
cargo run -p corelink-cli --release -- stat     # metadata of a blob
cargo run -p corelink-cli --release -- bench    # local benchmark (not for prod)
cargo run -p corelink-cli --release -- doctor   # 8-check health suite — fully read-only
cargo run -p corelink-cli --release -- version  # build version + commit
```

`doctor` runs a fixed 8-check suite (auth, network, config, audit
chain head reachability, etc — see `crates/corelink-cli/src/doctor.rs`)
and exits non-zero on any failure. It is fully read-only and safe to
run in prod. It does **not** yet take a `--tenant` filter or
per-store sub-subcommands; for tenant/store-specific reads, use
`wrangler d1` against the D1 mirror (§4, §5).

### 9.3 `jq` patterns

```bash
# Filter audit events by type
jq '.[] | select(.event_type == "corelink.cas.put_completed")' audit-export.json

# Group events by region, count
jq -r 'group_by(.region) | map({region: .[0].region, n: length})' logs.json

# Extract trace_ids that appeared in >1 region (suspicious — cross-region request)
jq -r '[.[] | {tid: .trace_id, r: .region}]
       | group_by(.tid)
       | map(select((. | map(.r) | unique | length) > 1))
       | map(.[0].tid)' logs.json
```

### 9.4 Custom forensic scripts (`scripts/forensics/`)

| Script                              | Purpose                                            |
|-------------------------------------|----------------------------------------------------|
| `time-bracket-slos.py`              | Given an incident window, list which SLOs breached |
| `audit-gap-detector.py`             | Walk chain head; detect missing/out-of-order seq#  |
| `replay-stripe-webhook.sh`          | Replay a single webhook event by id (staging)      |
| `tenant-state-snapshot.sh`          | Dump full tenant state (R2/D1/KV/DO) read-only     |

All scripts:

- `set -euo pipefail` (bash) / fail-fast (python).
- Refuse to write to prod by default; require explicit
  `--target prod` and an IC name to override.
- Output is structured (JSON or markdown) and goes into
  `incidents/INC-YYYYMMDD-N/` (see §10).

---

## 10. Evidence preservation

After triage, before stand-down, capture evidence for the retro. This
is **mandatory** for every SEV-1 and SEV-2; encouraged for SEV-3.

### 10.1 Directory layout

```
incidents/
└── INC-2026-05-15-1/
    ├── README.md              # IC's running timeline (markdown)
    ├── 00-baseline-grafana-<ts>.png
    ├── 01-postmortem-grafana-<ts>.png
    ├── audit-export.json      # §3.1
    ├── replay-narrative.md    # §3.3 (non-billing) — optional
    ├── req-<uuid>.json        # §6.2 — per request_id traced
    ├── tenant-snapshot/       # §9.4 dump
    │   ├── d1.json
    │   ├── kv.json
    │   └── do.json
    └── logs/
        ├── edge-<region>.json
        └── do-<region>.json
```

### 10.2 What MUST be in every incident folder

1. **Audit window export** (§3.1).
2. **Log snippet** for the canonical request_id (or 3-5 representative
   ones if no single request).
3. **Metric screenshot** at incident peak (the "red dashboard" shot).
4. **Timeline** (`README.md`) — IC owns this; updated live in war room.

### 10.3 Retention

Incident folders live in the `corelink-incidents` repo (private),
retained for **7 years** to match audit chain retention. Do not delete
even after retro is complete. Do not put PII in there — log lines
should already be redacted, but spot-check before committing.

### 10.4 Hand-off to postmortem

Once the incident is `RESOLVED`, follow
`specs/_runbooks/RB-POSTMORTEM-PROCESS.md`. The evidence folder is
the input.

---

## 11. Cross-link to runbooks

Every existing runbook maps to one or more sections of this guide.
Use this table to find the right RB once you have a hypothesis; use
§1–§2 of this guide to **form** the hypothesis.

| Runbook                               | Forensics §                | When to jump there |
|---------------------------------------|---------------------------|--------------------|
| `RB-ACTIVE-FAILOVER`                  | §6, §8.4                  | Region degradation |
| `RB-BACKUP-VERIFICATION`              | §10                       | Backup integrity   |
| `RB-BACKUP-VERIFICATION-FAILURE`      | §3, §10                   | Backup verify fail |
| `RB-CHAOS-CATALOG`                    | §8 (all)                  | Chaos drill incident |
| `RB-COLD-RESTORE-FROM-ZERO`           | §3.3, §10                 | DR from zero       |
| `RB-COMPLIANCE-WEEKLY-REVIEW`         | §10                       | Compliance evidence|
| `RB-CUSTOMER-SUPPORT-T-90`            | §7                        | Customer KMS issue |
| `RB-D1-MIGRATION-APPLY`               | §4                        | Migration incident |
| `RB-DPA-CHANGE`                       | §10                       | DPA-driven change  |
| `RB-DPO-ESCALATION`                   | §8.1, §10                 | DPO escalation     |
| `RB-DRATA-SYNC-FAILURE`               | §4, §9.1                  | Drata sync stuck   |
| `RB-DSR-GDPR`                         | §4, §8.1                  | GDPR DSR           |
| `RB-DSR-LGPD-FULL`                    | §4, §8.1                  | LGPD DSR           |
| `RB-DSR-TICKET-TRIAGE`                | §8.1                      | DSR triage         |
| `RB-ENDURANCE-24H-DRILL`              | §2, §10                   | 24h drill          |
| `RB-FIPS-ATTESTATION-RENEWAL`         | §7                        | FIPS renewal       |
| `RB-FM-SIGNUP-FAILED`                 | §6                        | Signup failure     |
| `RB-GA-LAUNCH-ROLLBACK`               | §2.2                      | Bad deploy         |
| `RB-LAUNCH-WAR-ROOM-COORDINATION`     | §1                        | War room mechanics |
| `RB-LIGHTHOUSE-CUSTOMER-INCIDENT`     | §1, §6                    | Lighthouse impact  |
| `RB-LIGHTHOUSE-PHASE-MANAGEMENT`      | §1                        | Lighthouse phase   |
| `RB-ONCALL-POLICY`                    | §1                        | On-call policy     |
| `RB-PENTEST-FINDING-RESPONSE`         | §7, §10                   | Pentest finding    |
| `RB-POSTMORTEM-PROCESS`               | §10                       | Retro              |
| `RB-SECRETS-DRIFT`                    | §5.4, §8.4                | Config DO stuck    |
| `RB-SECURITY-VULNERABILITY-INTAKE`    | §7, §10                   | Vuln intake        |
| `RB-SUBPROCESSOR-CHANGE`              | §10                       | Subprocessor swap  |
| `RB-SYNTHETIC-PAGE-DRILL`             | §2                        | Synthetic drill    |
| `RB-SYSTEM-CMK-ROTATION`              | §7                        | CMK rotation       |
| `RB-TABLETOP-TEMPLATE`                | §1                        | Tabletop exercise  |
| `RB-VENDOR-RISK-QUARTERLY-REVIEW`     | §10                       | Vendor review      |
| `RB-WEBHOOK-DLQ-REPLAY`               | §3, §4, §8.2, §8.3        | Webhook stuck      |
| `ONCALL-ESCALATION-MATRIX`            | §1.2                      | Who to page        |

Reverse direction: every runbook above includes a one-line back-link
to this guide at its top ("**Forensics:** see
`docs/internal/FORENSICS-GUIDE.md` §X."). When a runbook adds a new
diagnostic step that generalizes, lift it into this guide and update
the back-link.

---

## Appendix A — Quick-reference card

Print this and tape it to your monitor.

```
INCIDENT — first 10 minutes
1. Join #incident-active. Confirm IC. Mute mic.
2. Screenshot Grafana incident-overview.
3. Status page: Degraded / Partial outage if user-visible.
4. python3 scripts/forensics/time-bracket-slos.py --start ... --end ...
5. gh api ... deployments  (any deploy in ±10 min?)
6. config_change_log SELECT  (any flag flip in window?)
7. Pick the matching RB-* from FORENSICS-GUIDE.md §11.
8. If no obvious RB-*: read FORENSICS-GUIDE.md §2 → §3 → §4.
9. Capture into incidents/INC-YYYYMMDD-N/ as you go.
10. Cadence: status page update every 30 min until RESOLVED.
```
