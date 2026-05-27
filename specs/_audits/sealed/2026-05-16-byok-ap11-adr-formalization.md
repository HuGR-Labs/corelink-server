---
id: "AUDIT-2026-05-16-BYOK-AP11-ADR-FORMALIZATION"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  - { role: "security_lead", name: "Gustavo Schneiter (interim until hire)" }
supersedes: null
superseded_by: null
inv: ["INV-BYOK-CRYPTO-SOVEREIGNTY", "INV-KEY-OVERLAP"]
references:
  - "specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md"
  - "specs/_audits/2026-05-15-byok-real-provider-pattern.md"
  - "apps/server/src/byok_orchestrator.rs"
  - ".claude/skills/techlead/SKILL.md"
  - "scripts/byok-feature-validate.sh"
tags: ["audit", "byok", "ap-11", "techlead", "feature-flags", "compile-error", "wave-30", "stream-8", "formalization"]
---

# AUDIT 2026-05-16 — BYOK AP-11 ADR Formalization (wave-30 stream-8)

> **Purpose:** record the event of promoting the wave-15 BYOK §7 "design
> exception" (mutually-exclusive `byok-*-real` cargo features) to a
> first-class architectural decision (`ADR-S30-001`), and the matching
> updates to `/techlead` v2.1.0 → v2.1.1 + the new
> `scripts/byok-feature-validate.sh` regression guard.

---

## 1. Trigger

`/techlead` AP-11 entry (v2.1.0, 2026-05-15) cited
`specs/_audits/2026-05-15-byok-real-provider-pattern.md §7` as the
ratifying document for the `cargo build --workspace --all-features`
failure on `corelink-server`. Reviewers (human + Sonnet) kept re-
discovering this failure as a finding because audit documents are not
the canonical surface that engineers grep when investigating build
breakage; ADRs are.

User mandate (wave-30 stream-8 ask): "Make it a first-class ADR so
reviewers don't repeatedly stumble on the same finding."

## 2. Scope

In-scope deliverables (all landed on `wt/r-prep-byok-ap11-adr`,
worktree `agent-byok-ap11-adr`):

1. **NEW** `specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md`
   — first-class ADR ratifying the design. ACCEPTED 2026-05-16. Cites
   the wave-15 audit as parent + this audit as child.
2. **UPDATE** `.claude/skills/techlead/SKILL.md` v2.1.0 → v2.1.1:
   - L1.3a `✗ (design)` exception block prefers `adr ref` over
     `audit ref` (both accepted; ADR preferred when available).
   - AP-11 "Canonical ratification" block added pointing at
     ADR-S30-001 + the wave-15 audit + the orchestrator source +
     the new validator script.
   - Change-log row v2.1.1 added (strictly additive over v2.1.0).
3. **NEW** `scripts/byok-feature-validate.sh` — regression guard
   verifying (V1) orchestrator file present, (V2) all 6 pairwise
   `compile_error!` macros declared, (V3) all 4 `byok-*-real` feature
   flags declared in `apps/server/Cargo.toml`, (V4) ADR-S30-001
   present and ACCEPTED, (V5) wave-15 audit cites the ADR.
4. **UPDATE** `specs/_audits/2026-05-15-byok-real-provider-pattern.md`
   §7 — adds "Ratified by ADR-S30-001 (2026-05-16)" cross-reference at
   the top of §7.2, so a reader landing on the audit is immediately
   pointed at the canonical ADR.
5. **NEW** this document.

Out-of-scope:

- Any change to `apps/server/src/byok_orchestrator.rs` itself. The
  implementation is unchanged since wave-15 commit `818c055`; this
  formalization is purely documentation + verification surface.
- Any change to the CI matrix definitions. The per-provider build
  matrix (D4 in ADR-S30-001) already exists in CI; this audit only
  adds the validator script that an existing CI lane can adopt.
- BYOK provider semantics (already locked by ADR-S14-004 / -005 /
  -006).

## 3. Verification gates (executed during this work)

