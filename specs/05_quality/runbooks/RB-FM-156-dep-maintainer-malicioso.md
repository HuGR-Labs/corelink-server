---
id: "RB-FM-156"
type: "runbook"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "supply-chain", "dependency", "maintainer-compromise", "solarwinds", "rustsec", "cargo-audit", "dependency-track", "rb-fm-156", "wi-s12-007", "dry-run-executed"]
dry_run_executed: "2026-05-13"
dry_run_evidence: "specs/_audits/sealed/2026-05-13-rb-fm-156-dry-run.md"
---

# RB-FM-156 — Dep Maintainer Malicioso (SolarWinds-style Supply Chain Compromise)

> **FM:** FM-156 (S=5, O=1, D=5, RPN=25, P1 S=5→upgrade) | **CTRLs:** CTRL-SUPPLY-001 (SLSA L3) + CTRL-SUPPLY-004 (cargo-audit/deny) + CTRL-SUPPLY-006 (Dependency-Track) | **SLA:** detect ≤ 24h (cargo-audit advisory lag); alert ≤ 15 min (DT webhook p99); block auto-merge ≤ 5 min (CI gate)

## Scenario

A legitimate crate dependency (e.g., `serde`, `tokio`, `ring`) has its crates.io
maintainer credentials stolen or the maintainer account is compromised. The attacker
publishes a malicious new version that passes basic checks but contains a backdoor,
exfiltration payload, or sabotage mechanism. Dependabot opens an auto-merge PR.
CoreLink CI gates must detect, block, and escalate before any merge.

**Threat actor:** TA-5 (supply chain insider / account compromise).
**Blast radius:** all builds consuming the affected crate version — potentially
all tenants if deployed to production.

---

## Detecção

### Primary paths (≤ 24h post-publish)

1. **cargo-audit daily cron** (`corelink-supply-audit.yml`) scans `Cargo.lock`:
   - RUSTSEC advisory published within 24h of malicious crate version.
   - `cargo audit --deny warnings` exits non-zero; alert `corelink_supply_rustsec_advisory_total{severity="high|critical"}` fires.
   - Slack `#supply-chain-cve-alerts` receives webhook notification.
   - PagerDuty `corelink-supply` service: SEV-2 incident created.

2. **Dependency-Track CVE webhook** (WI-S12-005):
   - SBOM ingested post-build; DT matches component PURL against NVD/OSV/GitHub Advisory DB.
   - If crate has CVE or is flagged: `corelink_supply_dt_alert_delivered_seconds` p99 ≤ 15 min.
   - DT webhook fires to `#supply-chain-cve-alerts` + Email + PagerDuty `corelink-supply`.

3. **Dependabot PR opened** with version bump to malicious release:
   - Lockfile diff PR comment Action (WI-S12-004) emits `⚠️ Cargo.lock changed — review required`.
   - CI gate `cargo audit --deny warnings` FAILS → PR blocked from auto-merge.
   - Status check `supply-chain / cargo-audit` required by branch protection.

### Secondary paths

4. **crates.io yanked post-discovery**: cargo-deny `yanked = "deny"` policy blocks.
5. **cargo-deny bans advisory**: `deny.toml` advisory block triggers CI fail.
6. **Manual SBOM review**: DT review queue shows new component version anomaly.

### Metrics to watch

| Metric | Alert threshold | Severity |
|---|---|---|
| `corelink_supply_rustsec_advisory_total{severity="critical"}` | > 0 | SEV-1 |
| `corelink_supply_rustsec_advisory_total{severity="high"}` | > 0 | SEV-2 |
| `corelink_supply_dt_alert_delivered_seconds` p99 | > 900s (15 min) | SEV-2 |
| `corelink_supply_cargo_audit_ci_block_total` | > 0 | INFO (expected; CI working) |
| `corelink_supply_dependabot_automerge_blocked_total` | > 0 | INFO |

---

## Comunicação

- **SEV-2** initial (cargo-audit advisory detected; no confirmed exploit in prod yet).
- Escalate to **SEV-1** if: malicious version confirmed deployed to staging or production.
- Page: SRE on-call + Security Lead + Engineer (supply chain lead).
- Slack: `#supply-chain-cve-alerts` (primary) + `#incidents-corelink-sre` (incident bridge).
- External: Customer notification NOT required unless malicious version confirmed deployed
  and data exfiltration confirmed. Prepare draft notification in parallel (T+30 min).
- PagerDuty: `corelink-supply` service key → SEV-2 incident; escalate policy 5 min ack.

---

## Mitigação imediata (≤ 30 min)

### Step 1 — Confirm advisory and scope (T+0..T+10 min)

