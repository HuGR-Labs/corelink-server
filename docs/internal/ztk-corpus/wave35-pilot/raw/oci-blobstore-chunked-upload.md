# OCI BlobStore resists CAS SPI mapping
ops: x <-
vars: CL=corelink-server

CL OCI adapter: `BlobStore` port = chunked upload protocol (`open_upload`, `append_chunk`, `finalize_upload`...) x clean map onto CAS `CasReadHandler`/`CasWriteHandler` SPI traits <- chunked != CAS read/write shape (unlike cargo/brew adapters which map cleanly).

same adapter: `ManifestKvStore` = `KvBackend`; `TenantResolver` = `PatValidator`.