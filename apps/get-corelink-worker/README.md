# `get-corelink-worker`

Cloudflare Worker serving the **CoreLink CLI install one-liner** at
`https://corelink-get.humangr.com`.

```bash
curl -fsSL https://corelink-get.humangr.com | sh -s -- --token=$PAT --region=ord
```

The script:

1. Parses `--token=<…>` (required) and `--region=<…>` (optional, default `auto`).
2. Detects the host OS (`linux` / `darwin` / `windows`) and architecture
   (`x86_64` / `aarch64`).
3. Downloads the matching CoreLink CLI binary from
   `https://github.com/humangr-labs/corelink-cli/releases/latest/download/corelink-${OS}-${ARCH}`.
4. Writes `~/.corelink/config.toml` with `token`, `region`, and the
   default `endpoint = "https://corelink-api.humangr.com"`.
5. Runs `corelink ping` to verify connectivity. On success, the
   developer's `/welcome` SSE pane in `admin-ui` flips from "Waiting"
   to "Connected" within ~1 second.

## Conventions

| Property | Value |
| --- | --- |
| Worker name | `corelink-get-cli` |
| Custom domain | `corelink-get.humangr.com` |
| Compatibility date | `2026-04-01` (matches root + clerk-cf Workers) |
| `workers_dev` | `false` (security hardening — bot-scan resistant) |
| CPU cap | `30 ms` (static script render is sub-millisecond) |
| Outbound deps | none (pure string render) |

## Deploy

```bash
cd apps/get-corelink-worker
pnpm install
pnpm typecheck
pnpm test
pnpm deploy           # default = dev / staging env
pnpm deploy:prod      # env.prod with route binding to corelink-get.humangr.com
```

The `corelink-get.humangr.com` DNS + CF custom-domain mapping is set up via a
**separate Gustavo runbook** (CF dashboard click). Until DNS is live,
`pnpm dev` exposes the script on a local `wrangler dev` URL for E2E
testing the install flow.

## CLI Release Procedure

The install Worker at `https://corelink-get.humangr.com` serves binaries
from `https://github.com/humangr-labs/corelink-cli/releases/latest/download`.
Releases in that repo are created automatically by
`.github/workflows/release-cli.yml` in `humangr-labs/corelink-server`
whenever a `cli-v*` tag is pushed.

### One-time operator setup (do this once, before the first release)

1. **Create a GitHub PAT** with `repo` scope scoped to
   `humangr-labs/corelink-cli` (or a fine-grained token with
   "Contents: write" on that repo).

2. **Store the PAT as a repository secret** in
   `humangr-labs/corelink-server`:
   - Name: `CORELINK_CLI_RELEASE_TOKEN`
   - Value: the PAT from step 1

3. **Verify the `mlugg/setup-zig@v2` SHA pin** in the workflow:
   - Open https://github.com/mlugg/setup-zig/releases/tag/v2
   - Copy the commit SHA from the release page
   - Replace `uses: mlugg/setup-zig@v2` in
     `.github/workflows/release-cli.yml` with the pinned form:
     `uses: mlugg/setup-zig@<40-char-SHA>  # v2.x.y`
   - Commit the pin to `main` before the first production release.

### Releasing a new CLI version

```bash
# 1. Ensure the version in Cargo.toml workspace is bumped (e.g. 0.1.0)
# 2. Commit all changes to main
# 3. Push the release tag — the workflow fires automatically
git tag cli-v0.1.0
git push origin cli-v0.1.0
```

The workflow will:
1. Build 5 binaries via `cargo-zigbuild` on ubuntu-22.04
2. Compute SHA-256 checksums
3. Create a release in `humangr-labs/corelink-cli` tagged `v0.1.0`
4. Upload 11 files: 5 binaries + 5 `.sha256` files + `checksums.txt`

### Expected artifact list (per release)

| File | Platform |
|------|----------|
| `corelink-linux-x86_64`      | Linux x86_64   |
| `corelink-linux-x86_64.sha256` | checksum     |
| `corelink-linux-aarch64`     | Linux aarch64  |
| `corelink-linux-aarch64.sha256` | checksum    |
| `corelink-darwin-x86_64`     | macOS x86_64   |
| `corelink-darwin-x86_64.sha256` | checksum    |
| `corelink-darwin-aarch64`    | macOS aarch64  |
| `corelink-darwin-aarch64.sha256` | checksum   |
| `corelink-windows-x86_64.exe` | Windows x86_64 |
| `corelink-windows-x86_64.exe.sha256` | checksum |
| `checksums.txt`               | all digests combined |

### Tag format

| Tag | Meaning |
|-----|---------|
| `cli-v0.1.0` | CLI release 0.1.0 (triggers the workflow) |
| `v*` | Server/other releases (does NOT trigger release-cli.yml) |

### Troubleshooting

- **Release job fails with 401**: `CORELINK_CLI_RELEASE_TOKEN` is missing
  or expired — regenerate and update the secret.
- **cargo-zigbuild fails on a target**: check if the target uses `ring`
  assembly bootstrap (unlikely with `rustls` + `ring` — no `aws-lc-sys`).
  If it persists, fall back to Option B (native matrix per-OS runner) per
  the Stream 2.8 runbook.
- **mlugg/setup-zig fails**: pin to a known-good SHA (see step 3 above).

## Acceptance (Phase 0.H)

1. `GET https://corelink-get.humangr.com` returns `Content-Type: text/x-shellscript`
   and exits 200 in < 50 ms p99 (pure static render — no DO / KV / R2 /
   D1 calls).
2. `curl -fsSL https://corelink-get.humangr.com | sh -s -- --token=$TEST_TOKEN --region=ord`
   end-to-end succeeds on Linux x86_64 and Darwin aarch64.
3. `curl -fsSL https://corelink-get.humangr.com | sh -s --` (no `--token`) exits
   non-zero with `FATAL: --token required` on stderr.
4. The served script's `URL=` line points at the
   `humangr-labs/corelink-cli` Releases path verbatim — the GitHub
   Releases CI in that repo (separate runbook,
   `2026-05-27-phase-0-corelink-cli-repo-bootstrap.md`) must be green
   before end-to-end works for real customers.
