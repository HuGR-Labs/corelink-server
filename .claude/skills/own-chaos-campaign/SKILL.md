---
name: own-chaos-campaign
description: >-
  Route source-backed ownership work for the chaos-campaign manifest, in-memory
  scenario models and opt-in test declarations. Do not use for executing a
  campaign, production chaos, provider operations or other harness packages.
metadata:
  schema: "corelink-ownership/1.1"
  package: "chaos-campaign"
  manifest: "tests/chaos/Cargo.toml"
  source-commit: "cb94e251c0f17382565bf863f517945cbb2a84d6"
  evidence-set: "chaos-campaign-static-cb94e251c"
---

# Ownership — chaos-campaign

[Acionamento](#s01) · [Território](#s02) · [Roteamento](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use this skill | Do not use this skill |
|---|---|
| Positive: change/review `tests/chaos/Cargo.toml`, `src/lib.rs`, a declared target source, or these four ownership artifacts; trace a local API, feature gate, or fixture effect. | Negative: execute a fault campaign, Cargo test/fuzz, call provider/database/storage, deploy/change production, or claim execution. Operational work requires its separately authorized operator and runbook. |
| Positive: assess declared package/source dependencies or update the bounded records after static evidence. | Negative: work on distinct `e2e-chaos` or `corelink-r2-multipart` packages solely because their names mention chaos. |

Source/test *task* means inspect or edit declarations and assertions. It does not mean execute a Cargo test or trigger any failure mode.

<a id="s02"></a>
## S02 — Territory and authority

Owned sources: `tests/chaos/Cargo.toml`, `tests/chaos/src/lib.rs`, and 11 target files named in [R01](../../../docs/ownership/crates/chaos-campaign/REFERENCE.md#r01). Public model contracts are [API records](../../../docs/ownership/crates/chaos-campaign/REFERENCE.md#r04); current package boundaries are [REL records](../../../docs/ownership/crates/chaos-campaign/BLAST_RADIUS.md#b03). Test source is not production wiring.

Two refusals are mandatory:

1. Refuse a request to execute tests/fuzzing or contact providers, databases, network, deployment, or production unless that separate operation has explicit authority, an approved procedure, and its own scope. This ownership skill grants none.
2. Refuse to assume ownership of production analogues, `e2e-chaos`, or `corelink-r2-multipart`; route each to its actual package owner. The models expressly do not bind those implementations.

Package source maintainer and assignment are UNKNOWN; no source-owner escalation route is verified. Do not infer one from the runbook's SRE Lead, approver, or generic CODEOWNERS rule. For an active operational incident only, follow the scenario-specific on-call route in [RB-CHAOS-CAMPAIGN](../../../specs/_runbooks/RB-CHAOS-CAMPAIGN.md); that runbook does not authorize a new operation or identify this package's maintainer.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Question | Read only the needed route |
|---|---|
| Exact source contracts and gaps | [REFERENCE R01–R08](../../../docs/ownership/crates/chaos-campaign/REFERENCE.md#r01) |
| Consumers, effects and exclusions | [BLAST B01–B06](../../../docs/ownership/crates/chaos-campaign/BLAST_RADIUS.md#b01) |
| Safe procedure and validation mode | [MAINTENANCE M01–M06](../../../docs/ownership/crates/chaos-campaign/MAINTENANCE.md#m01) |
| Canonical policy context | [Verified OKF profile](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md); designated route/reference only—do not copy, redefine, or revalidate it. |
| Reachability question | [built-not-wired](../built-not-wired/SKILL.md), only when a build/target artifact is in the authorized evidence scope. |
| Source-domain context | [okf-context](../okf-context/SKILL.md), before unfamiliar subsystem work. |
| Independent review | Follow the independent, per-artifact cold-review requirement in [WAVE 014 plan](../../../docs/ownership/WAVE_014_PLAN.md). The checkout has no repo-local general review skill; the host `review-agent` skill was verified separately and must be independently assigned. |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condition → action | Evidence required | Stop when |
|---|---|---|
| Manifest, target, feature, model, or test assertion changes → update its exact R record and affected atomic B relation. | Pinned source line, target name, API/INV/REL IDs. | Selection, compilation, or execution would be needed to support the claim. |
| A model description conflicts with production behavior → keep the local contract and route to the actual implementation owner. | Separate source evidence for each package; [built-not-wired](../built-not-wired/SKILL.md) only if reachability is authorized. | No verified production source or responsible owner exists. |
| Numeric inventory is reported → state the population inline and reconcile discovered, documented, excluded, and unknown counts. | [B02/B06](../../../docs/ownership/crates/chaos-campaign/BLAST_RADIUS.md#b02). | Search scope or population is incomplete. |

Never turn a fixture assertion, descriptive manifest text, vector append, structural checker, or workspace membership into a test result, delivered event, resolved edge, runtime claim, or production guarantee.

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirm package identity `chaos-campaign`, manifest, pinned source commit, and allowed paths. 2. Classify the request: static source/test editing, document-only validation, or separately authorized execution; this package record itself authorizes only the first two. 3. Read the exact R API/INV and B REL records, then select an M procedure and its mode. 4. Record planned source changes and rollback before editing; preserve local test source as unexecuted unless separately authorized

evidence says otherwise. 5. Run only the selected allowed procedure. Structural checks prove document shape, not semantics or execution. 6. Update only relevant records, report the exact outputs, and request a fresh independent review of changed artifact bytes.

<a id="s06"></a>
## S06 — Paradas

Stop on source drift, unresolved target/feature selection, a new dependency or consumer, a contradictory production claim, integer-boundary uncertainty, missing recovery for an external effect, or a request outside the four-path scope. Mark the specific fact UNKNOWN; do not widen the search by assumption or run an operational command. Source-maintainer assignment remains unresolved; operational incidents use only the documented scenario route, not a package-owner inference.

<a id="s07"></a>
## S07 — Evidência e saída

Handoff must name objective, pinned baseline, changed paths, source/documentary checks actually run, affected `API`/`INV`/`REL`/`PROC` and R/B/M IDs, execution state, residual risk, and next responsible owner. If no package source owner is verified, mark assignment unresolved and ask the task requester to assign one; for an active incident only, identify the relevant documented on-call role/runbook as operational responder, not package owner. State explicitly that tests, runtime, deployment, external activity, and cold review were not established by static author checks.

[Voltar ao início](#s01)
