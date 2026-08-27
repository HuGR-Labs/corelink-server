# WP-10: Dogfood, Load Testing, Documentation & GA Checklist

**Status:** `NOT_STARTED`  
**Owner:** All TLs  
**Depends On:** WP-01 through WP-09  
**Estimate:** 2 weeks  
**Priority:** P0 (Release Gate)

---

## 1. Objective

Validate the DevEnv campaign through internal dogfooding, load testing, and complete documentation for GA release.

**Companion docs (authoritative; WP-10 is a derivative, not a replacement):**
- `specs/_compliance/GA-GATE-CRITERIA.md` — 59 GA gate criteria across 6 tracks; all must be `READY` before T-0.
- `specs/_runbooks/RB-GA-CUTOVER.md` — canonical pre-cutover checklist; T-7d..T+7d orchestration.
- `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` — rollback tree (8 trigger rows).
- `specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md` — war-room logistics.
- `marketing/launch/LAUNCH-CHECKLIST-V2.md` — T-7d..T+72h row-level checklist (L0..L27+).
- `specs/_runbooks/RB-CHAOS-CAMPAIGN.md` + `RB-CHAOS-CATALOG.md` — chaos experiments (WP-10 failure-injection tests are DevEnv entries in this catalog).
- `CLAUDE.md` (root) — changelog gate (`feat:`/`fix:` → `[Unreleased]` entry) + DCO + merge hygiene.

---

## 2. Scope

### In Scope
- Internal dogfood program (2 weeks, 5+ engineers, ≤ 2h/engineer/day on the dogfooded surface)
- Load testing (concurrency, latency, failure injection; per `tests/load/k6/lib/cogs.js` COGS attribution; Prometheus remote-write)
- Documentation (`docs/devenv/` user guide; OpenAPI reference; `specs/_runbooks/RB-DEVENV-*.md` runbooks; `doc_status: ACTIVE` frontmatter enforced)
- GA release checklist **derivation page** — pointers to the 59-criteria `GA-GATE-CRITERIA.md` + `RB-GA-CUTOVER.md` §0 + `RB-GA-LAUNCH-ROLLBACK.md` §5
- DevEnv runner-tier pricing addendum (runner axis ONLY — distinct from cache tier; cite the runner-vs-cache separation ratified 2026-06-13 in `CHANGELOG.md` + migration `0070_runners_entitlement.sql`)

