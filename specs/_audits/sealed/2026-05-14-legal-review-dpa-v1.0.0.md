---
id: "AUDIT-DPA-LEGAL-REVIEW-v1.0.0"
type: "audit"
doc_status: "DRAFT"
audit_status: "PENDING"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["audit", "s19", "dpa", "legal-review", "wi-s19-002", "v1.0.0"]
parent: "WI-S19-002"
---

# Legal review — DPA v1.0.0 (3 locales)

> **Status:** DRAFT. Awaiting Legal Counsel sign-off per locale. This
> template seeds the per-version audit artefact produced by every PR
> touching `legal/dpa/**` (see `specs/_runbooks/RB-DPA-CHANGE.md`).

## 1. Scope

- DPA template `legal/dpa/v1.0.0.en-US.md` — Legal review.
- DPA template `legal/dpa/v1.0.0.pt-BR.md` — Legal review + native-speaker review.
- DPA template `legal/dpa/v1.0.0.es-419.md` — Legal review + native-speaker review.

## 2. Sign-offs

| Locale | Legal Counsel | Native speaker | Status | Date |
|---|---|---|---|---|
| en-US | _TBD_ | n/a (source) | _pending_ | _TBD_ |
| pt-BR | _TBD_ | _TBD_ | _pending_ | _TBD_ |
| es-419 | _TBD_ | _TBD_ | _pending_ | _TBD_ |

## 3. Control mapping

| Control | Evidence |
|---|---|
| GDPR Art. 28 | §1, §5, §6, §11 across all locales |
| LGPD Art. 39 | §1, §6 in pt-BR + cross-references in es-419 |
| CCPA §1798.140(v) | §1, §11 in en-US |
| INV-CONSENT-PROOF-VERIFIABLE | Content-addressable `notice_text_hash` recompute (CTRL-PRIV-CONSENT-001) verified by `cargo test -p corelink-dpa-acceptance` happy path |
| CTRL-PRIV-CONSENT-001..006 | 6-field schema in `crates/corelink-dpa-acceptance/src/schema.rs` |

## 4. Decision

_To be completed by Legal Counsel of record._

## 5. Next steps

- Advance frontmatter `legal_review_status` in each
  `legal/dpa/v1.0.0.<locale>.md` from `draft` to `approved`.
- File `audit_status: ACTIVE` and clear `doc_status: DRAFT` here.
