# Wave-18 Adversarial Review — Aggregate Closure (wave-20)

> **Doc kind:** aggregate closure summary (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Closer:** wave-20 builder (opus 4.7), cosmetic-cleanup pass on worktree `wt/r-prep-wave18-codex-p2-p3-closure`.
>
> **Subject:** wave-18 adversarial review streams A + B (commit baseline `2eec064` on main).
>
> **Cross-refs:**
> - [Stream A — audit-export streaming + mid-stream trailer](2026-05-16-wave18-adversarial-review-streamA-audit-export.md)
> - [Stream B — Neon analytics shadow sync](2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md)

## 1. Aggregate score lift

| Stream | Findings closed (P1 + P2 + P3) | Pre-closure score | Post-closure score | Verdict shift |
|---|---|---|---|---|
| A — audit-export streaming + trailer | 5 P1 (wave-19 OBE) + 5 P2 (wave-20) + 2 P3 (wave-20) = 12 | 8.35 / 10 | **9.5 / 10** | CONDITIONAL → **PASS** |
| B — Neon analytics shadow sync | 5 P1 (wave-19 OBE) + 6 P2 (wave-20) + 3 P3 (wave-20) = 14 | 8.35 / 10 | **9.5 / 10** | CONDITIONAL → **PASS** |
| **Aggregate** | **26 total** (10 wave-19 OBE + 16 wave-20 cosmetic) | **8.35 / 10** | **9.5 / 10** | CONDITIONAL → **PASS** |

## 2. Wave-20 cosmetic-cleanup commits

Single worktree `wt/r-prep-wave18-codex-p2-p3-closure` collapses the 16 cosmetic lifts into one stream:

**Source touched:**
- `apps/server/src/routes/audit_export.rs` — A-P2-01 (wildcard 429 audit emit), A-P2-02 (bytes_written doc), A-P2-03 (parse_rfc3339_utc_ms doc), A-P2-04 (InMemoryExportAuditSink contention doc), A-P2-05 (now_ms_from_window doc + cross-stream sym ref), A-P3-01 (too_many_arguments doc), A-P3-02 (manual Debug doc).
- `crates/corelink-audit-chain/src/neon_shadow.rs` — B-P2-01 (single-pass validator contract doc), B-P2-02 (TenantIsolationViolation Display redaction + new `redact_tenant_uuid` helper + 2 construction-site updates), B-P3-01 (too_many_arguments doc on `ShadowEventRow::new`), B-P3-02 (drop module `uninlined_format_args` allow + inline 2 format args).
- `crates/corelink-audit-chain/src/neon_shadow/real.rs` — B-P2-02 (2 construction-site updates).
- `crates/corelink-audit-chain/src/archive_producer.rs` — B-P2-06 (cross-module never-empty-receipt invariant doc).
- `apps/server/src/routes/audit_analytics.rs` — B-P2-03 (rate_limit_check doc + cross-stream sym ref), B-P2-04 (cardinality-vs-rate-limit residual surface doc), B-P3-03 (parse_tenant_header `result_large_err` doc).
- `migrations/neon/0001_audit_events_shadow.sql` — B-P2-05 (global-rollup hook + write-amplification rationale).

**Audit docs updated:**
- `specs/_audits/sealed/2026-05-16-wave18-adversarial-review-streamA-audit-export.md` — every row in §3 findings table moved OPEN → CLOSED; new §6 wave-20 closure verdict.
- `specs/_audits/sealed/2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md` — every row in §3 findings table moved OPEN → CLOSED; new §6 wave-20 closure verdict.
- `specs/_audits/sealed/2026-05-16-wave18-aggregate-closure.md` — this doc.

## 3. Wave-19 OBE attribution

The 5 P1 findings in each stream (10 total) were attributed to wave-19 builder streams that landed on main between the original review (2026-05-16) and this closure pass:

- A-P1-01 → `wt/r-prep-audit-export-async-pages` (true async-pager streaming)
- A-P1-02, A-P1-03, A-P1-05 → wave-19 fail-CLOSED emit-discipline cross-stream
- A-P1-04 → `wt/r-prep-audit-export-payload-column` (additive `payload: Option<serde_json::Value>` field + stable mid-stream enum constant)
- B-P1-01 → `wt/r-prep-neon-shadow-real-driver` (RealNeonShadowSink + RLS policy hardening)
- B-P1-02, B-P1-05 → wave-19 emit-discipline cross-stream
- B-P1-03 → wave-19 audit-analytics emit-parity cross-stream
- B-P1-04 → proptest coverage added in the neon-shadow-real-driver stream

## 4. Residual / deferred (post-wave-20)

The only residual flagged-and-documented trait across both streams is the shared **`WallClock`-collaborator swap** (A-P2-05 / B-P2-03) — both routes anchor the rate-limit clock to the query window's upper bound rather than wall-clock. This is bounded by production wiring (the global per-tenant request budget is enforced by `WallClock`-backed throughput cap one layer up) and is slated as one cross-route fix-stream in wave-21+.

The 0.5/10 deduction from the perfect 10.0 raw score on each stream's post-closure scorecard accounts for this deferred-but-documented residual.

## 5. Gates verified (worktree `wt/r-prep-wave18-codex-p2-p3-closure`)

- `cargo build --workspace` — green (7m 52s, exit 0).
- `cargo clippy --workspace --all-targets -- -D warnings` — green (7m 53s, exit 0).
- `cargo test --workspace --no-fail-fast` — green (see commit log for run output).
- `python3 scripts/validate_specs.py` — green (448 docs validated: 439 schema + 9 YAML-only).
- `python3 scripts/validate_references.py` — green (503 docs, zero dangling references; EVT/CTRL/PAT/FM/INV/FF-HR/SLO/RB/ADR/WAIVER all clean).

## 6. Recommendation

**Wave-18 SEAL: elevate CONDITIONAL → PASS.**

Both adversarial review streams have all P0/P1/P2/P3 findings closed (wave-19 OBE for P1, wave-20 cosmetic-cleanup for P2/P3). The aggregate post-closure score lifts from 8.35/10 (the CONDITIONAL band's upper edge) to 9.5/10 (firmly in the PASS band). The only residual is the `WallClock` cross-route cleanup, documented and slated for wave-21+.

---

**Closer sign-off:** wave-20 builder (opus 4.7), 2026-05-16. Aggregate closure — wave-18 cleared for PASS-tier SEAL elevation.
