# corelink Region enum ambiguity in tests
ops: x => <-
vars: WR=`corelink_worker::Region`  RR=`corelink_replication::region_resolver::Region`

ctx: corelink-server has two distinct `Region` enums.
WR (corelink-worker/src/region.rs): only variant = `Wnam`.
RR: variants = `Wnam` `Enam` `Weur` `Sam`.

clash: importing both into one test module => ambiguous resolution;
       `Region::Enam` resolves to WR (which lacks it).
fix: qualify the path. x rely on bare `Region` <- ambiguity.