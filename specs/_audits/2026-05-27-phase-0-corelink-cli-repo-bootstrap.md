# Phase 0.H — `HumanGuardrail/corelink-cli` Repo Bootstrap Runbook

**Date.** 2026-05-27
**Owner.** Gustavo (GitHub org admin)
**Agent.** `CORELINK-CLI-INSTALL-SCRIPT` (Phase 0.H)
**Status.** **PAUSED — awaiting Gustavo's one-click org-repo creation.**
**Estimated Gustavo effort.** 5 minutes.

---

## §1 Why this runbook exists

The Phase 0.H mandate calls for shipping two artifacts in parallel:

1. **`HumanGuardrail/corelink-cli`** — new Rust workspace repo containing the
   `corelink` CLI binary (`ping`, `bazel-init`, `buck2-init` stub,
   `cargo-init` stub, `config show`). Cross-compiled for 5 target triples
   via GitHub Actions, published to GitHub Releases.
2. **`apps/get-corelink-worker/`** — Cloudflare Worker on `get.corelink.io`
   serving the `curl | sh` install script that downloads the binary from
   the GitHub Releases URL in artifact (1).

Phase 0.H pre-flight discovered that **artifact (1) cannot be created by
the agent** — `gh repo view HumanGuardrail/corelink-cli` returns:

```
GraphQL: Could not resolve to a Repository with the name 'HumanGuardrail/corelink-cli'.
```

The `HumanGuardrail` org requires **admin-tier** GitHub permissions to
create new repositories. The agent is running with a personal Gustavo
PAT but the PAT scope does NOT include `admin:org` write. Even if it did,
charter §8 forbids the agent from creating new org-level resources
without an explicit human-in-the-loop click.

Per Phase 0.H §6 (Hard-pause triggers) the agent has therefore:

- Emitted **this runbook** for Gustavo.
- Shipped the **`get-corelink-worker`** in this repo (artifact 2) — the
  Worker is dormant until the GitHub Releases URL in artifact (1) starts
  serving binaries, but it requires no admin action to deploy.
- Paused all CLI-repo work pending Gustavo's completion of §3 below.

---

## §2 What the agent has already done

### §2.1 Shipped (in this repo)

- `apps/get-corelink-worker/` — new CF Worker scaffolding (package.json,
  wrangler.toml, src/index.ts, tsconfig.json, tests). Serves the `curl |
  sh` install script with token + region argument parsing, OS+arch
  detection, and `~/.corelink/config.toml` write-out. The Worker is
  buildable and testable today; it can be deployed to `get.corelink.io`
  as soon as DNS + CF custom-domain are configured (separate runbook
  — see Phase 0.H §6 hard-pause trigger 2).
- Commit: `feat(get-corelink-worker): install-script serving Worker on get.corelink.io`.

### §2.2 Deferred (pending this runbook)

- `HumanGuardrail/corelink-cli` repo creation.
- `crates/corelink/{main.rs, commands/{ping,bazel_init,buck2_init,cargo_init}.rs, config.rs}` —
  the actual CLI source.
- `.github/workflows/release.yml` — cross-compile + publish to GitHub
  Releases for 5 target triples (linux-x86_64, linux-aarch64,
  darwin-x86_64, darwin-aarch64, windows-x86_64).

A follow-up agent dispatch (`CORELINK-CLI-IMPLEMENT`) will execute §2.2
once Gustavo has completed §3 below.

---

## §3 Gustavo's checklist (5 minutes)

Each item is a single click or single command. Do them in order.

### §3.1 Create the empty org repo (1 minute)

1. Open https://github.com/organizations/HumanGuardrail/repositories/new
2. **Owner.** `HumanGuardrail`
3. **Repository name.** `corelink-cli`
4. **Description.** `CoreLink CLI — Bazel/Buck2/Cargo remote cache bootstrapper.`
5. **Visibility.** **Public** (CLI install one-liner is public-facing).
6. **Initialize.** Leave all three checkboxes UNCHECKED (no README,
   no .gitignore, no LICENSE — the follow-up agent will scaffold via
   `cargo new corelink` and write a proper README + LICENSE in the
   first commit).
7. Click **Create repository**.

### §3.2 Grant the bot account write access (1 minute)

