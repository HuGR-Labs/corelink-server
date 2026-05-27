# DEBT-003 — AWS Artifact PDF Placeholder Slot + Verification Harness — Audit Doc

> **Doc kind:** wave-27 R-prep audit / engineering-side DEBT-003 closure-prep
> (no canonical front matter required — `_audits/` excluded from
> `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-27 R-prep DEBT-003 placeholder agent (Claude Opus 4.7) on
> branch `wt/r-prep-debt-003-aws-artifact-placeholder`.
> **Base:** `main` @ `a48bbec` ("merge wt/r-prep-cf-worker-prefetch-wire into
> main (wave-26)" — wave-26 SEAL tip).
> **Scope:** Author the engineering-side artifacts needed for the Owner to
> close DEBT-003 with minimal friction. Specifically: (a) a canonical
> placeholder slot doc describing the artifact, fetch flow, hash format, and
> closure protocol; (b) an append-only `SHA256SUMS` ledger; (c) a verifier
> shell script with three modes (`--check-empty`, `--verify-ledger`,
> `--pdf <path>`); (d) a `.gitignore` that excludes PDF binaries while
> keeping the ledger committed; (e) DEBT register reference uplift.
>
> **Cross-ref:** `specs/_audits/2026-05-15-debt-register.md` (DEBT-003 row),
> `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` (§2 AWS KMS row with
> `sha256:TBD-on-receipt` token), `specs/_compliance/fips-attestation-letters/LETTER-AWS-KMS.md`
> (vendor letter complement to the AWS Artifact PDF).

---

## 1. Charter

DEBT-003 ("AWS attestation doc hash placeholder", P0, target 2026-06-14) is
engineering-prep-light by design: the actual artifact is the AWS Artifact
PDF, which can only be downloaded by an AWS-authenticated principal with
NDA acceptance, and the SHA-256 binds to *that specific PDF byte-stream*.
Both of those are operator-bound. What engineering can prepare:

1. The canonical **slot doc** describing where the artifact comes from,
   what the SHA-256 represents, and how the operator records it.
2. The **ledger file** that holds the receipts (append-only audit chain).
3. The **verifier** that mechanically checks (a) the slot is in its
   initial empty state at CI time, (b) the ledger lines parse, (c) a
   local PDF copy matches a ledger row.
4. The **gitignore** rule that ensures PDFs don't accidentally land in the
   repo (AWS terms-of-use forbid redistribution).

The wave-27 R-prep stream lands all four engineering-side artifacts so
that on the day the Owner fetches the PDF, the closure flow is a single
git commit (append-ledger + update matrix). No further engineering work
is required between this wave and DEBT-003 closure.

---

## 2. Deliverables shipped this stream

| # | Artifact | Path | Notes |
|---|---|---|---|
| 1 | Placeholder slot doc | `specs/_compliance/aws-artifact-placeholder.md` | 9 sections; covers required artifact, fetch flow, SHA-256 record protocol, storage rationale, verifier modes, closure protocol, quality gates, freeze allowance, completion checklist. |
| 2 | Verifier script | `scripts/verify-aws-artifact-pdf.sh` | 3 modes (`--check-empty`, `--verify-ledger`, `--pdf <path>`); chooses `sha256sum` (Linux) or `shasum -a 256` (macOS) automatically; documented in script header; exits {0, 1, 2}. |
| 3 | SHA256SUMS ledger | `specs/_compliance/aws-artifact-pdfs/SHA256SUMS` | Header-only initial state; line format `<64-hex>  <filename>  # fetched=YYYY-MM-DD principal=<arn> kind=<fips\|soc2\|other>`. |
| 4 | Gitignore | `specs/_compliance/aws-artifact-pdfs/.gitignore` | Ignores everything except `SHA256SUMS` and `.gitignore` itself — guarantees PDF binaries can't slip into git. |
| 5 | This audit doc | `specs/_audits/2026-05-16-debt-003-aws-artifact-placeholder.md` | Land record + Owner action sequence. |
| 6 | DEBT register reference uplift | `specs/_audits/2026-05-15-debt-register.md` | DEBT-003 row gains `engineering-CLOSED; operator-bound` annotation + cross-ref to this doc and the verifier. |

---

## 3. Verifier mode reference

The verifier (`scripts/verify-aws-artifact-pdf.sh`) is intentionally
narrow. It is **not** a downloader; it is **not** a ledger mutator. It
mechanically checks invariants on the existing files. Operator workflow
keeps the audit chain explicit: every append is its own git commit.

### 3.1 `--check-empty`

```
bash scripts/verify-aws-artifact-pdf.sh --check-empty
```

- Exit 0 iff `SHA256SUMS` contains zero non-comment, non-blank lines.
- Used as a wave-27 quality gate (no PDFs yet) and a sanity check that
  the placeholder remains "empty" until the Owner appends the first
  receipt.
- Used in CI on every PR until DEBT-003 closes; once it closes (first
  receipt appended), the gate switches to `--verify-ledger` via a
  follow-up wave that wires it into `ga-cutover-prod-dressrun.sh`.

### 3.2 `--verify-ledger`

```
bash scripts/verify-aws-artifact-pdf.sh --verify-ledger
```

- Exit 0 iff every non-comment, non-blank line in `SHA256SUMS` matches
  `<64-hex>  <filename>  # fetched=YYYY-MM-DD principal=<arn> kind=<fips|soc2|other>`.
- Used post-receipt to keep the ledger format clean.
- Counts and reports the receipt line count.

### 3.3 `--pdf <path>`

```
bash scripts/verify-aws-artifact-pdf.sh --pdf /path/to/local-copy.pdf
```

- Computes SHA-256 of the local PDF.
- Looks up that digest in `SHA256SUMS` (skipping comment lines).
- Exit 0 iff a matching row is found. Prints the matching line so the
  operator can spot-check the `fetched=`, `principal=`, `kind=`
  metadata.

### 3.4 Smoke test (performed by this wave)

The verifier was smoke-tested against four scenarios pre-land:

| Scenario | Mode | Expected exit | Observed exit |
|---|---|---|---|
| Empty ledger (initial state) | `--check-empty` | 0 | 0 ✅ |
| Empty ledger (initial state) | `--verify-ledger` | 0 (0 receipts) | 0 ✅ |
| Local PDF, hash not in ledger | `--pdf <local>` | 1 | 1 ✅ |
| Local PDF + matching ledger row | `--pdf <local>` | 0 | 0 ✅ |
| Ledger with malformed line | `--verify-ledger` | 1 | 1 ✅ |
| `--check-empty` after a receipt landed | `--check-empty` | 1 | 1 ✅ |
| Missing PDF path | `--pdf /nonexistent` | 2 (invocation error) | 2 ✅ |
| Unknown flag | `--bogus` | 2 (invocation error) | 2 ✅ |

All eight scenarios passed pre-land.

---

## 4. Operator action sequence (DEBT-003 closure)

The Owner can close DEBT-003 entirely without further engineering input:

1. **Authenticate to AWS Artifact** (console or AWS CLI; see placeholder doc §2).
2. **Download the FIPS 140-3 KMS Validation Report PDF** (primary target).
   Optionally also download the SOC 2 Type II report (secondary; can be a
   second receipt with `kind=soc2`).
3. **Compute SHA-256:**
   ```bash
   shasum -a 256 AWS-Artifact-FIPS-140-3-KMS-2026-Q2.pdf
   ```
4. **Append one line to `specs/_compliance/aws-artifact-pdfs/SHA256SUMS`** in
   the format:
   ```
   <64-hex>  AWS-Artifact-FIPS-140-3-KMS-2026-Q2.pdf  # fetched=YYYY-MM-DD principal=<arn> kind=fips
   ```
5. **Update `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`** §2 AWS KMS
   row: replace the `sha256:TBD-on-receipt` token with the real value.
   Bump the matrix `updated:` front-matter field.
6. **Verify locally**:
   ```bash
   bash scripts/verify-aws-artifact-pdf.sh --verify-ledger
   bash scripts/verify-aws-artifact-pdf.sh --pdf /path/to/local.pdf
   ```
   Both must exit 0.
7. **Commit** both files with message `chore(debt-003): record AWS Artifact
   FIPS PDF SHA-256 (YYYY-MM-DD)`.
8. **Flip the DEBT-003 row** in `specs/_audits/2026-05-15-debt-register.md`
   from `(engineering-CLOSED; operator-bound)` to `CLOSED <date>, commit
   <sha>` and add the commit hash.

Total Owner time: ~10–15 min (most of which is the AWS Artifact
NDA-acceptance click-through).

---

## 5. Quality gates

Wave-27 R-prep land gates (executed pre-commit):

- `bash scripts/verify-aws-artifact-pdf.sh --check-empty` exits 0.
- `bash scripts/verify-aws-artifact-pdf.sh --verify-ledger` exits 0 (zero
  receipts is a well-formed-by-vacuity ledger).
- `python3 scripts/validate_specs.py` green (`_compliance/` + `_audits/`
  both in `SKIP_ALL`, no front-matter required).
- `python3 scripts/validate_references.py` green (no new cross-references
  introduced that aren't already in scope).

Post-receipt gates (Owner-bound; fired by the §4 closure flow):

- `bash scripts/verify-aws-artifact-pdf.sh --verify-ledger` exits 0.
- `bash scripts/verify-aws-artifact-pdf.sh --pdf <local.pdf>` exits 0.
- BYOK matrix no longer contains the `sha256:TBD-on-receipt` token.

---

## 6. Freeze allowance

This deliverable lands during GA-1 feature freeze. Per
`scripts/check-ga-freeze-allowed.py` `SKIP_PATH_PATTERNS`:

- `specs/_compliance/` is implicit-allow (§3.d).
- `specs/_audits/` is implicit-allow (§3.d).
- `scripts/*.sh` is not enumerated on the frozen surface list (§2 covers
  `crates/`, `apps/`, `openapi/`, `dashboards/`, `migrations/`,
  `schemas/`, and `specs/` minus implicit-allow subtrees only).

No `FREEZE-EXCEPTION:` token is required. The change is also a P1
GA-blocker prep per DEBT-003's status, which would qualify under §3.b
(`P1-ga-blocker`) if the freeze gate complained, but it does not — the
paths are all implicit-allow.

---

## 7. Why "engineering-CLOSED; operator-bound" (not just OPEN)

DEBT-003 was filed as a single-line action ("Human downloads AWS Artifact
PDF + records SHA-256 + updates matrix row") because the closure work is
mostly procedural. But the engineering-side procedural scaffolding (a
ledger format, a verifier, a `.gitignore` that prevents PDFs from leaking
into the repo, the canonical placeholder doc) is non-trivial and exactly
the kind of activation-energy lift that DEBT-026 (pentest tracker) also
performed in wave-26.

Without wave-27's scaffolding, the Owner's closure flow would require
deciding:

- Where do the PDFs go? (Repo? Off-repo? Both?)
- What is the SHA-256 line format?
- How do I prove the matrix row's hash matches a real PDF?
- How do I prevent accidental commit of the PDF binary?
- What does CI check between now and closure?

Wave-27 answers all five with canonical artifacts. The remaining work
(download + hash + commit) is then ~10 min of Owner time, with mechanical
verification at every step.

Hence the DEBT-003 row uplifts to `engineering-CLOSED; operator-bound`
rather than full CLOSED. Full CLOSED is reserved for the commit that
records the first real SHA-256 row in `SHA256SUMS` and replaces the
`TBD-on-receipt` token in the matrix.

---

## 8. Cross-product references

| Where | Reference | Why |
|---|---|---|
| `specs/_compliance/aws-artifact-placeholder.md` §1 | This audit doc + DEBT-003 row | Canonical placeholder slot. |
| `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` §2 AWS KMS row | `sha256:TBD-on-receipt` token | Will be flipped on Owner closure. |
| `scripts/verify-aws-artifact-pdf.sh` | Placeholder doc §4.2 ledger format + §5 verifier modes | Implements the gate logic. |
| `specs/_compliance/aws-artifact-pdfs/SHA256SUMS` | Placeholder doc §4.2 line format | Append-only ledger. |
| `specs/_compliance/aws-artifact-pdfs/.gitignore` | Placeholder doc §4.1 storage rationale | Ensures PDFs never commit. |
| `specs/_audits/2026-05-15-debt-register.md` DEBT-003 row | This audit doc + the verifier + the ledger + the placeholder doc | Row uplift to `engineering-CLOSED; operator-bound`. |

---

## 9. Land summary

- Branch: `wt/r-prep-debt-003-aws-artifact-placeholder`.
- Base: `main` @ `a48bbec`.
- Files touched: 6 (4 new in `specs/_compliance/` + `specs/_compliance/aws-artifact-pdfs/`,
  1 new in `scripts/`, 1 new + 1 updated in `specs/_audits/`).
- Quality gates: see §5; all wave-27 land gates green pre-commit.
- DEBT-003 status delta: OPEN → `engineering-CLOSED; operator-bound` (full
  closure pending the Owner's PDF fetch).
- Time budget: 30 min per wave-27 dispatch envelope.
