# corelink-adapter-host crate purpose
ops: => !
vars: CRATE=corelink-adapter-host

CRATE: in corelink-server, added Wave 35.
bridges Wave-34 adapter ports (cargo/brew/oci/npm/pip) => workspace Stage-1 SPI.
src: lib.rs cargo.rs brew.rs oci.rs npm.rs pip.rs.
!each src file <500 LOC.