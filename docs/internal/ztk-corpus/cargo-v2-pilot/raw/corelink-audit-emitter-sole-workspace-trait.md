# CoreLink sole workspace trait: AuditEmitter
ops: ! <-
vars: AE=`corelink_audit::ports::AuditEmitter`  IM=InMemoryAuditEmitter

AE = the one workspace trait CoreLink adapters consume. !chokepoint <- sync emit, fail-CLOSED. IM = existing impl, used for tests.