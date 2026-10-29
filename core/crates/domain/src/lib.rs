//! Crux core domain: plain data types and pure functions for every rule.
//!
//! Nothing in this crate does I/O, reads the clock or draws random numbers; `now`, rngs and config
//! are arguments, so every rule is a deterministic unit test (backend-spec 2.1).

pub mod auth;
pub mod builder;
pub mod catalog;
pub mod fsrs;
pub mod grading;
pub mod ids;
pub mod localtime;
pub mod mock;
pub mod page;
pub mod plan;
pub mod policy;
pub mod question;
pub mod study;
pub mod tier;
pub mod topics;
pub mod user;
pub mod validation;
pub mod weakspot;

pub use validation::{FieldError, ValidationErrors};
