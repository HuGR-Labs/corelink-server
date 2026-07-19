---
id: "AUDIT-2026-05-27-W32-PHASEG2-DNS-VERIFY-PREP-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-32", "phase-g", "dns", "tls", "verify", "harness", "seal"]
references:
  - "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md"
  - "specs/_audits/2026-05-27-wave32-di-dispatch-matrix.md"
  - "scripts/g-day-dns-verify-prod.sh"
  - "scripts/dns-prod-verify.sh"
---

# Wave 32 Phase G.2 — DNS + HTTPS + Cert-Chain Verify Harness PREP SEAL

**Date:** 2026-05-27  
**Agent worktree:** `agent-a5a3832b31ec01d71`  
**Mandate:** `specs/_audits/2026-05-27-wave32-di-dispatch-matrix.md` WP-G.2  
**Phase G spec:** `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md` §Phase G

---

## 1. Deliverable Inventory

| File | LOC | Status |
|------|-----|--------|
| `scripts/g-day-dns-verify-prod.sh` | ~280 | NEW — shellcheck clean |
| `specs/_audits/2026-05-27-w32-phaseG2-dns-verify-prep-seal.md` | this doc | NEW |

---

## 2. Scope confirmation

**IN-SCOPE (per WP-G.2 contract):**
- `scripts/g-day-dns-verify-prod.sh` — 7-host × 3-check (21 total) verification harness
- This audit document

**OUT-OF-SCOPE (confirmed not touched):**
- `wrangler.toml` — owned by WP-G.1 only
- DNS record creation/mutation — Owner G-day only
- `scripts/dns-prod-apply.sh` / `scripts/g-day-dns-apply-prod.sh` — WP-G.1 territory

---

## 3. Host × Check Matrix (21 assertions)

| # | Host | Type | DNS check | HTTPS check | Cert check |
|---|------|------|-----------|-------------|------------|
| 1 | `corelink-api.humangr.com` | Worker | CF IP (104.x/172.6x.x) | HTTP 2xx/3xx | Cloudflare issuer |
| 2 | `corelink-app.humangr.com` | Pages | CF IP | HTTP 2xx/3xx | Cloudflare issuer |
| 3 | `corelink-docs.humangr.com` | Pages | CF IP | HTTP 2xx/3xx | Cloudflare issuer |
| 4 | `corelink-signup.humangr.com` | Worker | CF IP | HTTP 2xx/3xx | Cloudflare issuer |
| 5 | `humangr.com` | Worker | CF IP | HTTP 2xx/3xx | Cloudflare issuer |
| 6 | `corelink-get.humangr.com` | Worker | CF IP | HTTP 2xx/3xx | Cloudflare issuer |
| 7 | `status.corelink.humangr.com` | StatusPage | CNAME→hugrl.betteruptime.com | HTTP 2xx/3xx | Let's Encrypt issuer |

**Total: 7 hosts × 3 checks = 21 assertions.**

---

## 4. Modes

| Flag | Behaviour | Use case |
|------|-----------|----------|
| _(default)_ | All 3 checks per host (21 total) | G-day post-apply full verify |
| `--mode=quick` | DNS dig only (7 checks) | Fast pre-flight before TLS up |
| `--mode=tls` | Cert chain only (7 checks) | Debugging cert issues |
| `--dry-run` | Print plan, no network calls | Pre-G-day review, agent env |

---

## 5. Expected Output Per Host (post-apply G-day)

### 5.1 `corelink-api.humangr.com` (Worker, proxied)

```
▸ corelink-api.humangr.com (worker)
[PASS]  dns:corelink-api.humangr.com: CF IP confirmed (104.21.x.x) [proxied=true]
[PASS]  https:corelink-api.humangr.com: HTTP 200
[PASS]  cert:corelink-api.humangr.com: issuer contains 'Cloudflare' — CF Universal SSL confirmed
        issuer= /C=US/O=Cloudflare, Inc./CN=Cloudflare Inc ECC CA-3
```

**Sample dig output (proxied record returns CF IPs directly):**
```
$ dig +short corelink-api.humangr.com
104.21.43.78
172.67.201.42
```

**Sample curl output:**
```
$ curl -sI --max-time 10 https://corelink-api.humangr.com/
HTTP/2 200
content-type: application/json
cf-ray: 8abc123def456789-GRU
```

**Sample openssl output:**
```
$ openssl s_client -connect corelink-api.humangr.com:443 -servername corelink-api.humangr.com \
    </dev/null 2>/dev/null | openssl x509 -noout -issuer
issuer=C = US, O = "Cloudflare, Inc.", CN = Cloudflare Inc ECC CA-3
```

---

### 5.2 `corelink-app.humangr.com` (Pages, proxied)

```
▸ corelink-app.humangr.com (pages)
[PASS]  dns:corelink-app.humangr.com: CF IP confirmed (104.21.x.x) [proxied=true]
[PASS]  https:corelink-app.humangr.com: HTTP 200
[PASS]  cert:corelink-app.humangr.com: issuer contains 'Cloudflare' — CF Universal SSL confirmed
```

