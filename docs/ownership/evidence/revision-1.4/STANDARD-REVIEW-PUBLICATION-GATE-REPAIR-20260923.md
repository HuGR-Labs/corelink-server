# Standard review and publication gate repair — 2026-09-23

Local `origin/main` read back immediately before recording this evidence: `7f966dda8234f54b58766f33e5fca2ad02cc898b`. The diff from the earlier `a18d1146ac6cea79ef36ff56c00682a869060460` pin contains no Cargo manifest, lockfile, Rust source, or ownership path. This is an observation pin, not a package SOURCE approval or frozen standard commit.

## Repaired checks

- A package review record must name the canonical `docs/ownership/STANDARD.md` and the version parsed from that candidate's `**Versão:**` line. The existing SHA-256 check still binds the record to the exact standard bytes. Identical bytes at another path and a made-up version are rejected.
- The registry derives `PUBLISHED` or `REUSED` only after both an exact issue readback and the publication preflight's frozen-standard and six-gate prerequisite check. An unfrozen or absent contract, wrong standard version, wrong published URL/commit, or missing/pending gate yields `INVALID_PREREQUISITES`. No issue or ledger state was mutated.

## Verification and scope

The targeted review/registry/publication run passed 30 tests. An earlier 87-test focused run passed 86; its sole failure was the concurrently edited `corelink-reapi/REFERENCE.md`, whose `BLAST_RADIUS.md#rel-030` and `#rel-031` links were broken during that read. `git diff --check` passed for the modified tools and tests. The independent full-suite readback is separate.

All 105 current ledger rows remain `BLOCKED`, with no frozen contract URL or issue ID. This repair does not freeze the candidate standard, approve package records, or publish issues. Published-main placement of the final standard and package current-main reconciliation remain separate gates.
