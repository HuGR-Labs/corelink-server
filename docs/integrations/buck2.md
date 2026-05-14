# Buck2 + CoreLink Integration Guide

**Audience:** Build engineers, platform teams, and developers using Buck2 who
want to connect their builds to CoreLink's shared remote cache.

---

## Table of contents

1. [Architecture overview](#architecture-overview)
2. [.buckconfig reference](#buckconfig-reference)
3. [Authentication](#authentication)
4. [Starter project](#starter-project)
5. [CI integration](#ci-integration)
6. [Troubleshooting](#troubleshooting)
7. [Performance tips](#performance-tips)
8. [Migration from Bazel](#migration-from-bazel)

---

## Architecture overview

```
  Developer machine / CI runner
  ┌─────────────────────────────┐
  │  buck2 build :target        │
  │                             │
  │  ┌─────────────────────┐   │
  │  │  Buck2 REAPI v2      │   │
  │  │  remote-cache client │   │
  │  └────────┬────────────┘   │
  └───────────┼─────────────────┘
              │ HTTPS  (REAPI v2 / HTTP/2)
              │ Authorization: Bearer ${CORELINK_PAT}
  ┌───────────▼─────────────────────────┐
  │  CoreLink CAS (Content-Addressable  │
  │  Storage) — REAPI v2 compliant      │
  │                                     │
  │  Digest algorithm: BLAKE3           │
  │  Tenant isolation: per-PAT          │
  │  Region: <tenant>.region.corelink   │
  └─────────────────────────────────────┘
```

Buck2's built-in REAPI v2 client communicates with CoreLink's CAS endpoint
over HTTPS.  Every downloaded artifact is verified with **BLAKE3** on the
client side (CTRL-CAS-002 — client verify default-on).

### Key properties

| Property | Value |
|---|---|
| Protocol | REAPI v2 (HTTP/2 + Protobuf) |
| Digest algorithm | BLAKE3 |
| Auth | Bearer token (PAT) injected via environment |
| Tenant isolation | Per-PAT; namespace derived from token |
| Compression | Enabled by default |
| Retries | Built-in (configurable via `[remote_cache]`) |

---

## .buckconfig reference

Place `.buckconfig` in the root of your Buck2 project.

```ini
# ── remote cache ─────────────────────────────────────────────────────────────
[remote_cache]
# CoreLink CAS HTTP endpoint (REAPI v2).
url = https://corelink.dev/v1/cache

# PAT injected from environment — never hardcode.
http_headers = Authorization: Bearer ${CORELINK_PAT}

# Enable both reads and writes.
read = true
write = true

# Retry transient network failures (FM-150).
retry_timeout_secs = 60
max_retries = 3

# Compress uploads.
http_header_prefix = x-corelink-client-version: buck2

# ── REAPI v2 client (for full RE API surface) ─────────────────────────────────
[buck2_re_client]
remote_cache_address = https://corelink.dev/v1/cache
http_headers = Authorization: Bearer ${CORELINK_PAT}

# ── client-side digest verification (CTRL-CAS-002) ────────────────────────────
[client]
hash_algorithm = BLAKE3
```

### All supported `[remote_cache]` options

| Key | Default | Description |
|---|---|---|
| `url` | — | CoreLink CAS endpoint URL. Override per-environment. |
| `http_headers` | — | HTTP headers appended to every request. Use for auth. |
| `read` | `true` | Fetch artefacts from remote cache. |
| `write` | `true` | Upload artefacts to remote cache. |
| `retry_timeout_secs` | `30` | Timeout per retry attempt. |
| `max_retries` | `3` | Maximum retry count on transient failure. |
| `http_header_prefix` | — | Prefix for custom headers (observability). |
| `tls_cert_path` | — | Path to client TLS cert (mTLS setups). |
| `tls_key_path` | — | Path to client TLS key. |
| `tls_ca_cert_path` | — | Path to custom CA bundle. |

---

## Authentication

### Using an environment variable (recommended)

```bash
export CORELINK_PAT=corelink_pat_...
buck2 build :target
```

The `.buckconfig` reads `${CORELINK_PAT}` via Shell interpolation.  This is
the **only approved pattern** (CTRL-CRED-001).

### Using a CI secret

GitHub Actions example:

```yaml
- name: Build with CoreLink cache
  run: buck2 build :target
  env:
    CORELINK_PAT: ${{ secrets.CORELINK_PAT }}
```

GitLab CI example:

```yaml
build:
  script:
    - buck2 build :target
  variables:
    CORELINK_PAT: $CORELINK_PAT   # from CI/CD variable
```

### Getting a PAT

1. Sign up at <https://corelink.dev>.
2. Navigate to **Settings → API Tokens → New token**.
3. Select scopes: `cache:read cache:write`.
4. Copy the token and store it in your secrets manager.

### Diagnosing auth issues

```bash
# Requires CoreLink CLI (WI-S15-001)
corelink doctor

# Or inspect Buck2 verbose output
buck2 build :target -v 2 2>&1 | grep -iE "auth|401|403|header"
```

---

## Starter project

The [`examples/buck2-starter/`](../../examples/buck2-starter/) directory
contains a minimal hello-world + 1 transitive dep project ready to run.

**Files:**

```
examples/buck2-starter/
├── .buckconfig          # Remote cache config (this guide)
├── .gitignore           # Buck2-specific ignores (buck-out/, .buck2d)
├── BUCK                 # cxx_library(:greeter) + cxx_binary(:hello)
├── BENCHMARK.md         # Auto-updated benchmark results
├── README.md            # Step-by-step ≤ 5 min setup
├── greeter.cc
├── greeter.h
├── main.cc
└── scripts/
    └── benchmark.sh     # 10 cold + 10 warm iteration benchmark
```

**Quick start (≤ 5 min):**

```bash
export CORELINK_PAT=<your-token>
git clone https://github.com/humangr-labs/corelink-server.git
cd corelink-server/examples/buck2-starter
buck2 build :hello          # cold build — populates remote cache
buck2 clean
buck2 build :hello          # warm build — ≤ 30 s, ≥ 80 % cache hits
```

---

## CI integration

The workflow [`.github/workflows/buck2-starter-ci.yml`](../../.github/workflows/buck2-starter-ci.yml) validates:

1. Cold build succeeds.
2. Warm build (post `buck2 clean`) completes in ≤ 30 s with ≥ 80 % cache hits.
3. Binary output is correct.
4. Four negative scenarios pass.
5. Weekly benchmark report committed to `BENCHMARK.md`.

**Trigger:** on-PR for `examples/buck2-starter/**` + weekly cron.

**Failure policy:** SEV-3 alert; blocks PR merge.

To add the workflow to your project, copy `.github/workflows/buck2-starter-ci.yml`
and set the `CORELINK_PAT` secret in your repository settings.

---

## Troubleshooting

### Build report inspection

```bash
buck2 build :target --build-report /tmp/report.json
jq '.cache_hits, .total_actions, .errors' /tmp/report.json
```

### Verbose remote cache logs

```bash
buck2 build :target -v 3 2>&1 | grep -iE "remote|cache|reapi|http"
```

### Common error patterns

| Error | Cause | Resolution |
|---|---|---|
| `HTTP 401 Unauthorized` | PAT missing or invalid | `export CORELINK_PAT=<valid-token>` |
| `HTTP 403 Forbidden` | Token lacks `cache:read` or `cache:write` scope | Re-issue token with correct scopes |
| `HTTP 429 Too Many Requests` | Tenant quota exceeded | Upgrade plan at corelink.dev/billing |
| `connection refused` / timeout | Network unreachable or bad endpoint URL | Check `url` in `.buckconfig`; verify firewall |
| `BLAKE3 digest mismatch` | Corrupted artefact (CTRL-CAS-002 trip) | Retry build; if persistent, open support ticket |
| Cache ratio < 80 % | Warm build not using remote cache | Confirm `read = true` in `.buckconfig` |

### corelink doctor (CLI)

```bash
corelink doctor --json | jq '.checks[] | select(.status != "ok")'
```

This runs 8 checks: network, auth, storage write, storage read, BYOK, region,
quota, and client-verify.  Each failed check includes a `next_action` field.

---

## Performance tips

1. **Enable compression** (already in reference `.buckconfig` via
   `http_header_prefix`).  Reduces upload bandwidth for large C++ artifacts.

2. **Region selection** — choose the CoreLink region closest to your CI
   runners.  Override the endpoint URL:

   ```ini
   [remote_cache]
   url = https://eu.corelink.dev/v1/cache   # EU region
   ```

3. **Parallel uploads** — Buck2 streams artefact uploads concurrently by
   default.  Ensure your network allows outbound HTTP/2 multiplexing.

4. **Avoid `buck2 clean` in CI between warm runs** — only clean when you
   _want_ to exercise the remote cache path (cold local, warm remote).

5. **Dependency graph size** — cache hit ratio scales with graph stability.
   Prefer `cxx_library` / `cxx_binary` with stable headers; header churn
   invalidates downstream cache entries.

6. **Monitor cache hit ratio weekly** — run `./scripts/benchmark.sh` or
   inspect `BENCHMARK.md` to detect regression early.

---

## Migration from Bazel

If you are migrating a project from Bazel to Buck2, the remote cache
configuration is analogous:

| Concern | Bazel (`.bazelrc`) | Buck2 (`.buckconfig`) |
|---|---|---|
| Cache endpoint | `--remote_cache=https://corelink.dev/v1/cache` | `[remote_cache] url = ...` |
| Auth | credential helper script (Bazel 6+ CTRL-CRED-001 compliant) | `http_headers = Authorization: Bearer ${CORELINK_PAT}` |
| Digest algorithm | SHA-256 (Bazel default) | BLAKE3 (Buck2 + CoreLink default) |
| Retry | `--remote_retries=3` | `max_retries = 3` |
| Build output | `bazel-bin/` | `buck-out/` |

**Full apples-to-apples DX comparison:** see
[`docs/integrations/bazel-vs-buck2.md`](./bazel-vs-buck2.md).

---

*CoreLink Buck2 integration guide — WI-S15-003 · last updated 2026-05-14.*
