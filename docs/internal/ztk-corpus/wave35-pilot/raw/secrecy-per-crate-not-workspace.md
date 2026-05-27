# secrecy pinned per-crate not workspace
ops: x <-
vars: SV=`secrecy = "0.10"`

corelink-server: secrecy crate pinned `"0.10"`.
x in `[workspace.dependencies]`; each crate declares SV directly, x `workspace = true`.
corelink-core uses 0.10 `ExposeSecret`/`SecretString` API <- this pin.