```bash
# 1a. Identify affected crate and version range from RUSTSEC advisory
cargo audit --json | jq '.vulnerabilities.list[] | {crate:.package.name, version:.package.version, advisory:.advisory.id, severity:.advisory.severity}'

# 1b. Identify whether malicious version is in Cargo.lock (currently pinned)
grep -A2 '<crate_name>' Cargo.lock | head -10

# 1c. Check if Dependabot PR is open
gh pr list --label "dependencies" --state open

# 1d. Check DT alert in review queue
# Via DT UI: Projects → CoreLink → Findings → sort by date desc
# Via API:
curl -s -H "X-Api-Key: $DT_API_KEY" \
  "$DT_BASE_URL/api/v1/finding/project/$DT_PROJECT_UUID?active=true&suppressed=false" \
  | jq '.[] | select(.component.name == "<crate_name>") | {vuln:.vulnerability.vulnId, severity:.vulnerability.severity}'
```

### Step 2 — Block auto-merge (T+0..T+5 min, automated by CI)

- CI gate `cargo audit --deny warnings` in PR workflow **automatically blocks** Dependabot PR.
- Verify branch protection: `supply-chain / cargo-audit` required status check is FAILING.
- If CI somehow passed (race): **manually block PR** via `gh pr review --request-changes`.

```bash
# Emergency manual block if CI passed (should not happen)
gh pr review <PR_NUMBER> --request-changes --body "EMERGENCY: cargo-audit advisory for <crate>/<version>. DO NOT MERGE until Security review complete."
```

### Step 3 — Assess whether malicious version is already deployed (T+5..T+15 min)

```bash
# Check what version is in current Cargo.lock on main/staging
grep -A3 'name = "<crate_name>"' Cargo.lock

# Check deployed Worker version via SLSA attestation in Rekor
rekor-cli search --rekor_server https://rekor.sigstore.dev \
  --artifact <release_bundle_sha256> | head -20

# Check SBOM of last deployed release for crate version
# Via GitHub Release: download sbom.cdx.json and inspect
jq '.components[] | select(.name == "<crate_name>") | {name:.name, version:.version}' sbom.cdx.json
```

### Step 4 — Immediate containment if malicious version deployed (T+15..T+30 min)

If confirmed malicious version is in production:

1. **Escalate to SEV-1** immediately.
2. **Initiate rollback**: `wrangler rollback --env production` to prior known-good version.
3. **Validate rollback**: re-run Cosign verify on rolled-back artifact.
4. **Preserve evidence**: snapshot Worker logs + DT findings + audit chain before rollback.

If confirmed malicious version is NOT in production (blocked by CI):

1. Maintain SEV-2.
2. Proceed to Investigation (§ below).

---

## Investigação

### Step 5 — Characterize the threat (T+15..T+60 min)

```bash
# 5a. Download malicious version and inspect (sandboxed environment ONLY)
# DO NOT run on production or developer workstations without isolation
cargo download <crate_name>@<malicious_version> 2>/dev/null
# Inspect in sandboxed container:
# docker run --rm -v $(pwd):/work rust:latest bash -c "cd /work && cargo tree -p <crate_name>"

# 5b. Diff the malicious version source against last-known-good version
# Via crates.io API:
curl -s "https://crates.io/api/v1/crates/<crate_name>/<malicious_version>/download" -o malicious.crate
curl -s "https://crates.io/api/v1/crates/<crate_name>/<last_good_version>/download" -o good.crate
tar xf malicious.crate && tar xf good.crate
diff -rq <crate_name>-<last_good_version>/ <crate_name>-<malicious_version>/

# 5c. Check if advisory is published on RustSec
curl -s "https://rustsec.org/advisories/<RUSTSEC_ID>.json" | jq '{id:.id, package:.package.name, affected_versions:.package.patched}'

# 5d. Check crates.io maintainer activity log (public)
curl -s "https://crates.io/api/v1/crates/<crate_name>/owner_user" | jq '.[].login'
```

### Step 6 — Determine blast radius

```bash
# 6a. Which corelink crates directly depend on the affected crate?
cargo tree -p corelink-server --invert <crate_name> 2>/dev/null || \
  cargo tree --workspace --invert <crate_name>

# 6b. DT: which projects/components are affected?
curl -s -H "X-Api-Key: $DT_API_KEY" \
  "$DT_BASE_URL/api/v1/component/identity?purl=pkg%3Acargo%2F<crate_name>%40<malicious_version>" \
  | jq '{affected_projects:.projects[].name}'

# 6c. Check if malicious version was in any prior release artifact (via SLSA + SBOM history)
# Inspect GitHub releases: download each sbom.cdx.json and grep for crate version
for release in $(gh release list --limit 30 --json tagName -q '.[].tagName'); do
  gh release download "$release" --pattern 'sbom.cdx.json' --output "/tmp/sbom-$release.json" 2>/dev/null
  if [ -f "/tmp/sbom-$release.json" ]; then
    echo "Release $release:"
    jq -r ".components[] | select(.name == \"<crate_name>\") | \"  version: \" + .version" "/tmp/sbom-$release.json"
  fi
done
```

