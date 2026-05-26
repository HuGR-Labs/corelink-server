# CoreLink adapter crate safety conventions
ops: ! => x
vars: SS=SecretString  CT=`subtle::ConstantTimeEq`

adapter crates => every pub type `#[non_exhaustive]`; `#![forbid(unsafe_code)]`.

!zero `unwrap`/`expect`/`panic` in `src/`.

PATs => SS + CT for comparison; x compare PATs any other way.