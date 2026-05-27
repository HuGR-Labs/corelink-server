---
id: "RB-FM-157"
type: "runbook"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "supply-chain", "typosquatting", "cargo", "lockfile-diff", "codeowners", "sbom", "dependency-track", "rb-fm-157", "wi-s12-007", "dry-run-executed"]
dry_run_executed: "2026-05-14"
dry_run_evidence: "specs/_audits/sealed/2026-05-14-rb-fm-157-dry-run.md"
---

# RB-FM-157 — Typosquatting Dependency (Cargo / crates.io)

> **FM:** FM-157 (S=4, O=2, D=4, RPN=32, P1) | **CTRLs:** CTRL-SUPPLY-001 (SLSA L3) + CTRL-SUPPLY-004 (cargo-audit/deny) + CTRL-SUPPLY-006 (Dependency-Track) | **SLA:** detect ≤ 24h (lockfile diff / CODEOWNERS review); PR block ≤ 5 min (required-status-checks gate); remediate ≤ 7d

## Scenario

An attacker publishes a package on crates.io with a name visually similar to a
popular, trusted crate (e.g., `serde` → `serde-util`, `tokio` → `tok1o`,
`corelink-server` → `corelink-server-util`). The typosquatted crate may carry a
functional-looking API to avoid immediate suspicion while silently exfiltrating
secrets, installing persistence mechanisms, or sabotaging build outputs.

The attack materialises when a developer inadvertently runs `cargo add <typo>` or
a dependency of a dependency pulls in the typosquatted crate via a transitive path.

**Threat actor:** TA-5 (supply chain — opportunistic attacker, low-barrier crates.io publish).
**Blast radius:** all tenants if typosquat deployed to production Workers;
at minimum: attacker can read Worker environment variables + Cloudflare bindings
in scope.

**Key distinction from FM-156:** the typosquatted crate may have *zero* CVEs in
NVD/OSV at time of introduction (it is a brand-new malicious crate). cargo-deny
`unknown-registry` is not the primary defence here — the crate IS on crates.io.
Manual lockfile review + CODEOWNERS + SBOM anomaly detection are the primary gates.

---

## Detecção

### Primary paths (≤ 24h post-introduction)

1. **Lockfile diff PR comment Action** (`lockfile-diff.yml`, WI-S12-004):
   - On every PR that changes `Cargo.lock`, the workflow emits a GitHub PR comment:
     `⚠️ Cargo.lock changed — new/modified crate detected: <crate_name> <version>`.
   - Reviewer is required to inspect the diff before approving.
   - Required status check `supply-chain / lockfile-diff` must pass.

2. **CODEOWNERS review** (`.github/CODEOWNERS`):
   - `Cargo.toml` + `Cargo.lock` are CODEOWNERS-protected; at least 1 designated
     supply-chain reviewer must approve any change.
   - Prevents blind auto-merge of a typosquat-introducing PR.

3. **cargo-deny sources policy** (`deny.toml`, WI-S12-004):
   - `[sources]` section enforces `unknown-registry = "deny"` — only crates.io
     is permitted. Git dependencies require explicit ADR.
   - Note: a typosquatted crate IS on crates.io, so cargo-deny will NOT block it
     by policy alone. The lockfile diff + manual review is the primary detection gate.

4. **SBOM ingestion Dependency-Track anomaly** (WI-S12-005):
   - After each build/release, the SBOM CycloneDX is ingested into Dependency-Track.
   - A previously unseen crate name appearing in the SBOM lands in the DT
     "new component" review queue.
   - DT CVE matching: if the typosquat has a published CVE (unusual but possible
     after community discovery), DT fires alert ≤ 15 min.
   - Even without a CVE, the new-component anomaly in DT review queue is a signal
     for manual triage.

5. **cargo-audit RustSec advisory** (post-community discovery, typically 24–72h delay):
   - Once the crates.io security team or community identifies the typosquat,
     a RustSec advisory is typically filed.
   - `cargo audit --deny warnings` in CI then blocks future merges.

### Secondary paths

6. **Developer awareness**: `cargo add <typo>` emits output; developer notices
   unfamiliar package name during local review.
7. **SLSA provenance check**: provenance attestation includes the full crate
   dependency tree; anomalous crate name visible in SBOM release asset.
