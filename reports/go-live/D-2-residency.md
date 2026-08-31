# D-2 — Residency: does data written for region X appear in region Y?

**Version stamp.** Every number below was read from production D1 (`d64742ea`)
on 2026-08-30 while the five prod containers ran **`cab16a3a-r1`**. `main` was
**21 commits ahead** of that image, **5 of them container-affecting**, so these
numbers describe the code that is SERVING, not the code that is merged.

**Method.** Same discipline as D-1: every query that could return zero carries a
control beside it that is known to return rows.

---

## 1. The naive answer, and why it is not the answer

| query | control | result |
|---|---|---|
| audit rows whose `region` ≠ the tenant's `primary_region` | the join itself returns **74 333** rows | **0 violations** |

Zero violations. Taken alone this reads as "residency holds". It does not mean
that, for two reasons.

## 2. The check silently drops 4.7% of the population

| query | result |
|---|---|
| rows in `audit_outbox` | **77 935** |
| rows that join to a `tenant` row | **74 333** |
| **rows whose `tenant_id` has NO `tenant` row** | **3 670**, across **175 distinct tenant ids** |

A `JOIN`-based residency check cannot evaluate its own predicate for those rows —
there is no `primary_region` to compare against — and a `JOIN` **drops them
without saying so**. The "0 violations" above was computed over a population that
had already had 4.7% removed from it, by the query itself.

This is the failure mode the campaign keeps meeting in a new costume: the
instrument returned a clean number by not looking at the part that could be
dirty. The orphan rows are not a rounding error — they are exactly the rows for
which the invariant is **unevaluable**, which is a weaker state than "satisfied"
and a different one from "violated".

### 2b. Where the orphans come from — and most of them are CORRECT

The obvious next question is whether those tenants were **erased** (the audit
trail deliberately outliving the tenant) or **never existed** (an audit write
accepting an unvalidated `tenant_id`). Those are different defects and one is far
worse. It separates with one query:

| orphan tenants | rows | reading |
|---|---|---|
| present in `dsr_erasure_log` — **erased** | **170** of 175 | **3 526** | correct by design: audit rows are RETAIN-class evidence (Art. 5(2) accountability) and must survive an Art. 17 erasure |
| **never erased** | **5** | **144** | unexplained |

*(control: `dsr_erasure_log` holds 2 044 rows, so the lookup is live.)*

**This materially shrinks the finding, and it is recorded rather than left
overstated.** 97% of the orphans are the system working as designed — erasure
removes the tenant and deliberately keeps the evidence. The residual is **5
tenants / 144 rows**: 3 in `wnam` (136 rows), 1 in `weur` (4 rows), 1 in `apac`
(4 rows).

So the `weur` rows in §3 are in the residual, not in the erased set. Whatever
they are, they are not explained by erasure.

What remains true regardless of cause: **the check still cannot evaluate its
predicate for any of the 3 670**, and it drops all of them silently. Correct
orphans and unexplained orphans are equally invisible to a `JOIN`.

## 3. There is data in a region no tenant is provisioned for

| | |
|---|---|
| tenants by `primary_region` | **enam 153, wnam 108, apac 1** — no tenant in `weur` |
| audit rows by `region` | enam **76 129**, wnam **1 869**, weur **4**, apac **4** |
| do the 4 `weur` rows have a tenant row? | **no — 0 of 4** |

Four rows sit in `weur` while **no tenant declares `weur`** as its primary
region, and all four are orphans, so the JOIN check never saw them. The count is
tiny; the property is not: a row exists in a region for which the residency
predicate has nothing to check.

## 4. The enforcement is essentially untested by production traffic

**76 129 of 77 935 rows (97.7%) are `enam`.** The cross-region rejection path
(`residency.rs` — 409 `residency_violation`, plus the migration-0023 triggers
that `RAISE(ABORT)`) is real code, but production has produced almost no
cross-region traffic for it to act on. A control that never fires has never been
shown to fire.

That is not a defect in itself. It is a statement about what the 0 above is
evidence FOR: it is evidence that nothing has gone wrong, not evidence that the
mechanism would catch it.

## 5. What this dossier does NOT decide

- **Whether the R2 object plane agrees with the D1 metadata plane.** CAS objects
  are keyed `<region>/<tenant_prefix_16>/<digest>`, and the tenant prefix is an
  HMAC under the TDK, which is a write-only secret. Without it a listing cannot
  be attributed to a tenant, so object-plane residency was not measured here.
- **Whether the 409 path actually fires.** Not exercised — that needs a
  deliberately cross-region request against production.
- **The two `Region` enums.** The erasure-attestation `Region` and
  `corelink-region`'s are distinct types; conflating them produces false
  attestation. Not re-verified here, and no finding above depends on
  distinguishing them, because every query above reads the `region` COLUMN, not
  either enum.
- **WORM / Object Lock.** R2 returns `NotImplemented`, so immutability promises
  are platform-blocked rather than unimplemented. Prior knowledge, not
  re-measured today.

## 6. Verdict

Residency, as far as production data can show it, **holds where it can be
evaluated** — and it cannot be evaluated for 3 670 rows, 4 of which are in a
region no tenant is provisioned for. The number to fix is not the violation count
(0); it is the **unevaluable** count.

Filed as **B-127**.
