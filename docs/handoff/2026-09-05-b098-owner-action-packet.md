# B-098 owner action packet — release and machine-local cleanup

Status at the exact D02 base `3213241ab326595e662868163a1084e104dffd3e`:

- Repository-owned hygiene is repaired: `corelink-runbook-tracker` inherits the
  workspace lint policy, and the population figures in `CLAUDE.md` are checked
  against Cargo metadata and the validator population.
- There is **no semver release tag** matching `^v<major>.<minor>.<patch>` in
  this clone. Existing tags such as `cli-v0.1.0` are product-specific tags,
  not a CoreLink semver release.
- The GA draft still contains release, evidence, and sign-off placeholders. It is not an
  attestation and must not be treated as one.
- No worktree, branch, credential, tag, or remote ref was created, moved,
  deleted, or pushed by this repair. The operator must **not create a tag** as
  part of this handoff until the release prerequisites below are complete.

## Owner-only actions

These actions require the owner’s machine, GitHub access, and the release
signers. They are deliberately not automated by the repository verifier.

1. Move the eighteen worktrees out of `/private/tmp` into a durable path,
   preserving each worktree’s branch and uncommitted work. Re-run
   `git worktree list --porcelain` and inspect every dirty worktree before any
   cleanup. Do not delete a worktree or branch to make the census look clean.
2. Review the local and remote branch list, then delete only branches already
   confirmed merged. Preserve active or unreviewed work. This is an operator
   action, not a CI assertion.
3. Promote `specs/00_framework.md` to FROZEN v1.0.0 under its own governance
   contract, create the cryptographically signed `framework-v1-0-0-ga` tag,
   and configure Git to trust the release signers. A lightweight or unverifiable
   prerequisite tag fails closed.
4. Complete RB-GA-CUTOVER against the exact release SHA. File a cutover
   JSON attestation containing typed GREEN, 6/6-greenlight and 11/11-step
   fields, plus a production deployment JSON containing the same release SHA,
   five exact environments and one healthy OCI digest. File the 19/19 bundled
   CI JSON with tree identity to the release. Obtain both independent sign-offs
   (or a legitimately invoked ADR-0034b path); never synthesize them.
5. Replace every placeholder in
   `docs/release/v1.0.0-GA-tag-draft-final.txt` with the exact source SHA,
   framework tag-object ID, OCI digest, SHA-256 of both evidence files, and
   real signer evidence. On clean `main`, run the strict dry-run:

   ```sh
   bash scripts/cut-v1-0-0-ga-tag.sh --dry-run \
     --expected-main-sha "<40-hex-reviewed-main-sha>" \
     --cutover-attestation "<evidence-commit>:<completed-cutover.json>" \
     --deployment-evidence "<evidence-commit>:<production-readback.json>" \
     --ci-evidence "<evidence-commit>:<bundled-ci.json>" \
     --signoff-manifest "<two-key-signoffs.json>"
   ```

6. After the strict dry run passes, run the same command without `--dry-run`.
   It creates `v1.0.0-GA` with `git tag -s`, verifies the signature before
   push, atomically anchors the evidence and both signoff commits under
   dedicated remote refs, and verifies every exact object from `origin`. Release
   notes remain DRAFT for the tag-triggered editorial PR. This packet does
   not authorize that external release action.

The B-098 backlog item remains **open** while the semver release/tag and the
machine-local worktree/branch actions remain outstanding. Verify the checked-in
portion at any time with:

```sh
python3 scripts/verify_b098_repo_hygiene.py
```
