### Fixed

- **D-3 go-live validation: the DPA acceptance record is client-asserted, and
  production already holds three hashes for one version.** Measured with
  prod containers verified at `ddd95560-r1` in all five regions by `GET` per
  application id — the list endpoint serves a stale view. `routes/dpa_accept.rs`
  stores the **client-attested** `notice_hash` and does not re-derive it,
  validating only hex64 well-formedness; its own module doc argues that is
  forensically preferable to re-deriving against a divergent copy. **But the
  justification cites a file that is not in the repository** —
  `legal/dpa/v1.0.0.<locale>.md` does not exist, and neither does `legal/dpa/`
  (control: `legal/**` holds 41 files and lists fine). **And production shows the
  divergence already:** 24 acceptances, all version `1.0.0` locale `en-US`, carry
  **3 distinct `notice_hash` values**. Two of them are a clean succession two
  hours apart — consistent with the notice text being edited **without a version
  bump**, which matters because the version is what `dpa:{tenant}:{version}` and
  the re-accept gate key on. The third is a **singleton inside the first
  cluster's window**, nine hours after it opened, while every other client
  attested the other hash: one customer attested bytes nobody else did, and
  nothing checked. Stale cache, partial render, wrong locale asset and a
  hand-made request are indistinguishable in that record, and remain so after the
  fact because no server-side copy of the served notice was kept. Dossier in
  `reports/go-live/D-3-dpa-consent.md`, with the four questions it does not
  decide.