8. **Dependency graph audit tool**: `cargo tree --workspace` output review
   during security walkthrough reveals unexpected crate.

### Metrics to watch

| Metric | Alert threshold | Severity |
|---|---|---|
| `corelink_supply_lockfile_changed_total` | any change on PR | INFO (expected; triggers review) |
| `corelink_supply_dt_new_component_total` | > 0 unlabelled | SEV-3 (manual triage) |
| `corelink_supply_rustsec_advisory_total{severity="high|critical"}` | > 0 | SEV-1 / SEV-2 |
| `corelink_supply_cargo_audit_ci_block_total` | > 0 after advisory | INFO (CI gate working) |

---

## Comunicação

- **SEV-3** initial (typosquat suspected; no confirmed malicious activity in prod yet).
- Escalate to **SEV-2** if: typosquat version confirmed in a merged PR or staging build.
- Escalate to **SEV-1** if: confirmed typosquat deployed to production.
- Page: Security Lead + Engineer (supply-chain lead).
- Slack: `#supply-chain-cve-alerts` + `#incidents-corelink-sre` if SEV-2+.
- External: Customer notification NOT required unless typosquat confirmed deployed
  and data exfiltration suspected. Prepare draft in parallel (T+30 min).

---

## Mitigação imediata (≤ 30 min)

### Step 1 — Confirm typosquat and scope (T+0..T+10 min)

```bash
# 1a. Identify the suspicious crate name from lockfile diff or DT alert
# Replace <suspicious_crate> with actual crate name
SUSPICIOUS="<suspicious_crate>"

# 1b. Check if it is in Cargo.lock currently
grep -A4 "name = \"$SUSPICIOUS\"" Cargo.lock

# 1c. Which PRs introduced it?
git log --oneline --all -- Cargo.lock | head -20
git diff HEAD~5..HEAD -- Cargo.lock | grep "^+.*$SUSPICIOUS" | head -10

# 1d. Check crates.io metadata for the suspicious crate
curl -s "https://crates.io/api/v1/crates/$SUSPICIOUS" | jq '{
  name: .crate.name,
  description: .crate.description,
  homepage: .crate.homepage,
  repository: .crate.repository,
  downloads: .crate.downloads,
  created_at: .crate.created_at,
  updated_at: .crate.updated_at
}'

# 1e. Compare to the legitimate crate (low download count + recent creation = red flag)
LEGITIMATE="<legitimate_crate>"
curl -s "https://crates.io/api/v1/crates/$LEGITIMATE" | jq '.crate | {downloads, created_at}'

# 1f. Check DT for new component
curl -s -H "X-Api-Key: $DT_API_KEY" \
  "$DT_BASE_URL/api/v1/component/identity?purl=pkg%3Acargo%2F$SUSPICIOUS" \
  | jq '{component: .[0].name, version: .[0].version, projects: [.[].projects[].name]}'
```

### Step 2 — Block PR / quarantine (T+0..T+5 min, if PR still open)

```bash
# If the introducing PR is still open — request changes
gh pr review <PR_NUMBER> --request-changes \
  --body "SUPPLY CHAIN HOLD: suspected typosquat crate '$SUSPICIOUS'. Do NOT merge until Security review complete."

# If already merged into a feature branch (not main):
# Revert the branch or create a hot-fix branch to remove the crate
git revert <merge_commit>
```

If the crate is NOT yet in `Cargo.lock` on `main` — the lockfile diff gate held.
Proceed with investigation only (no emergency rollback needed).

### Step 3 — Assess whether typosquat is in staging or production (T+5..T+20 min)

```bash
# 3a. Check deployed Worker SBOM for the suspicious crate
# Download last released SBOM from GitHub Releases
gh release download --pattern 'sbom.cdx.json' --output /tmp/sbom-latest.json
jq ".components[] | select(.name == \"$SUSPICIOUS\")" /tmp/sbom-latest.json

# 3b. Confirm via SLSA attestation (Rekor provenance)
# If crate was in a build, it will appear in Rekor-backed provenance
rekor-cli search --rekor_server https://rekor.sigstore.dev \
  --artifact <release_bundle_sha256> | head -20

# 3c. Check Worker environment variables for potential exfiltration surface
# Review CF Dashboard: Workers > <worker> > Settings > Variables
# Identify what secrets were in scope if typosquat ran during build
```

