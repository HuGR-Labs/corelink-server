---
id: "RB-FM-160"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p2", "auth", "credential-attack", "stub"]
---

# RB-FM-160 — Auth Invalid Storm (Credential Attack ou PAT Mass Revocation Replay)

> **FM:** FM-160 (S=4, O=3, D=2, RPN=24, P2) | **CTRL:** CTRL-AUTH-001..010, CTRL-CRED-004 | **INV:** **INV-AUTH-PAT-HMAC-SIG-VERIFIED** | **SLA:** detect ≤ 5 min, mitigate ≤ 30 min

> **INV-AUTH-PAT-HMAC-SIG-VERIFIED**: PAT verify path step 3a (HMAC sig check fast-fail) MUST execute before token_id lookup — rejects 401 with NO DB hit + NO Argon2 cost if sig mismatch. During a credential-stuffing storm, this invariant is the primary DDoS defense: forged or malformed PATs are rejected in ≤ 100µs at the HMAC layer, preventing DB amplification. If this invariant is observed failing (DB queries spiking without Argon2 cost correlation), suspect HMAC bypass or middleware layer reordering (INV-AUTH-5-LAYER-ORDERING violation).

## Detecção

- Alert `corelink_auth_401_total` spike > 100× baseline em 5 min.
- `corelink_auth_pat_revoked_use_total` spike (revoked PATs sendo usados).
- Per-IP rate limit spike (S-08 layer 3).
- SLO breach SLO-AVAIL-CAS-PUT/GET por > 1 min.

## Comunicação

- **SEV-2** se isolated to specific tenants/IPs.
- **SEV-1** se affecting > 1 tenant ou global SLO.
- Page SRE on-call + Security lead.
- Status page if customer-visible impact.

## Mitigação imediata

1. **Identify attack profile**: source IPs, target tenants, PAT IDs sendo testados.
2. **Pattern detection**:
   - Credential stuffing? (many PATs, low success rate)
   - Targeted PAT brute force? (single PAT, high frequency)
   - Revocation replay? (recently revoked PAT being retried in batch)
3. **Mitigation per pattern**:
   - Credential stuffing: WAF rule per-IP rate limit tightened; CAPTCHA challenge.
   - Brute force PAT: per-PAT rate limit lowered; PAT specific revoked.
   - Revocation replay: confirmar revocation propagation funcional (CTRL-CRED-004); KV cache invalidation OK.
4. **Customer notification** se PAT-specific attack on specific tenant.

## Diagnóstico

1. Query auth events R2 audit bucket últimos 30 min.
2. Cross-reference com Cloudflare WAF + Threat Intelligence.
3. Determine if breach attempt successful (any 200 OK pós-storm peak).

## Resolução

- Hot fix: WAF rule + per-IP/per-PAT rate limit tightening.
- Cold fix:
  - Revocation propagation review (target ≤ 60s p99).
  - PAT entropy review (impossível brute force se 32 bytes random).
  - Credential rotation policy reinforcement.

## Post-incident

- Post-mortem se SEV-2+.
- Customer outreach se tenant-specific.
- Threat intel feed update.

## References

- `failure_modes.md` FM-160.
- `specs/04_sprints/S03/_spec_contract.md` (auth path).
- `specs/04_sprints/S08/_spec_contract.md` (rate limit per-PAT/per-IP).
- OWASP ASVS V2/V3.
