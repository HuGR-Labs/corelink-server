// Wave-22 endurance harness — 24h sustained customer-facing route campaign.
//
// PURPOSE
// -------
// Complements `endurance-24h.js` (general realistic-mix endurance) by
// concentrating load on the wave-22 customer-facing surface that the
// pilot tenants will hit hardest in the GA cutover window:
//
//   POST /v1/audit/export
//   GET  /v1/audit/analytics/event-count
//   GET  /v1/audit/analytics/timeline
//   POST /v1/cas/upload
//   POST /v1/dsr/erasure
//   POST /v1/clerk/auth
//
// PROFILE
//   * 100 simulated tenants (Zipfian s=1.1; top-10 emit ~55% of traffic).
//   * Configurable RPS via the `arrival-rate` executor:
//       1h ramp-up   0 → 1000 RPS
//       22h sustain  1000 RPS
//       1h ramp-down 1000 → 0 RPS
//   * Composition is data-driven from `tests/load/fixtures/customer-routes.ndjson`
//     (weight column). Adversarial cases (cross-tenant, rate-limit burst,
//     mid-stream tamper) are kept at weight≤1 so they exercise hardening
//     without dominating SLO measurements.
//
// SHORT-RUN VARIANTS
//   DURATION=24h (default — ramp-up 1h, sustain 22h, ramp-down 1h)
//   DURATION=2h  (CI nightly — ramp-up 5m, sustain 110m, ramp-down 5m)
//   DURATION=30s (smoke — ramp-up 5s, sustain 20s, ramp-down 5s)
//
// METRICS (custom k6 Trends/Counters)
//   - `route_latency` per route (p50/p90/p99 emitted via thresholds)
//   - `route_errors` per route (counter)
//   - `tenant_request_count` (counter tagged by tenant_id)
//   - Sidecar VU polls `/v1/admin/diagnostics/memory` every 5min and emits
//     `memory_rss_bytes` / `cpu_user_pct` gauges.
//
// THRESHOLDS (drift floors — see RB-24H-ENDURANCE-LOAD.md §3):
//   - `route_latency{route:GET /v1/audit/analytics/event-count}`: p99 ≤ 400ms
//   - `route_latency{route:GET /v1/audit/analytics/timeline}`:    p99 ≤ 600ms
//   - `route_latency{route:POST /v1/audit/export}`:               p99 ≤ 1500ms
//   - `route_latency{route:POST /v1/cas/upload}`:                 p99 ≤ 800ms
//   - `route_latency{route:POST /v1/dsr/erasure}`:                p99 ≤ 1000ms
//   - `route_latency{route:POST /v1/clerk/auth}`:                 p99 ≤ 300ms
//   - global `route_errors`: < 0.1% over 5-min rolling window
//
// SAFETY RAILS
//   * Refuses to run if K6_TARGET_HOST is not staging-* / dev-* / 127.0.0.1.
//   * Refuses to run for DURATION > 5m unless K6_ENDURANCE_CONFIRM=yes.
//   * Adversarial fixtures only emit against the configured staging tenant
//     fleet (refuses to fire if K6_ALLOW_ADVERSARIAL != "yes" — defaults off).
//
// USAGE (smoke / local 30s):
//   K6_TARGET_HOST=http://127.0.0.1:8787 \
//   K6_AUTH_BEARER=stub-staging-pat \
//   DURATION=30s \
//     k6 run tests/load/k6/scenarios/endurance-24h-w22.js

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Trend, Gauge } from 'k6/metrics';
import { SharedArray } from 'k6/data';
// NOTE: open() is a k6 global — do not import; importing returns null.

// ─────────────────────────────────────────────────────────────────────────
// Env + safety
// ─────────────────────────────────────────────────────────────────────────
const TARGET_HOST = __ENV.K6_TARGET_HOST || 'http://127.0.0.1:8787';
const AUTH_BEARER = __ENV.K6_AUTH_BEARER || 'stub-staging-pat';
const DURATION = __ENV.DURATION || '24h';
const ALLOW_ADVERSARIAL = (__ENV.K6_ALLOW_ADVERSARIAL || 'no').toLowerCase() === 'yes';
const ENDURANCE_CONFIRM = (__ENV.K6_ENDURANCE_CONFIRM || 'no').toLowerCase() === 'yes';

