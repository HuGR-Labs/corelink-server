# Docs Reality Gate (`validate_docs_reality.py`)

A structural gate that makes **customer-facing documentation / marketing drift
from code impossible going forward**. It turns "what's left for go-live? does the
CLI actually do what the docs say?" from a manual audit into a green check.

## Why it exists

`docs/knowledge/` (the OKF wiki) is already anti-drift-gated against code
(`validate_okf.py`, C5 = citation stability). But OKF has **zero coverage** of
`apps/docs/` (the product docs site), `marketing/`, or `tools/cli/`. So
customer-facing claims drifted silently. Real cases this gate is built to stop:

- `corelink bazel-init` — documented in the tutorials and referenced in e2e, but
  **never present in the CLI `enum Commands`**. It does not exist.
- The quickstart tells users to compute a **SHA-256** digest (`sha256sum`) and
  upload it, but the CAS plane addresses + client-verifies blobs by **BLAKE3**
  (CTRL-CAS-002) — the server rejects a SHA-256 digest.
- Marketing sells **"BYOK across 4 KMS providers"** as an available capability
  while the OKF `byok` concept records the prod KmsProvider wiring as **deferred /
  still gated-inert** (zero active tenants).

## What it checks

| check | what it does | fails when |
|-------|--------------|-----------|
| **cli-existence** | Every `corelink <subcommand>` referenced in a **code context** under `apps/docs/**`, `marketing/**`, `docs/**` is validated against the CLI `enum Commands` (parsed live from `tools/cli/src/main.rs`), including two-level group actions (`audit export`, `ac put`, `config set`, …). | a referenced command/action does not exist and is not allowlisted |
| **okf-deferred-coherence** | Curated rules — each **grounded** in an OKF concept or the CLI source — flag customer-facing text that asserts a **deferred/unbuilt** capability as if live, or contradicts a canonical code fact (e.g. the CAS digest algorithm). A rule **auto-retires** (WARN: stale) once its grounding marker disappears (the capability shipped). | a rule matches un-hedged claim text |
| **endpoint-existence** *(best-effort)* | HTTP paths named in onboarding recipes (`/v1/cas/…`, `/bazel/v2/…`, `/turbo/…`, `grpcs://…`) are resolved against the wired route table (container `.route(`/`.nest(` + worker route literals). | unresolved → **WARN** (or **FAIL** for a `flagship_files` recipe) |

### Why it is low-false-positive

- CLI references are read **only from code contexts** — shell fenced blocks,
  inline `` `code` `` spans, and HTML `<code>`/`<pre>`. Prose like "CoreLink is a
  cache" or "a corelink mirror caches metadata" is **never** mistaken for a
  command.
- **Non-shell fenced blocks are skipped**, so a Python SDK snippet
  `from corelink import CoreLinkClient` is **not** mistaken for a `corelink import`
  subcommand.
- The CLI truth is parsed **live** from the source enum — it can never go stale
  against the binary.

## The allowlist (`docs_reality_allowlist.json`)

Two CLI buckets keep the gate **green today** yet **meaningful** (new drift
fails):

- **`roadmap_allow`** — intentional forward-looking / other-product / internal-
  tool references. Never a failure. (e.g. `corelink ci` = the build-acceleration
  expansion campaign; `corelink workspace` = the separate Workspaces product.)
- **`tracked_drift`** — **known** drift in customer docs/marketing being
  removed/reconciled by in-flight docs PRs. **Non-fatal** in the normal gate (so a
  green tree stays green while the fix PRs land) but **FAILS under `--strict`**.
  This is the punch-list; shrink it as fixes land. A stale entry is harmless — it
  simply stops matching.

`deferred_coherence` rules may set `"tracked": true` for the same non-fatal /
`--strict`-fatal semantics.

> Roadmap-labeled ≠ drift. Anything that is neither valid, roadmap-allowed, nor
> tracked **fails the gate** — that is how a *newly* introduced bogus command is
> caught the moment it appears.

## Usage

```bash
python3 scripts/validate_docs_reality.py            # the CI gate (green today)
python3 scripts/validate_docs_reality.py --strict   # also fail tracked_drift (the punch-list)
python3 scripts/validate_docs_reality.py --list-refs # dump every CLI reference discovered
```

Exit `0` on clean; non-zero prints a per-check offender list then
`DOCS-REALITY INVALID: <k> failures`. Stdlib-only (no PyYAML). CI wiring:
`.github/workflows/docs-reality.yml`.

## Extending

- **New CLI command shipped** → no action needed; the live enum parse picks it up,
  and any matching `tracked_drift` entry simply stops matching (delete it).
- **New deferred capability oversold** → add a `deferred_coherence` rule with a
  `grounding` marker (OKF concept or CLI source), `forbidden` patterns, and
  `allow` escape patterns for the hedged/legit phrasings.
- **Escalate a flagship recipe** → add its doc path to `endpoint.flagship_files`
  so an unresolved path in it becomes a hard FAIL instead of a WARN.
