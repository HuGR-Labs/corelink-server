# CasReadRequest.principal is plain String
ops: =>

vars: CRR=corelink-handler-cas/src/request.rs

`CasReadRequest.principal` (CRR) = `String` => bridge-time principal synthesis straightforward: bridge struct ctor takes `principal: String` param. In corelink-server.