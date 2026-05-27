# StubPatValidator::insert signature
vars: F=corelink-reapi/src/pat.rs

corelink-server: `StubPatValidator::insert` (F) sig=`(token, tenant_id: Uuid, principal_id: Uuid, region: Region, scopes: impl IntoIterator<Item=AuthScope>)`.