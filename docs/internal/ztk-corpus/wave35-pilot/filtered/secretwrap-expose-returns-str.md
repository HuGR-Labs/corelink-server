# SecretWrap::expose returns &str
ops: > !
vars: SW=`SecretWrap::expose()`  SECRET=corelink-core/src/types/secret.rs  ES=`secrecy::ExposeSecret::expose_secret`

corelink-server: SW (SECRET) returns `&str` direct. !SW > ES <- crates avoid direct `secrecy` dependency.