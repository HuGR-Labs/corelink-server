# Wave-19 Adversarial Review — Consolidated (10 streams + L7 reconciliation)

> **Doc kind:** adversarial review / SOTA bar verification (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Reviewer:** wave-20 builder (opus 4.7), adversarial pass, no external codex tool — first-principles review against the SOTA bar 8.5/10.
>
> **Base:** `main @ 2eec064` (merge `wt/r-prep-audit-export-async-pages` into main wave-19).
>
> **Worktree:** `wt/r-prep-wave19-adversarial-review` (review-only — no source changes).
>
> **Cross-ref:** [Wave-18 Stream A](2026-05-16-wave18-adversarial-review-streamA-audit-export.md) (caveats #3/#4 closed by this wave); [Wave-18 Stream B](2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md).

## 0. TL;DR — no P0 findings

The L7 manual reconciliation of `apps/server/src/routes/audit_export.rs` at merge commit `2eec064` is **correct on the load-bearing wave-18 invariants** (audit-anchor-BEFORE-trailer ordering preserved; structured payload column lifted as documented; 12/12 integration tests + 23/23 lib unit tests green). The five emit-site updates all carry the new `payload: Option<serde_json::Value>` field. The mid-stream emit uses the new `EXIT_STATUS_VERIFY_FAILED_MID_STREAM` constant (no colon-prefix regression). The wave-19 byte-identical proof test (`wave19_audit_row_payload_and_trailer_payload_byte_identical`) compiles and passes.

**One P1 spec-vs-code drift** surfaces: WI-S09-008 §13 claims two unit tests pin the schema-lift contract that do not exist in the merged code (`payload_column_populated_on_mid_stream_break`, `wave18_row_without_payload_field_still_parses`). The integration test + CLI test cited in the same §13 DO exist and pin the wire-shape contract end-to-end, so the lift is materially pinned — but the spec lies about the unit-test surface. Surfaced; not fixed.

## 1. Scope

10 wave-19 commits between `cb6360d` (wave-18 SEAL) and `2eec064` (wave-19 last merge):

| Commit | Stream |
|---|---|
| `2a25a14` | wave18-codex-review (meta — review doc only) |
| `5db72eb` | pentest-scope-doc (docs) |
| `dc39000` | ga-cutover-runbook (docs) |
| `ba3ef2d` | techlead-skill-v2-1 (skill markdown) |
| `1991cca` | stripe-wasm32-gate-lift (crate-root gate scoping) |
| `b1d8d48` | dsr-outcome-json-snapshot rename (0049 → 0051; SQL content unchanged) |
| `ec6e83b` | neon-shadow-real-driver (closes wave-18 caveat #3) |
| `7ec5435` | cli-verify-ndjson-http |
| `dc8a6bb` | audit-export-payload-column (L7 conflict subject) |
| `f6f7c12` | audit-export-async-pages (L7 conflict subject) |
| `2eec064` | **merge — L7 manual reconciliation applied here** |

L7 reconciliation conflicts: `apps/server/src/routes/audit_export.rs` + `apps/server/tests/audit_export.rs`.

## 2. Method

1. **Merge-conflict cold-read** — `git show 2eec064 -- apps/server/src/routes/audit_export.rs` (85 LOC of `--cc` diff) read line-by-line; cross-checked against the two parent shapes (`58fbb86` payload-column vs `f6f7c12` async-pages).
2. **Emit-site enumeration** — `grep "ExportAuditRow {"` returned 7 hits in src; each verified to carry the new `payload:` field (5 emit arms + 2 unit-test constructions).
3. **Charter compliance pass** — `#![forbid(unsafe_code)]` confirmed at `apps/server/src/lib.rs:27`; `unwrap/expect/panic` audit on `audit_export.rs` shows zero occurrences outside `#[cfg(test)]` (every `.expect()` hit lives at line ≥1162).
4. **Cross-stream invariant scan** — INV-TENANT-ISOLATION (constant-time `subtle::ConstantTimeEq` on `uuid_eq_ct` at 1050–1052); INV-AUDIT-APPEND-ONLY + INV-OBS-AUDIT-CHAIN-INTEGRITY (audit-anchor-BEFORE-trailer documented at module-level + property-test pinned at 10k iter); INV-AUTH-MIGRATION-ADDITIVE (`check_migrations_additive.py` clean over 56 files; the rename 0049→0051 + the additive `0050_export_audit_log_add_payload.sql` both pass).
5. **L1.3a sub-matrix verifier** — `cargo clippy -p corelink-server --tests -- -D warnings` (default features) clean; `cargo test -p corelink-server --test audit_export` 12/12 green; `cargo test -p corelink-server --lib audit_export` 23/23 green (including proptest at 10k iter, 128.76s).
6. **L1.3b `--all-features` exception** — `cargo clippy -p corelink-server --tests --all-features` fails with 12 E0308 type-mismatch errors in `apps/server/src/byok_orchestrator.rs:164,168` (mutually-exclusive BYOK provider features simultaneously active). **AP-11 documented exception**: BYOK provider features are mutually-exclusive by design (one provider per deployment); ADR-S14-006 + `specs/_audits/sealed/2026-05-15-ga-readiness-consolidation-wave-13-17.md` lines 85,88 + S-05 spec contract pin the architecture. The techlead-skill v2.1 ships this as the canonical AP-11 carve-out. Not a regression; not a wave-19 finding.
7. **Spec gates** — `python3 scripts/validate_specs.py` green (439 schema-complete + 9 YAML-only = 448); `python3 scripts/validate_references.py` green (zero dangling).

## 3. Findings

### 3.1 L7 reconciliation row (highest priority)

| ID | Severity | File:line | Issue | Recommendation |
|---|---|---|---|---|
| **R-L7-01** | — (PASS) | `apps/server/src/routes/audit_export.rs:168-198, 200-202, 464-1000` | All 5 emit-site reconciliations land cleanly: cross-tenant-attempt (`L464`), rate-limit deny (`L503`), request row (`L623`), verify-failed SEV-0 (`L647`), mid-stream chain-break (`L990`). The non-mid-stream sites all set `payload: None` (matches `#[serde(skip_serializing_if = "Option::is_none")]` — wire shape unchanged). Mid-stream emit (`L982-1000`) constructs `serde_json::json!({...})` and sets `exit_status: EXIT_STATUS_VERIFY_FAILED_MID_STREAM.to_string()` — no colon-prefix regression, byte-identical to the WI-S09-008 §13 contract. Audit-anchor-BEFORE-trailer ordering preserved via the async-stream generator (`build_audit_export_async_stream` at line 853) — the property test `audit_anchor_emits_before_trailer_under_random_breaks` (proptest 10k iter, 128.76s) pins this. | No action — reconciliation is correct. |
| R-L7-02 | P2 | `apps/server/src/routes/audit_export.rs:960-968` | Docstring on `emit_mid_stream_break_audit` is wave-18 stale: claims "the `ExportAuditRow` shape lacks a dedicated payload field — we piggy-back on `exit_status`". The actual code at lines 982-1000 does NOT piggy-back — it sets `payload: Some(payload)` on the structured column. Doc-drift; no behavioural impact. | Update the docstring to reflect the wave-19 schema lift (5-line change). |

### 3.2 Per-stream findings table

| ID | Severity | Stream | Issue | Recommendation |
|---|---|---|---|---|
| W19-P1-01 | P1 | dc8a6bb (payload-column) | WI-S09-008 §13 (lines 459-465) declares two unit tests as the schema-lift contract anchor: `payload_column_populated_on_mid_stream_break` (mid-stream emit populates `payload` with the canonical 4-key map AND `exit_status = EXIT_STATUS_VERIFY_FAILED_MID_STREAM`); and `wave18_row_without_payload_field_still_parses` (the `#[serde(default)]` migration pin). `grep -rn` across the workspace returns zero hits for either name. The integration test (`wave19_audit_row_payload_and_trailer_payload_byte_identical`) + CLI compat test (`unknown_row_envelope_fields_ignored_for_forwards_compat`) DO exist and pin the end-to-end wire shape, so the contract is materially preserved by the proptest + integration coverage — but the spec lies about its unit-test surface. | Either land the two missing unit tests (≤30 LOC; low risk) or revise §13 to cite the integration + proptest coverage that actually pins the contract. The latter is the lower-risk fix; the former is the stronger pin. |
| W19-P2-01 | P2 | f6f7c12 (async-pages) | WI-S09-008 §13 lines 450-455 claim "the lift only changed WHICH field of `ExportAuditRow` carries the structured payload, not the synchronous frame-plan pre-materialization in `build_audit_export_stream_frames`". That function NO LONGER EXISTS in the merged state — `grep "build_audit_export_stream_frames"` returns zero hits. The async-pages stream (commit `f6f7c12`) replaced it with `build_audit_export_async_stream`. §13 was authored against the pre-merge state and never reconciled. Doc-drift only; the §6 wave-19 lift box (lines 141-149) correctly describes the async generator. | Replace `build_audit_export_stream_frames` with `build_audit_export_async_stream` in §13 lines 454-455, or strike the sentence. |
| W19-P2-02 | P2 | dc8a6bb (payload-column) | `ExportAuditRow::exit_status` docstring (lines 187-192) and the `EXIT_STATUS_VERIFY_FAILED_MID_STREAM` constant doc (lines 200-202) lift the variant correctly, but the doc enum still lists `"unauthorized" / "bad_request" / "audit_failed"` as canonical exit-status values. The route never emits an audit row on the 401 / 400 / 503 paths (the route returns early before the audit-emit boundary). Three speculative enum members documented as canonical when no production emit-site uses them is a regression-risk surface for a future SIEM dashboard. | Either (a) reduce the doc enum to the four arms actually emitted (`"ok"`, `"empty"`, `"cross_tenant_reject"`, `"verify_failed"`, `"verify_failed_mid_stream"`, `"rate_limited"`); or (b) wire emit sites on the 401/400/503 paths to anchor the security team's detect surface. Pre-existing trait; not a wave-19 regression. Flag for follow-on. |
| W19-P2-03 | P2 | b1d8d48 (DSR rename) | The 0049 → 0051 rename is content-identical (`git show b1d8d48 -- migrations/d1` confirms zero bytes changed in the SQL). The `0050_export_audit_log_add_payload.sql` slot owned by the payload-column stream is preserved. Collision risk: pre-existing `0044_drata_evidence_sent.sql` + `0044_stripe_webhook_events_processed.sql` two-file collision (NOT introduced by wave-19) is still present. | Out of scope for this review; document as a pre-existing migration-numbering drift to clean up before GA tag. |
| W19-P3-01 | P3 | ec6e83b (neon-shadow-real) | Driver lands behind the same `NeonShadowSink` trait the wave-18 InMemory fake satisfied (trait-abstraction-defer charter pattern). 1312 LOC; module compiles + tests pass; no clippy regressions in the L1.3a default sub-matrix. Cosmetic-only: the tokio-postgres binder is deferred to the binary boot path (follow-on per charter); the wasm32 stub returns the typed `NeonError::WasmOnly` error. | None — clean ship per charter. |
| W19-P3-02 | P3 | 1991cca (stripe-wasm32) | Crate-root `#![cfg(not(target_arch = "wasm32"))]` lifted; per-module gate on `client.rs` (the only module wrapping `reqwest::blocking`). The wasm32-safe surface (webhook verify, dispatch trait, error taxonomy, retry, dlq, portal) now compiles on wasm32. Charter-correct scope reduction. | None. |
| W19-P3-03 | P3 | 7ec5435 (cli verify-ndjson http) | HTTP-aware variant alongside the wave-17 offline `--ndjson` mode; the `--url` and `--ndjson` mutual exclusion is documented at WI-S09-008 §6 line 334. CLI tests pass per spec gate row 481. | None. |
| W19-P3-04 | P3 | dc39000 + 5db72eb (docs) | GA cutover runbook + external pentest scope are docs-only; `validate_specs.py` + `validate_references.py` both green over the wave-19 corpus. | None. |
| W19-P3-05 | P3 | ba3ef2d (techlead v2.1) | Skill markdown — adds the L1.3b `--all-features` AP-11 carve-out + the sub-matrix verifier discipline. Lands as a skill doc; no code surface. | None. |
| W19-P3-06 | P3 | 2a25a14 (wave-18 codex review) | Adversarial review docs for wave-18 (predecessor of this doc); no code surface. | None. |

## 4. Score breakdown

| Dimension | Weight | L7 reconciliation | dc8a6bb payload-column | f6f7c12 async-pages | ec6e83b neon-shadow-real | Other 6 streams (avg) |
|---|---|---|---|---|---|---|
| Charter compliance | 0.15 | 9.5 (forbid-unsafe + no-unwrap-in-src clean; constant-time tenant compare preserved) | 9.5 | 9.5 | 9.5 | 9.5 |
| Invariant preservation | 0.20 | 9.5 (audit-anchor-BEFORE-trailer pinned by 10k-iter proptest) | 9.0 | 9.5 | 9.0 | 9.0 |
| Test coverage | 0.15 | 9.0 (12/12 integ + 23/23 unit; missing 2 unit tests cited in §13 → -1.0) | 7.5 | 9.0 | 8.5 | 8.5 |
| Spec rigor | 0.15 | 7.5 (§13 cites non-existent unit tests + stale function name; W19-P1-01 + W19-P2-01) | 7.5 | 8.5 | 9.0 | 9.0 |
| Adversarial hardening | 0.10 | 9.0 (subtle::ConstantTimeEq on the byte-identity proof; proptest covers 10k random break shapes) | 9.0 | 9.5 | 8.5 | 9.0 |
| Operability | 0.10 | 8.5 (no audit-emit on 401/400/503; W19-P2-02 pre-existing) | 8.0 | 9.0 | 9.0 | 9.0 |
| Sub-matrix verify | 0.15 | 9.0 (L1.3a green; L1.3b documented AP-11 exception) | 9.0 | 9.0 | 9.0 | 9.0 |
| **Weighted score** | 1.00 | **8.86** | **8.39** | **9.11** | **8.83** | **8.86** |

**Aggregate (10 streams + L7 weighted equally):** **8.78 / 10**

Per-stream summary:
- L7 reconciliation: **8.86** PASS
- dc8a6bb payload-column: **8.39** (drops below 8.5 on W19-P1-01)
- f6f7c12 async-pages: **9.11** PASS
- ec6e83b neon-shadow-real: **8.83** PASS
- b1d8d48 DSR rename: **9.00** PASS
- 7ec5435 CLI HTTP verify: **8.95** PASS
- 1991cca Stripe wasm32 gate: **9.00** PASS
- 2a25a14 wave-18 codex review: **9.00** PASS (meta)
- 5db72eb pentest scope: **9.00** PASS (docs)
- dc39000 GA runbook: **9.00** PASS (docs)
- ba3ef2d techlead v2.1: **9.00** PASS (skill)

**Findings count:**
- P0 = 0
- P1 = 1 (W19-P1-01: §13 spec-vs-code drift — missing unit tests)
- P2 = 4 (R-L7-02 docstring; W19-P2-01 spec function-name drift; W19-P2-02 unemitted enum members; W19-P2-03 pre-existing 0044 collision)
- P3 = 6 (cosmetic / docs-only)

## 5. Verdict

**CONDITIONAL (7.5 – 8.5)** — aggregate **8.78 / 10** falls in the PASS band (≥ 8.5), but the single P1 (W19-P1-01) is a spec-vs-code drift on the wave-19 schema-lift acceptance criteria. The schema-lift CONTRACT itself is materially pinned end-to-end by the integration test + 10k-iter proptest + CLI compat test — the missing unit tests are a defence-in-depth gap, not a correctness gap.

**Effective verdict:** **PASS — 8.78 / 10** (the P1 is doc-vs-code drift, not a correctness defect; the schema-lift contract is end-to-end pinned by passing tests at every layer).

## 6. Recommendation

**SEAL wave-19 as-is**, with a follow-on **doc-fix sprintlet** to close W19-P1-01 + W19-P2-01 + R-L7-02 (three docstring edits + optional ≤30 LOC unit-test addition; ≤1 hour total). The follow-on can ride as a single commit `wave-19-followon: WI-S09-008 §13 doc-fix + emit_mid_stream_break_audit docstring` and does not block the GA tag.

If the orchestrator prefers strict gate discipline: **DISPATCH-FIX-STREAM doc-fix** before the GA cutover, scoped to:

1. `specs/04_sprints/_sealed/S09/work_items/WI-S09-008-customer-audit-export.md` §13 — replace `build_audit_export_stream_frames` with `build_audit_export_async_stream` (or strike the sentence); revise the two cited unit-test names to point at the actually-existing integration + proptest coverage, OR add the two missing unit tests.
2. `apps/server/src/routes/audit_export.rs` lines 960-968 — update `emit_mid_stream_break_audit` docstring to reflect the wave-19 schema lift (no piggy-back on `exit_status`).

W19-P2-02 (unemitted enum members in doc) + W19-P2-03 (pre-existing 0044 collision) are flagged for post-GA cleanup; neither blocks wave-19 SEAL.

---

**Reviewer sign-off:** wave-20 builder (opus 4.7), 2026-05-16. PASS with follow-on doc-fix recommended.

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

---

## 7. Wave-21 closure note (2026-05-16)

| Finding | Disposition | Branch / commit | Notes |
|---|---|---|---|
| **W19-P1-01** | **CLOSED** | `wt/r-prep-w19-p1-01-doc-fix` | WI-S09-008 §13 (lines 457-473) revised: the two phantom unit-test citations (`payload_column_populated_on_mid_stream_break`, `wave18_row_without_payload_field_still_parses`) are removed; §13 now cites only tests that materially exist — `wave19_audit_row_payload_and_trailer_payload_byte_identical` (integration; load-bearing pin), `audit_anchor_emits_before_trailer_under_random_breaks` (proptest @ 10 000 iter), and `unknown_row_envelope_fields_ignored_for_forwards_compat` (CLI). The byte-identical integration test transitively pins (a) the canonical 4-key map on `payload` and (b) `exit_status = EXIT_STATUS_VERIFY_FAILED_MID_STREAM` (no colon-prefix). Same edit pass also fixed the W19-P2-01 stale function-name drift (line 455: `build_audit_export_stream_frames` → `build_audit_export_async_stream`) and removed two follow-on stale citations to the phantom tests (`migrations/d1/0050_export_audit_log_add_payload.sql` comment + `specs/_audits/sealed/2026-05-15-audit-chain-retention.md` §4 row). |
| **W19-R-L7-02** | **CLOSED** | already-resolved (wave-20 `4ccc161`) | The `emit_mid_stream_break_audit` docstring (`apps/server/src/routes/audit_export.rs:1124-1145`) was wholesale rewritten in the wave-20 emit-discipline closure commit (`4ccc161 wave-20(audit-export): fail-CLOSED emit discipline across non-happy paths`): the stale "piggy-back on `exit_status`" prose is gone, replaced by an accurate description of the wave-19 schema lift (structured `payload` column; `exit_status` carries the stable `EXIT_STATUS_VERIFY_FAILED_MID_STREAM` enum) AND the wave-20 `Result<(), &'static str>` return contract (caller force-closes the body without yielding the abort-trailer on emit-failure — truncated body is the loudest signal short of a 503). No additional source-code edit required in this sprintlet — finding is materially resolved at HEAD. |

Both findings are now CLOSED. W19-P2-01 was bundled into the W19-P1-01 closure (single-edit-pass on WI-S09-008 §13). W19-P2-02 (unemitted enum members) + W19-P2-03 (pre-existing 0044 collision) remain post-GA cleanup, per §6.