**Note:** Pages projects served via CF edge. Universal SSL Free covers `*.humangr.com` (2-level).
`corelink-app.humangr.com` is 2-level deep under `humangr.com` — covered.

---

### 5.3 `corelink-docs.humangr.com` (Pages, proxied)

```
▸ corelink-docs.humangr.com (pages)
[PASS]  dns:corelink-docs.humangr.com: CF IP confirmed (104.21.x.x) [proxied=true]
[PASS]  https:corelink-docs.humangr.com: HTTP 200
[PASS]  cert:corelink-docs.humangr.com: issuer contains 'Cloudflare' — CF Universal SSL confirmed
```

---

### 5.4 `corelink-signup.humangr.com` (Worker, proxied)

```
▸ corelink-signup.humangr.com (worker)
[PASS]  dns:corelink-signup.humangr.com: CF IP confirmed (104.21.x.x) [proxied=true]
[PASS]  https:corelink-signup.humangr.com: HTTP 200
[PASS]  cert:corelink-signup.humangr.com: issuer contains 'Cloudflare' — CF Universal SSL confirmed
```

---

### 5.5 `humangr.com` (Worker, proxied)

```
▸ humangr.com (worker)
[PASS]  dns:humangr.com: CF IP confirmed (104.21.x.x) [proxied=true]
[PASS]  https:humangr.com: HTTP 200
[PASS]  cert:humangr.com: issuer contains 'Cloudflare' — CF Universal SSL confirmed
```

---

### 5.6 `corelink-get.humangr.com` (Worker, proxied)

```
▸ corelink-get.humangr.com (worker)
[PASS]  dns:corelink-get.humangr.com: CF IP confirmed (104.21.x.x) [proxied=true]
[PASS]  https:corelink-get.humangr.com: HTTP 200
[PASS]  cert:corelink-get.humangr.com: issuer contains 'Cloudflare' — CF Universal SSL confirmed
```

---

### 5.7 `status.corelink.humangr.com` (StatusPage — DNS-only, BetterStack)

```
▸ status.corelink.humangr.com (statuspage)
[PASS]  dns:status.corelink.humangr.com: CNAME → hugrl.betteruptime.com [BetterStack DNS-only, expected]
[PASS]  https:status.corelink.humangr.com: HTTP 200
[PASS]  cert:status.corelink.humangr.com: issuer contains 'Let's Encrypt' — valid for status page
```

**Important:** `status.corelink.humangr.com` is a 3-level subdomain under `humangr.com`.
CF Universal SSL Free does NOT cover 3-level subdomains (`*.corelink.humangr.com` is out of scope).
This is by design: it routes DNS-only (no CF proxy orange-cloud) to BetterStack, which provisions
its own Let's Encrypt cert for the status page. Phase A SEAL confirms this was applied.

**Sample dig output:**
```
$ dig +short status.corelink.humangr.com CNAME
hugrl.betteruptime.com.
```

**Sample curl output:**
```
$ curl -sI --max-time 10 https://status.corelink.humangr.com/
HTTP/2 200
content-type: text/html; charset=utf-8
server: betteruptime
```

**Sample openssl output:**
```
$ openssl s_client -connect status.corelink.humangr.com:443 \
    -servername status.corelink.humangr.com </dev/null 2>/dev/null \
    | openssl x509 -noout -issuer
issuer=C = US, O = Let's Encrypt, CN = R3
```

---

## 6. CF IP Range Reference

The script validates IP against these Cloudflare ranges (prefix-based):

| Range | Regex prefix checked |
|-------|---------------------|
| `104.16.0.0/12` | `^104\.(1[6-9]\|[2-9][0-9]\|1[0-2][0-9]\|...)\.` |
| `172.64.0.0/13` | `^172\.(6[4-9]\|7[0-9])\.` |
| `188.114.96.0/20` | `^188\.114\.(9[6-9]\|1[0-1][0-9])\.` |
| `197.234.240.0/22` | `^197\.234\.(24[0-3])\.` |
| `198.41.128.0/17` | `^198\.41\.(12[8-9]\|1[3-9][0-9]\|...)\.` |
| `162.158.0.0/15` | `^162\.(15[89])\.` |

Source: https://www.cloudflare.com/ips-v4/

---

## 7. TLS Certificate Coverage Notes

