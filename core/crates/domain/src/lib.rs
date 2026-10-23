//! Crux core domain: plain data types and pure functions for every rule.
//!
//! Nothing in this crate does I/O, reads the clock or draws random numbers; `now`, rngs and config
//! are arguments, so every rule is a deterministic unit test (backend-spec 2.1).

pub mod auth;
pub mod catalog;
pub mod ids;
pub mod page;
pub mod policy;
pub mod tier;
pub mod topics;
pub mod user;
pub mod validation;

pub use validation::{FieldError, ValidationErrors};
