# CoreLink user-simulation suites — TEST-QUALITY / METHODOLOGY weakness map

> **Read-only audit (2026-06-23).** Lens = **HOW** the tests are built (assertion
> strength, false-confidence, fidelity), NOT what surface/journey is missing
> (those are the sibling maps `2026-06-23-gapmap-surfaces.md` +
> `-journeys.md`). This map hunts the owner's standing distrust — *"green CI ≠
> validated; a test that passes by 503/skip/gate is worse than no test."* Every
> entry: `file:line` · what it FALSELY implies · what real failure it would MISS ·
> severity.
>
> **Net read:** the per-surface Rust *happy-path* journeys are mostly STRONG
> (real byte round-trip compares: `cas.rs`, `ac.rs`, `concurrency.rs`,
> `pat_lifecycle.rs`, and the whole `oci.rs` protocol suite are genuinely good).
> The false-confidence is concentrated in **(a) the GREEN-by-vacuum runner**,
> **(b) the deny-probe assertion family that counts a 404 (broken/unmounted
> route) as a security PASS**, **(c) the `scripts/e2e-real-client` shell harness
> that is mostly curl-not-CLI and grades a known-bad 502 as PASS**, and **(d) a
> few journeys whose NAME/docstring claim a strong contract the code does not
> assert.** These are exactly the "passes by gate/skip/wrong-shape" class.

---

## TIER 1 — the 5 most dangerous false-confidence weaknesses (ranked)

### Q1 — GREEN-by-vacuum: the suite exits 0 + prints "GREEN" with ZERO journeys asserting
`tests/e2e-user-journeys/src/main.rs:115-132` + `:119-121`.

The verdict logic is `if fail>0 RED elif pass>0 GREEN else GREEN (all gated)`.
The process **exits 0 unless `fail>0`** (`:129`). A run with the `CORELINK_E2E_PAT_*`
map unset GATES *every* journey (every module's first line is
`Persona::…resolve(cfg)?` → `gated`) and still prints a banner containing the
word **GREEN** and exits 0.

- **Falsely implies:** "the ship gate is green." A CI invocation, a cron, or an
  operator who forgot to export the token map gets a 0 exit + a GREEN banner.
- **Misses:** EVERYTHING. The default `cargo run -p e2e-user-journeys` (no env)
  is an all-GATED 0-assertion pass. Green here can literally mean "ran nothing."
  The string "GREEN (all journeys gated…)" softens it for a human reader but the
  EXIT CODE and the word GREEN are what a machine/cron consumes.
- **Severity: CRITICAL.** This is the owner's exact nightmare. A gate that
  cannot fail-closed on "I asserted nothing" is worse than no gate. (Mitigation
  would be: exit non-zero, or a distinct verdict token, when `pass==0`.)

### Q2 — deny-probes PASS on **404**, so a broken / unmounted / wrong-URL route scores as "secure"
`harness.rs:326-334` (`expect_denied` accepts `401|403|404`), consumed in **50**
call-sites across the journeys (`grep expect_denied` = 50).

The single helper that backs nearly every adversarial assertion treats `404` as
an acceptable deny. That is defensible for *content* isolation (a hidden object
is a 404), but it is applied uniformly to **route-level** security probes where a
404 means "this endpoint does not exist on this deploy" — i.e. the gate under
test never ran.

Worst instances (security tests that pass without ever reaching the gate they name):
- `abuse.rs:475-549` `mint_internal_auth_required` and `abuse.rs:557-605`
  `customer_pat_cannot_reach_internal` + `introspect.rs:237-303`
  `adversarial_no_service_key`: all probe `POST /internal/v1/auth/introspect`,
  which the harness's own doc (`harness.rs:78-80`) says is **NOT reachable from
  the public edge → 404**. So the EXPECTED status is 404, and the test PASSES on
  exactly the response that means "I never touched the internal-auth gate."
  These three "no edge mint / no priv-esc" guarantees are **structurally
  un-failable** against prod — they assert the route's *absence*, not its gate.
- `billing.rs:586-598` `webhook_unsigned_rejected`: `expect_denied` PASSES on a
  404 — so pointing `CORELINK_E2E_SIGNUP_WORKER_ENDPOINT` at a host where
  `/webhooks/stripe` doesn't exist scores the **forgery-rejection** test GREEN.
