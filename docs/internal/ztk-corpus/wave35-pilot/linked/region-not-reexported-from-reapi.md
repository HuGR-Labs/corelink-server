# Region not re-exported from corelink_reapi
ops: x => <-
vars: REGION=`corelink_replication::region_resolver::Region`

corelink-server: x `Region` re-export from `corelink_reapi`.
`corelink-reapi` lib.rs exports only `AuthScope, AuthStubError, PatValidator, StubPatValidator, TenantContext`.
need Region => use REGION <- it re-exports `corelink_region` via `pub use corelink_region::*;`.

refs: [[authscope-variants-cacheread-cachewrite]] [[pat-validator-authenticate-signature]] [[stubpatvalidator-insert-signature]]