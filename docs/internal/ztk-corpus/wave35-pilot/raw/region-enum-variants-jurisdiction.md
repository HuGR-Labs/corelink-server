# Region enum jurisdiction binding
ops: => !

vars: REGION=`corelink-region/src/region.rs`

corelink-server: REGION `Region` enum variants = Wnam (us-west), Enam (us-east), Weur (eu-west), Sam (sa-east) => Cloudflare R2/D1 region codes.

!Weur => DO jurisdiction = "eu".