- `dsr.rs:215-222` `internal_erase_not_customer_reachable`: 404 PASS — the
  customer-can't-self-erase guarantee passes if the internal path simply isn't
  mounted at the edge.
- `dashboard.rs:352-356` `overview_no_auth`: 404 PASS for "no-auth must be denied."
- **Falsely implies:** the deny path is enforced.
- **Misses:** any regression that turns a *real* deny into a route that 404s
  (mis-mount, path rename, worker route drop) — the security test stays GREEN.
  Also misses the actual gate logic entirely on the three internal-route probes.
- **Severity: HIGH.** Security guarantees that cannot distinguish "denied" from
  "route absent." 404 should be a PASS only where a hidden-object 404 is the
  contract — never on route-existence/internal-gate probes.

### Q3 — `scripts/e2e-real-client` is curl-shaped-as-a-client, not the real CLI — and a known-bad **502 is graded PASS**
`scripts/e2e-real-client/lib/clients.sh` + `last-run.json`.

Only **2 of 7** probes drive a real binary: `probe_docker` (real `docker`) and
`probe_cargo_sccache` (real `cargo`+`sccache`). The other five are **curl** wearing
a client's request shape — the exact "curl passes while the real CLI fails" class
the memory warns about:
- `probe_native_cas:257-317`, `probe_bazel:349-366`, `probe_turbo:389-407`,
  `probe_identity:417-428` — all raw `curl`, no real CLI.
- `probe_brew:206-220` — curl auth-shape + at most `brew --version`; never a
  `brew fetch`/`install`.
- `probe_cargo_sccache:149-154,177-178` — GATES on this Mac's `bad protocol
  version` TLS bug, so the ONE real package-manager probe almost never runs.

Two compounding false-greens, **proven in the committed `last-run.json`**
(verdict `SHIP`, 17 PASS / 1 GATED / 0 FAIL):
1. `probe_brew:209-219` grades **any** non-401/403 as `pass` ("auth accepted").
   `last-run.json` records `brew auth … status:"PASS" … "HTTP 502"`. **A 502 is
   the documented `_public`-chain fail-closed signature** — the very regression
   `adapters.rs::brew_public_bottle_fetch` calls a hard FAIL. The shell harness
   ships it as PASS / SHIP.
2. `probe_native_cas` (the PRIMARY surface) GATES whenever `b3sum` is absent
   (`clients.sh:246-248`) — and `last-run.json` shows exactly `cas write … GATED
   … b3sum … not installed`. So the flagship CAS round-trip **did not run**, yet
   the run is `SHIP`.

Also: every shell "round-trip" (`cas`/`ac`/`bazel`/`turbo`) asserts **status
codes only — it never GETs the body back and compares bytes** (contrast the Rust
`cas.rs:79-89` which does). A backend that 200s but serves the wrong/empty bytes
passes the shell harness.

- **Falsely implies:** "the real client toolchains round-trip against prod →
  SHIP." The README literally calls it "the GO-LIVE MOAT … real-CLIENT
  conformance."
- **Misses:** real-CLI-only failures (npm/pip/turbo/bazel/brew wire-protocol
  bugs that curl doesn't reproduce), byte-corruption round-trips, AND the live
  `_public` 502 regression (graded PASS).
- **Severity: CRITICAL.** The harness sold as the real-client moat is mostly
  synthetic and actively passed a known-bad 502 as SHIP.

### Q4 — `run.sh` declares SHIP when it could **bootstrap nothing**, and grades the docker digest on an EMPTY value as PASS
`scripts/e2e-real-client/run.sh:117-124` + `lib/clients.sh:104-108`.

- `run.sh:117-124`: no Clerk secret → the **entire** run gates and **exits 0
  (SHIP)**. Same green-by-vacuum disease as Q1, in the shell harness. "no creds
  → SHIP" means CI without the secret is a guaranteed green that proved nothing.
- `clients.sh:104-108`: the docker digest round-trip check is
  `if [ -n push ] && [ -n pull ] && [ push != pull ]; then fail; else pass`. If
  `docker inspect` yields an **empty** `push_digest` (inspect failed / format
  drift), the comparison is skipped and the step **passes** ("round-trip digest
  verified (matched)") — the one integrity assertion in the docker probe is
  bypassed by its own empty-string guard.
- **Falsely implies:** SHIP = the real clients round-tripped; "digest verified."
- **Misses:** a credential-less CI run that validates nothing; a docker
  push/pull that silently mangles the digest when inspect can't read it.
- **Severity: HIGH.**

### Q5 — `ac.rs::divergent_body_reput` NAME + the in-code comment claim a "409 integrity guard" the test does NOT require
`ac.rs:140-225` (esp. the journey `name` at `:141-142` and the comment at
`:179-184`), vs the module docstring `ac.rs:25-32` which correctly says native AC
is **last-write-wins, no guard**.

The journey is *named* `"AC: divergent-body re-PUT - same digest, different body
-> 409 integrity guard"` and an inline comment asserts "The native AC route
ENFORCES a digest↔body integrity guard (verified live): … rejected with 409."
But the actual code (`:189-199`) accepts BOTH `409` (guard) AND `200/201`
(last-write-wins) and then merely checks the GET returns *whichever* body that
outcome implies. So a deployment with **no** integrity guard (cache-poisoning /
non-determinism risk for AC) passes this test, despite the name advertising the
guard as covered.