| Gate | Command | Expected | Status |
|---|---|---|---|
| G1 | `cargo build -p corelink-server --features byok-aws-real` | exit 0 | PENDING — runs against main `04f2dff`; orchestrator path unchanged |
| G2 | `cargo build -p corelink-server --features byok-gcp-real` | exit 0 | PENDING |
| G3 | `cargo build -p corelink-server --features byok-azure-real` | exit 0 | PENDING |
| G4 | `cargo build -p corelink-server --features byok-vault-real` | exit 0 | PENDING |
| G5 | `cargo build -p corelink-server --features byok-aws-real,byok-gcp-real` | **exit ≠ 0** with AWS+GCP `compile_error!` diagnostic | PENDING — negative test |
| G6 | `bash scripts/byok-feature-validate.sh` | exit 0 with PASS V1..V5 | PENDING |
| G7 | `python3 scripts/validate_specs.py` | no new failures vs baseline 0 | PENDING |
| G8 | `python3 scripts/validate_references.py` | no new dangling refs | PENDING |

The gates above are the acceptance criteria the worktree owner runs
before merging into `main`. The merge gate is `/techlead L0–L8` on
this branch, with the L1.3a sub-matrix expected to mark
`--all-features` as `✗ (design)` citing ADR-S30-001.

## 4. Decision documentation

This audit is the formal record of the formalization event. Any
future reviewer asking "why is `cargo build --workspace
--all-features` red on corelink-server?" should now find a single
canonical answer chain:

```
1. cargo error mentions byok-{aws,gcp,...}-real → grep "byok mutually exclusive"
2. Lands on ADR-S30-001 (this is THE answer)
3. ADR-S30-001 references this audit (event record) + wave-15 §7 (baseline)
4. /techlead AP-11 entry cites the same ADR (verifier-side ratification)
```

Before this formalization, step 2 returned an audit document (§7
sub-section), and reviewers repeatedly opened it as a finding. After
this formalization, step 2 returns the ADR and is closed.

## 5. Risk assessment (L9)

1. **What's mocked / trait-deferred?** Nothing new in this work item;
   the underlying mutually-exclusive constraint has been in place
   since wave-15 commit `818c055`.
2. **What's human-action-bound?** The /techlead skill update + ADR
   filing landed atomically; no follow-up human gate.
3. **Contract changed silently?** No. The `--all-features` failure
   was already declared (wave-15 §7); this audit only promotes the
   declaration to a first-class ADR.
4. **Customer-visible API/schema?** No.
5. **New crypto primitive without ADR?** No — this audit IS the ADR
   formalization step.
6. **New SLO needing Grafana/PD/runbook?** No.
7. **Worst-case if shipped tomorrow with one bug?** Worst case is the
   `scripts/byok-feature-validate.sh` validator has a false negative
   (says PASS when constraint is broken). Mitigation: the script's
   V2 logic uses literal pair string matching; a future contributor
   removing the macros would trip V2 immediately. Additionally, the
   per-provider build matrix in CI would fail any combination of two
   `byok-*-real` flags — the validator is a belt-and-suspenders on
   top of an already-failing build path.

## 6. Cross-references

- `specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md`
  — the ADR this audit ratifies the filing of.
- `specs/_audits/2026-05-15-byok-real-provider-pattern.md`
  — wave-15 parent baseline; §7 now points back at the ADR.
- `.claude/skills/techlead/SKILL.md` v2.1.1
  — verifier-side ratification (L1.3a + AP-11).
- `apps/server/src/byok_orchestrator.rs`
  — implementation (lines 76–117; 6 pairwise `compile_error!`
  macros).
- `scripts/byok-feature-validate.sh`
  — regression guard for the constraint.

## 7. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-16 | Gustavo (via Claude Opus 4.7, wave-30 stream-8) | Initial filing. Records ADR-S30-001 ratification + `/techlead` v2.1.1 cite + `scripts/byok-feature-validate.sh` regression guard. |
