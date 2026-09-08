mod implementation {
    include!("auth_introspect/part-00.rs");
    include!("auth_introspect/part-00-01.rs");
    include!("auth_introspect/part-01.rs");
}

pub use implementation::*;
