# `get-corelink-worker`

Cloudflare Worker serving the **CoreLink CLI install one-liner** at
`https://get.corelink.io`.

```bash
curl -fsSL https://get.corelink.io | sh -s -- --token=$PAT --region=ord
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
| Custom domain | `get.corelink.io` |
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
pnpm deploy:prod      # env.prod with route binding to get.corelink.io
```

The `get.corelink.io` DNS + CF custom-domain mapping is set up via a
**separate Gustavo runbook** (CF dashboard click). Until DNS is live,
`pnpm dev` exposes the script on a local `wrangler dev` URL for E2E
testing the install flow.

## Acceptance (Phase 0.H)

1. `GET https://get.corelink.io` returns `Content-Type: text/x-shellscript`
   and exits 200 in < 50 ms p99 (pure static render — no DO / KV / R2 /
   D1 calls).
2. `curl -fsSL https://get.corelink.io | sh -s -- --token=$TEST_TOKEN --region=ord`
   end-to-end succeeds on Linux x86_64 and Darwin aarch64.
3. `curl -fsSL https://get.corelink.io | sh -s --` (no `--token`) exits
   non-zero with `FATAL: --token required` on stderr.
4. The served script's `URL=` line points at the
   `humangr-labs/corelink-cli` Releases path verbatim — the GitHub
   Releases CI in that repo (separate runbook,
   `2026-05-27-phase-0-corelink-cli-repo-bootstrap.md`) must be green
   before end-to-end works for real customers.
