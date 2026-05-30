# CoreLink — Turborepo Remote Cache Example

Minimal Turborepo workspace (2 packages, no framework) that demonstrates
CoreLink as a drop-in Vercel-compatible remote cache backend.

---

## What this shows

Turborepo supports any HTTP server that implements the Vercel Remote Cache
`/v8/artifacts` API.  CoreLink exposes exactly that API:

| HTTP verb | Path | Purpose |
|-----------|------|---------|
| `GET` | `/v8/artifacts/:hash?teamId=<team>` | Download artifact |
| `PUT` | `/v8/artifacts/:hash?teamId=<team>` | Upload artifact |
| `POST` | `/v8/artifacts/events` | Telemetry (accepted and dropped) |
| `POST` | `/v8/artifacts/status` | Returns `{"status":"enabled"}` |

Point three env vars at your CoreLink instance and `turbo run build` uses it
automatically — no plugins, no wrappers.

---

## Prerequisites

- **Node.js 20+** — `node --version`
- **pnpm 10+** — `npm install -g pnpm` (or `corepack enable`)
- **turbo** — installed locally as a `devDependency`; no global install needed

---

## Step 1 — Clone (or copy) this directory

```sh
# If you are working from the CoreLink monorepo:
cd apps/examples/turborepo

# Or copy the directory to your own project root and work from there.
```

---

## Step 2 — Set environment variables

Copy the example file and fill in your values:

```sh
cp .env.example .env
```

Edit `.env`:

```
TURBO_API=https://corelink-api.humangr.com
TURBO_TOKEN=<your-corelink-pat>
TURBO_TEAM=<your-tenant-uuid>
```

- **TURBO_API** — base URL of the CoreLink API (no trailing slash).
- **TURBO_TOKEN** — Personal Access Token from CoreLink Dashboard → Settings → API tokens.
- **TURBO_TEAM** — Your tenant UUID from Dashboard → Tenant.  Turborepo sends
  this as the `teamId` query parameter on every artifact request; CoreLink uses
  it as the storage partition key.

> `.env` is in `.gitignore` and must never be committed.

---

## Step 3 — Install dependencies

```sh
pnpm install
```

This installs only `turbo` (the single `devDependency`).  The two example
packages have no runtime dependencies.

---

## Step 4 — First build (cold cache)

Load the env vars and run Turbo:

```sh
source .env   # bash/zsh
# Windows PowerShell: Get-Content .env | ForEach-Object { $v=$_ -split '=',2; [System.Environment]::SetEnvironmentVariable($v[0],$v[1]) }

pnpm build
```

Expected output (first run, cache MISS):

```
• Packages in scope: @corelink-example/shared, @corelink-example/web
• Running build in 2 packages
  @corelink-example/shared:build: cache miss, executing ...
  @corelink-example/shared:build: shared built at 2026-05-29T10:00:00Z
  @corelink-example/web:build: cache miss, executing ...
  @corelink-example/web:build: web built at 2026-05-29T10:00:00Z

 Tasks:    2 successful, 2 total
 Cached:   0 cached, 2 total
 Time:     ~200ms
```

Turbo uploads the `dist/**` outputs to CoreLink after each successful task.

---

## Step 5 — Second build (cache HIT)

Run again without changing any source files:

```sh
pnpm build
```

Expected output (second run, cache HIT):

```
• Packages in scope: @corelink-example/shared, @corelink-example/web
• Running build in 2 packages
  @corelink-example/shared:build: cache hit, replaying logs ...
  @corelink-example/web:build: cache hit, replaying logs ...

 Tasks:    2 successful, 2 total
 Cached:   2 cached, 2 total    <-- both tasks restored from CoreLink
 Time:     ~30ms
```

The `Cached: 2 cached` line confirms CoreLink served both artifacts.

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| `cache miss` on every run | `TURBO_TOKEN` or `TURBO_TEAM` wrong | Check Dashboard for correct values |
| `401 Unauthorized` | Expired or invalid PAT | Rotate token in Dashboard |
| `403 Forbidden` | `TURBO_TEAM` does not match token's tenant | Use the UUID shown on the Tenant page |
| `400 Bad Request` | `teamId` missing (env var not set) | Verify `TURBO_TEAM` is exported |
| `503 Service Unavailable` | CoreLink audit pipeline temporarily down | Retry; contact support if persists |

---

## Directory layout

```
apps/examples/turborepo/
├── .env.example          # copy to .env, never commit .env
├── .gitignore
├── package.json          # workspace root; turbo devDependency
├── pnpm-workspace.yaml   # pnpm workspace globs
├── turbo.json            # pipeline: build → caches dist/**
├── apps/
│   └── web/
│       └── package.json  # depends on @corelink-example/shared
└── packages/
    └── shared/
        └── package.json  # leaf package, no dependencies
```

---

## How it works

Turborepo reads `TURBO_API`, `TURBO_TOKEN`, and `TURBO_TEAM` from the
environment.  On each task:

1. It hashes all inputs (source files, env vars, `turbo.json` config).
2. Before running the task it sends `GET /v8/artifacts/<hash>?teamId=<team>`
   with `Authorization: Bearer <token>` to CoreLink.
3. On a **cache hit** (HTTP 200), Turbo downloads the artifact and replays
   the task's outputs without re-executing — you see `cache hit, replaying`.
4. On a **cache miss** (HTTP 404), Turbo runs the task locally, then
   `PUT /v8/artifacts/<hash>?teamId=<team>` uploads the result to CoreLink.

All artifact storage is tenant-isolated: `teamId` must match the token's
tenant or CoreLink returns 403.
