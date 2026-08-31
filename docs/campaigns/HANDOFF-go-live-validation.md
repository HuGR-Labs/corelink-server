# HANDOFF — go-live validation campaign

**As of:** 2026-08-31
**Written by:** the merge-guardian session (`audit-report-analysis-4e1455-0f`)
**For:** any model, session, or harness picking this campaign up.

This document is written so that a reader with **no memory of the campaign** can
continue it. Everything asserted here was measured on `origin/main` at the time of
writing; commands are included so you can re-derive rather than trust.

---

## 1. What the campaign is

Validate that CoreLink can go live, across three repos:

- `HuGR-Labs/corelink-server` — the product (Workers + Durable Objects + Containers + R2 + D1)
- `HuGR-Labs/corelink-runners` — the runner fleet / `fabricd` control plane
- `HuGR-Labs/corelink-workspaces` — workspace tooling + mutation lane

The standing contract is **`BACKLOG.md` at the repo root of `corelink-server`** —
the single source of truth for open work **across all three repos**. Not session
notes, not memory files, not a chat thread. If it is not in `BACKLOG.md`, it is
not tracked.

Owner mandate, restated by the owner during this campaign:

> *"o padrão é state of the art, tudo precisa ser absolutamente impecável. Não
> deixamos débitos, loose ends, gambiarras, band-aids, nem pra fazer depois."*

And on process:

> *"o correto é você pegar, jogar no backlog → planejar → delegar."*

Ceremony is not a virtue here. One cold review per PR; residue becomes backlog
items rather than another review round.

---

## 2. Role and authority

One session acts as **sole merge guardian** for all three repos. At the time of
writing that is `audit-report-analysis-4e1455-0f`.

- **Merging is done with exactly one command**, never piped:
  ```bash
  bash scripts/pre-merge-gate-check.sh --merge <PR>
  ```
  A pipeline's exit status is the last command's, so `... | tail -5 && gh pr merge`
  discards the gate's refusal. That is how a PR merged with 4 checks pending.
- **No PR merges without an adversarial cold review.** The reviewer contract is
  reproduced in §7.
- The guardian **does not deploy**. Prod deploys and container repins are the
  owner's, or explicitly delegated per occasion.

---

## 3. State on `origin/main`, measured

### Backlog

```bash
python3 - <<'PY'
import re,collections
s=open('BACKLOG.md').read()
blocks=re.findall(r'```backlog\n(.*?)```',s,re.S)
st=collections.Counter(); ow=collections.Counter(); ids=[]
for b in blocks:
    ids.append(re.search(r'^id:\s*(\S+)',b,re.M).group(1))
    st[re.search(r'^status:\s*(\S+)',b,re.M).group(1)]+=1
    ow[re.search(r'^owner:\s*(\S+)',b,re.M).group(1)]+=1
print(len(blocks), dict(st), dict(ow))
PY
```

Result at handoff: **167 items — 110 open, 56 done, 1 parked.** Owner field: 155
`tl`, 12 `owner`. Ids run **B-001 … B-167 with no gaps**, so the next free id is
**B-168**.

**Id allocation is a race.** Two sessions writing `B-168` do **not** produce a
merge conflict — the density gate refuses **gaps**, and `id: B-UNALLOCATED` passes
CONFIRMED while being invisible to the density check. **Allocate the id at push
time**, not while drafting.

**`status: done` requires INVERTED verify polarity.** An item written with `open`
polarity goes green in the PR that fixes it and **red on the next merge**,
contaminating every sibling PR. Check polarity before marking anything done.

**`python3 scripts/backlog_verify.py` with no `--id` fires ~24 external tools**
(cargo, wrangler, curl). Use `--id B-NNN`.

### Open PRs