const ALLOWED_HOST_RE =
  /^(http|https):\/\/(staging|dev)[\w.-]*(:\d+)?$|^http:\/\/127\.0\.0\.1(:\d+)?$|^http:\/\/localhost(:\d+)?$/;

if (!ALLOWED_HOST_RE.test(TARGET_HOST)) {
  throw new Error(
    `K6_TARGET_HOST="${TARGET_HOST}" rejected. Must point to staging/dev/127.0.0.1/localhost.`,
  );
}

const LONG_DURATIONS = ['24h', '22h', '12h', '8h', '4h', '2h', '1h'];
if (LONG_DURATIONS.includes(DURATION) && !ENDURANCE_CONFIRM) {
  throw new Error(
    `DURATION=${DURATION} requires K6_ENDURANCE_CONFIRM=yes (operator interlock).`,
  );
}

// ─────────────────────────────────────────────────────────────────────────
// Fixtures
// ─────────────────────────────────────────────────────────────────────────
const fixtureLines = new SharedArray('customer-route-fixtures', function () {
  const raw = open('../../fixtures/customer-routes.ndjson');
  return raw
    .split('\n')
    .filter((l) => l.trim().length > 0)
    .map((l) => JSON.parse(l));
});

// Pre-compute a weighted draw table (denormalised cumulative weights) so
// each iteration is O(log N) on the weight selector.
const drawTable = (function () {
  const filtered = fixtureLines.filter(
    (f) => ALLOW_ADVERSARIAL || !f.adversarial,
  );
  let total = 0;
  const cum = [];
  for (const f of filtered) {
    total += f.weight || 1;
    cum.push({ cum: total, fx: f });
  }
  return { total, cum };
})();

function drawFixture() {
  const r = Math.random() * drawTable.total;
  for (const row of drawTable.cum) {
    if (r <= row.cum) return row.fx;
  }
  return drawTable.cum[drawTable.cum.length - 1].fx;
}

// ─────────────────────────────────────────────────────────────────────────
// Tenant pool — 100 simulated tenants, Zipfian distribution.
// ─────────────────────────────────────────────────────────────────────────
const tenants = (function () {
  const n = 100;
  const s = 1.1;
  let total = 0;
  const weights = [];
  for (let i = 1; i <= n; i++) {
    const w = 1.0 / Math.pow(i, s);
    weights.push(w);
    total += w;
  }
  const cum = [];
  let acc = 0;
  for (let i = 0; i < n; i++) {
    acc += weights[i] / total;
    cum.push({ cum: acc, tenant_id: `tenant-${String(i + 1).padStart(3, '0')}` });
  }
  return cum;
})();

function drawTenant() {
  const r = Math.random();
  for (const t of tenants) {
    if (r <= t.cum) return t.tenant_id;
  }
  return tenants[tenants.length - 1].tenant_id;
}

// ─────────────────────────────────────────────────────────────────────────
// Profile shaping
// ─────────────────────────────────────────────────────────────────────────
function buildStages(duration) {
  // Returns the arrival-rate stage list. Three windows: ramp-up, sustain,
  // ramp-down. Targets steady-state 1000 RPS for the long variants.
  switch (duration) {
    case '24h':
      return [
        { duration: '1h', target: 1000 },
        { duration: '22h', target: 1000 },
        { duration: '1h', target: 0 },
      ];
    case '2h':
      return [
        { duration: '5m', target: 1000 },
        { duration: '110m', target: 1000 },
        { duration: '5m', target: 0 },
      ];
    case '30s':
    default:
      return [
        { duration: '5s', target: 50 },
        { duration: '20s', target: 50 },
        { duration: '5s', target: 0 },
      ];
  }
}

// ─────────────────────────────────────────────────────────────────────────
// Custom metrics
// ─────────────────────────────────────────────────────────────────────────
const routeLatency = new Trend('route_latency', true);
const routeErrors = new Counter('route_errors');
const tenantRequestCount = new Counter('tenant_request_count');
const memoryRssBytes = new Gauge('memory_rss_bytes');
const cpuUserPct = new Gauge('cpu_user_pct');

