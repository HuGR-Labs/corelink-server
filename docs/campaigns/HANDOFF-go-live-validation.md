# HANDOFF — go-live validation campaign

**Measured:** 2026-09-01T17:13:10Z
**Measurement ref:** `origin/main` @ `4a72560a6a1f77a60c10fd462f338c86011e89c3`
**Written by:** a merge-guardian session (the session identifier is historical)
**For:** any model, session, or harness picking this campaign up.

This document is written so that a reader with **no memory of the campaign** can
continue it. Mutable state below is measured against the ref and timestamp above;
commands are included so a later session can re-derive rather than trust this
snapshot. Claims from older audit reports are labelled with their own ref and
timestamp.

---

## 1. What the campaign is

Validate that CoreLink can go live, across three repos:

- `HuGR-Labs/corelink-server` — the product (Workers + Durable Objects + Containers + R2 + D1)
- `HuGR-dev/corelink-runners` — the runner fleet / `fabricd` control plane
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

One session acts as **sole merge guardian** for all three repos. The identity of
that session is runtime state, not a durable fact; query the active session before
merging.

- **Merging is done with exactly one command**, never piped:
  ```bash
  bash scripts/pre-merge-gate-check.sh --merge <PR>
  ```
  A pipeline's exit status is the last command's, so `... | tail -5 && gh pr merge`
  discards the gate's refusal. That is how a PR can merge while checks are
  pending; count the checks in the explicit run before treating the gate as green.
- **No PR merges without an adversarial cold review.** The reviewer contract is
  reproduced in §7.
- The guardian **does not deploy**. Prod deploys and container repins are the
  owner's, or explicitly delegated per occasion.

---

## 3. State on `origin/main`, measured

### Backlog

```bash
git fetch origin main
python3 - <<'PY'
import re,collections
import subprocess
s=subprocess.check_output(['git','show','origin/main:BACKLOG.md'], text=True)
blocks=re.findall(r'```backlog\n(.*?)```',s,re.S)
st=collections.Counter(); ow=collections.Counter(); ids=[]
for b in blocks:
    ids.append(re.search(r'^id:\s*(\S+)',b,re.M).group(1))
    st[re.search(r'^status:\s*(\S+)',b,re.M).group(1)]+=1
    ow[re.search(r'^owner:\s*(\S+)',b,re.M).group(1)]+=1
assert len(ids) == len(set(ids)), 'duplicate backlog IDs'
assert all(re.fullmatch(r'B-\d{3}', i) for i in ids), 'non-canonical backlog ID'
numbers=[int(i[2:]) for i in ids]
assert sorted(numbers) == list(range(1, max(numbers)+1)), 'backlog ID gap'
next_id=f'B-{max(numbers)+1:03d}'
print(len(blocks), dict(st), dict(ow), 'next', next_id)
PY
```

Result at the measured ref: **167 items — 110 open, 56 done, 1 parked.** Owner
field: 155 `tl`, 12 `owner`. IDs run **B-001 … B-167 with no gaps**, so the next
free ID is **B-168**. The command asserts duplicate-free canonical IDs and no
gaps, and prints `next B-168`; it reads the fetched `origin/main` blob, not the
current checkout, and must be rerun after every merge before allocating an ID.

The 12 `owner` items at this measurement are: **B-008, B-012, B-013, B-032,
B-035, B-065, B-086, B-089, B-097, B-110, B-111, B-154**. This is a census of
the `owner:` field, not a claim that every item is currently actionable; read each
block for its specific external dependency.

**Id allocation is a race.** Two sessions writing `B-168` do **not** produce a
merge conflict — the density gate refuses **gaps**, and `id: B-UNALLOCATED` passes
CONFIRMED while being invisible to the density check. **Allocate the id at push
time**, not while drafting.

**`status: done` requires INVERTED verify polarity.** An item written with `open`
polarity goes green in the PR that fixes it and **red on the next merge**,
contaminating every sibling PR. Check polarity before marking anything done.

**`python3 scripts/backlog_verify.py` with no `--id` fires external tools**
(including cargo, wrangler, and curl). Use `--id B-NNN` so the selected item is
the population being verified.

### Open PRs

