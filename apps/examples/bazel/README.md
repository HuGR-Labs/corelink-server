# CoreLink — Bazel remote cache example

This workspace shows how to point Bazel at CoreLink as a remote cache
using the REAPI v2 protocol. The example is intentionally small: one
`cc_library` (`greeting`) and one `cc_binary` (`hello`).

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| [Bazelisk](https://github.com/bazelbuild/bazelisk) | latest | `brew install bazelisk` (or download from GitHub releases) |
| C++ toolchain | any | ships with Xcode CLT on macOS; `build-essential` on Linux |
| CoreLink account | — | [app.corelink.humangr.com](https://app.corelink.humangr.com) — sandbox tenants are free |

Bazelisk reads `.bazelversion` if present; without it, it downloads the
latest stable Bazel. This example is compatible with **Bazel 7+** and
uses Bzlmod (`MODULE.bazel`).

## Quick start

### Step 1 — Get your credentials

Log in to the [CoreLink dashboard](https://app.corelink.humangr.com),
create a Personal Access Token (PAT), and note your tenant UUID.

### Step 2 — Export the environment variables

```bash
export CORELINK_PAT="corelink_t_xxx.yyy.zzz"
export CORELINK_TENANT="your-tenant-uuid-here"
```

Keep these in your shell profile (`.zshrc` / `.bashrc`) or a secrets
manager. Never commit them.

### Step 3 — Build

```bash
# From this directory (apps/examples/bazel/):
bazel build //...
```

Expected output on the first build (cache cold):

```
INFO: Analyzed 2 targets (... packages loaded, ... targets configured).
INFO: Found 2 targets...
INFO: From Compiling greeting.cc:
INFO: From Compiling hello.cc:
INFO: From Linking hello:
INFO: Build completed successfully, 3 total actions
```

Expected output on the second build (cache warm — all actions are remote hits):

```
INFO: Build completed successfully, 3 total actions (3 remote cache hits)
```

Run the binary:

```bash
bazel run //:hello
# Hello, CoreLink!
```

## How it works

The `.bazelrc` in this directory adds three Bazel flags:

| Flag | Value | Purpose |
|------|-------|---------|
| `--remote_cache` | `https://corelink-api.humangr.com/bazel/v2` | REAPI v2 cache endpoint |
| `--remote_header` | `Authorization: Bearer $CORELINK_PAT` | Authenticates the request |
| `--remote_instance_name` | `$CORELINK_TENANT` | Routes the request to your tenant's isolated cache namespace |

Bazel computes a SHA-256 digest for each action and its inputs, then
asks CoreLink whether a cached result already exists
(`GET /bazel/v2/<tenant>/blobs/ac/<hash>/<size>`). On a cache hit,
Bazel downloads the pre-built outputs and skips compilation entirely.
On a miss, it compiles locally and uploads the result
(`PUT /bazel/v2/<tenant>/uploads/<uuid>/blobs/<hash>/<size>`).

## Tenant isolation

CoreLink enforces tenant isolation at the REAPI level: the
`:instance` path segment (your `CORELINK_TENANT`) must match the
tenant your PAT belongs to. Cross-tenant reads return HTTP 403.

## Files

```
apps/examples/bazel/
  MODULE.bazel     — Bzlmod root (Bazel 7+)
  WORKSPACE        — legacy workspace root (kept for tooling compat)
  .bazelrc         — remote cache flags (commented)
  BUILD.bazel      — cc_library :greeting + cc_binary :hello
  greeting.h       — library header
  greeting.cc      — library implementation
  hello.cc         — binary entry point
  README.md        — this file
```

## Troubleshooting

**`ERROR: could not connect to remote cache`**
Check that `CORELINK_PAT` and `CORELINK_TENANT` are exported in your
current shell session, and that the PAT has not expired.

**`ERROR: remote cache returned HTTP 403`**
Your PAT does not have write access, or `CORELINK_TENANT` does not
match the tenant the PAT was issued for. Verify both values in the
dashboard.

**Cache not being used (no "remote cache hits" message)**
Add `--remote_cache_compression` and `--experimental_remote_cache_async`
to `.bazelrc` for improved hit rates on large builds. These flags are
not needed for this minimal example.
