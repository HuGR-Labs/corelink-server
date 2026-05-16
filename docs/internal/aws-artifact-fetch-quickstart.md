# AWS Artifact Fetch — Owner Quickstart

> **Audience:** HuGR Labs Owner (Gustavo) closing DEBT-003 by fetching the
> AWS Artifact FIPS 140-3 KMS Validation Report PDF (and optionally the
> SOC 2 Type II for AWS KMS).
>
> **Goal:** Get from "I am sitting at my laptop" → "the BYOK matrix
> placeholder is flipped and the SHA-256 ledger commit is ready" in five
> steps, of which only the first four involve a browser. The fifth step is
> one shell command.
>
> **Time budget:** ~10 minutes total (browser navigation dominates).
>
> **Cross-references:**
>
> - `specs/_compliance/aws-artifact-placeholder.md` — canonical DEBT-003
>   placeholder slot doc; §6 has the formal closure protocol this
>   quickstart implements.
> - `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` — the matrix row
>   whose `sha256:TBD-on-receipt` placeholder this flow flips.
> - `specs/_compliance/aws-artifact-pdfs/SHA256SUMS` — the append-only
>   ledger this flow writes to.
> - `scripts/admin/record-aws-artifact-pdf.sh` — the one-command recorder
>   this quickstart drives in step 5.
> - `scripts/verify-aws-artifact-pdf.sh` — the verifier the recorder
>   invokes for the post-write gate.

---

## What this collapses

The DEBT-003 closure protocol (`aws-artifact-placeholder.md` §6) is nine
steps. Browser-bound steps 1–4 (auth, navigate, agree, download) remain
manual. The post-download tail — previously five manual steps
(`shasum`, append-ledger, flip-matrix, verify, draft-commit) — is now a
single command in step 5 below.

| Old §6 step | New quickstart step |
|---|---|
| 1. Auth to AWS Artifact | Step 1 |
| 2. Download PDF | Steps 2–4 |
| 3. `shasum -a 256 <pdf>` | **Step 5** (folded) |
| 4. Append SHA256SUMS row | **Step 5** (folded) |
| 5. Flip BYOK matrix placeholder | **Step 5** (folded) |
| 6. Run `--verify-ledger` | **Step 5** (folded) |
| 7. Run `--pdf <path>` | **Step 5** (implicit via verify) |
| 8. Commit | Step 6 — copy-paste prompt printed by step 5 |
| 9. Update DEBT-003 row in debt-register | Manual follow-up |

---

## Step 1 — Authenticate to AWS Artifact

> **Screenshot needed:** AWS Console login screen with the
> `console.aws.amazon.com/artifact` URL bar visible. Save to
> `docs/internal/screenshots/aws-artifact-01-login.png` on first run.

1. Open: <https://console.aws.amazon.com/artifact>
2. Sign in with the HuGR Labs Owner account (root or
   `OwnerAdmin` IAM user — the principal must have
   `artifact:Get` + `artifact:DownloadAgreement` + `artifact:ListReports`
   permissions; `AWSArtifactAccountSync` covers this).
3. After login you land on the Artifact landing page. Note your IAM
   principal ARN displayed top-right — you will pass this to the recorder
   via `--principal` in step 5.

**If MFA prompts:** complete MFA as usual. The session cookie covers the
PDF download.

---

## Step 2 — Locate the report

> **Screenshot needed:** Artifact Reports list with the search box and
> the "FIPS 140-3" filter applied. Save to
> `docs/internal/screenshots/aws-artifact-02-search.png`.

1. From the Artifact dashboard, click **Reports** (left sidebar).
2. In the search box at the top, type:
   ```
   FIPS 140-3 KMS Validation Report
   ```
3. The canonical report title as of 2026-Q2 is:
   **"FIPS 140-3 Validation Report — AWS Key Management Service (KMS)"**
   (AWS revises this quarterly — title prefix is stable; the year-quarter
   suffix shifts).
4. Click the report row to open its detail page.

> **For the optional SOC 2 Type II run:** repeat from step 2 using the
> search term `SOC 2 Type II` and pick the row whose scope explicitly
> covers AWS KMS. Mark `--kind soc2` in step 5.

---

## Step 3 — Accept the agreement and download

> **Screenshot needed:** the click-through agreement modal (do not
> screenshot the agreement text itself — it is under NDA). Capture only
> the "I agree" button area and the "Download report" button. Save to
> `docs/internal/screenshots/aws-artifact-03-agreement.png`.

1. Click **Download report** on the report detail page.
2. AWS Artifact presents a click-through NDA / terms-of-use modal.
3. Read it (yes, really — your principal is bound by it on click).
4. Click **Accept and download**.
5. The PDF downloads via your browser's normal download flow.

**Typical filename produced by AWS:**

```
FIPS-140-3-Validation-Report-AWS-Key-Management-Service-KMS.pdf
```

(Exact filename varies by quarter — AWS may include a `-2026-Q2` suffix
or a UUID suffix. The recorder in step 5 accepts any filename.)

---

## Step 4 — Place the PDF at a canonical path

The recorder accepts any path, but the documented convention is:

```
~/Downloads/aws-artifact-fips-140-3.pdf
```

So either:

- **Option A (move + rename):**
  ```bash
  mv ~/Downloads/FIPS-140-3-Validation-Report-*.pdf \
     ~/Downloads/aws-artifact-fips-140-3.pdf
  ```

