### Fixed

- **The public pricing/llms redirects 307'd into 404s (LOTE SEPARADO).** The
  three admin-ui redirect routes carried over from the rejected devenv branch
  targeted `https://humangr.com/corelink/{pricing,pricing.txt,llms.txt}` — all
  three **404 on the live site**, while the apex equivalents (`/pricing`,
  `/pricing.txt`, `/llms.txt`) return 200. The tests shipped alongside asserted
  those exact 404 URLs, so they passed while the buyer funnel redirected into a
  dead page. Retargeted to the verified-live apex URLs through a single source
  of truth in `lib/public-url.ts`, and the test now asserts the SHAPE that was
  wrong (no `/corelink` basePath on a public destination), not just the string.

- **`worker/src/index.ts` replication-coordinator body used `undefined`.** Under
  `exactOptionalPropertyTypes`, `body: ArrayBuffer | undefined` is not assignable
  to `RequestInit`; `null` is the spec-correct "no body" for GET/HEAD. Measured
  by comparing the tsc error SETS, not the counts: 17 → 16, the only difference
  being the target error removed, zero new.
