# B-098 owner action packet — release and machine-local cleanup

Status at the exact D02 base `3213241ab326595e662868163a1084e104dffd3e`:

- Repository-owned hygiene is repaired: `corelink-runbook-tracker` inherits the
  workspace lint policy, and the population figures in `CLAUDE.md` are checked
  against Cargo metadata and the validator population.
- There is **no semver release tag** matching `^v<major>.<minor>.<patch>` in
  this clone. Existing tags such as `cli-v0.1.0` are product-specific tags,
  not a CoreLink semver release.
- The GA draft still contains the five sign-off placeholders. It is not an
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
3. Complete the RB-GA-CUTOVER runbook and obtain both independent sign-offs (or
   the documented ADR-0034b dual-hat fallback). Replace every placeholder in
   `docs/release/v1.0.0-GA-tag-draft-final.txt` with the real envelope or signed
   commit evidence and timestamps.
4. On a clean, reviewed `main` tip, run the cut script in dry-run mode with an
   explicit expected SHA:

   ```sh
   bash scripts/cut-v1-0-0-ga-tag.sh --dry-run \
     --expected-main-sha "<reviewed-main-sha>"
   ```

5. After the dry run and release approvals pass, the owner may run the cut
   script to create and push the annotated `v1.0.0-GA` tag. This packet does
   not authorize that external release action.

The B-098 backlog item remains **open** while the semver release/tag and the
machine-local worktree/branch actions remain outstanding. Verify the checked-in
portion at any time with:

```sh
python3 scripts/verify_b098_repo_hygiene.py
```