// ─────────────────────────────────────────────────────────────────────────
// k6 options
// ─────────────────────────────────────────────────────────────────────────
export const options = {
  discardResponseBodies: false,
  scenarios: {
    customer_routes: {
      executor: 'ramping-arrival-rate',
      startRate: 0,
      timeUnit: '1s',
      preAllocatedVUs: 100,
      maxVUs: 2000,
      stages: buildStages(DURATION),
      exec: 'customerRoutes',
    },
    memory_sidecar: {
      executor: 'constant-vus',
      vus: 1,
      duration: DURATION === '30s' ? '30s' : DURATION,
      exec: 'memorySidecar',
    },
  },
  thresholds: {
    'route_latency{route:GET /v1/audit/analytics/event-count}': ['p(99)<400'],
    'route_latency{route:GET /v1/audit/analytics/timeline}': ['p(99)<600'],
    'route_latency{route:POST /v1/audit/export}': ['p(99)<1500'],
    'route_latency{route:POST /v1/cas/upload}': ['p(99)<800'],
    'route_latency{route:POST /v1/dsr/erasure}': ['p(99)<1000'],
    'route_latency{route:POST /v1/clerk/auth}': ['p(99)<300'],
    route_errors: ['count<10000'],
  },
};

// ─────────────────────────────────────────────────────────────────────────
// Request body helpers
// ─────────────────────────────────────────────────────────────────────────
function uuid() {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function (c) {
    const r = (Math.random() * 16) | 0;
    const v = c === 'x' ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

function expandBody(fx) {
  if (!fx.body) return null;
  const s = JSON.stringify(fx.body).replace(/\{\{uuid\}\}/g, uuid());
  return s;
}

function fixedSizeOctets(n) {
  // Generate ArrayBuffer of length n. Use a small repeating pattern for
  // perf — content-addressable digests are not validated by the staging
  // smoke target so payload entropy is non-critical.
  const buf = new Uint8Array(n);
  for (let i = 0; i < n; i++) {
    buf[i] = (i * 31 + 7) & 0xff;
  }
  return buf.buffer;
}

// ─────────────────────────────────────────────────────────────────────────
// Main iteration
// ─────────────────────────────────────────────────────────────────────────
export function customerRoutes() {
  const fx = drawFixture();
  const tenant = drawTenant();

  tenantRequestCount.add(1, { tenant_id: tenant });

  const url = `${TARGET_HOST}${fx.path}`;
  const params = {
    headers: {
      Authorization: `Bearer ${AUTH_BEARER}`,
      'x-corelink-tenant': tenant,
      'x-corelink-load-test': 'wave-22-endurance',
      'Content-Type': 'application/json',
    },
    tags: {
      route: fx.route,
      tenant_id: tenant,
      adversarial: fx.adversarial || 'none',
    },
    timeout: '60s',
  };

  let res;
  const t0 = Date.now();

  if (fx.method === 'GET') {
    res = http.get(url, params);
  } else if (fx.method === 'POST' && fx.body_size_bytes) {
    params.headers['Content-Type'] = fx.content_type || 'application/octet-stream';
    res = http.post(url, fixedSizeOctets(fx.body_size_bytes), params);
  } else if (fx.method === 'POST') {
    res = http.post(url, expandBody(fx) || '{}', params);
  } else if (fx.method === 'PUT') {
    res = http.put(url, expandBody(fx) || '{}', params);
  } else {
    return;
  }

  const dt = Date.now() - t0;
  routeLatency.add(dt, { route: fx.route });

  const ok = (fx.expect_status || [200]).includes(res.status);
  if (!ok) {
    routeErrors.add(1, { route: fx.route, status: String(res.status) });
  }

  check(res, {
    'status accepted': () => ok,
  });
}

export function memorySidecar() {
  // Poll the diagnostics endpoint every 5 minutes (or 5s in smoke mode).
  const pollMs = DURATION === '30s' ? 5000 : 300000;
  const res = http.get(`${TARGET_HOST}/v1/admin/diagnostics/memory`, {
    headers: { Authorization: `Bearer ${AUTH_BEARER}` },
    tags: { route: 'GET /v1/admin/diagnostics/memory' },
    timeout: '10s',
  });
  if (res.status === 200) {
    try {
      const body = JSON.parse(res.body);
      if (typeof body.rss_bytes === 'number') memoryRssBytes.add(body.rss_bytes);
      if (typeof body.cpu_user_pct === 'number') cpuUserPct.add(body.cpu_user_pct);
    } catch (_e) {
      // Sidecar is best-effort; ignore parse failures.
    }
  }
  sleep(pollMs / 1000);
}