| Host | Level under `humangr.com` | Universal SSL Free covers? | Cert path |
|------|--------------------------|---------------------------|-----------|
| `corelink-api.humangr.com` | 2nd level | YES (`*.humangr.com`) | CF Universal SSL |
| `corelink-app.humangr.com` | 2nd level | YES | CF Universal SSL |
| `corelink-docs.humangr.com` | 2nd level | YES | CF Universal SSL |
| `corelink-signup.humangr.com` | 2nd level | YES | CF Universal SSL |
| `humangr.com` | 2nd level | YES | CF Universal SSL |
| `corelink-get.humangr.com` | 2nd level | YES | CF Universal SSL |
| `status.corelink.humangr.com` | 3rd level | NO — DNS-only | BetterStack (Let's Encrypt) |

**Wave 32 Phase H lesson applied:** 3-level wildcard (`*.corelink.humangr.com`) is NOT covered by
Universal SSL Free. The `status` subdomain is routed DNS-only to BetterStack which manages its own cert.
All other 6 hosts are 2-level (`*.humangr.com`) and are covered.

---

## 8. Dry-Run Acceptance Gate Output

```
Wave 32 Phase G.2 — DNS + HTTPS + Cert Verify Harness (DRY-RUN)
────────────────────────────────────────────────────────────
Mode      : full
Dry-run   : true
Timestamp : 2026-05-27T...

Would verify 7 hosts × 3 checks = 21 assertions:

  HOST                                        TYPE          CHECKS
  ----                                        ----          ------
  corelink-api.humangr.com                    worker        dns+https+cert
  corelink-app.humangr.com                    pages         dns+https+cert
  corelink-docs.humangr.com                   pages         dns+https+cert
  corelink-signup.humangr.com                 worker        dns+https+cert
  humangr.com                  worker        dns+https+cert
  corelink-get.humangr.com                    worker        dns+https+cert
  status.corelink.humangr.com                 statuspage    dns+https+cert

Check details:
  dns  : dig +short +time=5 +tries=2 <host>
         Workers/Pages -> CF IP (104.x / 172.6[4-9].x / 172.7[0-9].x)
         status page   -> CNAME to hugrl.betteruptime.com
  https: curl -sI --max-time 10 https://<host>/ -> 2xx or 3xx
  cert : openssl s_client -connect <host>:443 -servername <host>
         issuer must contain 'Cloudflare' or 'Let's Encrypt'

Exit code: 0 if 21/21 pass; 1 if any fail.
Run without --dry-run on G-day after DNS records are applied.
```

---

## 9. DoD Checklist

| # | Criterion | Status |
|---|-----------|--------|
| 1 | Script tests all 7 hosts with 3 checks each (21 total) | PASS |
| 2 | `--mode=quick` runs only dig (fast) | PASS |
| 3 | `--mode=tls` runs only cert check (debugging) | PASS |
| 4 | Output: per-host table with PASS/FAIL per check + overall verdict | PASS |
| 5 | Exit code 0 if all 21 pass; exit 1 with failing hosts at top | PASS |
| 6 | `shellcheck` clean | PASS |
| 7 | Audit doc tabulates expected output per host | PASS (§5 above) |
| 8 | Single commit | PENDING Owner merge |

---

## 10. Owner G-Day Instructions

**Pre-requisite:** WP-G.1 has applied all 6 DNS CNAME records + updated `wrangler.toml`.

**Step 1 — Quick DNS-only verify (fast, no TLS):**
```bash
bash scripts/g-day-dns-verify-prod.sh --mode=quick
```
Expected: 7/7 PASS. If any FAIL: DNS propagation still in progress (wait 5 min, retry).

**Step 2 — Full verify (DNS + HTTPS + cert):**
```bash
bash scripts/g-day-dns-verify-prod.sh
```
Expected: 21/21 PASS, exit code 0.

**Step 3 — If cert fails on a specific host:**
```bash
bash scripts/g-day-dns-verify-prod.sh --mode=tls
```
Isolates cert issues. Check CF dashboard Universal SSL status if `status` host fails.

**Step 4 — If any host fails after 30 min:**
- For Worker/Pages hosts: verify CF proxied orange-cloud is enabled (not gray-cloud)
- For `status`: verify CNAME record is DNS-only (not proxied)
- Check CF Universal SSL coverage: Dashboard → SSL/TLS → Edge Certificates

---

## 11. Cross-Reference

- **WP-G.1** (parallel agent): Authors `scripts/g-day-dns-apply-prod.sh` + wrangler.toml route additions.
  This script (WP-G.2) consumes the state WP-G.1 creates. Zero file overlap.
- **WP-7.1** (BetterStack probes): Cross-references `status.corelink.humangr.com` as probe target.
  This script verifies DNS + HTTPS reachability of that endpoint post-G.1.
- **WP-H.1** (Wave-32 smoke extension): Will call or cross-reference this harness for the
  "7 subdomains resolve + serve HTTPS" smoke check.

---

## SEAL

```
result: WP-G.2 DNS verify harness SEALED — 21-assertion script shellcheck-clean, dry-run gates pass.
DoD: 1/PASS 2/PASS 3/PASS 4/PASS 5/PASS 6/PASS 7/PASS 8/PENDING-COMMIT
hosts × checks: 7×3 = 21
commit SHA: pending
blockers: NONE — script is G-day-ready; DNS records do not exist yet (WP-G.1 owns apply)
```