- **Option B (pass the original path directly to step 5):** skip the move.
  The recorder will use whatever filename AWS gave you. (The
  `<filename>` recorded in the SHA256SUMS ledger row is the basename of
  the path you pass.)

> **Off-repo archive note:** also copy the PDF to your secure long-term
> archive per `aws-artifact-placeholder.md` §4.1 (1Password vault →
> "Compliance / AWS Artifact" folder, or encrypted S3 with object-lock).
> Do **not** commit the PDF itself — `specs/_compliance/aws-artifact-pdfs/.gitignore`
> blocks `*.pdf` for license-compliance reasons.

---

## Step 5 — Run the recorder (the only shell step)

From the repo root:

```bash
cd /Users/gustavoschneiter/Documents/HuGR/corelink-server

bash scripts/admin/record-aws-artifact-pdf.sh \
  ~/Downloads/aws-artifact-fips-140-3.pdf \
  "FIPS 140-3 KMS Validation Report" \
  --principal "arn:aws:iam::<your-account-id>:user/<your-iam-username>"
```

Substitute your real account ID and IAM username from step 1.

**What the script does (announced as it goes):**

1. Computes SHA-256 of the PDF.
2. Computes file size.
3. Checks the ledger for the SHA — if already present, exits 0
   idempotently (safe to re-run).
4. Appends a two-line metadata block + one canonical receipt row to
   `specs/_compliance/aws-artifact-pdfs/SHA256SUMS`.
5. Scans `specs/_compliance/` for a matrix-class doc containing
   `sha256:TBD-on-receipt`, and flips the **first** occurrence to
   `sha256:<your-hex>`. (If no match, the script logs the skip — this is
   the renewal case where the matrix already has a real digest.)
6. Runs `scripts/verify-aws-artifact-pdf.sh --verify-ledger` and exits
   non-zero if the append produced a malformed line.
7. Prints a copy-pasteable `git add` + `git commit -s` command block.

**For the SOC 2 Type II follow-up run:** add `--kind soc2`.
Note: SOC 2 PDFs do not have a matching `sha256:TBD-on-receipt`
placeholder in the matrix (the matrix is FIPS-only), so the recorder will
log "No matrix candidate file contains sha256:TBD-on-receipt" and skip
the matrix flip cleanly.

---

## Step 6 — Review and commit

The script prints a ready-to-run commit command. Before pasting it:

```bash
git diff --cached    # after running the git add line the script printed
```

Sanity-check the diff. You should see:

1. **`specs/_compliance/aws-artifact-pdfs/SHA256SUMS`** — two new lines
   (one comment, one receipt row) appended at the end.
2. **`specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`** —
   `sha256:TBD-on-receipt` flipped to `sha256:<64-hex>` in the AWS KMS
   row (line 66 as of 2026-05-15).

Then paste the printed `git commit` line. It includes `-s` (DCO sign-off)
and the canonical `Co-Authored-By: Claude Opus 4.7` trailer.

---

## Step 7 (manual follow-up) — Update DEBT-003 register row

After committing, edit `specs/_audits/2026-05-15-debt-register.md`:
flip the DEBT-003 row from
`(engineering-CLOSED; operator-bound)` to `(CLOSED <date>, commit
<short-sha>)`. This is a separate commit per the §6 step 9 convention
(keeps the receipt commit narrowly scoped).

---

## Troubleshooting

### "ERROR: multiple matrix-class files contain sha256:TBD-on-receipt"

Unusual — happens only if a renewal cycle introduces a fresh placeholder
in a second matrix-class doc. The recorder bails out to avoid ambiguous
flips. Resolution:

1. Re-run with `--no-matrix-update` (the ledger append still proceeds).
2. Manually flip the correct file via your editor or a targeted
   `sed -i.bak`.
3. Re-run the verifier: `bash scripts/verify-aws-artifact-pdf.sh --verify-ledger`.

### "FAIL: line N malformed" from `--verify-ledger`

Means the recorder produced a non-conforming ledger line — should never
happen for fresh appends but possible if the ledger was hand-edited.
Roll back via:

```bash
git checkout -- specs/_compliance/aws-artifact-pdfs/SHA256SUMS
```

then re-run. File a regression issue against the recorder.

### Re-running after a successful record

Safe. The recorder detects the existing SHA in the ledger and exits 0
without appending. The matrix flip is also a no-op on second run (the
TBD-on-receipt token is already gone). Pure idempotency.

### Reverting a smoke test or test run

```bash
git checkout -- specs/_compliance/aws-artifact-pdfs/SHA256SUMS \
                specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md
bash scripts/verify-aws-artifact-pdf.sh --check-empty
```

The `--check-empty` confirms the ledger is back to header-only state.

---

## Quality-gate alignment

After step 6 commit, the following gates remain green (they were green
before this flow started — the flow does not introduce gate regressions):

- `python3 scripts/validate_specs.py` — `_compliance/` is in `SKIP_ALL`,
  so neither the ledger nor the matrix front-matter trigger validation.
- `python3 scripts/validate_references.py` — no new spec cross-references.
- `bash scripts/verify-aws-artifact-pdf.sh --verify-ledger` — the
  newly-appended line conforms.

The `--check-empty` mode flips to **expected fail** after the first real
receipt — that gate was a wave-27 placeholder-state guard and is replaced
by `--verify-ledger` in the GA cutover preflight (`scripts/ga-cutover-prod-dressrun.sh`)
post-receipt per `aws-artifact-placeholder.md` §5.1.
