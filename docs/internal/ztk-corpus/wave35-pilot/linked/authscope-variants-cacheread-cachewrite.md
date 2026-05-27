# AuthScope cache variants
ops: x

`AuthScope` enum (corelink-reapi, used in corelink-server) variants = `CacheRead` `CacheWrite` `CacheFindMissing`.

x `CasRead`/`CasWrite` variants — do not exist.

`CacheFindMissing` x imply `CacheRead`.

refs: [[cas-read-request-principal-is-string]] [[region-not-reexported-from-reapi]]