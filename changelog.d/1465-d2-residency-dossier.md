### Fixed

- **D-2 go-live validation: 3 670 audit rows have no tenant, so the residency
  predicate is unevaluable — and the obvious check reports clean (B-127).**
  Measured against production D1 with containers running `cab16a3a-r1`, which was
  21 commits behind `main` (5 container-affecting) — so the numbers describe the
  code that is SERVING. Joining `audit_outbox` to `tenant` and comparing `region`
  to `primary_region` returns **0 violations** over 74 333 rows. That number is
  computed over a population the query itself had already trimmed: **3 670 rows
  (175 distinct tenant ids, 4.7%) have no `tenant` row at all**, so the predicate
  cannot be evaluated for them, and a `JOIN` drops them **without saying so**.
  Unevaluable is a third state — weaker than satisfied, different from violated —
  and nothing currently distinguishes it from satisfied. Four of those orphans
  sit in **`weur`, a region no tenant is provisioned for**; the count is trivial,
  the property is not. Bounding what any green result means here: **76 129 of
  77 935 rows (97.7%) are `enam`**, so the cross-region rejection path is real
  code that production traffic has barely exercised — a control that never fires
  has not been shown to fire. **Cause separated rather than left open:** 170 of
  the 175 orphan tenants (3 526 of the 3 670 rows) appear in `dsr_erasure_log`
  and are therefore CORRECT — audit rows are RETAIN-class evidence under
  Art. 5(2) and must survive an Art. 17 erasure — which shrinks the unexplained
  residual to **5 tenants / 144 rows**, the `weur` ones among them. The item
  states the smaller true number rather than the larger first one. Dossier in
  `reports/go-live/D-2-residency.md`,
  including the four questions it explicitly does not decide (object-plane
  residency, the 409 path, the two distinct `Region` enums, and R2's
  platform-blocked Object Lock).
