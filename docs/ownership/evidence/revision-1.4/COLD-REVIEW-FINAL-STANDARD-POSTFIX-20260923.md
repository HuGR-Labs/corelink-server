# Cold review: final standard and publication gates

**Verdict: FIX_FIRST.** This independent review inspected the current worktree at
`HEAD=3df52eb71acdd4e00084d42f0b64191674bf42eb`. It did not rely on earlier review
reports. It covers the ownership standard, record schema, validation and generation
gates, registry/index, publication ledger, current-main pin, and documentary checks.
It is a framework review, not approval of package documents, a contract freeze, or
authorization to publish issues.

## Findings

1. **The standard is still a candidate and cannot be frozen.** The current standard
   declares `1.1-candidate` and “proposta para revisão independente” at its header.
   The 105 ledger rows all remain `BLOCKED`, each with `frozen: false`, a null contract
   URL, and six `PENDING` gates. The publication gate requires the exact standard path,
   a commit containing byte-identical current standard contents, that commit reachable
   from live `origin/main`, and a version equal to the candidate. Current `origin/main`
   is `7f966dda8234f54b58766f33e5fca2ad02cc898b`; it has no
   `docs/ownership/STANDARD.md` tree entry. Therefore no qualifying frozen standard
   commit exists in the published main branch. This is the principal blocker to
   approval/freeze and any issue publication.

2. **The current-main registry pin is accurate, but does not resolve the freeze.**
   `git rev-parse origin/main` and `git ls-remote origin refs/heads/main` both returned
   `7f966dda8234f54b58766f33e5fca2ad02cc898b`. The registry and generated index use
   that same pin and show 105 packages, 105 `UNVERIFIED` cold reviews, and zero
   published items. The checked-out campaign branch is not descended from that remote
   main tip (`git merge-base --is-ancestor origin/main HEAD` returned 1), and the
   standard is absent from the remote tree. The live pin check in the generator
   correctly guards against a stale local ref or supplied SHA; it does not establish
   that candidate changes have been integrated into main.

3. **Publication state derivation is fail-closed for the current ledger.** A
   `CONFIRMED` or `REUSED` item must have a readback file whose bytes match its SHA,
   exact body hash and stable marker, canonical issue URL/number, matching repository
   ID, frozen-standard prerequisites, and six passing gates with evidence. Missing or
   mismatched data yields `READBACK_REQUIRED`, `INVALID_PREREQUISITES`, or
   `INVALID_READBACK`; `BLOCKED` items become `NOT_PUBLISHED`. The current ledger is
   wholly blocked, and the generated registry correctly reports no publication.
   The readback and gate evidence remain operator-supplied attestations, as documented;
   the generator checks their declared contents and consistency, not a GitHub signature.

4. **Canonical standard path/version binding is consistent across gates.** The record
   schema requires a standard version, path, and SHA. `ownership_gate.record_errors`
   requires the canonical `docs/ownership/STANDARD.md` path, extracts exactly one
   version from the current candidate, and verifies evidence bytes against their SHA.
   `publication_gate` independently requires the same path and version plus an
   immutable commit URL containing the same standard bytes. Tests cover wrong path,
   wrong version, uncommitted bytes, non-existent commits, and unpublished commits.
   These checks enforce a coherent local review baseline; the published-main test
   prevents promoting it while still unpublished.

5. **Generation remains deterministic and non-approving by construction.** Population
   enumeration and final package ordering are sorted; generated Markdown is derived
   from the registry object; the CLI verifies the live main pin before building and
   again before writing, and rejects stale calibration before either output write.
   The checked-in outputs agree on population and pin. The registry explicitly keeps
   structural PASS separate from cold review and publication. Documentary tests
   `test_population_is_deterministic_and_non_approving`,
   `test_cli_default_root_and_invalid_output_refusal`, and
   `test_publication_requires_exact_readback` passed.

## Verification

- `python3 -m unittest discover -s docs/ownership/tests -v`: **157 tests passed** in
  69.431 seconds.
- `python3 docs/ownership/tests/adversarial_probe.py`: **13/13 cases met expectation**;
  zero false acceptances.
- Read-only state check: remote and fetched `origin/main` agree at
  `7f966dda8234f54b58766f33e5fca2ad02cc898b`; 105 ledger rows are `BLOCKED`; registry
  publication count is 0; all 105 cold-review states are `UNVERIFIED`.

The supplied adversarial probe writes ignored fixture/result files under
`docs/ownership/evidence/revision-1.1/probes/` and
`docs/ownership/evidence/revision-1.1/adversarial-results.json`; those are outputs of
the requested probe, not changes to the campaign framework or publication state.

## Required before approval or freeze

Complete independent cold review and any resulting fixes; replace the candidate
version/state only after that review; integrate the final standard bytes into a
published `origin/main` commit; then pin that exact commit/version and rerun the
publication preflight. Keep package-level current-main source reconciliation, cold
reviews, and six publication gates separate until their own evidence is complete.
