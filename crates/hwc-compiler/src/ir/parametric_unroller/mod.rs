//! Parametric unroller for for loops in spaces (Sprint 3.4)
//!
//! This module expands for loops into individual component/pour/contact/route placements.
//! It supports loop variables in:
//! - Component names: `Adder[i]`
//! - Position expressions: `i * 10mm`
//! - Net names: `Bus[i]`

mod collision;
mod expression;
mod substitution;
mod unroll;

pub use collision::CollisionWarning;
pub use unroll::{unroll_for_loop, UnrolledStatements};
