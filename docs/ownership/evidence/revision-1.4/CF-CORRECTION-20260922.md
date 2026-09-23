# CF bindings correction — 2026-09-22

## Scope and baseline

- Work package: `FX-CF`.
- Baseline: `11898804e5c8cfb518e3f48fe84e096917879152`.
- Package source pin remains `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`; this correction changes ownership documentation only.
- Edited scope: `.claude/skills/own-corelink-cf-bindings/**`, `docs/ownership/crates/corelink-cf-bindings/**`, and this evidence file.

## Corrections

- Added stable `#rel-cf-NNN` anchors to the existing relations and retained one explicit Cargo edge per manifest declaration (REL-CF-016–031).
- Made formerly grouped semantic boundaries atomic: REL-CF-001 now records the concrete R2 trait implementation through the canonical CAS re-export; REL-CF-007/008 describe only the R2 audit/key-scope contracts; REL-CF-034–036 record D1/KV/DO audit hooks separately; REL-CF-037–039 record D1 SQL/bind, KV key-prefix, and DO name-scope contracts separately. Each new boundary has its own stable fingerprint, activation condition, failure behavior, and source validation pointer.
- Clarified that `serde` is target-specific to `wasm32`, alongside `getrandom_v04`; the latter is package-renamed from `getrandom` and enables `wasm_js`.
- Reconciled Reference R01 with `crates/corelink-cf-bindings/Cargo.toml:26-106`: seven normal declarations (REL-CF-016–022), two wasm-target declarations (REL-CF-023–024), and seven host dev declarations (REL-CF-025–031); all 16 manifest declarations are enumerated by package name. This explicitly includes normal `thiserror` and target-specific `serde`/`getrandom_v04`.
- Added the missing stable identity fingerprint to REL-CF-012 and corrected the skill's relation-maintenance guidance.
- PROC-001–004 remain required and `review_status=BLOCKED`, `execution_status=REVIEWED_NOT_EXECUTED`; execution result is not claimed as approval. PROC-005 remains blocked for external operation and is not required for this documentary acceptance.

## Evidence and limits

Static evidence was read from the package manifest, ownership documents, package source references, and the REL-CF records in `BLAST_RADIUS.md`. The 16 direct dependency declarations reconcile to the manifest's `[dependencies]`, wasm target table, and native dev-dependency table. Semantic relation count increases from 33 to 39 to represent six distinct contracts; source pins, unaffected relation identities, scope boundaries, and external-operation limits were retained.

No Cargo command, Rust test, wasm build, Cloudflare operation, binding, or runtime check was executed. Required local validation states therefore remain blocked/not executed; this record does not approve the package or establish resolved dependency selection, compilation, runtime, or deployment.
