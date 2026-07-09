# 🧊 FREEZE — corelink-server TL: ALL pending work, mapped + frozen (2026-07-08)

Complete snapshot of every open item on the server side, its exact state, and who owns the next move. Authoritative handoff — nothing is lost. Followed by the TechLead WAVE PLAN for the team deploy.

## A. DONE + LIVE (no action)
- **Launch**: #655 backend + #654 FE + #656 metering + #661 GDPR anchor fanout fix — merged + deployed (5 envs).
- **Cold-hydrate D1 herd**: #667 tier-cache + #669 PAT single-flight — deployed (`d86b1417-r1` → superseded by `55d4dcd2` worker).
- **Track-B real-user erase (GDPR go-live gate)**: #670 + #672 + #673 (fva through the exchange, both issuer paths) — DEPLOYED `55d4dcd2` all 5 envs; `fva_minutes:0`→erase `202` proven; **clw FINAL-GREENLIT**. Grace restored to 7d (githugr `85a9a7a`). Copy flip `solicitado→apagado` = **owner-deferred** (legal wording; honest "solicitado" + 7d is the safe interim).
- **Native-moat gate #1 code**: #674 (server: installation_id-optional + bearer-PAT-introspection) + runners #321 (mirror) — merged; #674 live in `55d4dcd2`.

## B. IN-FLIGHT (mine, auto-completing)
- **PR #675** — org-rename DOC core (README/LICENSE/marketing/infra dashboards/wrangler comment). 0 fails, auto-merges on green (watcher `b9hxjvz16`). validate_specs+OKF green.

## C. PENDING — MINE (the team executes; see WAVE PLAN)
- **#42 org-rename code+specs** — branch `chore/org-rename-code-and-specs` @ `863075ac` (code WIP committed: SLSA provenance identity in corelink-ops, homograph attacker-list → HumanGuardrail look-alikes, PURL/SBOM, install.ts, JSON-LD, github.io/email lowercase). **TODO**: specs/ + docs/knowledge (OKF-aware reconcile), .github/CODEOWNERS placeholder teams, + **gate verification** (cargo test -p corelink-ops {supply_chain_verify_adversarial, deploy_chaos, deploy_prop_verify}, sbom tests, clippy -D warnings, validate_okf, validate_specs).
- **#36 framework-majors** (apps/admin-ui) — Next 15→16, Clerk 6→7, Sentry 8→10, Stripe-js 4→9. One major at a time; bump + known migrations + gates + runtime QA (auth/billing/render). Owner: "should already be done."

## D. PENDING — OTHER TLs (not mine; tracked)
- **#41 allowlist seed** — clw runs the vetted one-shot: `runner_repo_allowlist(d863fafb, 'HumanGuardrail/corelink-runners')` + verify entitlement/not-offboarded (live creds). Then runners arm the E2E.
- **Track-A 410 (de-risk)** — digest delivered (githugr, `96b9bbae…`); witness cred = githugr's `d863fafb` PAT; githugr drives option B; clw re-confirms durable 410. No server mint needed.
- **Copy flip** — githugr, owner-gated wording.
- **hugit `Option<i64>` parse-hardening** — hugit lands; clw re-audits.

## E. SCHEDULED
- **#31 dependabot 5-major consolidation** — cron `b9eec689` armed, fires **July 9 01:07**.

## F. CONSTRAINTS (frozen)
branch→PR→merge (no direct main push); DCO `Signed-off-by` + `Co-Authored-By` contiguous; merge-commit NOT squash (OKF checkpoints); clippy `-D warnings`, expect/unwrap denied non-test; CF token DEAD locally → deploys via `gh workflow` / clw; OKF C5 = citation STABILITY (line-neutral edits / content-match repin); org = **HumanGuardrail** (humangr-labs DISCONTINUED).

---

# TechLead WAVE PLAN — org-rename-finish + framework-majors @ main | 2026-07-08

```
GO/NO-GO     : PARALLEL (2 tracks, isolated worktrees)
DISJOINTNESS : Track-42 owns {specs/, docs/knowledge/, crates/corelink-ops, tools/sbom, .github/CODEOWNERS}
               Track-36 owns {apps/admin-ui/**}. Different file trees + different branches → CONFLICT-FREE
               (only CHANGELOG.md union-resolvable at merge; each track edits its own [Unreleased] line).
CONTRACT     : frozen — org string = "HumanGuardrail" (github.com URLs), "humanguardrail" (github.io/winget lowercase),
               homograph test = HumanGuardrail look-alikes (intent preserved). #36 = ONE major (Next 16) first.
RETURN-SHAPE : { branch, commits[], files_changed, gates: {name: pass|fail|NA}, residual_breakage[], notes }
DOD (V1)     : Track-42 → cargo test -p corelink-ops + sbom green, clippy -D warnings clean, validate_okf + validate_specs green, 0 humangr-labs in code/specs (excl handoff/CHANGELOG history). Track-36 → deps bumped, official codemod applied, typecheck/vitest/build result CAPTURED (breakage reported, NOT blindly fixed — judgment returns to lead).
MERGE ORDER  : Track-42 (verified) → PR; Track-36 → PR after lead reviews the breakage report.
```

**Rigor note (AP-3):** Track-36's *fixes* are judgment — the agent bumps + applies OFFICIAL migrations + REPORTS residual breakage; it does NOT invent runtime fixes. The lead decides those.

— corelink-server TL
