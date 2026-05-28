---
id: "AUDIT-2026-05-27-SYNTHETIC-MONITORING"
type: "wave-seal-audit"
doc_status: "SEALED"
audit_status: "CLOSED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
closed: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags:
  - "audit"
  - "wave32"
  - "synthetic-monitoring"
  - "betterstack"
  - "wp-7.1"
references:
  - "monitoring/synthetic/probes.yml"
  - "monitoring/synthetic/README.md"
  - "scripts/apply-betterstack-probes.sh"
  - "specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md"
  - "specs/_audits/sealed/2026-05-26-w32-phaseI-signoff.md"
  - "marketing/launch/STATUS-PAGE-SPEC.md"
---

# WP-7.1 Synthetic Monitoring Probes — SEAL Audit (2026-05-27)

> **Doc kind:** wave-seal audit (evidence; `_audits/` excluded from canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-27 by Claude Sonnet 4.6.
>
> **Trigger:** WP-7.1 from `specs/_audits/2026-05-27-15-agent-dispatch-matrix.md` §2.
>
> **Charter compliance:** SOTA bar; dry-run only; no real BetterStack API calls;
> CTRL-CRED-001 preserved (no token in any committed file).

---

## §1 Scope delivered

| Artifact | Path | Status |
|---|---|---|
| Probe manifest | `monitoring/synthetic/probes.yml` | CREATED |
| Runbook | `monitoring/synthetic/README.md` | CREATED |
| Apply script | `scripts/apply-betterstack-probes.sh` | CREATED |
| Seal audit | `specs/_audits/2026-05-27-synthetic-monitoring-seal.md` | THIS DOC |

---

## §2 Probe inventory

7 probes defined (5 base per WP-7.1 contract + 2 added per R27):

| # | Name | URL | Interval | Method | Assertion |
|---|---|---|---|---|---|
| 1 | `corelink-api-health` | `https://corelink-api.humangr.com/health` | 30s | GET | status 200 + JSON `$.status == "ok"` |
| 2 | `corelink-app-ui` | `https://corelink-app.humangr.com` | 60s | GET | status 200 |
| 3 | `corelink-docs` | `https://corelink-docs.humangr.com` | 60s | GET | status 200 |
| 4 | `corelink-signup-health` | `https://corelink-signup.humangr.com` | 60s | GET | status 200 |
| 5 | `corelink-get-install` | `https://corelink-get.humangr.com` | 60s | GET | status 200 + `Content-Type: text/plain` |
| 6 (R27) | `corelink-admin-health` | `https://corelink-admin.humangr.com/health` | 60s | GET | status 200 |
| 7 (R27) | `corelink-get-head` | `https://corelink-get.humangr.com` | 60s | HEAD | status 200 |

All probes run from three regions: `us-east`, `eu`, `ap`.

BetterStack page ID: `247652` (confirmed in `specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md` §3).

---

## §3 Acceptance gate results

### YAML validation

```
python3 -c "import yaml; data=yaml.safe_load(open('monitoring/synthetic/probes.yml')); \
  probes=data['probes']; print(f'YAML valid: {len(probes)} probes')"
YAML valid: 7 probes
```

Exit: **0**. Result: **PASS**.

### shellcheck

```
shellcheck scripts/apply-betterstack-probes.sh
```

Exit: **0** (no output). Result: **PASS**.

### Dry-run execution

```
bash scripts/apply-betterstack-probes.sh --dry-run
```

Exit: **0**. All 7 probes listed with full API JSON bodies. No API calls made. Result: **PASS**.

Dry-run output summary:
- 7 probes iterated in manifest order
- Each printed: name, `POST/PATCH` endpoint, full JSON body
- `_tags_comment` field included in dry-run output; stripped before live API call
- Final line: `=== DRY-RUN COMPLETE: 7 probe(s) listed. ===`

---

## §4 Security notes (CTRL-CRED-001)

- `BETTERSTACK_API_TOKEN` is read exclusively from env; never printed, logged, or committed.
- Apply script guards token with `require_token()` which exits 1 with a helpful message if token is absent.
- Dry-run mode (`--dry-run`) requires no token and makes zero network calls.
- The `_tags_comment` field in JSON output is informational only; stripped via `python3` before any live API call.

---

## §5 Design decisions

1. **7 probes, not 5**: R27 adds `corelink-admin-health` (admin worker `/health`) and `corelink-get-head` (lightweight HEAD on the install endpoint). Both targets are confirmed live per Phase I sign-off (`specs/_audits/sealed/2026-05-26-w32-phaseI-signoff.md` §3).

2. **Probe 1 at 30s, rest at 60s**: API worker is the primary customer-facing surface. Tighter interval for faster failure detection without excessive overhead.

3. **Idempotent apply**: Script GETs `/api/v2/monitors`, matches by `pronounceable_name`, PATCHes existing. Prevents duplicate monitors on re-run.

4. **python3 for YAML parse + JSON build**: Avoids `yq` or `jq` as external dependencies. `python3` + `yaml` are universally available in CI.

5. **HEAD probe for `corelink-get`**: A HEAD check gives lightweight uptime signal without triggering a full install-script download on every probe tick.

---

## §6 Residual risks

| Risk | Severity | Mitigation |
|---|---|---|
| BetterStack API region names may differ from manifest | LOW | `us-east`/`eu`/`ap` are documented BetterStack POP identifiers; verify on first live apply |
| `corelink-app.humangr.com` behind Clerk may return 302 redirect pre-auth | LOW | BetterStack follows redirects by default; final 200 expected after CF Pages serves HTML |
| `corelink-get.humangr.com` `text/plain` assertion: CF Pages may serve `text/plain; charset=utf-8` | LOW | BetterStack header-contains matching is substring; `text/plain` matches both |

---

## §7 DoD checklist

1. `probes.yml` valid YAML: **PASS** (7 probes, python3 yaml.safe_load clean)
2. Apply script `--dry-run` lists all 7 probes and API JSON bodies, no real calls: **PASS**
3. `shellcheck` clean: **PASS** (exit 0, no warnings)
4. README documents create/update/delete via the script: **PASS**
5. Single commit on worktree: **PENDING** (commit follows this audit)

---

## §8 Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>.

---

**End of WP-7.1 synthetic monitoring SEAL audit.**