### Step 4 — Emergency containment if typosquat in production (T+20..T+30 min)

If confirmed in production:
1. Escalate to **SEV-1**.
2. Rollback: `wrangler rollback --env production` to prior known-good version.
3. Rotate all environment variables / Cloudflare API tokens accessible in the
   Worker environment (worst-case: exfiltrated during build step).
4. Preserve forensic evidence: Worker invocation logs + build logs + SBOM artifacts.
5. Notify security team + prepare customer communications.

---

## Investigação

### Step 5 — Characterise the typosquat crate (T+10..T+60 min)

```bash
# 5a. Download and inspect source (sandboxed environment ONLY)
# Use an isolated Docker container; do NOT run on dev workstation
docker run --rm -it rust:1.78 bash -c "
  cargo add $SUSPICIOUS 2>&1;
  cat Cargo.toml;
  cargo fetch 2>&1
"

# 5b. Inspect build.rs (common malware vector)
# Extract the .crate archive and inspect
curl -s "https://crates.io/api/v1/crates/$SUSPICIOUS/<version>/download" -o /tmp/suspicious.crate
mkdir -p /tmp/suspicious_inspect && tar -xf /tmp/suspicious.crate -C /tmp/suspicious_inspect
ls /tmp/suspicious_inspect/
cat /tmp/suspicious_inspect/$SUSPICIOUS-<version>/build.rs 2>/dev/null
cat /tmp/suspicious_inspect/$SUSPICIOUS-<version>/src/lib.rs 2>/dev/null | head -100

# 5c. Compare name similarity to known legitimate crates
# Levenshtein distance ≤ 2 = high typosquat risk
# Manual comparison: print both names side by side
echo "Suspicious: $SUSPICIOUS"
echo "Legitimate: $LEGITIMATE"
# Common patterns: swap chars, add digit, add suffix (-util, -rs, -sys)

# 5d. Check if RustSec advisory exists
curl -s "https://rustsec.org/advisories/index.json" 2>/dev/null | jq ".[] | select(.package == \"$SUSPICIOUS\")"
# Or: cargo audit --json | jq ".vulnerabilities.list[] | select(.package.name == \"$SUSPICIOUS\")"

# 5e. Review crate author history on crates.io
curl -s "https://crates.io/api/v1/crates/$SUSPICIOUS/owner_user" | jq '.[].login'
# New account + no other crates + recent creation = strong indicator
```

### Step 6 — Blast radius assessment

```bash
# 6a. Which corelink crates directly or transitively depend on the suspicious crate?
cargo tree --workspace --invert $SUSPICIOUS 2>/dev/null

# 6b. How long was it in the codebase? (commit history)
git log --all --format="%H %ai %s" -- Cargo.lock | head -20
# Find first commit that introduced it:
git log --all -S "$SUSPICIOUS" -- Cargo.lock

# 6c. Was it included in any published release?
for release in $(gh release list --limit 20 --json tagName -q '.[].tagName'); do
  gh release download "$release" --pattern 'sbom.cdx.json' --output "/tmp/sbom-$release.json" 2>/dev/null
  if [ -f "/tmp/sbom-$release.json" ]; then
    if jq -e ".components[] | select(.name == \"$SUSPICIOUS\")" "/tmp/sbom-$release.json" > /dev/null; then
      echo "FOUND in release $release"
    fi
  fi
done
```

---

## Resolução

### If typosquat NOT yet merged to main (typical path — lockfile gate held)

1. Close the introducing PR with explanation:
   ```
   Closing: suspected typosquatting crate '$SUSPICIOUS'. The intended crate is
   '$LEGITIMATE'. Please correct and re-open.
   ```

2. If the typosquat appeared as a transitive dep (pulled in by another dep update):
   ```bash
   # Pin the introducing dependency to an older version that does NOT pull in typosquat
   cargo update -p <introducing_dep> --precise <last_safe_version>
   ```

3. Add cargo-deny `[bans]` entry for the confirmed typosquat:
   ```toml
   [[bans.deny]]
   name = "<suspicious_crate>"
   # Reason: confirmed typosquat of <legitimate_crate>; see incident <DATE>
   ```

4. Submit fix PR; verify all supply chain CI gates pass.

