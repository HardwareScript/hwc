//! Compile-time expression AST, evaluation, and formatting.
//!
//! This module is the modular replacement for the former monolithic
//! `ast/expression.rs`. The public surface is unchanged: every type that used
//! to live in that file is re-exported here, so `crate::ast::expression::X`
//! and `crate::ast::X` continue to resolve exactly as before.
//!
//! Layout:
//! - [`types`]      — the [`Expression`] enum (pure data, no behavior)
//! - [`operators`]  — [`BinaryOperator`] / [`UnaryOperator`] and their semantics
//! - [`value`]      — the evaluated [`Value`] enum, unit conversions, [`EvaluationContext`]
//! - [`inspect`]    — read-only queries over an [`Expression`] (span, literal, anchors)
//! - [`eval`]       — the `Expression::evaluate` entry points
//! - [`arithmetic`] — typed binary/unary math over [`Value`] (dimensional analysis)
//! - [`functions`]  — built-in function calls (`sin`, `sqrt`, `min`, ...)
//! - [`display`]    — `Display` rendering back to Hardware Script source form

mod arithmetic;
mod display;
mod eval;
mod functions;
mod inspect;
mod operators;
mod types;
mod value;

#[cfg(test)]
mod tests;

pub use operators::{BinaryOperator, UnaryOperator};
pub use types::Expression;
pub use value::{EvaluationContext, Value};
