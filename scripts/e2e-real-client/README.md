# `scripts/e2e-real-client/` — the GO-LIVE MOAT

A real-**client** conformance harness. It drives the **actual** client toolchains
(`docker`, `cargo` + `sccache`, `brew`, `curl`) end to end against **PROD** and
emits a repeatable **SHIP / NO-SHIP** go-live certificate.

This is the protection moat: it codifies — as one re-runnable command — the manual
validation that already proved `docker login`/`push`/`pull`, `cargo` writes, and
`brew` all work against `corelink-api.humangr.com` / `corelink-oci.humangr.com`.

It complements (does NOT replace) the black-box Rust suite in
`tests/e2e-user-journeys/`: that suite exercises the **HTTP contract** with
reqwest; this harness exercises the **real third-party binaries** a paying
customer actually runs.

## What it does

1. **Bootstraps a real user the real way** (no internal mints):
   - Creates a Clerk user via the Clerk Backend API (`POST /v1/users`).
   - Polls that user's Clerk **`private_metadata.pat_plaintext`** (the
     signup-worker provisions a tenant + read-write PAT there; the field is
     scrubbed hourly, so polling is fast — typically well under 120 s).
   - Creates a **second** user for cross-tenant isolation tests.
   - **DSR-deletes both users at the end** (`DELETE /v1/users/{id}` → Clerk's
     `user.deleted` webhook enqueues the GDPR erasure). Run via an `EXIT` trap,
     so cleanup happens even on failure.

2. **Exercises each surface**, one `PASS` / `GATED` / `FAIL` line per step. Be
   honest about fidelity: only some surfaces drive a **real client binary**;
   the rest drive the **raw HTTP contract** with `curl` shaped like the client.
   `curl` passing does NOT prove the real CLI works — so the raw-HTTP probes are
   labelled as such in every verdict line and are NOT claimed as real-CLI passes.

   | surface | fidelity | what it does |
   |---------|----------|--------------|
   | `docker` | **REAL CLI** | `docker login` (PAT) → build a tiny `FROM scratch` image → `docker push` → `docker pull` → assert the digest round-trips (an **empty** RepoDigest is a **FAIL**, not a pass) |
   | `cargo`+`sccache` | **REAL CLI** | a tiny no-dep `cargo build` with `RUSTC_WRAPPER=sccache` + `SCCACHE_WEBDAV_ENDPOINT=.../cargo/<tenant>` + `SCCACHE_WEBDAV_TOKEN=<PAT>`; asserts **no `Cache errors`** in `sccache --show-stats` |
   | native CAS/AC | **raw-HTTP** (no first-party CLI exists) | `curl` round-trips with the PAT; BLAKE3-addressed CAS write→read **with a byte-compare**, tenant-isolation deny, and an AC write→read **with a byte-compare** |
   | bazel REAPI v2 | **raw-HTTP** (NOT the `bazel` CLI) | `curl` upload (`/bazel/v2/<tenant>/uploads/<uuid>/blobs/<sha256>/<size>`) → read, **with a byte-compare** |
   | turbo | **raw-HTTP** (NOT the `turbo` CLI) | `curl` artifact `PUT` → `GET` (`/v8/artifacts/<hash>?teamId=<tenant>`), **with a byte-compare** |
   | `brew` | **raw-HTTP** auth shape + a `brew` binary smoke (GATED) | the bottle-GET `Authorization: Bearer` auth shape via `curl` (no heavy `brew install` — the Mac is shared/disk-constrained); when `brew` is present, a `brew --version` env smoke is recorded **GATED** (it proves the binary runs, NOT a bottle round-trip — so it is never claimed as a real-CLI PASS) |
   | identity | **raw-HTTP** (no identity CLI) | `GET /v1/users/me` must `200` + tenant matches |

   **A `5xx` is never a `PASS`.** Every status-graded probe classifies the HTTP
   code: `2xx`/`3xx`/auth-accepted `4xx` (e.g. a `404` for an absent path) =
   PASS; `401`/`403` (auth rejected) = FAIL; **any `5xx` = FAIL** (a `502` on the
   `_public` path is the exact fail-closed signature — it means the server
   refused to serve, never a green); no response = GATED.

3. **Emits a SHIP / NO-SHIP cert** + a machine-readable JSON summary.

## The verdict model (mirrors the Rust suite)