---

## Resolução

### If malicious version NOT yet merged (typical path)

1. Pin safe version in `Cargo.toml` manually:
   ```toml
   # In Cargo.toml — pin to last-known-good version
   [dependencies]
   <crate_name> = "=<last_safe_version>"
   ```

2. Update `Cargo.lock`:
   ```bash
   cargo update -p <crate_name> --precise <last_safe_version>
   ```

3. Add advisory waiver in `deny.toml` with expiry comment:
   ```toml
   [[advisories.ignore]]
   id = "<RUSTSEC_ID>"
   # Reason: pinned to <last_safe_version>; tracking fix in issue #<N>
   # Expiry: <date+30d>
   ```

4. Submit PR with fix; verify CI passes all supply chain gates.

5. Close Dependabot PR with comment: "Blocked — malicious version <X>. Pinned to <last_safe> pending upstream fix."

### If malicious version confirmed deployed (escalated path)

1. Rollback already executed (Step 4 above).
2. Rotate any secrets accessible by Worker scope (worst-case compromise).
3. Audit chain: review audit logs for anomalous operations during window malicious version was active.
4. Customer notification if data exfiltration confirmed (GDPR Art. 33 — 72h window).
5. File CVE reservation if upstream has not acted within 24h.
6. Coordinate with RustSec maintainers for advisory publication.

---

## Post-incident

- **Post-mortem mandatory** if malicious version was deployed (however briefly).
- Post-mortem template: `docs/postmortems/template.md`.
- RCA: How did malicious version bypass detection window? Alert latency > 24h target?
- Update dry-run cadence documentation if drift discovered.
- Review cargo-audit SLA: RUSTSEC advisory expected within 24h of confirmed malicious publish.
- Quarterly: re-execute this runbook dry-run via `scripts/rb_fm_156_dry_run.rs`.

---

## Escalation matrix

| Condition | Severity | Action |
|---|---|---|
| RUSTSEC advisory published; crate NOT in prod | SEV-2 | SRE + Security Lead; 5 min ack |
| Malicious crate version confirmed in staging | SEV-1 | All hands; rollback; forensic |
| Malicious crate version confirmed in production | SEV-1 CRITICAL | CEO + customers; 72h GDPR clock |
| advisory lag > 48h (cargo-audit delay) | SEV-2 | SRE audit; alternative feed review |

---

## Evidence checklist (dry-run output)

- [ ] cargo-audit advisory detection ≤ 24h confirmed.
- [ ] DT webhook alert delivered ≤ 15 min confirmed.
- [ ] Slack `#supply-chain-cve-alerts` notification received.
- [ ] PagerDuty SEV-2 incident created.
- [ ] Dependabot auto-merge PR blocked via CI fail (`supply-chain / cargo-audit` required check FAILING).
- [ ] On-call ack within 5 min via PagerDuty test channel.
- [ ] Dashboard `DASH-SUPPLY > Supply Chain CVE Alerts` panel visible.
- [ ] All runbook commands executable; output captured in dry-run report.
- [ ] Drift findings documented.

---

## References

- `specs/03_architecture/failure_modes.md` FM-156.
- `specs/03_architecture/security_model.md` CTRL-SUPPLY-001 + CTRL-SUPPLY-004 + CTRL-SUPPLY-006.
- `specs/04_sprints/_sealed/S12/_spec_contract.md` R-S12-9 (cargo-audit CI) + R-S12-11 (Dependabot).
- `specs/04_sprints/_sealed/S12/work_items/WI-S12-004-cargo-audit-cargo-deny-dependabot.md`.
- `specs/04_sprints/_sealed/S12/work_items/WI-S12-005-dependency-track-self-host-cve-alerts.md`.
- RustSec Advisory Database: <https://rustsec.org/advisories/>.
- cargo-audit documentation: <https://github.com/rustsec/rustsec/tree/main/cargo-audit>.
- Dependency-Track API docs: <https://docs.dependencytrack.org/integrations/rest-api/>.
- XZ Utils 2024 supply chain attack (CVE-2024-3094) as reference scenario.

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-13 | Gustavo Schneiter (via Claude Sonnet 4.6) | Initial production-grade runbook for FM-156 (WI-S12-007 ship gate); dry-run executed 2026-05-13. |
