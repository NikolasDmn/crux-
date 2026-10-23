//! Use cases: one function per action, one transaction per use case, authorization calls inside
//! (spec 2.1). Returns view types, never repo rows.

pub mod admin;
pub mod auth;
pub mod contracts;
pub mod ctx;
pub mod error;
pub mod events;
pub mod invites;
pub mod jwt;
pub mod me;
pub mod password;
pub mod ratelimit;
pub mod sse;
pub mod time;
pub mod views;

pub use ctx::{Actor, Ctx};
pub use error::{AppError, AppResult};
