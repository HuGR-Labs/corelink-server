# GA-1 Feature Freeze Declaration — 2026-05-16

> **Doc kind:** sign-off-ready feature-freeze declaration (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-26 GA-1 feature-freeze agent (Claude Opus 4.7) — branch `wt/r-prep-ga-1-feature-freeze`.
> **Base:** `main` @ `2a4e00c` ("merge wt/r-prep-tenant-config-cf-prod-wire into main (wave-25)" — wave-25 SEAL tip).
> **Scope:** declare a formal **GA-1 feature freeze** over the spec corpus, invariant registry, ADR set, public API surface, OpenAPI envelope, runbooks, and dashboards. From the effective date below forward, only the §3 exception classes are permitted to merge into `main`. Every commit landing on `main` after the effective date must be auditable against the §3 allowlist; the §5 monitor records each such commit with its rationale and DEFER/exception class.
>
> **Companion docs:**
> - `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` §1.2 freeze-status row → **ACTIVE** (updated this commit).
> - `specs/_runbooks/RB-GA-CUTOVER.md` §1 (cutover freeze window operationalisation — this declaration *precedes* and *enables* §1).
> - `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` (2-key governance path for exceptional unfreeze decisions).
> - `scripts/check-ga-freeze-allowed.py` (mechanical gate that scans a diff against the §3 allowlist).
> - `reports/ga-freeze-monitor.json` (machine-readable freeze monitor — append-only).
> - `CONTRIBUTING.md` §0 (freeze-active notice — updated this commit).

---

## §1. Freeze effective date

**Effective:** **2026-05-16 (this commit's author timestamp)**, on merge of branch `wt/r-prep-ga-1-feature-freeze` into `main`.

**Anchor commit:** the merge commit of this branch into `main` is the canonical anchor; the SHA is recorded retroactively in `reports/ga-freeze-monitor.json` `freeze_anchor_commit` once merged. Until the merge SHA exists, the field carries the placeholder `"PENDING_MERGE"`.

**Freeze name:** `GA-1` (one feature-freeze label before GA cutover; not to be confused with `GA-1d` or `GA-1h` countdown anchors used by `RB-GA-CUTOVER.md` §0-§1).

**Freeze window length:** indefinite — extends from the effective date through the GA cutover *and* through the post-GA T+7d clean-state observation period. Termination is governed by §6 below.

---

## §2. Frozen surfaces

The following surfaces enter freeze on the effective date. Any change that adds, removes, or semantically alters a member of these surfaces requires §3 + §4 routing.

| # | Surface | Path / artefact | Frozen-state evidence |
|---|---|---|---|
| 2.1 | **Spec corpus** | `specs/**/*.md` (excluding `specs/_audits/`, `specs/_archive/`, `specs/_compliance/`, `specs/_schemas/`, `specs/_templates/`) | `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` §1.2 row "Spec corpus" ✅ GREEN; 262 docs / 197 INVs declared per wave-23 sweep. |
| 2.2 | **Invariant registry** | `specs/03_architecture/invariant_registry.md` | 197 INVs declared / 143 WI-coverage / 0 orphan; further additions deferred to wave-27+ unless P0 security or P1 GA-blocker. |
| 2.3 | **ADR set** | `specs/03_architecture/adrs/ADR-*.md` | ADR-0001 … ADR-0034b SEALED; new ADRs only via §3.b (P1 GA-blocker) or §3.a (P0 security) classes with 2-key per §4. |
| 2.4 | **Public API surface** | `crates/corelink-api/`, `apps/server/src/routes/**`, public Rust re-exports under `crates/*/src/lib.rs` | All routes wired wave-23 → wave-25; tenant-config + CF-prod wired wave-25 (commit `2a4e00c`). |
| 2.5 | **OpenAPI envelope** | `openapi/openapi.yaml` + any `openapi/components/*` | Generated from §2.4; freeze inherits transitively. |
| 2.6 | **Runbooks** | `specs/_runbooks/RB-*.md` (50+ runbooks) | All RBs cross-referenced from `RB-GA-CUTOVER.md`; runbook tracker SEALED wave-17 (`specs/04_sprints/S17/`). |
| 2.7 | **Dashboards** | `dashboards/*.json` + `dashboards/grafana/**` | SLO catalog wave-15; chaos + endurance dashboards wave-22; observability complete. |
| 2.8 | **Migrations** | `migrations/*.sql`, `migrations/d1/*.sql` | INV-AUTH-MIGRATION-ADDITIVE enforced by `scripts/check_migrations_additive.py`. Freeze adds: even *additive* migrations require §3.b classification. |
| 2.9 | **Schemas** | `schemas/*.json`, `schemas/*.yaml` | Event/audit schemas SEALED wave-9 (audit-chain) + wave-13 (Stripe webhooks). |

**Out-of-scope (NOT frozen):**

- `specs/_audits/**` — audit corpus is append-only and continues during freeze (this very doc lives there).
- `specs/_compliance/**` — compliance attestations + sub-processor list continue to receive routine updates (DPA, sub-processor changes) without §3 routing.
- `specs/_archive/**` — archival is reversible by definition; freeze does not apply.
- `reports/**` — operational telemetry + monitor outputs (including `ga-freeze-monitor.json` itself).
- `mutants.out/`, `target/`, `node_modules/`, `_archive/` — build/test artefacts.
- `CHANGELOG.md`, `TODO.md`, `ROADMAP-TO-GA.md` — release-coordination artefacts continue to update.
- Cosmetic typo fixes per §3.c.

---

## §3. Allowed exceptions

Only the following three exception classes may merge to `main` after the effective date. Every other change must wait for §6 thaw.

### §3.a — P0 security fix

**Definition:** a vulnerability with CVSS v3.1 ≥ 7.0 (HIGH) on a frozen surface that is reachable from a customer or untrusted-tenant path, OR a SEV-0/SEV-1 incident root-cause fix that must land before GA cutover.

**Routing:**

- Issue tagged `priority:P0` + `track:security`.
- 2-key approval per §4 mandatory (Owner + Security Lead).
- Must reference a `specs/_audits/` post-mortem or `SECURITY.md` advisory.
- Commit subject must start with `fix(security): ` and body must contain `FREEZE-EXCEPTION: P0-security`.

### §3.b — P1 GA-blocker fix

**Definition:** a defect on a frozen surface that prevents `RB-GA-CUTOVER.md` §0 greenlight criteria from evaluating to `true`, OR a regression discovered during a `RB-GA-CUTOVER.md` dry-run that blocks the rehearsal from completing within the SLO budget.

**Routing:**

- Issue tagged `priority:P1` + `track:ga-blocker`.
- 2-key approval per §4 mandatory (Owner + on-call SRE).
- Must reference `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` §13.1 (the pre-condition gate row that flips from `true` to `false`).
- Commit subject must start with `fix(ga-blocker): ` and body must contain `FREEZE-EXCEPTION: P1-ga-blocker`.

### §3.c — Cosmetic doc fix

**Definition:** typo, broken link, formatting (heading level, list marker, code-block language tag), or trivially-correct cross-reference. **No semantic change** to the underlying claim. Must be reviewable in < 60 seconds.

**Examples that ARE in-scope:**
- `recieve` → `receive` in a doc.
- Fixing a relative path that already resolves wrong (target unchanged).
- Adding a missing trailing newline.

**Examples that are NOT in-scope (and require §3.a or §3.b):**
- Changing an INV definition's predicate.
- Changing a runbook step ordering.
- Changing a dashboard threshold.
- Updating an OpenAPI field's `required:` list.
- Adding/removing a sub-processor.

**Routing:**

- Issue tagged `priority:P3` + `track:cosmetic-doc` (optional — direct PR is acceptable for ≤ 5 line changes).
- Single-reviewer approval (CODEOWNERS) sufficient.
- Commit subject must start with `docs: ` and body must contain `FREEZE-EXCEPTION: cosmetic-doc`.

### §3.d — Implicitly allowed (no §4 routing)

- Additions under `specs/_audits/**` (this audit corpus is append-only by design).
- Additions under `specs/_compliance/**` (sub-processor / DPA updates).
- `reports/**` mutations (operational telemetry).
- `CHANGELOG.md` updates that summarise §3.a / §3.b / §3.c entries already merged.
- `mutants.out/`, `target/`, `node_modules/` — gitignored anyway; defensive only.

---

## §4. Decision protocol

Any change *not* classifiable under §3.c (cosmetic-doc) or §3.d (implicitly allowed) requires the **2-key authorisation** specified in `ADR-0034b-framework-reviewer-dual-hat-fallback.md` §3 (dual-hat fallback), with the role pairings below:

| Exception class | Key 1 (decision authority) | Key 2 (technical correctness) | Recording artefact |
|---|---|---|---|
| §3.a P0-security | Owner (Gustavo) | Security Lead | `reports/ga-freeze-monitor.json` entry with `class: "P0-security"` + `keys: ["owner", "security-lead"]`. |
| §3.b P1-ga-blocker | Owner (Gustavo) | On-call SRE | `reports/ga-freeze-monitor.json` entry with `class: "P1-ga-blocker"` + `keys: ["owner", "oncall-sre"]`. |
| §3.c cosmetic-doc | CODEOWNERS approver | (single key) | `reports/ga-freeze-monitor.json` entry with `class: "cosmetic-doc"` + `keys: ["codeowner"]`. |

**Recording requirement:** every merge to `main` after the effective date must append a row to §5 below *and* to `reports/ga-freeze-monitor.json`. The `scripts/check-ga-freeze-allowed.py` gate will refuse to pass a diff whose commit message lacks one of the four `FREEZE-EXCEPTION:` tokens (`P0-security`, `P1-ga-blocker`, `cosmetic-doc`, or `implicit-allow`).

**Escalation:** any change the routing team cannot classify into §3.a / §3.b / §3.c / §3.d MUST be deferred to the §6 thaw window. There is no "P2 freeze-exception" class.

---

## §5. Freeze monitor (commits on `main` since freeze)

Append-only ledger. Each merge to `main` after the §1 effective date appends one row here (and one entry to `reports/ga-freeze-monitor.json`). Initially empty.

| # | Commit SHA | Date (UTC) | Commit subject | §3 class | 2-key recorded | Rationale |
|---|---|---|---|---|---|---|
| _(none yet)_ | — | — | — | — | — | — |

**Convention:** when this audit is updated to add a row, the update *itself* is implicitly allowed (it lives under `specs/_audits/`) and does not require its own row. The row records the *other* commit that triggered the entry.

---

## §6. Thaw conditions

The freeze terminates ("thaws") when **all** the following are satisfied:

1. **GA cutover executed.** `RB-GA-CUTOVER.md` §6 declares cutover complete; a `specs/_audits/<DATE>-ga-cutover-postmortem.md` exists and is SEALED.
2. **T+7d clean window.** Seven calendar days have elapsed since the cutover completion timestamp with **zero SEV-0 and zero SEV-1 incidents** attributable to the freeze surfaces enumerated in §2. PagerDuty incident query is the canonical source.
3. **Owner formal thaw declaration.** The Owner publishes a `specs/_audits/<DATE>-ga-1-feature-thaw.md` companion document that:
   - cross-references this declaration's §1 anchor commit SHA;
   - asserts the cutover post-mortem is SEALED;
   - asserts the T+7d clean window has elapsed (with the PagerDuty query result attached);
   - re-classifies the surfaces in §2.1 → §2.9 as **post-GA mutable** (with the wave-27+ framework promotion path already documented in `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` §17 firing on the Owner key).
4. **`scripts/check-ga-freeze-allowed.py` retired or made permissive.** Either the script is deleted from `scripts/` or it gains a `--post-thaw` mode that short-circuits to exit 0; the choice is at the Owner's discretion and is recorded in the thaw declaration.

**Until all four conditions are met, the freeze is ACTIVE.** A partial thaw (e.g. "API surface only") is not provided for in this declaration; if the Owner needs a partial thaw before T+7d (e.g. to ship a wave-27 framework v1.0.0 promotion that touches §2.1 spec corpus), the §3.b P1-ga-blocker exception class applies and the framework promotion is recorded as the rationale.

**Pre-thaw posture:** the freeze is the *normal* state. New work that is neither §3.a / §3.b / §3.c / §3.d MUST be queued in `TODO.md` or `ROADMAP-TO-GA.md` with a `post-thaw` tag and waits for §6 satisfaction.

---

## §7. Cross-references

- `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` §1.2 (freeze-status row → **ACTIVE**, set this commit).
- `specs/_runbooks/RB-GA-CUTOVER.md` §0 (greenlight criteria — operates *within* the freeze) and §1 (cutover freeze window — narrower T-72h window nested inside this freeze).
- `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` §3 (dual-hat 2-key path).
- `scripts/check-ga-freeze-allowed.py` (mechanical gate; this audit is the authoritative spec).
- `reports/ga-freeze-monitor.json` (machine-readable mirror of §5).
- `CONTRIBUTING.md` §0 (freeze-active notice for community contributors).

---

_End of GA-1 feature-freeze declaration._
