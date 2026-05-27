# InMemoryCasHandler verifies fake_hash
ops: x => <-
vars: ICH=InMemoryCasHandler  CWR=CasWriteRequest

ICH (test/stub in corelink-server): verifies `fake_hash(bytes) == claimed_hash`.
=> `claimed_hash` in a CWR MUST equal `fake_hash(bytes)` when using ICH.

real prod CAS handler: x such constraint <- verifies BLAKE3.

refs: [[cas-read-request-principal-is-string]]