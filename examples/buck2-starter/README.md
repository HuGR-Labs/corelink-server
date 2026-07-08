# CoreLink Buck2 Starter

Get your **first remote-cache hit in under 5 minutes** using Buck2 + CoreLink.

> Parity project: same hello-world + 1 transitive dep scope as
> [`examples/bazel-starter`](../bazel-starter/README.md) —
> see [`docs/integrations/bazel-vs-buck2.md`](../../docs/integrations/bazel-vs-buck2.md)
> for an apples-to-apples DX comparison.

---

## Prerequisites

| Tool | Minimum version | Install |
|---|---|---|
| Buck2 | latest stable | [Buck2 releases](https://github.com/facebook/buck2/releases) |
| C++ toolchain | GCC 11+ / Clang 13+ | System package manager |
| `jq` | 1.6+ | System package manager (benchmark script only) |

---

## Step 1 — Get a CoreLink PAT (~1 min)

1. Sign up at <https://corelink.humangr.com> (free tier available).
2. Navigate to **Settings → API Tokens → New token**.
3. Copy the token; you will export it in the next step.

---

## Step 2 — Set your PAT (~30 s)

```bash
export CORELINK_PAT=corelink_pat_...   # never commit this
```

> **Security note:** The `.buckconfig` in this project reads `CORELINK_PAT` from
> the environment via `${CORELINK_PAT}` interpolation.  Never hard-code a token
> in `.buckconfig` or any committed file (CTRL-CRED-001).

---

## Step 3 — Clone and enter the starter directory (~30 s)

```bash
git clone https://github.com/HumanGuardrail/corelink-server.git
cd corelink-server/examples/buck2-starter
```

---

## Step 4 — First build (cold cache) (~1 min)

```bash
buck2 build :hello
```

Expected output:

```
Build ID: ...
Jobs completed: 3/3.  Time elapsed: 12.3s.
BUILD SUCCEEDED
```

Run the binary:

```bash
buck2 run :hello
# Hello, World! (cached by CoreLink)

buck2 run :hello -- Alice
# Hello, Alice! (cached by CoreLink)
```

---

## Step 5 — Verify remote cache hit (~30 s)

Clean local artefacts (remote cache is still warm):

```bash
buck2 clean
```

Re-build — should complete in ≤ 30 s with ≥ 80 % remote cache hits:

```bash
buck2 build :hello
```

Expected output (cache hit):

```
Build ID: ...
Jobs completed: 3/3.  Time elapsed: 2.1s.
BUILD SUCCEEDED
```

Check the build report for cache hit details:

```bash
buck2 build :hello --build-report /tmp/report.json
jq '.cache_hits, .total_actions' /tmp/report.json
# 3
# 3
```

**Total elapsed: ≤ 5 minutes.**

---

## Troubleshooting

### Auth error (401)

```
Error: HTTP 401 Unauthorized
```

- Check `echo $CORELINK_PAT` — confirm the variable is set and non-empty.
- Verify the token is valid: `corelink doctor` (requires CoreLink CLI from
  [WI-S15-001](../../specs/04_sprints/S15/work_items/WI-S15-001-corelink-cli.md)).
- Ensure the token has `cache:read` and `cache:write` scopes.

### Cache miss on warm build

- Confirm `[remote_cache] write = true` in `.buckconfig`.
- Check for proxy or firewall blocking `https://corelink.humangr.com`.
- Run with verbose logging: `buck2 build :hello -v 2`.

### Quota exceeded (429)

```
Error: HTTP 429 Too Many Requests
```

- Your tenant has reached its usage limit.  Upgrade at <https://corelink.humangr.com/billing>.

### Network unreachable

Buck2 retries transient failures automatically (`max_retries = 3` in `.buckconfig`).
If the cluster is persistently unreachable the build falls back to local execution.

---

## Benchmark

Run the included benchmark script to measure cold vs warm build times:

```bash
./scripts/benchmark.sh                         # 10 + 10 iterations (default)
./scripts/benchmark.sh --iterations 5          # faster smoke run
./scripts/benchmark.sh --output-md my-report.md
```

Results are written to `BENCHMARK.md`.  See [BENCHMARK.md](./BENCHMARK.md) for
the latest CI-measured numbers.

---

## Configuration reference

See [`.buckconfig`](./.buckconfig) for all supported options and
[`docs/integrations/buck2.md`](../../docs/integrations/buck2.md) for the full
user guide.

---

## Next steps

- Read the [Buck2 integration guide](../../docs/integrations/buck2.md).
- Compare DX with the [Bazel starter](../bazel-starter/README.md) via
  [bazel-vs-buck2.md](../../docs/integrations/bazel-vs-buck2.md).
- Run `corelink doctor` to verify your full setup (CLI from WI-S15-001).
