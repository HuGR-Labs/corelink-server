#[forbid(unsafe_code)]
mod implementation {
    include!("bazel_v2/part-00.rs");
    include!("bazel_v2/part-01.rs");
}

pub use implementation::*;