At the measurement timestamp there were **33 open PRs in `corelink-server`, 1 in
`corelink-runners` (#516), and 2 in `corelink-workspaces` (#256 plus draft #247)**.
The former workspace #255 is already merged (2026-09-01); #1530 in the server
repo is closed (not merged). No current conflicting set is asserted here;
recompute mergeability from the live inventory before choosing a rebase wave.

> ⚠️ **A conflicting PR can expose an incomplete or stale check set, and the
> gate can read that as green.** Do not ask "are there failures?" — **count the
> checks against a sibling PR at the same base**. An absent check never becomes
> pending.

Re-derive the list:
```bash
date -u '+%Y-%m-%dT%H:%M:%SZ'
for r in corelink-server corelink-runners corelink-workspaces; do
  gh pr list -R HuGR-Labs/$r --state open --limit 60 \
    --json number,title,isDraft,mergeable,headRefOid,baseRefOid,updatedAt \
    --jq '.[]|"\(.number)\t\(if .isDraft then "DRAFT" else .mergeable end)\t\(.headRefOid)\t\(.baseRefOid)\t\(.updatedAt)\t\(.title[0:78])"'
  sleep 2
done
```

---

## 4. The open decision that blocks real work: BYOK

**The owner has said BYOK must be turned on** (*"Sim tem que ligar"*), and, when
told the design was single-provider, rejected that as implausible for a product —
correctly.

The code observations in the next table are from `origin/main` at the measurement
ref/time in §3. The `file:line` values are locators into that snapshot, not
permanent identifiers; re-run `git show origin/main:<path>` after a merge.

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
`crates/corelink-container/src/byok_orchestrator.rs:76-117` and
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

### Independent evidence lanes (do not join them into one conclusion)

These are deliberately separate populations and dates. A green result in one
lane does not prove either of the others:

| lane | evidence and population | status to revalidate |
|---|---|---|
| BYOK | Static code/build evidence above: four provider client files exist, but `Dockerfile` builds with no real-provider feature. This says what the tree can compile, not what a tenant can use. | Product decision remains open; no production enablement is claimed. |
| audit drain | `B-064` in `BACKLOG.md` at `origin/main` @ `4a72560a` records the server/cron contract mismatch (`rows_sealed`/`partitions_drained`/`incomplete` versus `j.sealed`/`j.partitions`). A separate historical production observation in `B-026` reports **45,836** archived rows and **11,818** quarantined rows across **8** forked partitions on 2026-08-24. Those rows are `audit_outbox` evidence, not `customer_audit_events`, and do not prove that the current drain is healthy. | B-064 is still `open`; rerun the source-contract check and obtain a fresh production observation before closing it. |
| billing | `B-065` records a historical detector result of **8/8** failed `billing-health-daily` runs as of 2026-08-30, with **3** event types observed under both ID schemes in the last **30 days**. This is the detector's event population, not proof of a current Stripe configuration. | Owner-only Stripe action; do not infer billing health from BYOK or audit evidence. |

The command for the repository portions is fixed and non-secret:

```bash
git show origin/main:BACKLOG.md | rg -n -A45 '^### B-064|^### B-065'
```

The production portions require the owner-controlled systems and must be
re-measured with their native read-only instruments; no credential or payload is
to be copied into this handoff.

---

## 5. In-flight work at handoff

The inventory above is the live population. The table below preserves review
findings as historical evidence; it is not a claim that any finding is still
unfixed. Re-open the PR at its recorded head and re-run the cold review before
merging.

| target | what was dispatched |
|---|---|
| `corelink-server#1531` | cold review returned **FIX-FIRST**. The B-090 `verify` uses `[^-]` to exclude `npm install -g`, which also excludes **every flag-first npm install** including `npm install --legacy-peer-deps` — the exact command the PR removes. Reviewer reintroduced it and the gate stayed **green, exit 0**. Additional findings covered the specifier count, modified-lane coverage, wrangler-version provenance, and changelog fragment filename; revalidate them at the PR head before merging. |
| `corelink-workspaces#255` | **MERGED 2026-09-01**. Its historical cold review returned **FIX-FIRST**: replacing the selftest's own comparator with `if true` left it green despite the forced mutation, and it stayed green with the PR's blocking defect reintroduced. The separate hole at `scripts/gate-lane-selftest.sh:47` remains out of scope for that merged PR; revalidate it as a new backlog item. |
| BYOK / Buck2 / pentest residue | measuring how many published claims assert "4 providers" versus "BYOK exists", and what enabling requires beyond `--features`. |
| C-4 contract findings | Historical C-4 snapshot below: ToS SLA credits with no mechanism, `/openapi.json` serving the DevEnv spec, **13** phantom paths, **8** dead DevEnv endpoints, and **4** live legal/trust 404s. These counts belong to the C-4 report ref/time, not automatically to the current tree. |

At the measurement timestamp, the current server PR set also included open work
on **#1540, #1533, #1534, #1535, #1536, #1538, #1539, and #1542–#1544**.
`#1537` merged at 2026-09-01T17:01:37Z and is not part of the open set.
Their review status and heads are mutable; use the inventory command above rather
than treating this list as a dispatch ledger.

### C-4 numeric provenance

The C-4 figures are not one current population. The report
`de3ec37c` is dated **2026-08-31** and states its own base as `origin/main` @
`0dd4a31b`. Its **44** rows are capability claims over the report's published
surface (the report enumerates **1,148** files and **512** EN files); its
**13** phantom paths are the canonical OpenAPI YAML population discussed by
#1542; its **8** dead DevEnv endpoints are the one production `/openapi.json`
response sampled by #1542; and its **4** live 404s are the legal/trust-link
population in the C-4 report. The strict route comparator used by #1539 reports
**14** missing routes because it measures a broader documented-route population.
These numbers must not be added together or described as a live post-merge
count. Re-run the relevant instrument at the current ref after #1539/#1542 land.

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
  overwritten by parallel sessions, and a victim then executed the other
  session's script in its own worktree, producing a large unintended diff.
- **Do not touch `~/Downloads`** — it holds B-013 private keys, an owner action.
- **`.env.local` contains LIVE keys under `*_LIVE_*` names.** Never paste secret
  values; redact by **name**.
- **Commit trailers must be contiguous, with no blank line between them:**
  ```
  Signed-off-by: Gustavo Schneiter <cachorronarigudo26@gmail.com>
  Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
  ```
- **Measure the intersection of ITEMS, not themes, before opening a wave.**
  Overlapping waves have produced duplicate PRs that were later closed; compare
  the exact backlog IDs and owned files before dispatch.
- **`gh api rate_limit` is blind to the secondary limit.** A prior probe read the
  primary quota as available and the next burst still hit a secondary-limit
  response.
  The secondary limit triggers on **rate**, not volume, and retrying **extends**
  the window. The fix is to **serialize**, not to wait. Architecturally: CI
  measurement was taken *out* of the reviewers — one session measures, reviewers
  read code from disk.

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
  set it measures, and that set is not what the sentence promises.* Multiple
  measured instances are recorded in the corresponding backlog blocks. No gate
  catches it because tests check the number, review checks the prose, and
  **nobody checks the join**.
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

At the measured `origin/main` ref, the owner field names these 12 items:
**B-008** PagerDuty escalation · **B-012** non-Actions CI credential · **B-013**
three private keys in `~/Downloads` · **B-032** Drata evidence · **B-035** TLS
contract decision · **B-065** Stripe dashboard · **B-086** D1 residency decision ·
**B-089** SLA-credit/legal decision · **B-097** account scale/quota decision ·
**B-110** hosted-Actions billing · **B-111** Apple/Windows certificates ·
**B-154** executed legal instruments.

This list is the owner-field census, not a claim that all 12 are blocked by the
same person or have the same next action. Read the corresponding backlog block
at the measurement ref before attempting any action. In particular, the former
references to **B-028** and **B-160** here were stale and have been removed.

Also awaiting an owner decision: the **Neon quota suspension**. The
`corelink-fabricd` control-plane status is external and must be checked live; the
decision is whether to raise the plan or accept the degraded in-memory ledger.

---

## 10. First moves for whoever picks this up

1. Re-derive §3 (backlog census, open PRs) — **do not trust the numbers above**;
   they are a timestamped snapshot. Fetch `origin/main` first and record the new
   SHA/time.
2. Inspect the live PR list before rebasing. #1530 is closed, #255 is merged, and
   #256 is the current workspace PR; do not resurrect the stale references.
3. Rebase or close only PRs that are actually conflicting, counting checks against
   a sibling when GitHub reports `UNKNOWN`.
4. Land the reviewed-and-fixed PRs, one cold review each, via
   `pre-merge-gate-check.sh --merge`.
5. Get the owner's answer on BYOK scope, then freeze the contract in §4 and
   dispatch WP-1 and WP-2 in parallel.
6. Keep writing skills as classes emerge. That is an explicit owner instruction:
   *"vai criando skills de manutenção com essas coisas, pra não perdermos tempo
   outras vezes."*
