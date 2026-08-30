### Added

- **WP-6 — `docs/campaigns/remediation/devenv-manifest.tsv`, the file-by-file
  classification of PR #1397 (173 rows: SALVAR 16 · DESCARTAR 110 · ARQUIVAR 47).**
  The frozen contract itemized the DISCARD list but left SALVAGE as "everything
  else, by filename", handing the classification of 173 files back to the
  executing agent — the exact decision a frozen contract exists to remove. Each
  row now carries a verdict and the evidence behind it, checked against the
  blobs rather than inherited: `0a5e3349`'s payload is still absent from `main`
  (`byok-aws-real`, `ERASURE_ATTESTATION_SEED_HEX`, `AUDIT_CHAIN_SIGNING_SEED_HEX`,
  `binding = "AUDIT_BUCKET"` — 0 occurrences each), so a merge would publish the
  plaintext Ed25519 seed for the first time; `gc_worker/` is 2,222 lines across
  13 files that no `mod` statement declares, and this commit is its only copy;
  3 of the 4 "added" workflows already exist on `main`; `devenv_guard.ts` reads
  `max_concurrency` and `max_vcpu_h` and uses neither, with a bare `catch {}`
  returning `allowed: true`. Salvageable surface is **942 lines of 28,642**.
  Nothing was extracted — the manifest is the deliverable.
