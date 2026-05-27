# AWS Artifact PDF Placeholder — DEBT-003 Slot & Verification Harness

> **Doc kind:** wave-27 R-prep DEBT-003 engineering-side placeholder slot doc
> (`_compliance/` is in `validate_specs.py::SKIP_ALL` — no canonical front
> matter required).
>
> **Author:** wave-27 R-prep DEBT-003 placeholder agent (Claude Opus 4.7) on
> branch `wt/r-prep-debt-003-aws-artifact-placeholder`.
>
> **Base:** `main` @ `a48bbec` ("merge wt/r-prep-cf-worker-prefetch-wire into
> main (wave-26)" — wave-26 SEAL tip).
>
> **Scope:** Authoritative placeholder slot for the AWS Artifact PDF that
> closes DEBT-003. Engineering side (placeholder file + verifier + SHA256SUMS
> ledger) is landed by this doc; the actual PDF fetch + SHA-256 record is
> Owner-bound per DEBT-003 registry row.
>
> **Cross-ref:**
> - `specs/_audits/sealed/2026-05-15-debt-register.md` (DEBT-003 row)
> - `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` (§2 AWS KMS row —
>   `sha256:TBD-on-receipt`)
> - `scripts/verify-aws-artifact-pdf.sh` (the verifier this doc binds)
> - `specs/_compliance/aws-artifact-pdfs/SHA256SUMS` (the append-only ledger)
> - `specs/_compliance/fips-attestation-letters/LETTER-AWS-KMS.md` (vendor
>   letter complement; this doc tracks the AWS Artifact PDF specifically)

---

## 1. Required artifact

DEBT-003 ("AWS attestation doc hash placeholder") requires the operator to
download the AWS-issued attestation PDF that backs the `BYOK-FIPS-ATTESTATION-MATRIX.md`
§2 AWS KMS row. The current matrix cell records the placeholder
`sha256:TBD-on-receipt`; that placeholder must be replaced with a real
SHA-256 digest of the downloaded PDF.

Acceptable artifacts (any one closes DEBT-003 for the **FIPS attestation**
half of the slot; both are recommended for the full SOC 2 + FIPS dual
coverage):

| # | Artifact | AWS Artifact section | Maps to matrix evidence |
|---|---|---|---|
| 1 | **SOC 2 Type II report** for AWS KMS | "Reports → SOC → SOC 2" | SOC 2 CC6.1 / C1.1 fieldwork support |
| 2 | **FIPS 140-3 Validation Report for KMS** (CMVP `#4523`) | "Reports → Certifications → FIPS" | BYOK-FIPS matrix §2 AWS KMS row |

The "FIPS 140-3 Validation Report" PDF is the primary GAP-02 deliverable;
the SOC 2 Type II report is a secondary doc that supports the
`SOC2-EVIDENCE-ROLLUP-2026-05-15.md` C1.1 control. Both are version-pinned
on receipt (AWS revises them quarterly).

**Out of scope** for this placeholder slot (covered elsewhere):

- Vendor signed letter on AWS Inc letterhead — tracked separately in
  `fips-attestation-letters/LETTER-AWS-KMS.md`. The AWS Artifact PDF
  is the *first-party download*; the letter is a *vendor-counter-signed*
  artifact. SOC 2 evidence chain requires both (matrix §1 status legend).
- AWS Artifact for non-KMS services (S3, EC2, etc) — out of CoreLink BYOK
  scope; tracked at the org level if/when needed.

---

## 2. Where to fetch

1. **Console:** <https://console.aws.amazon.com/artifact>
2. **AWS CLI** (alternative, scriptable, recommended for renewal cadence):
   ```bash
   aws artifact get-report \
     --report-id <FIPS-KMS-report-id> \
     --output-format application/pdf \
     > AWS-Artifact-FIPS-140-3-KMS-2026-Q2.pdf
   ```
   (Report IDs change quarterly; query
   `aws artifact list-reports --type-filter "Report"` for the active set.)
3. **Required AWS IAM permissions** on the principal performing the fetch:
   `artifact:Get`, `artifact:DownloadAgreement`, `artifact:ListReports`.
   (Standard `AWSArtifactAccountSync` role grants these.)
4. **Acceptance of NDA-style terms-of-use** is mandatory before download.
   Click-through acceptance binds the downloading principal; record the
   accepting-principal IAM ARN in the SHA256SUMS ledger header (§4).

> **Renewal cadence note:** AWS Artifact PDFs are reissued by AWS roughly
> quarterly. Each new quarter's report supersedes the previous. The
> SHA256SUMS ledger (§4) tracks the full history; the BYOK matrix tracks
> only the *current-active* hash.

---

## 3. SHA-256 verification target

The operator records the SHA-256 of the downloaded PDF and pins it in the
BYOK matrix row and in the SHA256SUMS ledger.

### 3.1 Compute the hash

```bash
# macOS / BSD:
shasum -a 256 AWS-Artifact-FIPS-140-3-KMS-2026-Q2.pdf

# Linux / GNU coreutils:
sha256sum AWS-Artifact-FIPS-140-3-KMS-2026-Q2.pdf
```

Both produce the same canonical 64-hex-character output. Record the
**lowercase hex** form (no `sha256:` prefix in the SHA256SUMS file —
that prefix is matrix-side display only).

### 3.2 Update the canonical matrix row

In `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` §2 AWS KMS row,
replace the `sha256:TBD-on-receipt` token with the real value:

```diff
- `fips-attestation-letters/AWS-KMS-attestation-2026-Q1.pdf` (hash `sha256:TBD-on-receipt`)
+ `fips-attestation-letters/AWS-KMS-attestation-2026-Q1.pdf` (hash `sha256:<64-hex>`)
```

Also bump the matrix `updated:` front-matter field to the current date.

### 3.3 Append the row to SHA256SUMS

Append a new line to `specs/_compliance/aws-artifact-pdfs/SHA256SUMS`
(format documented in §4 below). Do **not** rewrite or reorder prior
lines — the file is append-only, audit-evidence material.

---

## 4. Where to store

### 4.1 PDF binary itself

PDF binaries live **off-repo** in the operator's secure long-term archive.
The repo does **not** track the raw PDF — the `.gitignore` at
`specs/_compliance/aws-artifact-pdfs/.gitignore` enforces this. Rationale:

1. **Size:** AWS Artifact FIPS reports are typically 2–8 MB; the SOC 2
   Type II is often 1–3 MB. Multiplied by quarterly renewal × 5 years of
   GA history that becomes ~150 MB of binary blob in the repo.
2. **License:** AWS Artifact terms-of-use prohibit redistribution of the
   PDF. Storing it in a git repo (even a private one) that has multiple
   contributors with read access *may* count as redistribution per AWS
   counsel reading. Storing only the hash sidesteps this entirely.
3. **Audit utility:** the SHA-256 *is* the evidence-chain anchor. An
   auditor performing fieldwork holds their own copy of the PDF and
   verifies the hash matches the one CoreLink committed at receipt time
   — that's a stronger evidence chain than "trust the PDF in our repo".

Recommended off-repo storage location (operator's discretion):

- HuGR Labs 1Password vault → "Compliance / AWS Artifact" folder.
- Encrypted S3 bucket (`s3://hugr-compliance-artifacts/`) with
  object-lock retention ≥ 7 years.
- Encrypted backup of the above on an offline drive in the company safe.

### 4.2 SHA256SUMS ledger (this repo)

`specs/_compliance/aws-artifact-pdfs/SHA256SUMS` is an **append-only**
ledger of every AWS Artifact PDF ever fetched. Lines follow the GNU
coreutils `sha256sum` output format with a leading hash comment block:

```
# AWS Artifact PDF SHA-256 ledger — append-only.
# DEBT-003 evidence anchor; cross-ref:
#   specs/_compliance/aws-artifact-placeholder.md
#   specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md
#
# Line format (after this header):
#   <64-hex sha256>  <filename>  # fetched=YYYY-MM-DD principal=<iam-arn> kind=<fips|soc2|other>
#
# Lines are append-only. Never edit or reorder. Audit chain depends on
# monotonic ordering of receipts.

<sha256-64hex>  AWS-Artifact-FIPS-140-3-KMS-2026-Q2.pdf  # fetched=2026-06-14 principal=arn:aws:iam::<acct>:user/<name> kind=fips
```

The verifier (§5) is the authoritative parser; it tolerates only the
above format. Hand-edits that break the format will fail the gate.

### 4.3 Directory layout

```
specs/_compliance/aws-artifact-pdfs/
├── .gitignore       # ignores *.pdf and anything not SHA256SUMS / this dir
└── SHA256SUMS       # append-only ledger
```

PDF files placed *into* this directory locally for verification will be
ignored by git (per `.gitignore`). The verifier (§5) supports passing the
PDF path via `--pdf <path>` so the operator can run verify on a local
copy without committing it.

---

## 5. Verification harness

`scripts/verify-aws-artifact-pdf.sh` is the canonical verifier. It runs in
three modes:

| Mode | Invocation | Purpose | Exit 0 condition |
|---|---|---|---|
| `--check-empty` | `bash scripts/verify-aws-artifact-pdf.sh --check-empty` | CI / quality-gate use when no PDF has been received yet. Confirms SHA256SUMS is in its initial header-only state. | SHA256SUMS exists and contains zero non-comment, non-blank lines. |
| `--verify-ledger` | `bash scripts/verify-aws-artifact-pdf.sh --verify-ledger` | Parse-only check that SHA256SUMS lines are well-formed (64-hex digest + filename + `# fetched=... principal=... kind=...` trailer). | Every non-comment line conforms. |
| `--pdf <path>` | `bash scripts/verify-aws-artifact-pdf.sh --pdf /path/to/local.pdf` | Operator-side verification on a freshly fetched PDF: computes SHA-256, looks it up in SHA256SUMS, prints `OK` if a matching line exists. | The PDF's SHA-256 is present in SHA256SUMS. |

The verifier intentionally does **not** mutate SHA256SUMS — appending
rows is a manual `git`-tracked operation by the operator after the
fetch+hash flow above. This keeps the audit chain explicit: every
append is its own git commit.

### 5.1 Wiring

- This wave (wave-27, engineering-side): verifier landed; `--check-empty`
  is green; no production CI wiring yet (wired by the operator on first
  receipt — at that point the verifier graduates to a deploy-gate role
  alongside `secrets-checklist-verify.sh` and `verify-fips-endpoints.py`).
- Post-receipt: add `bash scripts/verify-aws-artifact-pdf.sh --verify-ledger`
  to the GA cutover preflight (`scripts/ga-cutover-prod-dressrun.sh`) so a
  malformed SHA256SUMS line blocks production deploys.

---

## 6. DEBT-003 closure protocol

The full DEBT-003 closure flow, end-to-end:

1. Owner authenticates to AWS Artifact (console or CLI per §2).
2. Owner downloads the FIPS 140-3 KMS Validation Report PDF.
3. Owner runs `shasum -a 256 <pdf>` to compute SHA-256.
4. Owner appends one line to `specs/_compliance/aws-artifact-pdfs/SHA256SUMS`
   following the §4.2 format (incl. `fetched=`, `principal=`, `kind=`
   trailers).
5. Owner updates `BYOK-FIPS-ATTESTATION-MATRIX.md` §2 AWS KMS row's
   `sha256:TBD-on-receipt` token with the real 64-hex value (per §3.2).
6. Owner runs `bash scripts/verify-aws-artifact-pdf.sh --verify-ledger`
   locally to confirm SHA256SUMS lines parse.
7. Owner runs `bash scripts/verify-aws-artifact-pdf.sh --pdf <local.pdf>`
   to confirm the local PDF matches the appended SHA256SUMS row.
8. Owner commits both files in a single commit with message
   `chore(debt-003): record AWS Artifact FIPS PDF SHA-256 (<short-date>)`.
9. Owner flips the DEBT-003 row in `specs/_audits/sealed/2026-05-15-debt-register.md`
   from `(engineering-CLOSED; operator-bound)` to `(CLOSED <date>, commit
   <sha>)` and notes the commit hash.

Optional but recommended step 10: repeat steps 2–8 for the SOC 2 Type II
report PDF — same flow, same SHA256SUMS file (use `kind=soc2` trailer).

---

## 7. Quality gates

This placeholder slot ships with the following gates green at wave-27
land time:

- `bash scripts/verify-aws-artifact-pdf.sh --check-empty` exits 0
  (no PDFs received yet — SHA256SUMS contains only header comments).
- `python3 scripts/validate_specs.py` green (this doc lives under
  `_compliance/`, which is in `SKIP_ALL` — no front-matter required).
- `python3 scripts/validate_references.py` green (no new spec
  cross-references introduced by this doc).

Post-receipt gates (operator-bound, fired by the §6 closure protocol):

- `bash scripts/verify-aws-artifact-pdf.sh --verify-ledger` exits 0.
- `bash scripts/verify-aws-artifact-pdf.sh --pdf <local.pdf>` exits 0.
- BYOK matrix `sha256:TBD-on-receipt` token replaced (manual grep:
  `grep -n "TBD-on-receipt" specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`
  should return 0 lines after closure).

---

## 8. Freeze allowance

This deliverable lands under the GA-1 feature freeze (active per
`specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md`). All paths touched:

- `specs/_compliance/aws-artifact-placeholder.md` — §3.d implicit-allow
  (under `specs/_compliance/`).
- `specs/_compliance/aws-artifact-pdfs/.gitignore` — §3.d implicit-allow.
- `specs/_compliance/aws-artifact-pdfs/SHA256SUMS` — §3.d implicit-allow.
- `scripts/verify-aws-artifact-pdf.sh` — not on the frozen-surface list
  (new shell script; `scripts/` is not enumerated in §2 of the freeze).
- `specs/_audits/sealed/2026-05-16-debt-003-aws-artifact-placeholder.md` —
  §3.d implicit-allow.
- `specs/_audits/sealed/2026-05-15-debt-register.md` — §3.d implicit-allow.

The change set is GA-blocker preparation per DEBT-003's P0 status
(target 2026-06-14) and is freeze-compliant by virtue of the implicit-allow
subtrees. No `FREEZE-EXCEPTION:` token is required.

---

## 9. Engineering-side completion checklist

Wave-27 R-prep deliverable acceptance:

- [x] `specs/_compliance/aws-artifact-placeholder.md` — this file.
- [x] `scripts/verify-aws-artifact-pdf.sh` — verifier.
- [x] `specs/_compliance/aws-artifact-pdfs/SHA256SUMS` — empty initial
      ledger (header-only).
- [x] `specs/_compliance/aws-artifact-pdfs/.gitignore` — ignores PDF
      binaries.
- [x] `specs/_audits/sealed/2026-05-16-debt-003-aws-artifact-placeholder.md` —
      land audit.
- [x] DEBT-003 row uplift in `specs/_audits/sealed/2026-05-15-debt-register.md`
      to `engineering-CLOSED; operator-bound PDF fetch pending`.

Operator-side acceptance criteria for full DEBT-003 closure:
see §6 closure protocol above.
