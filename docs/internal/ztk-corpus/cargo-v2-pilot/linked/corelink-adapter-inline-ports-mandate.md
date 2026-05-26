# CoreLink adapter inline-ports mandate
ops: x ! <-
vars: README=specs/_proposals/adapters/README.md

!adapters declare adapter-local port traits (e.g. `pub trait CasStore`, `pub trait TenantResolver`) in own `src/ports.rs`.

x import workspace SPI: x `corelink_cas::CasStore`; x `corelink_auth::TenantResolver`; x `corelink_handler_cas::*`.

rationale <- README §"Architectural pattern — inline-ports".

refs: [[corelink-workspace-spi-traits-fictional]] [[corelink-cargo-adapter-no-upstream-fetch]] [[corelink-adapter-test-floor-10]]