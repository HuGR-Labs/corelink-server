# CoreLink audit-before-mutation chokepoint
ops: ! => x

audit-event ! => emit BEFORE every state mutation in CoreLink adapters
  (e.g. 1 event before each CAS put). fail-closed chokepoint =>
  x mutation w/o preceding audit-event.