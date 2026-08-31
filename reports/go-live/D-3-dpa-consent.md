# D-3 — DPA / consent: is the accepted hash the hash of the bytes the customer saw?

**Version stamp.** Production containers were verified at **`ddd95560-r1`** in all
five regions by `GET /containers/applications/{id}` **per id** — not from the list
endpoint, which serves a stale view. `main` was **12 commits ahead**, **1 of them
container-affecting**. D1 numbers read from `d64742ea` on 2026-08-31.

**Method.** Every query that could return zero carries a control beside it.

---

## 1. The answer: the system cannot tell, and says so in its own doc comment

`crates/corelink-container/src/routes/dpa_accept.rs` stores the **client-attested**
`notice_hash` and does not re-derive it. Its module doc states the reason:

> the engine's `DpaAcceptanceService::accept` also RE-derives the notice hash from
> a server-side `LocaleNoticeRegistry` and rejects a mismatch. That defence-in-depth
> requires the container to hold the EXACT bytes the client hashed.

and then explains that it cannot, because the admin-ui content is not
byte-identical to the legal artifact.

So the recorded answer to "what did the customer see?" is **whatever the client
said it saw**, validated only for well-formedness (hex64). A client that attests
any well-formed hash is accepted.

That is a defensible forensic choice — the comment argues it is *more* honest than
re-deriving against a second, divergent copy — but it means the acceptance record
is **client-asserted, not server-verified**, and nothing in the artifact says so to
a reader who has not read this file.

## 2. The justification cites a file that does not exist

The doc comment names the legal artifact as `legal/dpa/v1.0.0.<locale>.md`.

| query | control | result |
|---|---|---|
| `legal/dpa/v1.0.0.en.md` on `origin/main` | `legal/**` holds **41 files**, listed fine | **MISSING** |
| `apps/admin-ui/src/content/dpa.en.md` | — | present, **2 151 bytes** |

There is no `legal/dpa/` tree at all. The stated reason for not re-deriving — "the
two copies diverge" — cites a second copy that is **not in the repository**. The
conclusion may still be right; the argument for it is not checkable as written.

## 3. Production shows THREE distinct hashes for one version and locale

| | |
|---|---|
| control: rows in `dpa_acceptances` | **24** |
| version / locale groups | **one**: `1.0.0` / `en-US` |
| distinct `notice_hash` in that one group | **3** |

Twenty-four customers accepted the *same* DPA version in the *same* locale and
attested **three different byte-sequences**.

### The three, in time

| hash | accepts | first | last |
|---|---|---|---|
| `acd4f7b5704c` | 12 | 2026-07-10 22:30 | 2026-07-11 13:08 |
| `2d711642b726` | **1** | 2026-07-11 03:37 | 2026-07-11 03:37 |
| `0b8d023331a3` | 11 | 2026-07-11 15:20 | 2026-08-25 19:06 |

`acd4f7b5` → `0b8d0233` is a clean succession: the second starts two hours after
the first ends. That is consistent with the notice text being edited **without a
version bump** — itself worth noting, because the version is what the uniqueness
key `dpa:{tenant}:{version}` and the re-accept gate are keyed on.

**`2d711642` is the finding.** One acceptance, at 03:37, sitting **inside**
`acd4f7b5`'s window — nine hours after it started and nine and a half before it
ended. At that moment every other client was attesting `acd4f7b5`. One customer
attested bytes no one else attested, and nothing checked.

Whether that is a stale browser cache, a partially-rendered notice, a different
locale asset, or a hand-crafted request, **the record cannot distinguish them** —
and neither can we, after the fact, because there is no server-side copy to
compare against.

## 4. What this dossier does NOT decide

- **Which byte-sequence each hash corresponds to.** Without a server-side copy of
  the notice as served on those dates, none of the three can be resolved to text.
  That is the same gap the finding is about.
- **Whether the notice text actually changed** between the two clusters. Strongly
  suggested by the timing, not proven — the admin-ui content file's history was
  not walked.
- **Whether `LocaleNoticeRegistry` is populated at runtime.** The route does not
  re-derive regardless; whether the engine's path is reachable was not exercised.
- **The signup-worker's copy of this flow**, if any.

## 5. Bearing on the promise

An acceptance record exists, is per-tenant, per-version, idempotent, and carries a
hashed IP — the machinery is real. What it does **not** carry is the ability to
answer an auditor's actual question, "show me what this customer agreed to". It
records a hash the customer's own browser computed over bytes we did not keep.

Three distinct hashes for one version, one of them a singleton inside another's
window, is what that gap looks like in production after 24 acceptances.

Backlog item pendente: **id nao alocado**. Enquanto a corrente de PRs de backlog abertos nao entrar na `main`, nao existe id valido — qualquer numero acima do ultimo da `main` ou colide com um elo, ou abre lacuna, e o portao recusa lacuna.
