### Fixed

- **D-1 go-live validation: the audit chain seals 200 rows/hour, so evidence lags
  the event by hours (B-125).** Measured against production D1, with a control
  query beside every count so a zero means absence and not a broken instrument.
  Six consecutive hours sealed **exactly 200 rows each** — a hard cap, not a load
  curve — while seal latency over the last 24 h (n=2692) ran min 12 s, **mean 1 h
  28 min, max 5 h 13 min**, with a 4 243-row backlog whose oldest entry was 6.8 h
  old. Arrivals averaged ~100/h over the same window, so the drain outpaces the
  mean and the backlog shrinks; the entire margin is a factor of two against a
  bursty process whose **observed peak of 300/h already exceeds the ceiling**.
  The customer-visible consequence: an event is not tamper-evident when it
  happens, only ~1.5 h later on average. **Also settled, and it retires one of my
  own claims:** chain-head signing is correct and live — 365 of 365 heads carry
  an Ed25519 signature and the seed is a write-only Cloudflare secret with
  **zero** occurrences in `wrangler.toml` against a control of 5. I had asserted
  a plaintext seed in the repo from memory, without measuring; that is false.
  Dossier in `reports/go-live/D-1-audit-trail.md`, including the three questions
  it explicitly does not decide.
