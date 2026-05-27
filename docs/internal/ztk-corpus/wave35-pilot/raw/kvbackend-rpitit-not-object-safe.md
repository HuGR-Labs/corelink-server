# KvBackend RPITIT not object-safe
ops: x => <-

vars: KB=KvBackend  CL=corelink-server

KB trait (`corelink-worker/src/cache/kv.rs:158` in CL) uses RPITIT (`impl Future` return types) <- not async_trait. => x object-safe: x usable as `Arc<dyn KvBackend>`. bridges over KB => generic `K: KvBackend + 'static`.