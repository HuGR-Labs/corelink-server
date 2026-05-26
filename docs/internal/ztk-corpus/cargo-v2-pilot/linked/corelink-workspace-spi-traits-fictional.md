# corelink workspace SPI traits fictional
ops: x ~

vars: CAS=corelink_cas::CasStore  AUTH=corelink_auth::TenantResolver  CRH=corelink_handler_cas::CasReadHandler  CWH=corelink_handler_cas::CasWriteHandler

adapters x need workspace SPI traits CAS, AUTH, CRH, CWH; some x exist as named in workspace.
~ v1 cargo packet HALTed pre-mutation <- these fictional trait surfaces.

refs: [[corelink-adapter-inline-ports-mandate]] [[corelink-cargo-adapter-no-upstream-fetch]] [[corelink-audit-before-mutation]]