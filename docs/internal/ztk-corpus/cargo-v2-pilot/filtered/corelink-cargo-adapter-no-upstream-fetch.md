# CoreLink cargo adapter has no upstream fetch
ops: x <-
vars: SCCACHE=sccache

cargo (SCCACHE) adapter x upstream fetch path; unlike brew adapter.
<- SCCACHE pushes build artifacts to server: adapter receives PUTs, x fetch from upstream domain.