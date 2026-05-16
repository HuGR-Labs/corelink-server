# Bazel + CoreLink Integration Guide

CoreLink implements the
[Bazel Remote Execution API v2 (REAPI v2)](https://github.com/bazelbuild/remote-apis),
so any Bazel 6+ workspace can use CoreLink as a remote cache with no plugins or
custom rules.

**Time-to-first-cache-hit target: ≤ 5 min from clone.**

---

## Architecture overview

```
Bazel client
   │
   │  REAPI v2 (HTTP/2 + Protobuf)
   │
   ├── FindMissingBlobs   (check which blobs are absent)
   ├── GetActionResult    (look up a cached build action)
   ├── UpdateActionResult (store a completed action)
   ├── Read / Write       (CAS blob transfer)
   │
   ▼
CoreLink CAS (Content-Addressable Store)
   │
   ├── BLAKE3 integrity verify (CTRL-CAS-002, default-on)
   ├── Tenant isolation        (tenant prefix per request)
   └── Cloudflare R2 backend  (durable, geo-distributed)
```

Bazel stores each build action by its hash (action digest) in the Action Cache
(AC). Blobs (compiled objects, test results) are stored in the CAS.  On a
cache hit, Bazel skips execution entirely and fetches the outputs directly.

---

## Quick start (≤ 5 min)

### 1. Generate a PAT

1. Log in to <https://app.corelink.humangr.com>
2. Navigate to **Settings → Tokens → New Token**
3. Grant scopes: `cache:read`, `cache:write`
4. Copy the token (`corelink_prod_...`)

### 2. Add the credential helper

Copy `.bazel/corelink-credential-helper.sh` from
[`examples/bazel-starter/`](../../examples/bazel-starter/) to your workspace:

```bash
mkdir -p .bazel
curl -fsSL https://raw.githubusercontent.com/corelink-dev/examples/main/bazel-starter/.bazel/corelink-credential-helper.sh \
    -o .bazel/corelink-credential-helper.sh
chmod +x .bazel/corelink-credential-helper.sh
```

The helper reads `CORELINK_PAT` from the environment and emits a Bazel JSON
credential response.  The PAT is **never** placed in argv
(CTRL-CRED-001 — see §Security below).

### 3. Add `.bazelrc` flags

Append to your project's `.bazelrc`:

```ini
# CoreLink remote cache (REAPI v2)
build --remote_cache=https://corelink.humangr.com/v1/cache
build --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh
build --remote_timeout=30s
build --remote_retries=3
build --remote_upload_local_results=true
build --remote_download_minimal
build --experimental_remote_cache_compression=true
```

### 4. Set the environment variable

```bash
export CORELINK_PAT=corelink_prod_...
```

Add to your shell profile (`~/.bashrc`, `~/.zshrc`) or CI secret store.

### 5. Build and verify

```bash
bazel build //...           # cold — results uploaded to cache
bazel clean --expunge
bazel build //...           # warm — should be ≥ 80% cache hits
```

---

## .bazelrc flags reference

| Flag | Recommended value | Notes |
|---|---|---|
| `--remote_cache` | `https://corelink.humangr.com/v1/cache` | REAPI v2 endpoint |
| `--credential_helper` | `%workspace%/.bazel/corelink-credential-helper.sh` | Bazel 6+ protocol; reads `CORELINK_PAT` |
| `--remote_timeout` | `30s` | Per-request deadline; FM-150 (transient) |
| `--remote_retries` | `3` | Retry on transient errors |
| `--remote_upload_local_results` | `true` | Upload new results; set `false` on read-only runners |
| `--remote_download_minimal` | (flag) | Fetch only explicitly requested outputs; fastest for CI |
| `--remote_download_outputs=all` | (override) | Use when you need all outputs locally (e.g. packaging) |
| `--experimental_remote_cache_compression` | `true` | Zstd compression — 30–50% bandwidth reduction |
| `--remote_max_connections` | `200` | Tune for large parallel builds |
| `--jobs` | `auto` | Let Bazel use all available cores |

### CI-only `.bazelrc` section

```ini
# Applied via --config=ci in GitHub Actions / GitLab / CircleCI
build:ci --remote_download_outputs=all
build:ci --noremote_upload_local_results  # read-only for warm-cache verification
```

---

## GitHub Actions integration

```yaml
- name: Setup Bazelisk
  uses: bazelbuild/setup-bazelisk@b39c379c82683a5f25d34f0d062761f62693e0b2 # v3.0.0

- name: Build with CoreLink cache
  env:
    CORELINK_PAT: ${{ secrets.CORELINK_PAT }}
  run: bazel build //... --config=ci
```

Add `CORELINK_PAT` to your repository secrets
(**Settings → Secrets and variables → Actions → New repository secret**).

### Verifying cache hits in CI

```yaml
- name: Warm build with execution log
  env:
    CORELINK_PAT: ${{ secrets.CORELINK_PAT }}
  run: |
    bazel build //... \
        --noremote_upload_local_results \
        --execution_log_json_file=/tmp/bazel-exec.json

- name: Assert ≥ 80% cache hit ratio
  run: |
    python3 - <<'EOF'
    import json, sys
    hits = total = 0
    with open("/tmp/bazel-exec.json") as f:
        for line in f:
            if not line.strip(): continue
            entry = json.loads(line)
            if "remoteCacheHit" in entry:
                total += 1
                if entry["remoteCacheHit"]: hits += 1
    ratio = hits / total * 100 if total else 0
    print(f"Cache hit ratio: {ratio:.1f}%")
    sys.exit(0 if ratio >= 80 else 1)
    EOF
```

---

## Troubleshooting

### `ERROR: CORELINK_PAT environment variable is not set`

The credential helper cannot find the PAT.

```bash
export CORELINK_PAT=corelink_prod_...
# Verify:
echo "${CORELINK_PAT}" | cut -c1-20
```

In CI: add `CORELINK_PAT` to repository secrets and reference it as
`${{ secrets.CORELINK_PAT }}`.

### `(401) Unauthorized`

The PAT is present but rejected.

1. Run `corelink doctor` — check #2 (Auth).
2. Verify the PAT has `cache:read` + `cache:write` scopes.
3. Check PAT expiry in the CoreLink dashboard.
4. Error code reference: `COR_AUTH_001` (invalid PAT), `COR_AUTH_003` (expired).

### `(403) Forbidden`

The PAT is valid but lacks the required scope for this tenant.

```bash
corelink doctor --json | jq '.checks[] | select(.id == "auth")'
```

### `(429) Too Many Requests`

Tenant quota exceeded.

```bash
corelink stat --quota
# or
corelink doctor | grep -i quota
```

Upgrade plan at <https://app.corelink.humangr.com/billing> or reduce parallel jobs
(`--jobs=50`).

### Cache misses on repeated builds

1. Check that `--remote_upload_local_results=true` is set on the first run.
2. Verify actions are deterministic:
   ```bash
   bazel build //... --execution_log_json_file=/tmp/exec1.json
   bazel clean && bazel build //... --execution_log_json_file=/tmp/exec2.json
   diff /tmp/exec1.json /tmp/exec2.json
   ```
3. Check for non-deterministic timestamps:
   `--action_env=SOURCE_DATE_EPOCH=0` can help with embed-timestamp macros.
4. Inspect action keys:
   ```bash
   bazel aquery //... --output=jsonproto 2>/dev/null | \
       python3 -c "import sys,json; [print(a['actionKey']) for a in json.load(sys.stdin)['actions']]"
   ```

### `remote cache is disabled` or `WARNING: Remote cache disabled`

- Check that `.bazelrc` is in the workspace root (where you run `bazel`).
- Verify the `--remote_cache` flag is not overridden by a downstream `.bazelrc`.
- Add `--announce_rc` to see which `.bazelrc` files Bazel reads.

### Build times not improving

Enable `--verbose_failures --subcommands` to see what Bazel executes locally:

```bash
bazel build //... --verbose_failures --subcommands 2>&1 | grep -c "^SUBCOMMAND"
```

A high subcommand count means actions are executing locally (cache misses).
Add `--execution_log_json_file=/tmp/exec.json` and inspect
`remoteCacheHit: false` entries to find which actions miss.

---

## Performance tips

### `--remote_download_minimal`

Only downloads outputs you explicitly request (e.g. `bazel run` target).
Intermediate `.o` files stay in the remote cache and are never transferred.
Use this in CI for compile-only jobs (30–60% bandwidth reduction).

### `--experimental_remote_cache_compression=true`

Enables Zstandard compression on all CAS transfers.
Reduces bandwidth by 30–50% for typical C++/Rust/Go projects.

### Remote caching large outputs

For artifacts > 100 MB (e.g. Docker layers, test data):

```ini
build --remote_max_connections=500
build --jobs=200
```

Tune `--remote_max_connections` to match your network concurrency limit.

### Action parallelism

CoreLink scales horizontally.  Increase `--jobs` to maximise parallelism:

```bash
bazel build //... --jobs=$(nproc) --remote_max_connections=500
```

---

## Migration from BuildBuddy / NativeLink

Both BuildBuddy and NativeLink also implement REAPI v2.  Migration is a
one-line `.bazelrc` change:

**From BuildBuddy:**

```ini
# Before:
build --remote_cache=grpcs://remote.buildbuddy.io
build --remote_header=x-buildbuddy-api-key=<key>

# After (CoreLink — credential helper replaces header in argv):
build --remote_cache=https://corelink.humangr.com/v1/cache
build --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh
```

**From NativeLink (Nativelink remote):**

```ini
# Before:
build --remote_cache=grpcs://scheduler.nativelink.net:443
build --remote_header=x-nativelink-api-key=<key>

# After:
build --remote_cache=https://corelink.humangr.com/v1/cache
build --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh
```

Then:

```bash
export CORELINK_PAT=corelink_prod_...
bazel build //...   # first run populates CoreLink cache
```

---

## Security

### CTRL-CRED-001 — Credential helper protocol (Bazel 6+)

CoreLink enforces that the PAT is **never** placed in Bazel argv.
The alternative (`--remote_header=Authorization:Bearer ${CORELINK_PAT}`)
shell-expands the secret into the Bazel process argument list, making it
visible in `ps aux` and process dumps.

The credential helper protocol (Bazel 6+) routes the token through a side
channel:

```
Bazel → .bazel/corelink-credential-helper.sh (stdin: JSON request)
         ↓ reads CORELINK_PAT from env (not argv)
         ↓ stdout: {"headers":{"Authorization":["Bearer <token>"]}}
Bazel ← injects header into REAPI HTTP requests
```

Reference: <https://bazel.build/docs/credential-helper>

### CTRL-CAS-002 — Client-side integrity verify (default-on)

CoreLink CAS verifies BLAKE3 digests on every download.
If a blob is corrupted in transit or storage, the client rejects it with
`COR_INTEGRITY_001` and Bazel retries or falls back to local execution.

### Secret hygiene

- Add `CORELINK_PAT` to `.gitignore`-covered locations; never commit it.
- Rotate PATs via <https://app.corelink.humangr.com/tokens>.
- Scope PATs to minimum required: `cache:read` for read-only runners;
  `cache:read cache:write` for build runners.

---

## REAPI v2 endpoint reference

CoreLink implements the following REAPI v2 gRPC-over-HTTP/2 methods:

| Method | Path | Purpose |
|---|---|---|
| `FindMissingBlobs` | `/build.bazel.remote.execution.v2.ContentAddressableStorage/FindMissingBlobs` | Check which blobs need upload |
| `BatchUpdateBlobs` | `/build.bazel.remote.execution.v2.ContentAddressableStorage/BatchUpdateBlobs` | Upload a batch of blobs |
| `BatchReadBlobs` | `/build.bazel.remote.execution.v2.ContentAddressableStorage/BatchReadBlobs` | Download a batch of blobs |
| `Read` | `/google.bytestream.ByteStream/Read` | Stream-download large blob |
| `Write` | `/google.bytestream.ByteStream/Write` | Stream-upload large blob |
| `GetActionResult` | `/build.bazel.remote.execution.v2.ActionCache/GetActionResult` | Look up cached action |
| `UpdateActionResult` | `/build.bazel.remote.execution.v2.ActionCache/UpdateActionResult` | Store completed action |

All methods require `Authorization: Bearer <PAT>` header (provided by the
credential helper).

Staging endpoint for testing: `https://staging.corelink.humangr.com/v1/cache`

---

## Further reading

- [examples/bazel-starter/](../../examples/bazel-starter/) — runnable starter project
- [REAPI v2 specification](https://github.com/bazelbuild/remote-apis)
- [Bazel credential helper protocol](https://bazel.build/docs/credential-helper)
- [corelink doctor](../cli/doctor.md) — diagnostic for auth / network / quota issues
- [Failure modes FM-150 (transient) / FM-160 (auth)](../architecture/failure-modes.md)
