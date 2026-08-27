// Wave-DevEnv load test — concurrent DevEnvs + WebSocket storm.
import http from 'k6/http';
import ws from 'k6/ws';
import { check, sleep } from 'k6';
import { Counter, Trend, Gauge } from 'k6/metrics';

const TARGET_HOST = __ENV.K6_TARGET_HOST || 'http://127.0.0.1:8787';
const DURATION = __ENV.DURATION || '30s';
const CONFIRM = __ENV.K6_ENDURANCE_CONFIRM || '';

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

const vu = __VU;
const BEARER = __ENV[`K6_AUTH_BEARER_${vu}`] || __ENV.K6_AUTH_BEARER || 'test-pat-token';

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
      vus: 10,
      duration: DURATION,
      gracefulStop: '30s',
      tags: { scenario: 'concurrent_devenvs' },
    },
    websocket_storm: {
      executor: 'constant-vus',
      vus: 5,
      duration: DURATION,
      startTime: '10s',
      gracefulStop: '30s',
      tags: { scenario: 'websocket_storm' },
      env: { SCENARIO: 'ws_storm' },
    },
  },
  thresholds: {
    'devenv_create_latency': ['p(99)<8000'],
    'devenv_ws_connect_latency': ['p(99)<3000'],
    'devenv_create_total': ['count>0'],
    checks: ['rate>0.95'],
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
  const headers = authHeaders();

  const t0 = Date.now();
  const createRes = http.post(
    `${TARGET_HOST}/v1/customer/devenv`,
    JSON.stringify({ workspace_name: `loadtest-${vu}-${__ITER}` }),
    { headers },
  );
  createLatency.add(Date.now() - t0);
  const ok = check(createRes, {
    'create status 201 or 200': (r) => r.status === 201 || r.status === 200,
  });
  if (!ok) return;
  createTotal.add(1);

  const devenv = createRes.json();
  if (devenv && devenv.vnc_url) {
    const vncWs = devenv.vnc_url.replace(/^http/, 'ws');
    ws.connect(vncWs, { subprotocols: ['binary'] }, (socket) => {
      socket.on('open', () => {
        wsConnectLatency.add(1);
        check(socket, { 'vnc connected': () => true });
      });
      socket.on('message', (msg) => {
        check(socket, { 'vnc subprotocol forwarded': () => msg && msg.length > 0 });
      });
      socket.setTimeout(() => socket.close(), 2000);
    });
  }

  http.post(
    `${TARGET_HOST}/v1/customer/devenv/resize`,
    JSON.stringify({ width: 1920, height: 1080 }),
    { headers },
  );
  resizeTotal.add(1);
  sleep(1);

  const snapRes = http.post(
    `${TARGET_HOST}/v1/customer/devenv/snapshot`,
    JSON.stringify({ force: true }),
    { headers },
  );
  if (check(snapRes, { 'snapshot 200': (r) => r.status === 200 })) {
    snapshotTotal.add(1);
  }

  const stopRes = http.del(`${TARGET_HOST}/v1/customer/devenv`, null, { headers });
  if (check(stopRes, { 'stop 200': (r) => r.status === 200 })) {
    stopTotal.add(1);
  }

  sleep(2);
  activeDOs.add(1);
}

function wsStorm() {
  const headers = authHeaders();
  const listRes = http.get(`${TARGET_HOST}/v1/customer/devenv`, { headers });
  if (listRes.status !== 200) return;
  const devenv = listRes.json();
  if (!devenv || !devenv.vnc_url) return;

  const endpoints = [
    { name: 'vnc', url: devenv.vnc_url, subprotocols: ['binary'] },
    { name: 'tty', url: devenv.tty_url, subprotocols: [] },
    { name: 'code', url: devenv.code_url, subprotocols: [] },
  ];
  for (const ep of endpoints) {
    if (!ep.url) continue;
    ws.connect(ep.url.replace(/^http/, 'ws'), { subprotocols: ep.subprotocols }, (s) => {
      s.on('open', () => check(s, { [`${ep.name} ws open`]: () => true }));
      s.on('message', (msg) => check(s, { [`${ep.name} msg non-empty`]: () => msg && msg.length > 0 }));
      s.setTimeout(() => s.close(), 1000);
    });
  }
}
