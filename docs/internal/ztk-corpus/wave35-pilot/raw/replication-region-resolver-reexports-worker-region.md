# corelink_replication Region re-exports corelink_worker Region
ops: x <- =>
vars: RR=`corelink_replication::region_resolver::Region`  WR=`corelink_worker::Region`

RR <- `pub use corelink_worker::{Region, TenantCtx};` in corelink-server => RR IS WR.
variants: only `Wnam`. x `Enam` x `Sam` x `Weur` (do not exist on RR).
corelink-reapi pat.rs imports same RR.