The follow-up agent will push using the same PAT that drives all other
`HumanGuardrail/*` commits today.

1. https://github.com/HumanGuardrail/corelink-cli/settings/access
2. Click **Add people** (or the existing CI bot group, if you have one).
3. Add the user / team that owns the PAT used by this orchestrator.
4. Permission: **Write**.

(If you already have a Personal-PAT-as-org-member pattern documented in
`/Users/gustavoschneiter/.claude/projects/-Users-gustavoschneiter-Documents-HuGR/`,
this step is already covered — skip and tell the agent "PAT already
has write to HumanGuardrail/*".)

### §3.3 Configure repo Settings (2 minutes)

In https://github.com/HumanGuardrail/corelink-cli/settings:

- **General → Default branch.** `main`.
- **General → Features.** Disable Wikis, Discussions, Projects (we don't
  use them on `corelink-server` either — keep consistency).
- **Branches → Branch protection rules → Add rule.**
  - Branch name pattern: `main`.
  - Require a pull request before merging: **off** (solo founder; agent
    pushes directly to `main` per existing pattern on `corelink-server`).
  - Require status checks to pass before merging: **on** (we'll add the
    `release.yml` CI checks in the follow-up agent's first commit).
  - Allow force pushes: **off**.
  - Allow deletions: **off**.

### §3.4 Tell the agent it's done (30 seconds)

Reply to the orchestrator with:

> `HumanGuardrail/corelink-cli` is created and PAT has write access.
> Dispatch `CORELINK-CLI-IMPLEMENT`.

The orchestrator will then dispatch the follow-up agent which executes
§2.2 (CLI source + release CI) and SEALs the
`chore(corelink-cli): bootstrap repo with ping + bazel-init + cross-compile CI`
commit per the original Phase 0.H §7 commit message.

---

## §4 Why we did NOT just push to a personal repo

The Phase 0 plan §2.H mandates `HumanGuardrail/corelink-cli` specifically
because:

1. **Install URL stability.** `get.corelink.io` shell-script template
   hard-codes
   `https://github.com/HumanGuardrail/corelink-cli/releases/latest/download/corelink-${OS}-${ARCH}`.
   If we ship under `gustavoschneiter/corelink-cli` and later migrate to
   `HumanGuardrail/`, the install URL breaks for existing customers between
   migrations (GitHub does redirect `/owner-rename/repo` for ~1 year, but
   not for `/old-owner/repo` after a transfer-then-rename pattern).
2. **Brand surface area.** The CLI repo is a customer-facing surface
   (every install one-liner Gustavo sends to a prospect links here). It
   needs to live under the company org, not under a personal account, to
   match the rest of `HumanGuardrail/*` (corelink-server, corelink-docs,
   admin-ui).
3. **License + DPA chain of title.** Sub-processor lists in
   `apps/admin-ui/src/content/sub-processors.json` reference
   "HumanGuardrail as data controller of record". Source under a personal
   account would create a chain-of-title gap that breaks the DPA story
   on first enterprise legal review.

---

## §5 Hard-pause exit criteria

This runbook is **closed** when:

1. `gh repo view HumanGuardrail/corelink-cli` returns 200 (repo exists).
2. The orchestrator has dispatched (or is ready to dispatch) the
   `CORELINK-CLI-IMPLEMENT` follow-up agent.

Until both are true, Phase 0.H acceptance items 1-3 (the four `cargo
build --release` succeeds; `corelink ping` ≤ 2 s; `corelink bazel-init`
idempotency) are **not satisfiable** and Phase 0 cannot graduate to
ship-gate for the F-pane "Connected" badge demo. Acceptance items 4-5
(end-to-end `curl | sh` smoke; SEAL commit) require both runbooks closed.

---

## §6 Cross-references

- Phase 0 plan: `specs/_audits/2026-05-27-phase-0-execution-plan.md` §2.H.
- Hard-pause charter: see Gustavo's
  `corelink_autonomous_execution_charter.md` §inflection-points
  (HumanGuardrail org-admin clicks always require human-in-the-loop).
- Follow-up agent mandate: TBD `CORELINK-CLI-IMPLEMENT`, will pick up
  the §2.2 deferred work list verbatim from Phase 0 plan §2.H "Files to
  write (new repo)".
