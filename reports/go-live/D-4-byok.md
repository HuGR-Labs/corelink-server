# D-4 — BYOK: what the docs promise against what the binary does

**Version stamp.** Prod containers verified at **`ddd95560-r1`** in all five regions
by `GET /containers/applications/{id}` **per id** (the list endpoint serves a stale
view). D1 read from `d64742ea` on 2026-08-31.

**Method.** The three levels are kept apart on purpose — **exists**, **linked**,
**reached** — because a crate that compiles proves only the first. Every query
that could return zero carries a control.

---

## Level 1 — does the code exist? YES

`crates/corelink-byok` holds **57 files**, with real AWS KMS, GCP KMS, Azure Key
Vault and Vault provider implementations.

## Level 2 — is it linked into the shipped binary? YES

| query | control | result |
|---|---|---|
| `cargo tree -p corelink-server --edges normal` ⇒ `corelink-byok` | `corelink-handler-cas` → **5** hits | **1** hit, at depth 1 |

Not read off the crate — read off the dependency graph of the binary that ships.

## Level 3 — which provider does that binary actually construct? **the fake**

Provider selection is **compile-time**, not runtime
(`byok_orchestrator.rs::build_active`, a chain of `#[cfg(feature = "byok-*-real")]`
arms with a `#[cfg(not(any(...)))]` fallback).

| fact | value |
|---|---|
| `corelink-server` `[features] default` | **`[]`** |
| Dockerfile build command | `cargo build --release --locked -p corelink-server --bin corelink-server` |
| `--features` in the Dockerfile | **0 occurrences** (control: the string `features` appears 0 times) |

No real-provider feature is enabled, so `build_active()` compiles to its fallback
arm: **`Ok(Arc::new(InMemoryFake::new()))`**.

### What `InMemoryFake` is, in its own words

> **Not for production.** Wraps a DEK by storing the plaintext bytes as the
> ciphertext (XOR-masked with a fixed module-private key so the raw 32-byte
> material does not appear verbatim in memory dumps); unwraps by re-applying the
> XOR mask.

and on the mask constant:

> this is a fake and offers no cryptographic confidentiality; the mask exists only
> so that the raw DEK bytes are not byte-identical to the "ciphertext"

`IN_MEMORY_FAKE_MASK` is a hard-coded 32-byte array in the source. The "wrap" is
XOR against a public constant, and the code says so.

**The docs-vs-binary verdict for this station:** the honest description of the
shipped BYOK is not "designed, not yet wired" — it IS wired, and what it is wired
to is a provider whose own doc-comment says *not for production*.

⚠️ **One thing this dossier corrects in the prevailing shorthand.** "BYOK is an
in-memory XOR" is right about the provider but is sometimes supported by pointing
at `aad_fingerprint` in the AWS provider, which also XORs. That function is an
8-byte AAD tamper-check used in **mock mode** and is not the key wrap. The finding
does not need it, and citing it weakens the case.

## Production state: nothing uses it, and everyone is marked as using it

| query | control | result |
|---|---|---|
| rows in `tenant_byok_config` | `tenant` → **262** rows | **0** |
| rows in `byok_envelope` | same | **0** |
| tenants with `byok_status = 'active'` | same | **262 — all of them** |

Zero configurations, zero envelopes, and **every tenant in production carries
`byok_status = 'active'`**.

That is not a grant: `migrations/d1/0031_byok_tenant_status.sql` declares
`ALTER TABLE tenant ADD COLUMN byok_status TEXT DEFAULT 'active'`. The column
defaults to `active`, so it says `active` for tenants who never bought BYOK,
never configured it, and have no envelope.

**A status column whose default is the affirmative value cannot distinguish "on"
from "never asked".** Anything that reads `byok_status` as an entitlement — a
dashboard, an export, a compliance answer — reads `active` for all 262.

## What this dossier does NOT decide

- **Whether any customer was billed for BYOK.** Not queried; that is the money
  side and belongs with the documentary lane.
- **Whether the SLA text promises what B-083 says it promises.** Deliberately out
  of scope here — this lane is the binary, the other lane is the document, and
  they are meant to be compared *after* both are settled independently.
- **Whether a real provider works.** The AWS/GCP/Azure implementations were not
  exercised; the finding is that they are not the ones compiled in.
- **Whether the boot audit line fires.** `make_provider` emits
  `provider = <label>` at `corelink.byok.orchestrator.audit` on boot, which would
  name the active provider in the logs. Not read here, and it is the cheapest
  possible confirmation of everything above — worth doing next.

## Verdict

Exists: **yes**. Linked: **yes**. Reached: **yes — and it lands on a fake that
documents itself as unfit for production**, while every tenant row says `active`
because that is the column's default.

Backlog item pendente: **id nao alocado**. Enquanto a corrente de PRs de backlog abertos nao entrar na `main`, nao existe id valido — qualquer numero acima do ultimo da `main` ou colide com um elo, ou abre lacuna, e o portao recusa lacuna.
