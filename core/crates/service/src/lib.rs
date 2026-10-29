//! Use cases: one function per action, one transaction per use case, authorization calls inside
//! (spec 2.1). Returns view types, never repo rows.

pub mod account;
pub mod activity;
pub mod admin;
pub mod ai;
pub mod attempts;
pub mod auth;
pub mod bank;
pub mod catalog;
pub mod checks;
pub mod contracts;
pub mod courses;
pub mod ctx;
pub mod deadlines;
pub mod digests;
pub mod error;
pub mod events;
pub mod flash;
pub mod import;
pub mod invites;
pub mod jwt;
pub mod me;
pub mod mistakes;
pub mod mocks;
pub mod next;
pub mod password;
pub mod plan;
pub mod practice;
pub mod prod_guard;
pub mod ratelimit;
pub mod saver;
pub mod sse;
pub mod stats;
pub mod time;
pub mod views;

pub use ctx::{Actor, Ctx};
pub use error::{AppError, AppResult};
