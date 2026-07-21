# Real-client "operate-the-binary" e2e ledger — 2026-07-19

Every cache surface exercised against **prod** with the **actual client binary a real
user runs** (not curl-as-proxy — curl masked real defects here before). Tenant `f0006`
(RW PAT). Companion to the HTTP black-box suite (`tests/e2e-user-journeys`, 101 PASS)
which is necessary but NOT sufficient: it never runs the real clients.

## Ledger

| Surface | Real client | Verb(s) proven | Result |
|---|---|---|---|
| Native CAS/AC | `corelink` CLI | login·whoami·doctor(6ok/2skip)·put·stat·get·ac put/get·audit tail | ✅ **GREEN** (BLAKE3 local == server) |
| Native CAS `ls` | `corelink ls` | list | ❌ **BUG** → PR **#845** (invented route `/v1/cas/list?tenant=` → 403; real is `/v1/cas/{tenant}`; +schema; +`cas export`/`tenant export` same bug) |
| OCI registry | `docker` | login·build·push·pull | ✅ **GREEN** round-trip (`corelink-oci.humangr.com`) |
| npm mirror | `npm` | `npm install is-odd` | ✅ **GREEN** (added 2 packages) |
| pip mirror | `pip3` | `pip install six` | ✅ **GREEN** (six.py installed) |
| Turborepo remote cache | `/v8/artifacts` | PUT·GET + **cross-tenant** | ✅ **GREEN** (turbo binary absent → protocol proven). Cross-tenant **ISOLATED live** (A writes, B→404) |
| Bazel REAPI v2 | REAPI/ByteStream | CAS write(204)/read(200)·AC·findMissingBlobs(200) | ✅ **GREEN** for a REAPI client. Stock `bazel --remote_cache` → 404 **by documented design** |
| sccache / cargo | `sccache` 0.15 | connect·store | ❌ **2 layered bugs** — TLS-1.3 [FIXED] + **MKCOL 403** [open #96] |
| Homebrew | `brew` 6.0.11 | fetch | ❌ real client can't route via documented recipe [open #97] (mirror itself serves manifests 200 w/ Bearer) |

## Findings

1. **CLI `ls` broken** (+ `cas export`, `tenant export`) — invented route + wrong response schema. → **PR #845** (230 tests, clippy/fmt clean). Same class as #842.
2. **TLS-1.3-only floor** on `corelink-api`/`signup`/`oci` — blocked native-tls/SecureTransport-macOS clients (sccache) & any TLS≤1.2 client. **FIXED**: lowered humangr.com zone `min_tls_version` 1.3→1.2 via CF API (owner-authorized). All Workers hostnames now accept 1.2; curl still negotiates 1.3. Not codified in repo → **consider ADR/IaC note** so it can't silently drift back.
3. **sccache MKCOL 403** [#96] — the cargo/WebDAV adapter fails-closed on `MKCOL`; real sccache (opendal) shards keys `X/Y/Z/hash` and issues MKCOL to create parent dirs before PUT → 403 "insufficient cache scope" → 0% hit. `PUT /cargo/{t}/6/b/4/hash` + `GET` both 200 (sharded path fine); only MKCOL rejected. curl PUT never does MKCOL → the #434 "verified live" was curl-only. **Fix:** treat MKCOL (likely OPTIONS/PROPFIND too) as a success no-op in `cargo_gate` (flat KV store, dirs implicit).
4. **brew real-client gap** [#97] — mirror serves bottle manifests (200 w/ Bearer) but Homebrew 6.0.11 didn't route via `HOMEBREW_ARTIFACT_DOMAIN`/`BOTTLE_DOMAIN`, and the mirror requires Bearer auth the domain-var mechanism can't attach. Recipe outdated and/or public-bottle reads should be anonymous. Needs product/config decision.
5. **turbo isolation HIGH is STALE** — `docs/FINDING-turbo-tenant-isolation.md` ("awaiting Owner's call", 2026-06-05) describes a fixed state; `caller_tenant` now = PAT-resolved `x-corelink-tenant-id`. **Verified isolated LIVE** (A writes, B→404). → close/update the finding doc (a live-looking HIGH in-repo misleads).
6. **bazel findMissingBlobs**: malformed JSON → 500 (Internal) instead of 400. Intentional (test `bad_json_is_internal`) but a **LOW** API-hygiene nit (client error surfaces as 5xx, pollutes error rates).
7. **CLI release follow-up** — #842/#845 fix the *source*; users get the working binary only when a new `corelink-cli` GitHub release is cut (ships separately).

## Scope note

Launch product (cache + governance via the `corelink` CLI + browser) = **GREEN**. The
sccache/brew defects are on the **post-launch CI/build-acceleration expansion** surfaces
(cache still stores/serves via curl+PUT; only the specific real-client handshake is
broken). Not launch-blocking, but real for the expansion.

---

## Rigorous evidence re-grade (owner-challenged: edge/malicious/cross-org/warm-cold/perf, evidence per request)

**Loose "GREEN" was not good enough.** Re-evaluated every request return. Findings the loose checks masked:

### Adversarial / isolation matrix — 14/14 correct (per-request evidence)
| Attack | Request | Result |
|---|---|---|
| Cross-org read | TenantB GET /v1/cas/A/{blob} + list | **403 / 403** ✓ |
| Cross-org write | TenantB PUT /v1/cas/A, PUT /cargo/A | **403 / 403** ✓ |
| Scope escalation | RO PUT cas, PUT cargo, DELETE cas | **403 / 403 / 403** ✓ |
| Forged headers | TenantB+`x-corelink-tenant-id:A`; RO+`x-corelink-scope:write` | **403 / 403** (stripped) ✓ |
| Unauth | no-PAT GET cas, PUT cargo | **401 / 401** ✓ |
| Edge | hash-mismatch PUT; short digest; `..%2f` traversal | **422 / 400 / 400** ✓ |

### Real-client evidence re-grade
- **npm** → **PARTIAL** (was falsely GREEN). Routes through mirror (verified: `GET 200 corelink-api/npm/…/is-odd` + tarball; `is-odd(3)=true` runtime). **BUT 503 on large-metadata packages** (react 6.8MB→503, npm 25MB→503; express 805KB→200) — `npm install react` fails. Finding #98.
- **pip** → **GREEN** (evidence): `Looking in indexes: …corelink-api/pip/…/simple/` → `Downloading six-1.17.0…whl (11kB)` → `six 1.17.0, PY3=True` runtime.
- **turbo/bazel** → curl/protocol-level only (real turbo binary not run; stock bazel 404 by design) — labeled honestly, NOT real-binary-GREEN.

### Performance (warm/cold, live from BR/GRU edge)
- CAS hit (warm GET) median **1115ms**; miss 881ms; HEAD 1126ms; PUT cold 2.5s.
- **PAT-verify floor ~750ms/request** (`/users/me` 794ms vs `/health` 40ms) — Argon2id on the auth hot path. For a CI cache at scale this undermines the speed value-prop. Finding #99.

### Still owed for full "do jeito que definimos" coverage
16-persona set (need to provision Expired/Revoked/Quota/Runner/PastDue/Fresh/TeamMember/Acct — only 9 PATs exist) × every surface × scenarios (cross-team teamId, power/enterprise large-payload + concurrency) × warm/cold × perf-SLO × the red-team/audit workflows. This is a structured multi-phase campaign, not a single sweep.