### Out of Scope
- Feature development (complete in WP-01 to WP-09)
- Cache-tier pricing or `tier_select.rs` checkout (owned by `corelink-tier-selection`; see `marketing/corelink-feature-catalog.html` cards under "Subscription lifecycle")
- Cross-WP: `tests/load/launch-day-projection.rs` (cited by `LAUNCH-CHECKLIST-V2.md:L3` + `GA-GATE-CRITERIA.md:GA-GATE-O09`; this WP flags it as a campaign-plan-level gap, raised in the Review's cross-WP coordination)
- Cross-WP (N4): GA-GATE-O10 denominator re-baselining when 5 DevEnv chaos rows land (gate text says "8 of 8", will be 13 of 13 after WP-10)
- Cross-WP (N5): GA-GATE-O06 text re-parameterisation from "All 47" to "all `RB-*.md`" (current count 55, becomes 62 after WP-10)
- Cross-WP (N6): OKF wiki DevEnv concept authoring (5 concepts, owner = TechLead, not in this WP's deliverable list — only the SOW/contract for the concepts is here)
- Cross-WP (N3): Runner entitlement provisioning for dogfooders (WP-07 owns `runners_entitlement` INSERT path; WP-10 only consumes it)

---

## 3. Dogfood Program (Week 1-2)

### 3.1 Participants & Workloads

| Engineer | Primary Workload | Dogfood Surface | Hours/Day | Runner Entitlement | Success Criteria |
|----------|------------------|------------------|-----------|---------------------|------------------|
| Backend 1 | Rust API development (cargo build/test) | Cold-start + build caching | 2h | dogfood-promo (1, 100h) | Cold start p95 < 30s; cargo cache > 80% hit |
| Backend 2 | Go microservices (go build/test) | Terminal reliability | 2h | dogfood-promo (1, 100h) | ttyd survives 8h; 0 disconnects |
| Frontend 1 | React/TypeScript (npm run dev/build) | code-server + terminal + browser | 2h | dogfood-promo (1, 100h) | Editor 100%; browser persistence 100% |
| Fullstack 1 | Python/TypeScript (pytest + vitest) | Multi-language workflow | 2h | dogfood-promo (1, 100h) | All language servers work |
| DevOps 1 | Terraform/Helm + CI debugging | Multi-device access (iOS 17+ Safari + Desktop Chrome) | 2h | dogfood-promo (1, 100h) | iOS 17+ WS subprotocol `binary` works |

**Total: ≤ 10h dogfood/engineer * 2 weeks = 100h team-wide** (fits alongside normal sprint work).

**Runner entitlement provisioning (N1-N3 + N1' fix):** All dogfooders start with the **default `free` cache + ZERO `runners_entitlement` row** (per `seedTenantEntitlements` policy of 2026-08-02; the function deliberately omits the row because the row IS the entitlement — absent row = reject; see doc-comment at `apps/signup-worker/src/lib/d1.ts` "Why NO `runners_entitlement` row"). **Three real provisioning paths** (in priority order):

  1. **Stripe test-mode checkout (preferred, canonical).** The corelink-runners TL creates a $0 test-mode subscription for the dogfooder using `STRIPE_PRICE_ID_RUNNER_STARTER` (env var on `corelink-signup-worker`); the already-shipped webhook handler at `apps/signup-worker/src/webhooks/stripe.ts` (`INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h)`) provisions the row with `plan='dogfood-promo', max_concurrency=1, max_vcpu_h=100`. This is the only path that uses the audited, test-mode code.
  2. **`wrangler d1 execute` ad-hoc (staging fallback).** If Stripe test-mode is unavailable, the TL runs one-shot: `wrangler d1 execute corelink --env staging --remote --command="INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h) VALUES ('<tenant_uuid>', 1, 'dogfood-promo', $(date +%s%3N), 100)"`. Column list matches migration 0070 (`tenant_id`, `max_concurrency`, `plan`, `created_at_ms`) + 0072 (`max_vcpu_h`); `max_vcpu_h` is nullable per 0072. **There is NO `apps/signup-worker/scripts/d1.py`** in HEAD (iter 2 N1' finding — verified `ls apps/signup-worker/` shows `{src, tests, package.json, wrangler.jsonc, README.md}` only); the only repo-root D1 helpers are `scripts/d1-migration-verify.py` (schema verifier, not an INSERT helper) + `scripts/d1-migration-runner.sh` (applies migrations).
  3. **`clw admin grant-runner` follow-up (does not exist today).** A future `tools/cli/src/commands/admin_grant_runner.ts` would be the CLI-shaped path; not a blocker for dogfood but worth raising as a follow-up so the corelink-runners TL has a real tool next time.

**No `max_vcpu` column** — the metering is `max_vcpu_h` (monthly vCPU-hours, same unit as the `$0.30/vCPU-h` overage rate from the 2026-08-02 billing changelog). Columns per migrations `0070_runners_entitlement.sql` + `0072_runners_entitlement_max_vcpu_h.sql`. The row is read by `runner_concurrency_for_tenant()` (per `marketing/corelink-feature-catalog.html` card "runner entitlement axis (separate from cache tier)"). **Do NOT modify `seedTenantEntitlements`** — that function's doc-comment explicitly documents why the row was removed. Escalation to higher quota: TL request → corelink-runners TL.

### 3.2 Dogfood Checklist (Per Engineer)

| Item | Target | Verification |
|------|--------|--------------|
| **Cold start** | < 30s | Stop → wait 1h → Start → timer |
| **Warm resume** | < 5s | Stop → immediate Start → timer |
| **Browser persistence** | 100% | Login Claude/Codex → stop → start → still logged in |
| **Terminal reliability** | 0 disconnects/8h | ttyd session survives 8h |
| **Editor functionality** | 100% | code-server: edit, save, git, terminal |
| **Build caching** | > 80% hit rate | cargo/npm/go builds use CAS |
| **Resize functionality** | 100% presets work | Apply each preset → verify |
| **Snapshot integrity** | SHA-256 of `/workspace/user` matches pre-snapshot hash | `clw ls --name <workspace> --ref-domain runner --json` + `clw status` (cite WP-06 invariant) |
| **Multi-device access** | Desktop + iOS 17+ Safari | iOS 17+ Safari (binary subprotocol supported) + Desktop Chrome |
| **Crash recovery** | Zero data loss | In-container `stress-ng --vm 1 --vm-bytes 80M --timeout 60s` under `MEMORY_LIMIT_MB=64` → OOM → onError fires → emergency snapshot → restart → state intact (NOT `docker kill -9` from outside the CF microVM) |

### 3.3 Dogfood Reporting Template

```markdown
# Dogfood Report — [Engineer Name] — [Date]

## Workload
- Primary language/framework:
- Hours used:
- Primary DevEnv ID:

## Checklist Results
| Item | Pass/Fail | Notes |
|------|-----------|-------|
| Cold start < 30s | | |
| Warm resume < 5s | | |
| Browser persistence | | |
| Terminal reliability | | |
| Editor functionality | | |
| Build cache hit rate | | |
| Resize functionality | | |
| Snapshot integrity (SHA-256) | | |
| Multi-device access (iOS 17+) | | |
| Crash recovery (stress-ng) | | |

## Issues Found
| Severity | Description | Steps to Reproduce | Workaround |
|----------|-------------|-------------------|------------|

## Feature Requests
| Priority | Description |
|----------|-------------|

## Overall Rating (1-10)
```

### 3.4 Dogfood Gate Criteria

| Metric | Threshold | Go/No-Go |
|--------|-----------|----------|
| Critical bugs (P0) | 0 | Required |
| Major bugs (P1) | ≤ 2 | Required |
| Cold start p95 | < 30s | Required |
| Warm resume p95 | < 5s | Required |
| Browser persistence | 100% | Required |
| Data loss incidents | 0 | Required |
| Engineer satisfaction | ≥ 8/10 avg | Required |

---

## 4. Load Testing (Week 2)

### 4.1 Test Scenarios

| Scenario | Description | Target |
|----------|-------------|--------|
| **Concurrent DevEnvs** | 50 tenants, 1 DevEnv each, all running | 50 concurrent Containers (must stay < 90% of CF Container account quota — currently 100) |
| **WebSocket concurrency** | 100 WebSocket connections (mixed vnc/tty/code) | 100 concurrent WS per DO (subject to CF DO outbound-TCP cap) |
| **Session duration** | 4-hour sessions with activity every 5min | 4h sustained |
| **Warm rapid start/stop** | 50 warm start/stop cycles in 1 hour (no container teardown) | 50 ops/hr |
| **Snapshot load** | 10GB workspace snapshots | 10GB upload/download |
| **Failure injection** | Container OOM, network partition, network partition recovery | Recovery < 60s; `onError` hook fires (assert via WP-06) |

**CF Container + DO constraints (verified before iteration):** account-level `cf:container:concurrent_instances < 90`; per-DO outbound-TCP cap = 6 active sockets; outbound WS message cap = 32 KiB. Iteration MUST verify these before the first ramp-up, or it will 503 mid-test.

### 4.2 Load Test Scripts (k6) — 2 scenarios, mirrors `endurance-24h-w22.js` patterns

**File:** `tests/load/k6/scenarios/devenv-concurrent.js` (new, lands in this WP)

```javascript
// Wave-DevEnv load test — concurrent DevEnvs + WebSocket storm.
//
// PURPOSE
// -------
// Two scenarios run in parallel for 5 minutes (smoke) / 4 hours (full):
//
//   1. `concurrent_devenvs`  — 50 distinct staging tenants, each creates 1 DevEnv
//      and holds a WS connection to /v1/customer/devenv/vnc for 4m. Exercises
//      tenant isolation, WS subprotocol forwarding (noVNC `binary`), cold start
//      variance, and the per-account CF Container quota at 50/100.
//
//   2. `websocket_storm`     — single DO, 100 sequential WS connections across
//      /vnc + /tty + /code, message round-trip, close. Exercises the DO's
//      hibernation API, the per-DO outbound TCP cap, and the WebSocket
//      subprotocol forwarding path (noVNC binary, ttyd text, code-server JSON).
//
// SAFETY RAILS (mirrored from endurance-24h.js:127-131)
//   * Refuses to run if K6_TARGET_HOST is not a `staging.*` / `dev.*` / 127.0.0.1
//     hostname — production hostname guard.
//   * Refuses to run for DURATION > 30s unless K6_ENDURANCE_CONFIRM=yes.
//   * Requires per-VU K6_AUTH_BEARER_<VU> (1..50) so each VU exercises a
//     DISTINCT staging tenant (NOT one shared PAT — that would defeat
//     INV-TENANT-ISOLATION and the "50 tenants" scenario).
//
// METRICS (custom k6 Trend/Counter/Gauge)
//   - `devenv_create_latency` per route
//   - `devenv_ws_connect_latency` per WS endpoint
//   - `devenv_create_total` / `devenv_resize_total` / `devenv_snapshot_total` / `devenv_stop_total`
//   - `devenv_active_dos` (gauge, polled by sidecar VU)
//   - COGS attribution: `r2_class_a_ops` / `r2_class_b_ops` via `lib/cogs.js`
//
// THRESHOLDS (load-test floors — drift on Grafana)
//   - `devenv_create_latency`: p99 ≤ 8000 ms
//   - `devenv_ws_connect_latency`: p99 ≤ 3000 ms
//   - global error rate: < 1% over 5-min rolling window
//   - `devenv_active_dos`: max ≤ 50 (do not exceed scenario target)
//
// USAGE (smoke / local 30s):
//   K6_TARGET_HOST=http://127.0.0.1:8787 \
//   DURATION=30s \
//   K6_ENDURANCE_CONFIRM=no \
//     k6 run tests/load/k6/scenarios/devenv-concurrent.js
//
// USAGE (CI nightly 2h):
//   K6_TARGET_HOST=https://staging.corelink.humangr.com \
//   K6_AUTH_BEARER_1=pat-tenant-01 ... K6_AUTH_BEARER_50=pat-tenant-50 \
//   DURATION=2h K6_ENDURANCE_CONFIRM=yes \
//   K6_PROMETHEUS_RW_SERVER_URL=https://prom-rw.staging.corelink.humangr.com/api/v1/write \
//     k6 run --out experimental-prometheus-rw \
//     tests/load/k6/scenarios/devenv-concurrent.js

import http from 'k6/http';
import ws from 'k6/ws';
import { check, sleep } from 'k6';
import { Counter, Trend, Gauge } from 'k6/metrics';
import { recordClassA, recordClassB } from '../lib/cogs.js';

const TARGET_HOST = __ENV.K6_TARGET_HOST || 'http://127.0.0.1:8787';
const DURATION = __ENV.DURATION || '30s';
const CONFIRM = __ENV.K6_ENDURANCE_CONFIRM || '';

// Production-hostname guard (mirrors endurance-24h.js:108-110).
if (/\.corelink\.humangr\.com$/.test(TARGET_HOST) && TARGET_HOST.indexOf('staging.') === -1) {
  throw new Error('refusing to run load test against production hostname');
}
const parseSec = (d) => {
  const m = /^(\d+)(s|m|h)$/.exec(d);
  if (!m) throw new Error(`DURATION must match /^\\d+(s|m|h)$/, got ${d}`);
  return m[2] === 'h' ? +m[1] * 3600 : m[2] === 'm' ? +m[1] * 60 : +m[1];
};
const TOTAL_S = parseSec(DURATION);
if (TOTAL_S > 30 && CONFIRM !== 'yes') {
  throw new Error(`K6_ENDURANCE_CONFIRM=yes required for DURATION>30s (got ${DURATION})`);
}

// Per-VU bearer token (REQUIRED for tenant isolation).
const vu = __VU;
const BEARER = __ENV[`K6_AUTH_BEARER_${vu}`] || __ENV.K6_AUTH_BEARER || '';
if (!BEARER) throw new Error(`K6_AUTH_BEARER_${vu} or K6_AUTH_BEARER required`);

// Custom metrics.
const createLatency = new Trend('devenv_create_latency');
const wsConnectLatency = new Trend('devenv_ws_connect_latency');
const createTotal = new Counter('devenv_create_total');
const resizeTotal = new Counter('devenv_resize_total');
const snapshotTotal = new Counter('devenv_snapshot_total');
const stopTotal = new Counter('devenv_stop_total');
const activeDOs = new Gauge('devenv_active_dos');

const authHeaders = () => ({
  'Authorization': `Bearer ${BEARER}`,
  'Content-Type': 'application/json',
});

export const options = {
  scenarios: {
    concurrent_devenvs: {
      executor: 'constant-vus',
      vus: 50,
      duration: DURATION,
      gracefulStop: '30s',
      tags: { scenario: 'concurrent_devenvs' },
    },
    websocket_storm: {
      executor: 'constant-vus',
      vus: 10,
      duration: DURATION,
      startTime: '30s',
      gracefulStop: '30s',
      tags: { scenario: 'websocket_storm' },
      env: { SCENARIO: 'ws_storm' },
    },
  },
  thresholds: {
    'devenv_create_latency': ['p(99)<8000'],
    'devenv_ws_connect_latency': ['p(99)<3000'],
    'devenv_create_total': ['count>0'],
    checks: ['rate>0.99'],
  },
};

export default function () {
  if (__ENV.SCENARIO === 'ws_storm') {
    wsStorm();
  } else {
    concurrentDevEnv();
  }
}

function concurrentDevEnv() {
  const tenantId = `t_load_${String(vu).padStart(3, '0')}`;
  const headers = authHeaders();

  // 1. Create DevEnv (gate downstream on check()).
  const t0 = Date.now();
  const createRes = http.post(
    `${TARGET_HOST}/v1/customer/devenv`,
    JSON.stringify({ workspace_name: `loadtest-${vu}-${__ITER}` }),
    { headers },
  );
  createLatency.add(Date.now() - t0);
  const ok = check(createRes, {
    'create status 201': (r) => r.status === 201,
  });
  if (!ok) {
    recordClassA(1);
    return;
  }
  createTotal.add(1);
  recordClassA(1);

  const devenv = createRes.json();
  const vncWs = devenv.vnc_url.replace(/^https/, 'wss');
  const ttyWs = devenv.tty_url.replace(/^https/, 'wss');

  // 2. WS connect (noVNC `binary` subprotocol — B4 fix).
  ws.connect(vncWs, { subprotocols: ['binary'] }, (socket) => {
    socket.on('open', () => {
      wsConnectLatency.add(1);
      check(socket, { 'vnc connected': () => true });
    });
    socket.on('message', (msg) => {
      // noVNC server emits a `binary`-framed banner; non-empty = subprotocol forwarded.
      check(socket, { 'vnc subprotocol forwarded': () => msg && msg.length > 0 });
    });
    socket.setTimeout(() => socket.close(), 5000);
  });

  // 3. Resize (3 iterations).
  for (let i = 0; i < 3; i++) {
    http.post(
      `${TARGET_HOST}/v1/customer/devenv/resize`,
      JSON.stringify({ width: 1920, height: 1080 }),
      { headers },
    );
    resizeTotal.add(1);
    sleep(2);
  }

  // 4. Snapshot.
  const snapRes = http.post(
    `${TARGET_HOST}/v1/customer/devenv/snapshot`,
    JSON.stringify({ force: true }),
    { headers },
  );
  if (check(snapRes, { 'snapshot 200': (r) => r.status === 200 })) {
    snapshotTotal.add(1);
    recordClassB(1);
  }

  // 5. Stop (use http.delete, not the deprecated http.del — B2.3).
  //    M9 fix: drop the `null` body arg for k6 http.delete consistency with http.post calls in this script.
  const stopRes = http.delete(`${TARGET_HOST}/v1/customer/devenv`, { headers });
  if (check(stopRes, { 'stop 200': (r) => r.status === 200 })) stopTotal.add(1);

  sleep(5 + Math.random() * 10);
  activeDOs.add(1);
}

function wsStorm() {
  const headers = authHeaders();
  // Pick a tenant that exists; reuse one from concurrent_devenvs.
  const listRes = http.get(`${TARGET_HOST}/v1/customer/devenv`, { headers });
  if (listRes.status !== 200) return;
  const list = listRes.json();
  if (!list.devenvs || list.devenvs.length === 0) return;
  const devenv = list.devenvs[0];
  const endpoints = [
    { name: 'vnc', url: devenv.vnc_url, subprotocols: ['binary'] },
    { name: 'tty', url: devenv.tty_url, subprotocols: [] },
    { name: 'code', url: devenv.code_url, subprotocols: [] },
  ];
  for (const ep of endpoints) {
    ws.connect(ep.url.replace(/^https/, 'wss'), { subprotocols: ep.subprotocols }, (s) => {
      s.on('open', () => check(s, { [`${ep.name} ws open`]: () => true }));
      // M8 fix: assert subprotocol-forwarded message non-empty (mirror concurrentDevEnv line 303-306)
      s.on('message', (msg) => check(s, { [`${ep.name} msg non-empty (subprotocol forwarded)`]: () => msg && msg.length > 0 }));
      s.setTimeout(() => s.close(), 3000);
    });
  }
}
```

### 4.3 Failure Injection Tests (also: DevEnv entries in `RB-CHAOS-CATALOG.md`)

| Test | Method | Expected Behavior | Blast Radius | Rollback |
|------|--------|-------------------|--------------|----------|
| Container OOM | `MEMORY_LIMIT_MB=64` + in-container `stress-ng --vm 1 --vm-bytes 80M --timeout 60s` (via `this.ctx.container.exec`) | `onError` fires → emergency snapshot → errored state; subsequent start restores | 1 DO; no cross-tenant | Kill `stress-ng`; `cf-do-evict <do-id>` via SRE-OC if no auto-recovery |
| Network partition | `tc qdisc add dev eth0 loss 100%` (in-container) | Health check fails → 3 failures → errored | 1 DO | `tc qdisc del dev eth0 root`; DO recovers on next request |
| Disk full | `dd if=/dev/zero of=/data/fill bs=1G count=19` (in-container) | Snapshot fails gracefully → error logged | 1 DO | `rm /data/fill`; snapshot retry |
| DO hibernation | No WebSocket for 5min | DO hibernates → wake on next request | 1 DO | Auto; no rollback needed |
| Concurrent snapshots | 2x manual + auto | Second fails with `SNAPSHOT_IN_PROGRESS` | 1 DO | Auto; second wins on retry |

### 4.4 Capacity Headroom Load Test (cross-WP gap)

`tests/load/launch-day-projection.rs` is **referenced by both `marketing/launch/LAUNCH-CHECKLIST-V2.md:L3` and `specs/_compliance/GA-GATE-CRITERIA.md:GA-GATE-O09` but does not exist today.** WP-10 flags this as a campaign-plan-level gap. The test must:

- Live at `tests/load/launch-day-projection.rs` (Rust; same harness style as `endurance-24h-*.js` companion)
- Replay a production-projected traffic mix (3× peak per S-13 capacity model) for 1h against staging
- Emit capacity-headroom proof: Workers / R2 / D1 utilization < 50% at peak
- Sign-off path: SRE Lead reads the run, archives evidence to `specs/_audits/2026-MM-DD-capacity-headroom.md`, and the GA-GATE-O09 row flips to `READY`
- **This WP does NOT own the implementation** (no Rust harness expertise + cross-crate surface); the campaign plan must assign WP-09-or-WP-10-or-new-WP owner before T-14d.

---

## 5. Documentation

### 5.1 User Guide (Markdown)

Path: `docs/devenv/` (new subtree; created by this WP).

**Every file in `docs/devenv/` MUST ship with `doc_status: ACTIVE` (or `FROZEN` for the API reference).** Verified by `python3 scripts/validate_specs.py` (target 463/0 → 473/0 after WP-10 lands; the 10 new files = +10).

```
docs/devenv/
├── quickstart.md           # 5-minute quickstart
├── concepts.md             # DevEnv, workspace, profile, snapshot
├── cli-reference.md        # clw commands for DevEnv
├── dashboard-guide.md      # UI walkthrough
├── connections.md          # noVNC, ttyd, code-server usage
├── snapshots.md            # Manual/auto snapshots, recovery (SHA-256 content-hash, NOT byte-identical)
├── resizing.md             # Viewport presets, custom sizes
├── troubleshooting.md      # Common issues + solutions
├── faq.md                  # Frequently asked questions
└── limits.md               # Tier limits, quotas, runner-vs-cache pricing split
```

### 5.2 API Reference (OpenAPI)

- Auto-generated from `devenvOpenApiSpec` (WP-08)
- **Auth gate decision required (H10 fix):** either gate `https://docs.corelink.humangr.com/api/devenv` behind Clerk session (recommended; minimal surface = minimal fingerprint) OR publish a scoped public read-only subset (paths documented, examples redacted, error schemas elided). Decision logged in §6.1 row "OpenAPI published".
- Includes: request/response examples, error codes, WebSocket protocol (`binary` subprotocol for noVNC)

### 5.3 Runbooks (Internal)

**Path:** `specs/_runbooks/` (repo convention; matches the 55 existing `RB-*.md` runbooks — verified via `ls specs/_runbooks/RB-*.md | wc -l` = 55, NOT repo-root `runbooks/` which does not exist).

```
specs/_runbooks/
├── RB-DEVENV-STUCK-STARTING.md
├── RB-DEVENV-WONT-STOP.md
├── RB-DEVENV-WEBSOCKET-CONNECTION-FAILS.md
├── RB-DEVENV-SNAPSHOT-CORRUPTION.md
├── RB-DEVENV-BILLING-DISCREPANCY.md
├── RB-DEVENV-QUOTA-EXCEEDED.md
└── RB-DEVENV-DISASTER-RECOVERY.md
```

**Frontmatter is MANDATORY** (per `validate_specs.py` schema + `RB-GA-CUTOVER.md:1-15` canonical pattern). The 7 runbooks above are ADDITIONS to the 55 existing `RB-*.md` runbooks; the new total is 62.

**N5 cross-WP issue (MUST be raised before T-7d):** `GA-GATE-CRITERIA.md:GA-GATE-O06` currently reads "All **47** runbooks `doc_status: FROZEN` or `ACTIVE` (none `DRAFT`)" — the count is stale (off by 8 from the 55 in HEAD today, off by 15 after the 7 DevEnv runbooks land). The gate text must be re-parameterised from "All 47 runbooks" to "All `RB-*.md` files in `specs/_runbooks/`" (regex-sweep) to be maintainable. Owner: SRE Lead + Compliance. This is a blocker for GA-GATE-O06 flipping to `READY` at T-7d.

### 5.4 Runbook Template (canonical frontmatter)

```markdown
---
id: "RB-DEVENV-STUCK-STARTING"
type: "runbook"
doc_status: "ACTIVE"          # or "FROZEN" once stabilized
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-MM-DD"
updated: "2026-MM-DD"
owner: "<corelink-runners TL>"
final_approver: "<TechLead>"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "devenv", "p1", "sre", "wave-devenv"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-DEVENV-STUCK-STARTING — [One-line summary]

> **Severity floor:** P1 (was P0 if customer-facing).
> **Detect → Acknowledge → Engage signers:** <one-line protocol>.
> **Companion docs:** [list].

---

## 0. Pre-flight (≤ 5 min)
1. ...

## 1. Symptoms
- User reports: ...
- Alerts: ...
- Logs: ...

## 2. Diagnosis
1. ...
2. ...

## 3. Resolution
### Option A: [Preferred]
Steps: ...

### Option B: [Alternative]
Steps: ...

## 4. Post-Incident
- Metrics to monitor: ...
- Follow-up ticket: ...
- Prevention: ...
```

**Cross-references the runbook MUST add:** `RB-CHAOS-CAMPAIGN.md` (if any failure-mode here is a known chaos experiment), `RB-INCIDENT-RESPONSE.md` (escalation), `ONCALL-ESCALATION-MATRIX.md` (paging tiers), `RB-GA-LAUNCH-ROLLBACK.md` (if the trigger is GA-blocking).

### 5.5 OKF Wiki Coverage (N6 — gap closure)

The `docs/knowledge/` OKF wiki (157 concepts, code-grounded, anti-drift gated) has **zero DevEnv concepts today** (verified: `rg -i "devenv|runner_devenv|wave-devenv" docs/knowledge/` = 0 matches). WP-10 owns the **following 5 concepts** (one per major surface area) to ground the campaign in the architecture wiki so a future engineer can `python3 scripts/okf_context.py --file <devenv-route>` and get a concept back:

| Concept ID | Title | `source_files` (must cite) |
|------------|-------|----------------------------|
| `dev-env/lifecycle` | `RunnerDevEnvDO` state machine + transitions | `corelink-runners/src/durable_objects/runner_dev_env.ts:280-312` + WP-01 §3.3 + WP-06 |
| `dev-env/ws-subprotocol-forwarding` | DO `proxyWebSocket` + Hibernation + noVNC `binary` | WP-01 `proxyWebSocket` + WP-05 (Worker DO contract) |
| `dev-env/snapshot-content-hash` | `clw ls --name <ws> --ref-domain runner` hash verification | WP-06 (snapshot impl) + WP-04 (clw library) |
| `dev-env/runner-vs-cache-axis` | `runners_entitlement` ≠ `tier_selections`; Option B 2026-06-13 | `migrations/d1/0070_runners_entitlement.sql` + `marketing/corelink-feature-catalog.html` "runner entitlement axis" card |
| `dev-env/tenant-quota-and-throttle` | Per-tenant `max_concurrency` gate at fabric + per-DO alarm | WP-07 (billing guard) + WP-08 (Worker `checkDevenvQuota`) |

Each concept MUST declare the implementing file(s) in its `source_files` field (per the OKF schema), MUST be cross-linked from the corresponding `RB-DEVENV-*.md` runbook, and MUST pass `python3 scripts/okf_validate.py` (no drift against HEAD). Drift gate C5 must stay green after these concepts land. Owner: TechLead (or a new WI) — not corelink-runners TL (no OKF authoring precedent in that team).

---

## 6. GA Release Checklist — DERIVATION from existing GA-gate machinery

**Authoritative sources (WP-10 does NOT re-implement; it derives):**
- `specs/_compliance/GA-GATE-CRITERIA.md` — 59 criteria × 6 tracks (Engineering / Security / Operations / Customer / Legal / Launch)
- `specs/_runbooks/RB-GA-CUTOVER.md` — 12-step pre-cutover checklist + cutover sequence
- `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` — 8 trigger rows
- `marketing/launch/LAUNCH-CHECKLIST-V2.md` — T-7d..T+72h row-level (L0..L27+)
- `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md` — Go/No-Go meeting template

**This WP owns the DevEnv-campaign-specific rows below; everything else derives upward.**

### 6.1 DevEnv-Campaign-Specific Sign-Offs

| Component | TL | Evidence Pointer | Date |
|-----------|-----|------------------|------|
| DevEnv load test green | corelink-runners | `tests/load/k6/scenarios/devenv-concurrent.js` last 7d run + Prometheus panel | |
| 7× `RB-DEVENV-*.md` runbooks landed + dry-run | corelink-runners | `specs/_runbooks/RB-DEVENV-*.md`; `GA-GATE-CRITERIA.md:GA-GATE-O06` | |
| `tests/load/launch-day-projection.rs` sign-off | SRE Lead | `GA-GATE-CRITERIA.md:GA-GATE-O09` evidence (`specs/_audits/2026-MM-DD-capacity-headroom.md`) | |
| DevEnv chaos entries in `RB-CHAOS-CATALOG.md` | SRE Lead | `GA-GATE-CRITERIA.md:GA-GATE-O10` (N4: gate text reads "8 of 8" but adding 5 DevEnv rows brings total to 13 — gate denominator MUST be re-baselined; this WP raises the cross-WP issue) | |
| `docs/devenv/` user guide published, `doc_status: ACTIVE` | Tech Writers | `python3 scripts/validate_specs.py` (target 463/0 → 473/0) | |
| DevEnv pricing addendum live | Marketing Lead | `marketing/launch/DEVENV-PRICING.md` (runner axis ONLY — distinct from cache tier) | |
| OpenAPI published (auth-gated or scope-restricted) | corelink-server | `https://docs.corelink.humangr.com/api/devenv` access control decision (H10) | |
| Dogfood report ≥ 8/10 avg | All TLs | `docs/devenv/dogfood/reports/` aggregated score | |

### 6.2 DevEnv-Specific Quality Gates (these are WP-10's; others derive upward)

| Gate | Status | Evidence |
|------|--------|----------|
| `tests/load/k6/scenarios/devenv-concurrent.js` smoke + 4h green | | `tests/load/results/<UTC-stamp>-<mode>/k6-summary.json` |
| 7× `RB-DEVENV-*.md` `doc_status: ACTIVE` + dry-run ≤ 30d | | `specs/_runbooks/` sweep |
| DevEnv chaos expts in `RB-CHAOS-CATALOG.md` last 30d | | `RB-CHAOS-CATALOG.md` + `GA-GATE-O10` |
| `docs/devenv/` frontmatter `doc_status: ACTIVE\|FROZEN` 100% | | `python3 scripts/validate_specs.py` |
| CHANGELOG `[Unreleased]` covers every WP-01..WP-10 `feat:` / `fix:` | | `CHANGELOG.md` (root) |
| DCO trailer on every commit | | `git log --format='%(trailers:key=Signed-off-by)'` |
| Zero P0 bugs (overall campaign) | | `gh issue list --label P0 --state open` |
| Zero P1 bugs (DevEnv surface) | | DevEnv-labeled issues |
| Cross-WP gap closed: `tests/load/launch-day-projection.rs` owned | | TechLead-issued owner assignment |
| **N5 cross-WP:** GA-GATE-O06 text re-parameterised "All 47" → "all `RB-*.md`" (N5) | | PR to `GA-GATE-CRITERIA.md` merged before T-7d |
| **N4 cross-WP:** GA-GATE-O10 denominator re-baselined from 8 to (8 + 5 DevEnv) before DevEnv rows ship | | PR to `GA-GATE-CRITERIA.md` merged before DevEnv chaos catalog patch |
| **N6:** ≥ 5 DevEnv OKF concepts authored (`dev-env/lifecycle`, `dev-env/ws-subprotocol-forwarding`, `dev-env/snapshot-content-hash`, `dev-env/runner-vs-cache-axis`, `dev-env/tenant-quota-and-throttle`) | | `python3 scripts/okf_validate.py` exit 0; C5 drift gate green |
| **N3 + N1':** Runner entitlement provisioning path for dogfooders documented AND working: Stripe test-mode checkout (preferred — uses `apps/signup-worker/src/webhooks/stripe.ts` which writes the row) OR `wrangler d1 execute` ad-hoc (fallback) | | One dogfooder successfully started + held a DevEnv end-to-end before T-14d; INSERT path matches migration 0070+0072 column list (`tenant_id`, `max_concurrency`, `plan`, `created_at_ms`, `max_vcpu_h`) |

### 6.3 Infrastructure Readiness (cross-WP; cite upstream)

| Item | Owner (UPSTREAM) | Pointer |
|------|------------------|---------|
| Cloudflare Container quota increased (≥ 150 active) | Platform / SRE Lead | CF account ticket; not WP-10's job to file |
| D1 migrations applied to prod | SRE Lead | `RB-D1-MIGRATION-APPLY` + `migrations/applied.json` (per `GA-GATE-E15`) |
| Secrets rotated (billing keys) | Security Lead | `docs/internal/secrets-checklist.md` + `GA-GATE-S06` |
| DNS records for API | Platform | existing ops doc |
| TLS certificates valid | Platform | existing ops doc |
| Monitoring alerts configured | SRE Lead | `GA-GATE-O05` (status page) + `GA-GATE-O07` (synthetic page) |
| Log retention set (90 days) | SRE Lead | existing ops doc |

### 6.4 Rollback Plan (cross-WP; defer to authoritative)

**Defer entirely to `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md`** (8 trigger rows + reverse cutover). WP-10's DevEnv-specific additions:

| Trigger | Action | Owner | ETA |
|---------|--------|-------|-----|
| DevEnv P0 bug in prod | `cf-worker-flag disable DEVENV_ENABLED` (or equivalent, per the feature-flag mechanism WP-08 specifies) | Platform | 5min |
| DevEnv billing mis-attribution | Pause runner-axis billing ingest only (NOT cache-tier, per `CHANGELOG.md:528` fix that scopes revocation to the specific subscription) | Billing | 10min |
| DevEnv WS subprotocol regression (noVNC `binary` stripped) | Disable `RB-DEVENV-WEBSOCKET-CONNECTION-FAILS` mitigation; revert WP-05 | Platform | 5min |
| DevEnv snapshot corruption | Restore from CAS snapshots (per WP-06) | Infra | 30min |

---

## 7. Launch Timeline

| Date | Milestone | Owner |
|------|-----------|-------|
| T-14d | Dogfood starts | All engineers |
| T-7d | Dogfood ends; load test starts | QA/Platform |
| T-5d | Load test complete; bugs triaged | All TLs |
| T-3d | Documentation freeze | Tech Writers |
| T-2d | GA checklist complete | TechLead |
| T-1d | Final sign-offs | All TLs |
| **T-0** | **GA RELEASE** | **All** |
| T+1d | Post-launch monitoring | Platform |
| T+7d | Post-launch retrospective | All |

---

## 8. Post-Launch Metrics (First 30 Days)

| Metric | Target | Dashboard |
|--------|--------|-----------|
| DevEnv adoption | > 20% of Runner tier users | Mixpanel |
| Session duration | > 2h avg | Custom |
| Cold start p95 | < 30s | Grafana |
| Warm resume p95 | < 5s | Grafana |
| Browser persistence | 100% | Custom |
| Snapshot success rate | > 99.9% | Grafana |
| Billing accuracy | 100% match CF invoice | Internal |
| Support tickets | < 5/day | Zendesk |
| NPS score | > 50 | `marketing/lighthouse-kit/` Day+3 / Day+30 NPS pulse (cite the survey mechanism) |

---

## 9. Sign-Off

| Role | Name | Signature | Date |
|------|------|-----------|------|
| TechLead (Orchestrator) | | | |
| corelink-runners TL | | | |
| corelink-workspaces TL | | | |
| corelink-server TL | | | |
| Frontend TL | | | |
| Platform/Infra TL | | | |
| Security TL | | | |
| Finance/Billing TL | | | |

---

## 10. Appendices

### Appendix A: Dogfood Participant Onboarding

```bash
# Quick start for dogfooders
curl -fsSL https://corelink.humangr.com/install-devenv.sh | bash
clw auth login
# Create DevEnv via API or dashboard:
curl -X POST https://corelink-api.humangr.com/v1/customer/devenv \
  -H "Authorization: Bearer $CORELINK_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"workspace_name": "my-workspace"}'
# Opens dashboard → click "Open" → start coding
```

### Appendix B: Load Test Execution (via wrapper script)

**Do NOT invoke raw k6.** The repo-canonical pattern is `scripts/run_24h_endurance.sh` (smoke / dressrun / nightly / full modes + summary export). This WP lands `scripts/run_devenv_load.sh` with the same shape.

```bash
# Run load test
bash scripts/run_devenv_load.sh smoke     # 30s local smoke
bash scripts/run_devenv_load.sh dressrun  # 10min wave dress-run
bash scripts/run_devenv_load.sh nightly   # 2h CI variant
bash scripts/run_devenv_load.sh full      # 4h manual drill (PD-paged)
```

**Required env (full / nightly):**
- `K6_TARGET_HOST` e.g. `https://staging.corelink.humangr.com`
- `K6_AUTH_BEARER_1` ... `K6_AUTH_BEARER_50` (one staging PAT per VU, scoped to distinct tenants — INV-TENANT-ISOLATION)
- `K6_PROMETHEUS_RW_SERVER_URL` (e.g. `https://prom-rw.staging.corelink.humangr.com/api/v1/write`)
- `K6_ENDURANCE_CONFIRM=yes` (required for any DURATION > 30s)

**Output layout (mirrors `run_24h_endurance.sh`):**
```
tests/load/results/<UTC-stamp>-<mode>/
├── k6-summary.json
├── k6-stdout.log
├── run-meta.json
└── analysis.md  (after scripts/analyze_devenv_load.py)
```

### Appendix C: Emergency Contacts

| Role | Name | Phone | Slack |
|------|------|-------|-------|
| TechLead | | | |
| Platform On-Call | | | |
| Billing On-Call | | | |
| Security On-Call | | | |

### Appendix D: Print-Ready Dogfood Checklist (1 page)

```
┌─────────────────────────────────────────────────────────────────────────┐
│  CoreLink DevEnv — Daily Dogfood Checklist                              │
│  Date: __________   Engineer: ________________   Entitlement: dogfood-promo (1 concurrent / 100 vCPU-h monthly)│
│  DevEnv ID: __________                                                   │
├─────────────────────────────────────────────────────────────────────────┤
│ ☐ Cold start < 30s       (Stop → wait 1h → Start → timer = ____s)        │
│ ☐ Warm resume < 5s       (Stop → immediate Start → timer = ____s)        │
│ ☐ Browser persistence    (Login Claude/Codex → stop → start → still in)  │
│ ☐ Terminal reliability   (ttyd session survives 8h, 0 disconnects)       │
│ ☐ Editor functionality   (code-server: edit, save, git, terminal)        │
│ ☐ Build caching          (cargo/npm/go hit rate: ____%)                  │
│ ☐ Resize functionality   (Apply each preset → verify)                    │
│ ☐ Snapshot integrity     (clw ls / clw status → SHA-256 OK)              │
│ ☐ Multi-device access    (iOS 17+ Safari + Desktop Chrome WS works)     │
│ ☐ Crash recovery         (stress-ng OOM → onError → snapshot → restart) │
├─────────────────────────────────────────────────────────────────────────┤
│ Issues found today:                                                     │
│   Severity | Description | Steps | Workaround                           │
│   ________ | ____________ | ______ | ___________                        │
├─────────────────────────────────────────────────────────────────────────┤
│ Feature requests:                                                       │
│   Priority | Description                                                │
│   ________ | ____________                                              │
├─────────────────────────────────────────────────────────────────────────┤
│ Overall rating today (1-10): ____                                       │
└─────────────────────────────────────────────────────────────────────────┘
```

---

**END OF WP-10 — END OF CAMPAIGN PLAN**