- **Falsely implies:** the AC divergent-body integrity guard ("same action
  digest can't be remapped to a different result") is tested.
- **Misses:** the AC poisoning / non-determinism class entirely — the test is a
  tautology (it passes on either policy). The name/comment will mislead a future
  reader into thinking the strong contract is gated.
- **Severity: HIGH** (correctness *and* doc-honesty — Dr-House "comment lies").

---

## TIER 2 — provisioning fidelity & determinism

### Q6 — the ONE real-provision script seeds **4 of 12 personas**; 8 persona-keyed deny journeys GATE in every real run yet are counted as coverage
`scripts/e2e-real-client/provision-and-run-suite.sh:82-98`.

It exports only `PAT_RW`, `PAT_RO`, `PAT_REVOKED`, `PAT_TENANT_B` (+ tenants).
It **never** sets `PAT_ADMIN / PAT_EXPIRED / PAT_FREE / PAT_SOLO / PAT_PRO /
PAT_ENTERPRISE / PAT_PASTDUE`. Therefore, in the canonical go-live run:
`billing.rs::past_due_data_plane_denied` (needs `P11PastDue`),
`security.rs::expired_every_op_denied` (P5), every tier-limit journey, and the
admin-surface journeys **all GATE** — but the suite still prints PASS+GATED with
a GREEN verdict (Q1). The JOURNEY-MATRIX claims a "persona × surface" matrix; the
provisioner backs 1/3 of the personas.

- **Falsely implies:** the persona matrix is exercised.
- **Misses:** expired-PAT-still-works, past-due-served-anyway (the documented
  billing-state-integrity launch-blocker), tier-cap bypass, admin priv-esc — all
  dark in a real run. (This overlaps the journeys-map's "8 of 12 personas never
  run," but the methodology point is sharper: GATED is silently folded into a
  GREEN verdict, so the gap is invisible in the headline.)
- **Severity: HIGH.**

### Q7 — provisioning the revoked persona races on a fixed `sleep 4`; revoke→deny is timing-dependent
`provision-and-run-suite.sh:87-88` (`POST …/revoke … ; sleep 4`) and the
PAT-lifecycle revoke-propagation it feeds (`pat_lifecycle.rs:197-222`).

Revocation propagation across the edge/container is asserted with a hard-coded
4-second sleep before the suite runs. On a slow propagation the
revoked-PAT-still-authenticates check (`security.rs` P4, `pat_lifecycle.rs`)
could FAIL on a non-bug, or (more dangerously) a *fast-but-not-yet* window could
let a stale-cached revoke read as still-valid intermittently.

- **Falsely implies:** deterministic revoke coverage.
- **Misses / introduces:** flake either direction; no bounded retry/poll on the
  revoke-took-effect signal (contrast the principled retry in
  `adapters.rs::brew_public_bottle_fetch:711-732`).
- **Severity: MEDIUM.**

### Q8 — `audit.rs` PASSES on an EMPTY `rows` array — it asserts SHAPE, never that the events it generated landed
`audit.rs:91-93` (empty-rows → `pass`), generating events at `:46-49`.

The journey fires 3 `GET /v1/users/me` calls to "generate audit-worthy events"
then PASSES if `rows` is a well-formed array — **including empty**. So the audit
pipeline being completely dead (request-path events never reaching the customer
sink) is indistinguishable from a quiet tenant. The docstring justifies this
(separate sink, legitimately empty), but the result is: the journey can never
prove an event it caused is observable.

- **Falsely implies:** audit export round-trips a real event.
- **Misses:** a broken request-path → audit-sink pipeline (events silently
  dropped) — passes as "empty page, shape ok."
- **Severity: MEDIUM** (audit is a compliance surface).

### Q9 — `rate_limit_present` can only FAIL on a 5xx — "limiter absent" is structurally a PASS
`abuse.rs:96-159` (esp. `:154-158`).

The probe fires 30 GETs and PASSes if it sees either all-served OR a clean 429,
failing only on a 5xx. The comment is honest ("PASS whether or not a 429
appeared"), but the journey is *named* "rate-limit present" while it cannot
detect an ABSENT limiter (30 requests under any real bound are all served → PASS).

- **Falsely implies:** a rate limiter is present/enforced.
- **Misses:** a fully-disabled rate limiter (revenue/DoS abuse vector) — the
  test passes identically whether the limiter exists or not.
- **Severity: MEDIUM** (name overstates; can't distinguish present from absent
  with a 30-req burst).

### Q10 — `byte_accounting_under_concurrency` only FAILs on an exact 2×/N× multiple, GATES on everything else
`concurrency.rs:472-491`.

The accounting check FAILs only when the observed delta is *precisely* `2×` or
`N×` the written bytes, or strictly negative; ANY other wrong delta (e.g. 1.5×,
or N+1 double-counts, or a partial loss that isn't a clean multiple) GATES as
"not safe to assert black-box." Defensible against an eventually-consistent
meter, but it means the only accounting bugs it catches are the two textbook
clean-multiple signatures.

- **Falsely implies:** concurrent byte-accounting correctness is verified.
- **Misses:** any non-clean-multiple miscount.
- **Severity: LOW-MEDIUM** (honestly labelled best-effort; narrow detector).

---

## TIER 3 — narrower / lower-blast-radius methodology issues

### Q11 — the webhook→tier "upgrade reflected" success lexicon is over-broad
`billing.rs:738-755`: after a signed `active` webhook it accepts billing
`status` ∈ {`active`,`paid`,`trialing`,`current`,`ok`}. `trialing` and the
catch-all `ok` would pass even if the *paid* upgrade silently didn't apply (a
tenant left trialing/ok). The test proves "not canceled," not "the purchased
tier took." (Also entirely GATED in practice — write-only whsec → 400-gate per
`:694-700`.) **Severity: LOW-MEDIUM.**

### Q12 — `cargo`/`pip`/`npm` public-fetch passes on a **200 that may be an upstream error doc**
`adapters.rs:770-808` (`public_fetch_assert`) + the npm/pip callers. It asserts
status `==200` and "not 502," but never inspects the BODY — a 200 carrying an
upstream "package not found" JSON, or a cached error doc, passes as a healthy
moat hit. No content/shape assertion on the public metadata. **Severity: LOW.**

### Q13 — the OCI `oci_token_two_leg` (adapters.rs) treats `/v2/` **404 as authorized**
`adapters.rs:442-448`: a minted bearer reaching `/v2/` is "authorized" on 200
**or 404**. If the whole OCI mount regressed to a blanket 404, the "token
exchange honored" leg passes. (The dedicated `oci.rs` suite is much stronger and
asserts real challenge/Content-Length, so blast radius is limited to this
duplicate cell.) **Severity: LOW.**