5. Report to crates.io security team: `security@crates.io` with evidence.
   - Reference: https://crates.io/security

6. File RustSec advisory if crates.io team confirms malicious intent:
   - https://github.com/rustsec/advisory-db/blob/main/CONTRIBUTING.md

### If typosquat was merged and/or deployed (escalated path)

1. Rollback already executed (Step 4 above).
2. Rotate credentials as identified in Step 3.
3. Remove crate from `Cargo.toml` + `Cargo.lock`:
   ```bash
   cargo remove $SUSPICIOUS
   cargo update
   ```
4. Add `[bans.deny]` entry (see above).
5. Submit forensic report to crates.io security team within 24h.
6. Customer notification if data exfiltration confirmed (GDPR Art. 33 — 72h window).

---

## Post-incident

- **Post-mortem mandatory** if typosquat was deployed (however briefly).
- Post-mortem template: `docs/postmortems/template.md`.
- RCA: Why did lockfile diff review not catch the crate? Was CODEOWNERS bypass used?
- Update CODEOWNERS policy if gap found.
- Consider adding `cargo-vet` or `cargo-crev` review requirement for new crates.
- Update deny.toml `[bans]` if similar pattern emerges.
- Quarterly: re-execute this runbook dry-run via `scripts/rb_fm_157_dry_run.rs`.

---

## Escalation matrix

| Condition | Severity | Action |
|---|---|---|
| Typosquat detected in PR, not merged | SEV-3 | Security Lead + Engineer; PR blocked; 30 min review |
| Typosquat merged to feature branch, not in main | SEV-2 | Revert branch; notify Security; 4h containment |
| Typosquat in main or staging build | SEV-2 | Emergency PR revert + cargo-deny ban; 2h |
| Typosquat confirmed in production | SEV-1 CRITICAL | Rollback + credential rotation + customer notification |
| Multiple typosquats same campaign | SEV-1 | Engage crates.io security team + RustSec advisory |

---

## Evidence checklist (dry-run output)

- [ ] Lockfile diff PR comment Action emitted warning for new/suspicious crate.
- [ ] CODEOWNERS review gate triggered (requires supply-chain reviewer approval).
- [ ] cargo-deny `[sources]` policy passed (typosquat IS on crates.io — policy does not block).
- [ ] SBOM ingestion DT detected new component; anomaly in DT review queue.
- [ ] PR blocked / quarantined before merge to main.
- [ ] `cargo tree --workspace --invert <suspicious_crate>` executed; output captured.
- [ ] crates.io metadata inspection executed; low download count / recent creation confirmed.
- [ ] Drift findings documented in dry-run report.
- [ ] `[bans.deny]` entry added to `deny.toml` (post-dry-run cleanup: remove test entry).
- [ ] Report filed to `security@crates.io` (simulated in dry-run; actual filing only on real incident).

---

## References

- `specs/03_architecture/failure_modes.md` FM-157.
- `specs/03_architecture/security_model.md` CTRL-SUPPLY-001 + CTRL-SUPPLY-004 + CTRL-SUPPLY-006.
- `specs/04_sprints/_sealed/S12/_spec_contract.md` R-S12-9 + R-S12-10 + R-S12-18.
- `specs/04_sprints/_sealed/S12/work_items/WI-S12-004-cargo-audit-cargo-deny-dependabot.md`.
- `specs/04_sprints/_sealed/S12/work_items/WI-S12-005-dependency-track-self-host-cve-alerts.md`.
- RustSec Advisory Database: <https://rustsec.org/advisories/>.
- cargo-deny documentation: <https://embarkstudios.github.io/cargo-deny/>.
- crates.io security contact: <https://crates.io/security>.
- Real-world reference: `ua-parser-js` 2021 typosquatting incident (npm).
- Real-world reference: `coreutils` vs `coreutils-util` pattern (periodic).

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo Schneiter (via Claude Opus 4.7) | Initial stub. |
| 1.0.0 | 2026-05-14 | Gustavo Schneiter (via Claude Sonnet 4.6) | Full production-grade runbook (WI-S12-007 ship gate §6.2); §6-8 coverage: Detecção + Comunicação + Mitigação imediata (Steps 1–4) + Investigação (Steps 5–6) + Resolução + Post-incident + Escalation matrix + Evidence checklist; dry-run executed 2026-05-14. |
