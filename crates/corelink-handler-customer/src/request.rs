//! Request / response envelopes for the customer handler surface.
//!
//! All shapes are `#[non_exhaustive]` + expose a `pub fn new()`
//! constructor so out-of-crate callers can construct them without
//! struct-literal syntax. Field names mirror the `customer-types.ts`
//! TypeScript types exactly (snake_case).

mod billing;
mod keys;
mod overview;
mod team;
mod usage;

pub use billing::*;
pub use keys::*;
pub use overview::*;
pub use team::*;
pub use usage::*;

#[path = "audit_query.rs"]
mod audit_query;
pub use audit_query::*;