### Q14 — Turbo happy path SELF-DISABLES on a known prod 500 (gate-not-fail), so a paying Turbo customer's core flow is un-asserted
`turbo.rs:115-117,127-129,152-163` + `cross_tenant_isolation:309-311`. The
`turbo_storage_finding_gate` converts the live `corelink-turbo-prod`-missing 500
into a GATE on BOTH the round-trip and the isolation seed. Honest and loud, but
the consequence is the Turbo value-path + Turbo cross-tenant isolation are
currently **un-asserted** (not just untested) — and folded into GREEN. (Also
flagged in the surfaces map; the methodology angle is that a self-imposed gate on
a known live defect keeps the gate green over a real customer-facing 500.)
**Severity: MEDIUM** (it's a deliberate gate over a live bug).

### Q15 — `dsr.rs` erased-read accepts **404 as well as 410**, weakening the tombstone proof
`dsr.rs:112-124,316-329,490-491`. The GDPR "erased → 410 Gone" contract is the
strong proof; the journey also passes on 404 ("never-existed / hidden,
acceptable-but-weaker"). A regression where erasure stops writing the tombstone
and the object just 404s (vs a never-erased object that also 404s) is
indistinguishable → the erasure *mechanism* isn't pinned, only "not 200."
(Plus the whole DSR live path is GATED on an unprovisioned `TOMBSTONED_HASH`.)
**Severity: LOW-MEDIUM.**

### Q16 — the `pilot-onboarding` + `signup-flow` "e2e" suites are IN-MEMORY fakes, not the live system
`tests/e2e-pilot-onboarding/src/harness.rs:1` ("in-memory fakes"),
`:604,694` (`batch_upload_blobs`/`cas_blob_count` over an in-memory R2),
`tests/e2e-signup-flow/tests/happy_path_starter_stripe_test_mode.rs:1-9` (the
real-Stripe path is `#[ignore]`). They carry the "e2e" name and the offboarding /
30-day-grace / DSR-finalize / audit-chain / tier-select coverage, but a passing
in-memory test says nothing about the deployed worker+container. Counting them as
"e2e / real-user" coverage is the false-confidence. **Severity: MEDIUM** (mostly a
naming/claim-honesty issue; the orchestration logic IS validly tested).

### Q17 — coverage-claim honesty: the headline counts mix cred-free deny-probes + GATED into "PASS/GREEN"
`JOURNEY-MATRIX.md:8,45,52` (claims a full persona × 17-surface matrix);
`README.md` "GO-LIVE MOAT"; `last-run.json` `"pass":17`.
A large fraction of the asserting journeys are **cred-free negatives** (anonymous
deny, unauthed deny, malformed-input 4xx) — valuable, but they are not the
positive value-path the headline implies, and several of them pass on 404 (Q2).
Of the headline `17 PASS` in `last-run.json`: the flagship CAS write was GATED,
brew "PASS" was a 502, and bazel/turbo/ac "PASS" were status-only-no-bytes. The
honest count of *real positive value-path byte-verified* assertions in that run
is far below 17. **Severity: MEDIUM** (claim vs. asserting-code mismatch).

---

## What is GENUINELY strong (so the map isn't read as "all broken")

- `cas.rs:79-109`, `ac.rs:114-128`, `concurrency.rs` (all three fan-outs +
  read-during-write) — **real byte-for-byte round-trip + corruption checks.**
- `oci.rs` (J1–J9) — real OCI protocol: Www-Authenticate realm/scope assertions,
  push→HEAD Content-Length **byte-equality** (`:711`), push⇒pull scope subsume.
  The best-built module in the suite.
- `pat_lifecycle.rs` — true mint→use→revoke→deny lifecycle with the minted
  plaintext token, and it correctly REFUSES to accept 404 on the revoke-deny
  step (`:214` requires 401/403). A model for how Q2 should be fixed elsewhere.
- `edge.rs` — malformed-digest / hash-mismatch-not-stored / oversized-body are
  real negative-path assertions (4xx-family is appropriate for malformed input).
- `abuse.rs::cache_poison_mismatch_rejected` — real digest-binding poisoning probe.
- The Stripe-signature HMAC helper (`harness.rs:551-596`) is unit-pinned against
  RFC 4231 + a cross-language vector — a genuine cross-language contract lock.

---

## Cross-cutting fix themes (for whoever acts on this)

1. **Fail-closed on "asserted nothing."** Q1+Q4: a verdict of GREEN/SHIP must
   require `pass>0` on the journeys that were *supposed* to run; an all-gated run
   should be a distinct non-green verdict / non-zero exit.
2. **Split the deny helper.** Q2: a `expect_denied_authgate` (401/403 only, NO
   404) for route-existence/internal-gate probes; keep the 404-tolerant variant
   only for content-isolation reads. Especially the three internal-introspect
   probes need a *positive* reachability assertion (the gate must actually run).
3. **Grade real-client probes on bytes + on the 502 signature.** Q3: brew/npm/pip
   must FAIL (not PASS) on 502; cas/bazel/turbo shell probes must GET-and-compare
   bytes; prefer the real CLI where the binary exists.
4. **Make names/comments match assertions.** Q5/Q9/Q14: rename or strengthen the
   AC-409 journey and the rate-limit journey to what they actually prove.
5. **Provision the full persona set** (Q6) and replace the `sleep 4` revoke race
   with a bounded poll (Q7).