- **PASS** — the real client round-tripped against PROD.
- **GATED** — a prerequisite was absent (tool not installed → `command -v`; a
  cred unset; or a **known client/env limitation**). Recorded, **never** a silent
  skip, **never** blocks the verdict.
- **FAIL** — the client ran and the live contract was violated → **NO-SHIP**.

`FAIL ⇒ NO-SHIP` and a non-zero exit. `GATED` never blocks **on its own**.

### Anti-vacuum floor (no green-by-vacuum)

A run that gates **everything** (no Clerk secret, no tools) asserts nothing — it
must **not** print `SHIP`. The harness mirrors the Rust suite's
`CORELINK_E2E_MIN_PASS` floor: if the number of real `PASS`es is **below** the
floor, the verdict is **`NO-SHIP (vacuum)`** and the exit is non-zero.

- Default floor = **1** (at least one positive assertion must run). So a
  credential-less run is now **NO-SHIP**, not a silent green.
- A provisioned prod run should set it high (e.g. `CORELINK_E2E_MIN_PASS=12`) so
  a silent provisioning/probe regression that re-gates the suite cannot sail
  through as `SHIP`.
- Set `CORELINK_E2E_MIN_PASS=0` to opt back into "all-gated is acceptable" for
  throwaway/uncredentialed environments.

### Known GATED-not-FAIL cases (by design)

- A tool is absent (`docker`/`cargo`/`sccache`/`brew`/`b3sum`/…).
- **sccache + TLS on this Mac**: the sccache client here hits a TLS
  *"bad protocol version"* against the WebDAV endpoint. That is a known
  client/env issue, **not** a server fault — so a connect/TLS failure is GATED.
- **`brew` binary smoke**: when `brew` is installed, `brew --version` with the
  `HOMEBREW_*` env is recorded **GATED** (it only proves the binary runs, not a
  real bottle round-trip). The bottle auth shape itself is graded via raw HTTP.

## Running

```sh
# Credentials: env first, else .env.local (CLERK_LIVE_SECRET_KEY or CLERK_SECRET_KEY).
bash scripts/e2e-real-client/run.sh

# Override the targets (defaults shown):
CORELINK_E2E_API_HOST=https://corelink-api.humangr.com \
CORELINK_E2E_OCI_HOST=https://corelink-oci.humangr.com \
  bash scripts/e2e-real-client/run.sh

# Help:
bash scripts/e2e-real-client/run.sh --help
```

Exit codes: `0` SHIP (no FAIL **and** `PASS ≥ CORELINK_E2E_MIN_PASS`) · `1`
NO-SHIP (≥1 FAIL **or** a below-floor "vacuum" run) · `2` cannot bootstrap (no
`curl`/`jq`/`openssl`) · `3` usage error.

The JSON summary is written to `scripts/e2e-real-client/last-run.json` by default
(override with `CORELINK_E2E_OUT_JSON`). Shape:

```json
{
  "schema": "corelink.e2e-real-client.v1",
  "timestamp": "2026-06-21T00:00:00Z",
  "endpoint": "https://corelink-api.humangr.com",
  "oci_endpoint": "https://corelink-oci.humangr.com",
  "verdict": "SHIP",
  "counts": { "pass": 12, "gated": 2, "fail": 0 },
  "results": [ { "surface": "docker", "step": "docker push", "status": "PASS", "detail": "…" } ]
}
```

## Idempotency & safety

- Content-addressed payloads embed a per-run timestamp/PID, so every run uses a
  fresh digest — re-running never collides.
- The two Clerk users are throwaway and DSR-deleted on exit (trap).
- A scratch dir under `$TMPDIR` holds the docker context / cargo project / sccache
  home and is removed on exit.
- **No secret is ever echoed** (only length + last-4).

## Layout

```
scripts/e2e-real-client/
├── run.sh            # orchestrator: bootstrap → probes → cert
├── lib/
│   ├── common.sh     # logging, PASS/GATED/FAIL accounting, JSON + cert emit
│   ├── clerk.sh      # Clerk Backend API: create / poll-provision / DSR-delete
│   └── clients.sh    # the per-client probes (docker/cargo/brew/curl surfaces)
└── README.md
```

## Black-box rules

HTTP + the real binaries only. No `corelink-*` crate import, no direct D1/R2/KV
access, no mock of the system under test. The harness is `shellcheck`-clean
(`shellcheck -x scripts/e2e-real-client/run.sh`).
