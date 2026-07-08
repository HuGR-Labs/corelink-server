# CoreLink CI — self-hosted runners

As of 2026-05-31 **all 95 workflows run on self-hosted runners** — zero
GitHub-hosted minutes, zero Actions billing. Two runner pools:

| Pool | `runs-on` | Jobs | Handles |
|---|---|---|---|
| **macOS fleet** | `[self-hosted, mac, corelink-builder]` | 169 | Rust build/clippy/test, node, python, codesign + notarytool |
| **Linux** | `[self-hosted, Linux, X64]` | 29 | valgrind, buck2 (clang/libc++), k6, semgrep, actionlint `docker://`, osslsigncode, reproducible-build |

> Custom labels (`mac`, `corelink-builder`) are declared in
> `.github/actionlint.yaml`. `Linux` + `X64` are built-in.

---

## 1. Linux runner (NEW — must be set up)

There is **no Linux runner registered yet** — the 29 Linux jobs queue until one
exists. It's free: a Docker container (run it on a Mac via Colima/Docker
Desktop, or any Linux box). See [`linux/docker-compose.yml`](./linux/docker-compose.yml)
for the full one-time setup. TL;DR:

```sh
cd infra/ci-runners/linux
cp .env.example .env          # paste a registration token from repo Settings → Actions → Runners
docker compose up -d
```

For `reproducible-build` (2-runner byte-diff) bring up **2** runners (distinct
`RUNNER_NAME`).

## 2. macOS fleet (currently OFFLINE — power on)

5 runners `corelink-builder-1..5` are registered but **offline**. Bring the
`actions-runner` service back up on each Mac:

```sh
# on each builder Mac, in the actions-runner install dir:
./run.sh            # foreground, or:
sudo ./svc.sh start # if installed as a service
```

Verify all online:

```sh
gh api repos/HumanGuardrail/corelink-server/actions/runners \
  --jq '.runners[] | {name, os, status}'
# expect status: "online" for corelink-builder-1..5 (+ corelink-linux-1)
```

## 3. Windows signing (osslsigncode) — verified, one caveat

`sign-windows.yml` was migrated off Windows-hosted: `signtool` → **osslsigncode**
cross-signing on the Linux runner. Verified locally 2026-05-31 (sign + verify +
RFC-3161 DigiCert timestamp all pass on osslsigncode 2.13).

**Caveat:** `WINDOWS_CODE_SIGNING_CERT` must be a **legacy-cipher PFX** (the
standard for CA-issued code-signing certs). A PFX exported with modern OpenSSL-3
AES ciphers makes osslsigncode fail with a passphrase/UI error — re-export it
legacy (see the comment in `sign-windows.yml`). The job is gated behind the
cert-present check, so it's a no-op until a real signed release runs — **verify
the signed `.exe` on Windows (SmartScreen) on that first release.**

## 4. reproducible-build re-baseline

`reproducible-build.yml` moved from pinned `ubuntu-22.04` to the self-hosted
Linux env. The 2-runner byte-diff stays valid (both land on the same Linux env),
but the **absolute** build hash baseline changes. On the first run after the
Linux runner is up, capture the new hash and update any pinned expected value /
the WI-S12-006 record so the diff has a correct reference.

---

## Billing note

GitHub Actions billing is currently failed/over-limit (jobs on hosted runners
won't start). This migration removes that dependency entirely. The remaining
hosted-billing exposure is **zero** once both pools are online.
