# CoreLink Bazel Starter — Time-to-First-Cache-Hit ≤ 5 min

Reference project for wiring Bazel 7.x to CoreLink remote cache via REAPI v2.

## Prerequisites

- Bazel 7.x (or [Bazelisk](https://github.com/bazelbuild/bazelisk))
- C++ toolchain (`gcc` / `clang`)
- A CoreLink Personal Access Token (PAT) — [generate one](https://humangr.com/corelink/en/customer/keys)

## Step-by-step setup (~3 min)

### 1. Clone

```bash
git clone https://github.com/HuGR-Labs/corelink-server
cd corelink-server/examples/bazel-starter
```

### 2. Export your PAT

```bash
export CORELINK_PAT=corelink_prod_...
```

The PAT is read by `.bazel/corelink-credential-helper.sh` at build time.
It is **never** placed in `.bazelrc` or CLI args (CTRL-CRED-001).

### 3. First build (cold cache)

```bash
bazel build //:hello
```

Expected: ~60–120 s (compiling locally; results uploaded to CoreLink CAS).

### 4. Verify cache hit

```bash
bazel clean --expunge
bazel build //:hello \
    --execution_log_json_file=/tmp/bazel-exec.json
```

Expected: ≤ 30 s (all actions resolved from remote cache).

Check the hit ratio:

```bash
python3 scripts/cache_hit_ratio.py /tmp/bazel-exec.json
```

The execution log is pretty-printed JSON objects, not one object per line, and
the hit is recorded as `cacheHit` alongside a `runner` that names which cache
answered — so a hand-rolled line-by-line reader looking for `remoteCacheHit`
sees nothing at all and reports an empty cache. That is exactly the bug this
example shipped with until 2026-08-24. The script above counts a spawn as a hit
only when `runner` says a *remote* cache served it, so a local `--disk_cache`
hit cannot flatter the number; pass `--allow-disk-cache` if you want both.

Expected: ≥ 80 % cache hit ratio.

### 5. Run the binary

```bash
./bazel-bin/hello
# Hello, World! (built via CoreLink remote cache)
```

---

## Credential helper protocol (Bazel 6+)

`.bazel/corelink-credential-helper.sh` implements the
[Bazel credential helper protocol](https://bazel.build/docs/credential-helper).
Bazel calls it with a JSON request on stdin; the helper emits an
`Authorization: Bearer <token>` header on stdout.

```
stdin:   {"uri":"https://corelink-api.humangr.com/bazel/cache"}
stdout:  {"headers":{"Authorization":["Bearer corelink_prod_..."]}}
```

The PAT is read from `CORELINK_PAT` env var — never shell-expanded into argv
(which would expose it in `ps aux` output).

---

## .bazelrc flags reference

| Flag | Default | Purpose |
|---|---|---|
| `--remote_cache` | — | CoreLink REAPI v2 endpoint |
| `--credential_helper` | — | Helper script path (relative to workspace) |
| `--remote_timeout` | 30s | Per-request deadline |
| `--remote_retries` | 3 | Retry on transient errors (FM-150) |
| `--remote_upload_local_results` | true | Upload new build results to cache |
| `--remote_download_minimal` | — | Download only explicitly requested outputs |
| `--experimental_remote_cache_compression` | true | Zstd compression (REAPI v2) |

---

## Troubleshooting

**`ERROR: CORELINK_PAT environment variable is not set`**
→ `export CORELINK_PAT=corelink_prod_...`

**`(401) Unauthorized`**
→ PAT may be expired or lack `cache:write` scope.
→ Run `corelink doctor` for a full auth diagnostic.

**`(429) Too Many Requests`**
→ Tenant quota exceeded; check `corelink stat --quota`.

**`remote cache is disabled`**
→ Verify `.bazelrc` is present and `--remote_cache` line is not commented out.

**Build does not use remote cache**
→ Add `--verbose_failures --subcommands` to see what Bazel executes locally.
→ Check execution log: `--execution_log_json_file=/tmp/exec.json`

---

## Bzlmod (opt-in)

This starter uses the legacy `WORKSPACE` file (Bazel 7.x GA baseline).
To try Bzlmod, add a `MODULE.bazel` and pass `--enable_bzlmod` (see
[Bzlmod migration guide](https://bazel.build/external/migration)).

---

## Next steps

- Copy `.bazelrc` + `.bazel/corelink-credential-helper.sh` to your project.
- See [docs/integrations/bazel.md](../../docs/integrations/bazel.md) for
  the full user guide including migration from BuildBuddy / NativeLink.
- Run the benchmark: `bash scripts/benchmark.sh`