30 open in `corelink-server`, 1 in `corelink-runners` (#516, conflicting), 3 in
`corelink-workspaces` (#255 plus two drafts marked do-not-merge).

Six `corelink-server` PRs are **CONFLICTING** and need a rebase before anything
else: **#1530, #1497, #1493, #1490, #1450, #1397, #1393**.

> ⚠️ **A conflicting PR gets either ~6 trivial checks or 54 stale ones, and the
> gate can read that as green.** Do not ask "are there failures?" — **count the
> checks against a sibling PR**. An absent check never becomes pending.

Re-derive the list:
```bash
for r in corelink-server corelink-runners corelink-workspaces; do
  gh pr list -R HuGR-Labs/$r --state open --limit 60 \
    --json number,title,isDraft,mergeable \
    --jq '.[]|"\(.number)\t\(if .isDraft then "DRAFT" else .mergeable end)\t\(.title[0:78])"'
  sleep 2
done
```

---

## 4. The open decision that blocks real work: BYOK

**The owner has said BYOK must be turned on** (*"Sim tem que ligar"*), and, when
told the design was single-provider, rejected that as implausible for a product —
correctly.

### What was measured

| piece | state |
|---|---|
| `crates/corelink-byok/src/byok_core.rs:72` | `pub trait KmsProvider: Send + Sync` exists |
| `crates/corelink-container/src/byok.rs:94` | `make_active_provider() -> Arc<dyn KmsProvider>` — **already a trait object** |
| `crates/corelink-byok/src/byok_revocation/detector.rs:96` | **already holds `Vec<Arc<dyn KmsProvider>>`** — several providers at once, at runtime |
| `migrations/d1/0030_byok_envelope.sql` | `PRIMARY KEY (tenant_id, blob_hash)` + `kms_provider CHECK IN ('aws_kms','gcp_kms','azure_key_vault','hashicorp_vault')` + `kms_key_id` + `kms_region` — **the schema is already per-tenant, per-provider** |
| four provider clients | `byok_aws/real.rs` 1008 lines · `byok_gcp/real.rs` 1035 · `byok_azure/real.rs` 1314 · `byok_vault/real.rs` 902 — **~4,250 lines, all real HTTP clients** |
| `Dockerfile:182` | `cargo build --release --locked -p corelink-server --bin corelink-server` — **no `--features`**, so `default = []` wins and only the in-memory fake compiles |

### The misread to not repeat

`byok_orchestrator.rs:157` — `pub const fn active_provider() -> ActiveProvider` —
was read as the dispatch. **It is a label.** Its call sites are `/healthz`
(`main.rs:1058`), a metrics label (`byok_orchestrator.rs:209`) and a log field.
The conclusion "single-provider architecture, needs a rewrite" was **wrong** and
was retracted. See `.claude/skills/built-not-wired/`.

### What actually limits it to one provider

Only the **construction site** (`#[cfg(feature = "byok-*-real")]` around where the
real type is built) and the mutual-exclusion `compile_error!` guards in
`crates/corelink-byok/src/lib.rs:105-118`. The Cargo.toml comment states the guard
checks **only the public features**, and `_matrix-test` already combines all four
`_internal-*` flags — so combining is not architecturally forbidden.

### The real gap: credentials are per-process, not per-tenant

```
AwsKmsRealProvider::new(region)                    // ambient credentials
AzureKeyVaultRealProvider::new(region, vault_url)  // AZURE_CLIENT_SECRET from the process
VaultRealProvider::from_env(region)                // VAULT_APPROLE_ROLE_ID / _SECRET_ID
```

One credential per process serves one customer. **This is where the
one-per-deployment design is actually written down** — not in the dispatch.

### Frozen decision (guardian's call, not yet implemented)

**Store no customer cloud credential.** AWS, GCP and Azure all support identity
federation — the customer grants access to *our* identity on *their* key
(cross-account role with `external_id`, workload identity federation,
multi-tenant app registration with consent). Nothing secret passes through our
hands, and revocation stays unilaterally theirs. **Vault is the exception**: it
requires a stored AppRole `secret_id`, which goes encrypted in the per-tenant
store.

### Work breakdown (disjoint; not yet dispatched)

1. Provider constructors accept explicit per-tenant credentials — `corelink-byok`, 4 files.
2. D1 table for per-tenant BYOK configuration (provider, key id, region, federation identifiers).
3. Registry `tenant_id → Arc<dyn KmsProvider>` with cache, replacing the `cfg` construction; `active_provider()` label becomes a *set*. **Depends on 1 and 2 — freeze the contract first.**
4. `--features` in the `Dockerfile` + `/healthz` reporting the set.

The owner's instruction if this turns out to be too large: *"Se for muito
complexo, escolhemos 1 ou 2 provedores iniciais."* It is **not** too large — the
hard part (four clients, trait, type erasure, per-tenant schema) already exists.

---

## 5. In-flight work at handoff

Background agents were running against these; check the PRs' current state rather
than assuming the work landed.

| target | what was dispatched |
|---|---|
| `corelink-server#1531` | cold review returned **FIX-FIRST**. The B-090 `verify` uses `[^-]` to exclude `npm install -g`, which also excludes **every flag-first npm install** including `npm install --legacy-peer-deps` — the exact command the PR removes. Reviewer reintroduced it and the gate stayed **green, exit 0**. Plus 6 lower findings (wrong specifier count, neither modified lane runs on this PR, wrangler version divergence in a bundle-size claim, changelog fragment filename). |
| `corelink-workspaces#255` | cold review returned **FIX-FIRST**. Replacing the selftest's own comparator with `if true` leaves it **green at 12/12**, and it stays green with the PR's own blocking defect reintroduced. Same hole exists at `scripts/gate-lane-selftest.sh:47` — **file that as a backlog item, it is out of scope for #255**. |
| BYOK / Buck2 / pentest residue | measuring how many published claims assert "4 providers" versus "BYOK exists", and what enabling requires beyond `--features`. |
| C-4 contract findings | ToS SLA credits with no mechanism, `/openapi.json` serving the DevEnv spec, 13 phantom paths, 8 dead DevEnv endpoints, 4 live 404s. |

Also open and unreviewed: **#1540** (this campaign's three maintenance skills),
**#1533/#1534/#1535/#1536/#1537/#1538/#1539**.

---

## 6. Standing constraints — violating these has cost real time

- **Zero hosted GitHub Actions.** `ubuntu-latest` is billing-blocked. Every
  `runs-on` must be `corelink` or `[self-hosted, macOS, X64]`.
- **The self-hosted runners ARE the owner's Mac.** Heavy compiles and CI bursts
  have crashed it. Use `CARGO_BUILD_JOBS=4`. **Never `pkill` by pattern** — it
  kills the CI runner. Kill by PID; restart wedged runners with
  `launchctl kickstart` by label.
- **Every agent/session gets its own worktree.** Never work in the root checkout.
  Worktrees share the clone, index and stash: **never `git add -A`** (it steals
  other sessions' WIP) and never bare `git stash` / `git stash pop`.
- **Scratchpad filenames must be unique per agent.** Generic names have been
  overwritten by parallel sessions, and the victim then executed the other
  session's script in its own worktree — once producing 4,083 modified files.
- **Do not touch `~/Downloads`** — it holds B-013 private keys, an owner action.
- **`.env.local` contains LIVE keys under `*_LIVE_*` names.** Never paste secret
  values; redact by **name**.
- **Commit trailers must be contiguous, with no blank line between them:**
  ```
  Signed-off-by: Gustavo Schneiter <cachorronarigudo26@gmail.com>
  Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
  ```
- **Measure the intersection of ITEMS, not themes, before opening a wave.** Two
  waves with overlapping item lists produced four duplicate PRs that were all
  closed.
- **`gh api rate_limit` is blind to the secondary limit.** It read 4935/5000 while
  the next call took a 403. The secondary limit triggers on **rate**, not volume
  (65 calls in one second was enough), and retrying **extends** the window. The
  fix is to **serialize**, not to wait. Architecturally: CI measurement was taken
  *out* of the reviewers — one session measures, reviewers read code from disk.

---

## 7. The cold-review contract (reproduce this for every PR)

A reviewer is given the PR and told to:

1. Create its **own** worktree/clone under `/private/tmp` with a unique name
   including a random suffix. Never the root checkout.
2. **Read code from disk, not CI.** Do not spend API calls measuring checks; the
   guardian does that once.
3. **Attack the PR's central claim** — name the claim, then try to falsify it by
   reading files, not by trusting the body.
4. **Teeth-test any gate the PR adds or changes**: mutate by **content** (never by
   line number), verify the mutation actually applied, report the count and the
   printed string before and after. See `.claude/skills/teeth-test/`.
5. For **every number in the PR body**, state the population it measures and
   whether the sentence it supports is about that same population. See
   `.claude/skills/verify-population/`.
6. Confirm `runs-on` is self-hosted.
7. Return a **VERDICT** (APPROVE / FIX-FIRST / REJECT), findings one line each
   with `file:line`, **and an explicit list of what could NOT be verified**.
8. Never merge, never push, never comment on the PR.

When the reviewer reads CI at all, it does **one** structured read and **says the
total number of checks it saw** — so an absent check can be spotted by comparison
with a sibling PR.

---

## 8. Defect classes this campaign keeps paying for

These are now written up as skills in `.claude/skills/` (PR #1540 at handoff):

- **`verify-population`** — the dominant class: *the number is correct about the
  set it measures, and that set is not what the sentence promises.* Six measured
  instances. No gate catches it because tests check the number, review checks the
  prose, and **nobody checks the join**.
- **`teeth-test`** — a gate that has never failed has not been tested. Includes a
  measured false pass where a `verify` replacement exited 0 and printed CONFIRMED
  **while running the old predicate**, having silently deleted a neighbouring
  backlog item.
- **`built-not-wired`** — existing is not shipping. Reachability is a property of
  the **target**; `cargo tree` needs an explicit `--target`, and `--target all`
  returns paths that exist in no build.

Two further rules worth carrying:

- **A self-accusing report skips scrutiny.** A claim that indicts its own author
  does not trigger the suspicion a defensive claim triggers, and is wrong just as
  often. Re-derive it. A working monitor was switched off this way.
- **A caveat is not the same as not asserting.** Hedging a sentence does not fix a
  wrong population.

---

## 9. Genuinely owner-blocked (do not attempt; do not re-file)

**B-008** PagerDuty escalation · **B-013** three private keys in `~/Downloads` ·
**B-028** three advisories with no upstream patch · **B-032** Drata evidence ·
**B-065** Stripe dashboard · **B-111** Apple/Windows certificates · **B-160**
account plus two PATs.

Also awaiting an owner decision: the **Neon quota suspension** — the
`corelink-fabricd` control plane has been down since 2026-08-19; either raise the
plan or accept the degraded in-memory ledger.

---

## 10. First moves for whoever picks this up

1. Re-derive §3 (backlog census, open PRs) — **do not trust the numbers above**;
   they were true at the moment of writing.
2. Rebase or close the conflicting PRs, counting checks against a sibling.
3. Land the reviewed-and-fixed PRs, one cold review each, via
   `pre-merge-gate-check.sh --merge`.
4. Get the owner's answer on BYOK scope, then freeze the contract in §4 and
   dispatch WP-1 and WP-2 in parallel.
5. Keep writing skills as classes emerge. That is an explicit owner instruction:
   *"vai criando skills de manutenção com essas coisas, pra não perdermos tempo
   outras vezes."*
