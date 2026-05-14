# Bazel vs Buck2 — CoreLink DX Comparison

**Purpose:** Apples-to-apples comparison of the CoreLink remote-cache
developer experience using the Bazel starter project (WI-S15-002) and the
Buck2 starter project (WI-S15-003).

Same scope: `hello-world` binary + 1 transitive C++ library dep.

---

## Table of contents

1. [Project layout parity](#project-layout-parity)
2. [Configuration comparison](#configuration-comparison)
3. [Setup steps comparison](#setup-steps-comparison)
4. [CI workflow comparison](#ci-workflow-comparison)
5. [Benchmark methodology](#benchmark-methodology)
6. [Cache hit ratio parity](#cache-hit-ratio-parity)
7. [Key differences](#key-differences)
8. [Decision guide](#decision-guide)

---

## Project layout parity

| File | Bazel (`examples/bazel-starter/`) | Buck2 (`examples/buck2-starter/`) |
|---|---|---|
| Build definition | `BUILD.bazel` | `BUCK` |
| Workspace declaration | `WORKSPACE` | (implicit in `[repository]` section) |
| Remote cache config | `.bazelrc` | `.buckconfig` |
| Source files | `greeter.cc`, `greeter.h`, `main.cc` | `greeter.cc`, `greeter.h`, `main.cc` |
| Ignore file | `.gitignore` | `.gitignore` |
| Setup guide | `README.md` | `README.md` |
| Benchmark script | `scripts/benchmark.sh` | `scripts/benchmark.sh` |
| Benchmark report | `BENCHMARK.md` | `BENCHMARK.md` |
| CI workflow | `.github/workflows/bazel-starter-ci.yml` | `.github/workflows/buck2-starter-ci.yml` |

Source files `greeter.cc`, `greeter.h`, and `main.cc` are **identical** across
both starters.  Only the build descriptor and config differ.

---

## Configuration comparison

### Bazel `.bazelrc`

```bash
# Remote cache endpoint
build --remote_cache=https://corelink.dev/v1/cache

# Auth via credential helper (Bazel 6+ CTRL-CRED-001 compliant)
# PAT read from CORELINK_PAT env var; emitted as JSON response; never in argv.
build --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh

# Compression + retries
build --remote_cache_compression=true
build --remote_retries=3
```

**Note:** Bazel requires a credential helper script to avoid leaking the PAT
via `ps aux` (argv exposure).  See `.bazel/corelink-credential-helper.sh` for
the helper implementation.

### Buck2 `.buckconfig`

```ini
[remote_cache]
url = https://corelink.dev/v1/cache
http_headers = Authorization: Bearer ${CORELINK_PAT}
read = true
write = true
max_retries = 3

[client]
hash_algorithm = BLAKE3
```

**Note:** Buck2 uses Shell env-var interpolation in `http_headers` — safe
because the header value is not exposed in the process argument list.

### Auth complexity

| Aspect | Bazel | Buck2 |
|---|---|---|
| Mechanism | Credential helper script | Env-var interpolation in `http_headers` |
| Setup steps | Write helper script + `chmod +x` + `.bazelrc` line | Single `.buckconfig` key |
| CTRL-CRED-001 compliance | Via helper (no argv leak) | Via env-var header (no argv leak) |
| Complexity | Medium | Low |

---

## Setup steps comparison

| Step | Bazel | Buck2 |
|---|---|---|
| 1. Get PAT | Same | Same |
| 2. Export PAT | `export CORELINK_PAT=...` | `export CORELINK_PAT=...` |
| 3. Clone repo | Same | Same |
| 4. Install build tool | `brew install bazel` / `bazelisk` | Download Buck2 binary |
| 5. Configure credential | Write `.bazel/corelink-credential-helper.sh` + chmod | (none — handled in `.buckconfig`) |
| 6. First build | `bazel build //:hello` | `buck2 build :hello` |
| 7. Verify cache hit | `bazel clean && bazel build //:hello` | `buck2 clean && buck2 build :hello` |

**Setup time target:** ≤ 5 min for both (R-S15-8).

**Buck2 advantage:** No credential helper script required; simpler one-file
configuration.

**Bazel advantage:** More mature tooling ecosystem; wider CI provider support.

---

## CI workflow comparison

| Dimension | Bazel (WI-S15-002) | Buck2 (WI-S15-003) |
|---|---|---|
| Workflow file | `bazel-starter-ci.yml` | `buck2-starter-ci.yml` |
| Trigger | on-PR + weekly cron | on-PR + weekly cron |
| Cold build | `bazel build //:hello` | `buck2 build :hello` |
| Warm build | `bazel clean && bazel build //:hello` | `buck2 clean && buck2 build :hello` |
| Cache hit assertion | ≥ 80 % (Bazel BEP parse) | ≥ 80 % (Buck2 build report JSON) |
| Warm build timeout | ≤ 30 s | ≤ 30 s |
| Negative scenarios | 4 | 4 |
| Benchmark job | 10+10 iterations | 10+10 iterations |
| SHA-pinned actions | Yes | Yes |

Both workflows use identical negative-scenario patterns:

1. PAT missing → auth error.
2. PAT invalid → 401 + clear message.
3. Bad endpoint URL → retry + graceful fallback.
4. Quota exceeded → 429 + next-action.

---

## Benchmark methodology

Both starter projects use the same benchmark methodology for reproducible
apples-to-apples comparison:

1. 10 **cold-cache** iterations: `clean` → `build` → record elapsed ms.
2. 10 **warm-cache** iterations: `clean` (local only) → `build` → record ms
   + cache hit count.
3. Statistics: median + p95 for cold and warm; cache hit ratio over warm runs.
4. Output: `BENCHMARK.md` updated by CI weekly.
5. Threshold: cache hit ratio ≥ 80 % (WI-S15-003 §8 + sprint contract R-S15-8).

### Running locally

```bash
# Bazel
cd examples/bazel-starter
./scripts/benchmark.sh --iterations 10

# Buck2
cd examples/buck2-starter
./scripts/benchmark.sh --iterations 10
```

---

## Cache hit ratio parity

Per sprint contract §7 completeness criterion 10.s15.2:

> Bazel + Buck2 real builds in CI sustained 7 days.

Per WI-S15-003 AC §8 parity scenario:

> Cache hit ratio comparable: Bazel ratio ± 10 % Buck2 ratio.

| Metric | Target |
|---|---|
| Bazel cache hit ratio | ≥ 80 % |
| Buck2 cache hit ratio | ≥ 80 % |
| Parity delta | ≤ 10 % absolute difference |

Both hit the CoreLink CAS via REAPI v2 backed by the same storage layer, so
ratios should be near-identical for deterministic C++ builds.  Divergence
> 10 % triggers a DX investigation (WI-S15-003 §24 post-mortem hooks).

---

## Key differences

| Dimension | Bazel | Buck2 |
|---|---|---|
| Language | Starlark (Python-like) | Starlark (Python-like) |
| Build descriptor | `BUILD.bazel` | `BUCK` |
| Workspace file | `WORKSPACE` required | Not required |
| Digest algorithm | SHA-256 (default) | BLAKE3 (CoreLink default) |
| Auth config | Credential helper script | Env-var in `http_headers` |
| Config file | `.bazelrc` | `.buckconfig` |
| Binary distribution | Bazelisk wrapper common | Single prebuilt binary |
| Ecosystem maturity | Very mature; wide CI support | Growing; Meta + Discord + Sentry |
| REAPI v2 support | Native | Native |
| Remote execution | Yes (Bazel RBE) | Yes (Buck2 RE) |
| CoreLink starter tested CI | Yes (WI-S15-002) | Yes (WI-S15-003) |

---

## Decision guide

**Choose Bazel if you:**
- Are already using Bazel in your org (Bazel ecosystem, Gazelle, rules_*).
- Need the widest third-party rules library.
- Require remote execution (RBE) in addition to remote caching.

**Choose Buck2 if you:**
- Are at Meta, Discord, Sentry, or similar Meta-adjacent org already using Buck2.
- Prefer simpler credential configuration (no helper script).
- Want BLAKE3 digest verification by default.
- Are migrating from Buck1 (Meta legacy) to Buck2.

**Both are fully supported** with CoreLink REAPI v2, CI integration tests, and
maintained starter projects.

---

*CoreLink Bazel vs Buck2 DX comparison — WI-S15-002 + WI-S15-003 · 2026-05-14.*
