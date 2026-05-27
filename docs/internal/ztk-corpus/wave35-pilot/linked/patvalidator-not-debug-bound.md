# PatValidator trait lacks Debug bound
ops: x =>
vars: PV=PatValidator

PV in corelink-server: trait bound x require `Debug`. => structs holding `Arc<dyn PV>` x `#[derive(Debug)]`, => need manual `Debug` impl.

affected: tenant bridges CargoTenantBridge, BrewTenantBridge, NpmTenantBridge, PipTenantBridge, OciTenantBridge in corelink-adapter-host.

refs: [[pat-validator-authenticate-signature]] [[stubpatvalidator-insert-signature]]